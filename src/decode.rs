//! One-shot decode: a collecting frame callback over the streaming core
//! (`stream` module owns the container state machines and caps).

use crate::error::Result;
use crate::options::DecodeOptions;
use crate::stream::decode_streaming;

/// Decoded AAC at native sample rate (planar f32, mono-mixed or split).
///
/// Split channel order: mono/stereo as-is; multichannel AAC-LC
/// (`channel_configuration` 3–6) follows the libavcodec layout order
/// (5.1 = FL FR FC LFE BL BR); PCE streams follow PCE declaration order
/// (front, side, back, LFE). Speech mono is the mean of the non-LFE planes.
#[derive(Debug, Clone)]
pub struct DecodedAac {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

/// Decode ADTS, M4A, or LATM/LOAS bytes with speech-ingest defaults.
#[inline]
pub fn decode(data: &[u8]) -> Result<DecodedAac> {
    decode_with(data, &DecodeOptions::speech())
}

/// Alias of [`decode`].
#[inline]
pub fn decode_bytes(data: &[u8]) -> Result<DecodedAac> {
    decode(data)
}

/// Read a file and [`decode`] it.
pub fn read(path: impl AsRef<std::path::Path>) -> Result<DecodedAac> {
    decode(&std::fs::read(path)?)
}

/// Read a file and [`decode_with`] it.
pub fn read_with(path: impl AsRef<std::path::Path>, opts: &DecodeOptions) -> Result<DecodedAac> {
    decode_with(&std::fs::read(path)?, opts)
}

/// Decode ADTS, M4A, or LATM/LOAS bytes under `opts`.
pub fn decode_with(data: &[u8], opts: &DecodeOptions) -> Result<DecodedAac> {
    let mut tracks: Vec<Vec<f32>> = Vec::new();
    let info = decode_streaming(data, opts, |f| {
        if tracks.is_empty() {
            tracks.resize_with(f.planar.len(), Vec::new);
        }
        for (dst, src) in tracks.iter_mut().zip(f.planar.iter()) {
            dst.extend_from_slice(src);
        }
        Ok(())
    })?;
    Ok(DecodedAac {
        sample_rate: info.sample_rate,
        channels: tracks,
    })
}
