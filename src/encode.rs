//! One-shot encode: planar f32 → AAC-LC (default) or HE-AAC v1
//! ([`EncodeOptions::with_he`]) in ADTS or M4A.
//!
//! The default encoder writes AAC-LC (no SBR/PS), mono or stereo, with block
//! switching (long windows for steady content, short windows on detected
//! attacks). CBR-ish via a per-frame global scalefactor offset search plus
//! a bounded one-frame bit credit. Output is conformant enough that
//! libavcodec decodes it; the lavc-decoded goldens under `src/goldens/`
//! are the committed proof.
//!
//! PCM samples must be finite and in `[-1, 1]`. NaN, infinities, and
//! `|x| > 1` are [`AacError::InvalidPcm`]. There is no silent clip. The
//! push [`crate::Encoder`] uses the same rule.

use crate::engine::adts::{ADTS_SAMPLE_RATES_HZ, AdtsHeader};
use crate::engine::enc_frame::LcEncoder;
use crate::engine::enc_he::HeEncoder;
use crate::engine::swb::LONG_WINDOW_LEN as FRAME;
use crate::error::{AacError, Result};
use crate::m4a_write;
use crate::options::{EncodeContainer, EncodeOptions};

/// Build an LC encoder honoring lookahead, ATH, and tonality from `opts`.
pub(crate) fn new_lc(sample_rate: u32, channels: usize, opts: &EncodeOptions) -> Result<LcEncoder> {
    Ok(LcEncoder::new(sample_rate, channels, opts.bitrate_bps)?
        .with_quality(opts.quality)
        .with_lookahead(opts.lookahead)
        .with_psy(opts.ath, opts.tonality)
        .with_short_tns(opts.short_tns)
        .with_short_group(opts.short_group)
        .with_band_refine(opts.band_refine)
        .with_pns(opts.pns)
        .with_intensity(opts.intensity))
}

/// Encode planar f32 PCM in `[-1, 1]` to an ADTS stream: AAC-LC at 128 kbps.
///
/// Planes are anything `AsRef<[f32]>`: `&[Vec<f32>]`, `&[&[f32]]`, arrays —
/// no copy is made to satisfy the type (TASK-55).
///
/// One plane per channel (mono or stereo), all planes the same length; the
/// last content block is zero-padded to 1024 samples, then one extra zero
/// MDCT drains the overlap so the last source samples reconstruct. The
/// stream carries 1024-sample codec priming. Decoded ADTS length is
/// `(ceil(N/1024)+1)*1024`; valid duration is N after skipping priming.
#[inline]
pub fn encode<P: AsRef<[f32]>>(pcm: &[P], sample_rate: u32) -> Result<Vec<u8>> {
    encode_with(pcm, sample_rate, &EncodeOptions::default())
}

/// Build the HE v1 encoder: the output rate must halve to an ADTS rate and
/// the whole-stream bitrate must fit the core's 6144 bits/channel/frame.
pub(crate) fn new_he(sample_rate: u32, channels: usize, opts: &EncodeOptions) -> Result<HeEncoder> {
    let core = crate::engine::enc_sbr_prep::he_core_rate(sample_rate)
        .map_err(|_| AacError::Unsupported(crate::UnsupportedFeature::EncodeHeRate(sample_rate)))?;
    let max_bps = crate::engine::enc_frame::max_bitrate_bps(core, channels);
    if opts.bitrate_bps > max_bps {
        return Err(AacError::encode(format!(
            "encode: HE bitrate {} bps exceeds 6144 bits/channel at the {core} Hz core (max {max_bps} bps)",
            opts.bitrate_bps
        )));
    }
    Ok(HeEncoder::new(
        sample_rate,
        channels,
        opts.bitrate_bps,
        opts.lookahead,
    )?)
}

