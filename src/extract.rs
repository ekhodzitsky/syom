//! Bytes in → 16 kHz s16le mono. WAV, ADTS, M4A/MP4 AAC.

use crate::error::{SyomError, media};
use crate::resample::to_pcm16_mono_16k;
use crate::wav;
use std::path::Path;

pub fn file_to_pcm16(path: &Path) -> Result<Vec<u8>, SyomError> {
    if !path.is_file() {
        return Err(media(format!("not a file: {}", path.display())));
    }
    let bytes = std::fs::read(path)?;
    bytes_to_pcm16(&bytes)
}

pub fn bytes_to_pcm16(bytes: &[u8]) -> Result<Vec<u8>, SyomError> {
    if wav::sniff_wav(bytes) {
        return wav::decode_wav(bytes);
    }
    let decoded = syom_aac::decode(bytes)?;
    let ch = decoded.channels.len();
    if ch == 0 {
        return Err(media("aac produced no channels"));
    }
    let frames = decoded
        .channels
        .first()
        .map(Vec::len)
        .ok_or_else(|| media("aac empty"))?;
    let mut interleaved = Vec::with_capacity(frames.saturating_mul(ch));
    for i in 0..frames {
        for c in 0..ch {
            let track = decoded
                .channels
                .get(c)
                .ok_or_else(|| media("aac channel"))?;
            interleaved.push(*track.get(i).ok_or_else(|| media("aac sample"))?);
        }
    }
    to_pcm16_mono_16k(&interleaved, ch, decoded.sample_rate)
}

pub fn write_out(path: &Path, pcm: &[u8]) -> Result<(), SyomError> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("wav")
        .to_ascii_lowercase();
    match ext.as_str() {
        "pcm" | "raw" | "s16" => wav::write_raw_pcm(path, pcm),
        _ => wav::write_16k_mono_s16(path, pcm),
    }
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod extract_tests;
