//! Sink and muxer entry points of the encoder (line cap): `encode_write`
//! over any `io::Write`, `wrap_adts_au`, and the raw-AU M4A muxers.

use super::{FRAME, adts_frame_into, fs_index};
use crate::error::{AacError, Result};
use crate::m4a_write;
use crate::options::EncodeOptions;

/// Encode planar PCM incrementally into any [`std::io::Write`] sink (TASK-59):
/// ADTS frames (or raw access units with [`EncodeContainer::Raw`]) are
/// written as they are produced, so the codec workspace stays one frame
/// and no compressed buffer is collected. Bytes are identical to
/// [`encode_with`] for the same options. M4A is
/// [`crate::UnsupportedFeature::EncodeM4aStreaming`].
///
/// Lifecycle: each frame is written once with `write_all`; a sink error
/// ends the encode with [`AacError::Io`] after the frames that already
/// succeeded (nothing is retried or written twice), so the sink holds an
/// exact prefix of the stream. The sink is not flushed.
///
/// ```
/// use syom::{EncodeOptions, encode_write};
/// let pcm = vec![0.0f32; 3000];
/// let mut sink = Vec::new();
/// let info = encode_write(&mut sink, &[&pcm[..]], 48_000, &EncodeOptions::adts())?;
/// assert_eq!(info.bytes as usize, sink.len());
/// assert_eq!(info.samples, 3000);
/// # Ok::<(), syom::AacError>(())
/// ```
pub fn encode_write<W: std::io::Write, P: AsRef<[f32]>>(
    mut sink: W,
    pcm: &[P],
    sample_rate: u32,
    opts: &EncodeOptions,
) -> Result<crate::EncodeInfo> {
    let planes: Vec<&[f32]> = pcm.iter().map(AsRef::as_ref).collect();
    let mut enc = crate::Encoder::new(sample_rate, planes.len(), opts)?;
    let n = planes.first().map_or(0, |p| p.len());
    let mut at = 0usize;
    while at < n {
        let end = (at + 4096).min(n);
        let chunk: Vec<&[f32]> = planes.iter().map(|p| &p[at..end]).collect();
        enc.feed(&chunk, |f| sink.write_all(f.au).map_err(AacError::Io))?;
        at = end;
    }
    enc.finish(|f| sink.write_all(f.au).map_err(AacError::Io))
}

/// Wrap one `raw_data_block` in an ADTS frame (no CRC, fullness `0x7FF`).
/// For HE v1 access units pass the **core** rate (half the input rate):
/// ADTS signals SBR implicitly and the header carries the core rate.
///
/// ```
/// use syom::{EncodeOptions, Encoder, wrap_adts_au};
/// let pcm = vec![0.1f32; 2048];
/// let mut enc = Encoder::new(48_000, 1, &EncodeOptions::raw())?;
/// let mut aus = Vec::new();
/// enc.feed(&[&pcm], |f| { aus.push(f.payload.to_vec()); Ok(()) })?;
/// enc.finish(|f| { aus.push(f.payload.to_vec()); Ok(()) })?;
/// let mut adts = Vec::new();
/// for au in &aus {
///     adts.extend_from_slice(&wrap_adts_au(au, 48_000, 1)?);
/// }
/// assert_eq!(adts[0], 0xff);
/// # Ok::<(), syom::AacError>(())
/// ```
pub fn wrap_adts_au(payload: &[u8], sample_rate: u32, channels: usize) -> Result<Vec<u8>> {
    let fs = fs_index(sample_rate)?;
    if !(1..=2).contains(&channels) {
        return Err(AacError::encode(format!(
            "encode: channels must be 1 or 2, got {channels}"
        )));
    }
    let mut out = Vec::with_capacity(crate::engine::adts::ADTS_HEADER_BYTES_NO_CRC + payload.len());
    adts_frame_into(payload, fs, channels, &mut out);
    Ok(out)
}

/// Mux raw HE v1 access units (push [`crate::Encoder`] with
/// [`EncodeOptions::with_he`] + [`EncodeContainer::Raw`]) into M4A:
/// explicit two-rate ASC, output-rate timeline, `elst` priming as
/// reported by [`crate::EncodeInfo::priming`].
pub fn mux_raw_he_m4a(
    payloads: &[&[u8]],
    output_rate: u32,
    channels: usize,
    valid_samples: u64,
) -> Result<Vec<u8>> {
    let core = crate::engine::enc_sbr_prep::he_core_rate(output_rate)
        .map_err(|_| AacError::Unsupported(crate::UnsupportedFeature::EncodeHeRate(output_rate)))?;
    if !(1..=2).contains(&channels) {
        return Err(AacError::encode(format!(
            "encode: channels must be 1 or 2, got {channels}"
        )));
    }
    let owned: Vec<Vec<u8>> = payloads.iter().map(|p| p.to_vec()).collect();
    m4a_write::mux_aac_he(
        &owned,
        fs_index(core)?,
        fs_index(output_rate)?,
        channels,
        output_rate,
        valid_samples,
        crate::engine::enc_he::HE_PRIMING_OUT,
    )
}

/// Mux raw LC access units into M4A (`elst` priming 1024, presentation `valid_samples`).
pub fn mux_raw_lc_m4a(
    payloads: &[&[u8]],
    sample_rate: u32,
    channels: usize,
    valid_samples: u64,
) -> Result<Vec<u8>> {
    let fs = fs_index(sample_rate)?;
    if !(1..=2).contains(&channels) {
        return Err(AacError::encode(format!(
            "encode: channels must be 1 or 2, got {channels}"
        )));
    }
    let owned: Vec<Vec<u8>> = payloads.iter().map(|p| p.to_vec()).collect();
    m4a_write::mux_aac_lc(
        &owned,
        fs,
        channels,
        sample_rate,
        valid_samples,
        FRAME as u64,
    )
}