/// HE v1 one-shot: every access unit, then ADTS (implicit SBR, core rate
/// in the header) or M4A (explicit two-rate ASC, output-rate timeline).
fn encode_he(planes: &[&[f32]], sample_rate: u32, opts: &EncodeOptions) -> Result<Vec<u8>> {
    let channels = planes.len();
    let mut enc = new_he(sample_rate, channels, opts)?;
    let mut aus: Vec<Vec<u8>> = Vec::new();
    enc.push(planes, |au| {
        aus.push(au.to_vec());
        Ok(())
    })?;
    let info = enc.finish(|au| {
        aus.push(au.to_vec());
        Ok(())
    })?;
    match opts.container {
        EncodeContainer::Adts => Ok(wrap_adts(&aus, enc.fs_index(), channels)),
        EncodeContainer::Latm => {
            let asc = crate::engine::asc::write_he(
                enc.fs_index(),
                fs_index(sample_rate)?,
                channels as u8,
            );
            mux::wrap_latm(&aus, &asc)
        }
        EncodeContainer::M4a => m4a_write::mux_aac_he(
            &aus,
            enc.fs_index(),
            fs_index(sample_rate)?,
            channels,
            sample_rate,
            info.source,
            info.priming_out,
        ),
        EncodeContainer::Raw => Err(AacError::Unsupported(
            crate::UnsupportedFeature::EncodeRawOneShot,
        )),
    }
}

/// Encode planar f32 PCM under `opts` (container + bitrate + lookahead +
/// optional ATH / tonality / short TNS / short grouping / HE v1).
///
/// With [`EncodeOptions::lookahead`] on, the attack detector runs one
/// frame ahead (better pre-echo suppression on early-in-frame onsets) at
/// one extra frame of internal latency; drain still emits one silent
/// successor so the last source samples reconstruct.
pub fn encode_with<P: AsRef<[f32]>>(
    pcm: &[P],
    sample_rate: u32,
    opts: &EncodeOptions,
) -> Result<Vec<u8>> {
    let pcm: Vec<&[f32]> = pcm.iter().map(AsRef::as_ref).collect();
    let pcm = pcm.as_slice();
    validate(pcm, sample_rate, opts)?;
    if opts.he {
        return encode_he(pcm, sample_rate, opts);
    }
    let channels = pcm.len();
    let mut enc = new_lc(sample_rate, channels, opts)?;
    let n_samples = pcm[0].len();
    let n_frames = n_samples.div_ceil(FRAME);
    let mut payloads: Vec<Vec<u8>> = Vec::with_capacity(n_frames);
    let mut bufs = [[0.0f32; FRAME]; 2];
    for f in 0..n_frames {
        let start = f * FRAME;
        let end = (start + FRAME).min(n_samples);
        for (ch, plane) in pcm.iter().enumerate() {
            bufs[ch] = [0.0; FRAME];
            bufs[ch][..end - start].copy_from_slice(&plane[start..end]);
        }
        let planes: Vec<&[f32]> = bufs[..channels].iter().map(|b| &b[..]).collect();
        if opts.lookahead {
            if let Some(payload) = enc.push_frame(&planes)? {
                payloads.push(payload);
            }
        } else {
            payloads.push(enc.encode_frame(&planes)?);
        }
    }
    payloads.extend(enc.drain_overlap()?);
    match opts.container {
        EncodeContainer::Adts => Ok(wrap_adts(&payloads, enc.fs_index(), channels)),
        EncodeContainer::Latm => {
            let asc = crate::engine::asc::write_lc(enc.fs_index(), channels as u8);
            mux::wrap_latm(&payloads, &asc)
        }
        EncodeContainer::M4a => m4a_write::mux_aac_lc(
            &payloads,
            enc.fs_index(),
            channels,
            sample_rate,
            n_samples as u64,
            FRAME as u64,
        ),
        EncodeContainer::Raw => Err(AacError::Unsupported(
            crate::UnsupportedFeature::EncodeRawOneShot,
        )),
    }
}

#[path = "encode_mux.rs"]
mod mux;
pub use mux::{
    encode_write, encode_write_m4a, mux_raw_he_m4a, mux_raw_lc_m4a, wrap_adts_au, wrap_loas_au,
};

fn fs_index(sample_rate: u32) -> Result<u8> {
    ADTS_SAMPLE_RATES_HZ
        .iter()
        .position(|&r| r == sample_rate)
        .map(|i| i as u8)
        .ok_or_else(|| {
            AacError::encode(format!(
                "encode: unsupported sample rate {sample_rate}Hz (not in the AAC table)"
            ))
        })
}

/// Encode to ADTS and write the file.
pub fn write<P: AsRef<[f32]>>(
    path: impl AsRef<std::path::Path>,
    pcm: &[P],
    sample_rate: u32,
) -> Result<()> {
    write_with(path, pcm, sample_rate, &EncodeOptions::default())
}

