//! PCM WAV ↔ 16 kHz mono s16le. Compressed fourccs are `Media`.

use crate::error::{SyomError, media};
use crate::resample::SAMPLE_RATE;
use std::path::Path;

pub fn encode_16k_mono_s16(pcm: &[u8]) -> Vec<u8> {
    let n = pcm.len() as u32;
    let byte_rate = SAMPLE_RATE * 2;
    let mut o = Vec::with_capacity(44usize.saturating_add(pcm.len()));
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(36 + n).to_le_bytes());
    o.extend_from_slice(b"WAVE");
    o.extend_from_slice(b"fmt ");
    o.extend_from_slice(&16u32.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    o.extend_from_slice(&byte_rate.to_le_bytes());
    o.extend_from_slice(&2u16.to_le_bytes());
    o.extend_from_slice(&16u16.to_le_bytes());
    o.extend_from_slice(b"data");
    o.extend_from_slice(&n.to_le_bytes());
    o.extend_from_slice(pcm);
    o
}

pub fn write_16k_mono_s16(path: &Path, pcm: &[u8]) -> Result<(), SyomError> {
    std::fs::write(path, encode_16k_mono_s16(pcm))?;
    Ok(())
}

pub fn write_raw_pcm(path: &Path, pcm: &[u8]) -> Result<(), SyomError> {
    std::fs::write(path, pcm)?;
    Ok(())
}

pub fn sniff_wav(bytes: &[u8]) -> bool {
    bytes.len() >= 12
        && bytes.get(..4) == Some(&b"RIFF"[..])
        && bytes.get(8..12) == Some(&b"WAVE"[..])
}

pub fn decode_wav(bytes: &[u8]) -> Result<Vec<u8>, SyomError> {
    if !sniff_wav(bytes) {
        return Err(media("not a RIFF/WAVE file"));
    }
    let mut off = 12usize;
    let mut fmt: Option<Fmt> = None;
    let mut data: Option<&[u8]> = None;
    while off + 8 <= bytes.len() {
        let id = bytes
            .get(off..off + 4)
            .ok_or_else(|| media("truncated chunk"))?;
        let sz = u32_le(bytes, off + 4)?;
        let start = off + 8;
        let end = start
            .checked_add(sz as usize)
            .ok_or_else(|| media("chunk overflow"))?;
        if end > bytes.len() {
            return Err(media("truncated chunk body"));
        }
        match id {
            b"fmt " => {
                fmt = Some(parse_fmt(
                    bytes.get(start..end).ok_or_else(|| media("fmt"))?,
                )?)
            }
            b"data" => data = bytes.get(start..end),
            _ => {}
        }
        off = end + (sz as usize % 2);
    }
    let fmt = fmt.ok_or_else(|| media("missing fmt"))?;
    let data = data.ok_or_else(|| media("missing data"))?;
    if fmt.format != 1 {
        return Err(media("only PCM WAV (format 1)"));
    }
    if fmt.bits != 16 {
        return Err(media("only 16-bit PCM"));
    }
    if fmt.channels == 0 {
        return Err(media("zero channels"));
    }
    let samples = pcm16_samples(data, fmt.channels)?;
    let f32s: Vec<f32> = samples.iter().map(|s| f32::from(*s) / 32768.0).collect();
    crate::resample::to_pcm16_mono_16k(&f32s, 1, fmt.rate)
}

struct Fmt {
    format: u16,
    channels: u16,
    rate: u32,
    bits: u16,
}

fn parse_fmt(b: &[u8]) -> Result<Fmt, SyomError> {
    if b.len() < 16 {
        return Err(media("fmt too short"));
    }
    Ok(Fmt {
        format: u16_le(b, 0)?,
        channels: u16_le(b, 2)?,
        rate: u32_le(b, 4)?,
        bits: u16_le(b, 14)?,
    })
}

fn pcm16_samples(data: &[u8], channels: u16) -> Result<Vec<i16>, SyomError> {
    let ch = channels as usize;
    let frame = ch.saturating_mul(2);
    if frame == 0 {
        return Err(media("bad frame"));
    }
    let n = data.len() / frame;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let mut acc = 0i32;
        for c in 0..ch {
            let off = i.saturating_mul(frame).saturating_add(c.saturating_mul(2));
            let lo = data.get(off).copied().ok_or_else(|| media("short pcm"))?;
            let hi = data
                .get(off + 1)
                .copied()
                .ok_or_else(|| media("short pcm"))?;
            acc += i32::from(i16::from_le_bytes([lo, hi]));
        }
        out.push((acc / ch as i32) as i16);
    }
    Ok(out)
}

fn u16_le(b: &[u8], off: usize) -> Result<u16, SyomError> {
    let s = b.get(off..off + 2).ok_or_else(|| media("short u16"))?;
    let a = s.first().copied().ok_or_else(|| media("short u16"))?;
    let c = s.get(1).copied().ok_or_else(|| media("short u16"))?;
    Ok(u16::from_le_bytes([a, c]))
}

fn u32_le(b: &[u8], off: usize) -> Result<u32, SyomError> {
    let s = b.get(off..off + 4).ok_or_else(|| media("short u32"))?;
    let a = s.first().copied().ok_or_else(|| media("short u32"))?;
    let b1 = s.get(1).copied().ok_or_else(|| media("short u32"))?;
    let c = s.get(2).copied().ok_or_else(|| media("short u32"))?;
    let d = s.get(3).copied().ok_or_else(|| media("short u32"))?;
    Ok(u32::from_le_bytes([a, b1, c, d]))
}

#[cfg(test)]
#[path = "wav_tests.rs"]
mod wav_tests;