/// Encode under `opts` and write the file.
pub fn write_with<P: AsRef<[f32]>>(
    path: impl AsRef<std::path::Path>,
    pcm: &[P],
    sample_rate: u32,
    opts: &EncodeOptions,
) -> Result<()> {
    let bytes = encode_with(pcm, sample_rate, opts)?;
    std::fs::write(path, bytes)?;
    Ok(())
}

/// Mode combinations shared by one-shot and push encode: a quality level
/// must be `0..=10` and is LC only.
pub(crate) fn check_mode(opts: &EncodeOptions) -> Result<()> {
    if let Some(q) = opts.quality {
        if q > crate::engine::enc_frame::QUALITY_MAX {
            return Err(AacError::encode(format!(
                "encode: quality level {q} is above 10"
            )));
        }
        if opts.he {
            return Err(AacError::encode(
                "encode: quality VBR is LC only (he must be off)",
            ));
        }
    }
    Ok(())
}

fn validate(pcm: &[&[f32]], sample_rate: u32, opts: &EncodeOptions) -> Result<()> {
    check_mode(opts)?;
    if !ADTS_SAMPLE_RATES_HZ.contains(&sample_rate) {
        return Err(AacError::encode(format!(
            "encode: unsupported sample rate {sample_rate}Hz (not in the AAC table)"
        )));
    }
    if !(1..=2).contains(&pcm.len()) {
        return Err(AacError::encode(format!(
            "encode: channels must be 1 or 2, got {}",
            pcm.len()
        )));
    }
    if opts.bitrate_bps == 0 {
        return Err(AacError::encode("encode: bitrate must be > 0"));
    }
    let max_bps = crate::engine::enc_frame::max_bitrate_bps(sample_rate, pcm.len());
    if opts.bitrate_bps > max_bps {
        return Err(AacError::encode(format!(
            "encode: bitrate {} bps exceeds AAC-LC 6144 bits/channel (max {max_bps} bps)",
            opts.bitrate_bps
        )));
    }
    let n = pcm[0].len();
    if n == 0 {
        return Err(AacError::InvalidPcm(crate::PcmReject::Empty));
    }
    if pcm.iter().any(|p| p.len() != n) {
        return Err(AacError::InvalidPcm(crate::PcmReject::PlaneLength));
    }
    check_pcm_samples(pcm.iter().copied())?;
    Ok(())
}

/// Finite samples in `[-1, 1]`. Shared by one-shot encode and push encode.
pub(crate) fn check_pcm_samples<'a, I>(planes: I) -> Result<()>
where
    I: IntoIterator<Item = &'a [f32]>,
{
    for plane in planes {
        for &x in plane {
            if !x.is_finite() {
                return Err(AacError::InvalidPcm(crate::PcmReject::NonFinite));
            }
            if x.abs() > 1.0 {
                return Err(AacError::InvalidPcm(crate::PcmReject::Amplitude));
            }
        }
    }
    Ok(())
}

/// ADTS-wrap each raw_data_block (no CRC, VBR fullness marker).
fn wrap_adts(payloads: &[Vec<u8>], fs_index: u8, channels: usize) -> Vec<u8> {
    let total: usize = payloads
        .iter()
        .map(|p| p.len() + crate::engine::adts::ADTS_HEADER_BYTES_NO_CRC)
        .sum();
    let mut out = Vec::with_capacity(total);
    for p in payloads {
        adts_frame_into(p, fs_index, channels, &mut out);
    }
    out
}

/// Append one ADTS-wrapped `raw_data_block` to `out` (no CRC, VBR fullness
/// marker). Shared by one-shot [`encode_with`] and the push [`crate::Encoder`].
pub(crate) fn adts_frame_into(payload: &[u8], fs_index: u8, channels: usize, out: &mut Vec<u8>) {
    let hdr = AdtsHeader {
        mpeg_version_mpeg2: false,
        protection_absent: true,
        profile: 1, // LC
        sampling_frequency_index: fs_index,
        channel_configuration: channels as u8,
        aac_frame_length: (crate::engine::adts::ADTS_HEADER_BYTES_NO_CRC + payload.len()) as u16,
        adts_buffer_fullness: 0x7FF,
        number_of_raw_data_blocks_in_frame: 1,
    };
    out.extend_from_slice(&hdr.write());
    out.extend_from_slice(payload);
}
