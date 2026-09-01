//! PCM in ISO-BMFF. `sowt` / `twos` / `ipcm` / `lpcm` / `raw ` → f32 frames.

use super::boxes::{be_u16, be_u32, find, find_all, fourcc};
use super::table::sample_locs;
use crate::error::{SyomError, media};
use crate::resample::to_pcm16_mono_16k;

pub(crate) fn is_pcm(fcc: [u8; 4]) -> bool {
    matches!(&fcc, b"sowt" | b"twos" | b"ipcm" | b"lpcm" | b"raw ")
}

pub(crate) fn sound_format(mp4: &[u8]) -> Result<[u8; 4], SyomError> {
    sample_format(sound_trak(mp4)?)
}

pub(crate) fn to_pcm16(mp4: &[u8]) -> Result<Vec<u8>, SyomError> {
    let trak = sound_trak(mp4)?;
    let fcc = sample_format(trak)?;
    if !is_pcm(fcc) {
        return Err(format_err(fcc));
    }
    let raw = sample_bytes(trak, mp4)?;
    let (ch, bits, rate) = audio_params(trak)?;
    let le = fcc != *b"twos";
    let unsigned = bits == 8;
    let mut samples = pcm_to_f32(&raw, ch, bits, le, unsigned)?;
    let skip = skip_samples(trak, rate)?.saturating_mul(usize::from(ch));
    if skip > samples.len() {
        return Err(media("elst past end of pcm"));
    }
    samples.drain(..skip);
    to_pcm16_mono_16k(&samples, usize::from(ch), rate)
}

pub(crate) fn format_err(fcc: [u8; 4]) -> SyomError {
    media(format!("sample format {}", String::from_utf8_lossy(&fcc)))
}

fn sound_trak(mp4: &[u8]) -> Result<&[u8], SyomError> {
    let moov = find(mp4, *b"moov")?.ok_or_else(|| media("moov"))?;
    for trak in find_all(moov, *b"trak")? {
        if handler_of(trak)? == *b"soun" {
            return Ok(trak);
        }
    }
    Err(media("no sound track"))
}

/// `elst.media_time` in native samples, or 0 if the box is missing / empty.
fn skip_samples(trak: &[u8], sample_rate: u32) -> Result<usize, SyomError> {
    let Some(elst) = find(trak, *b"elst")? else {
        return Ok(0);
    };
    let Some(mdhd) = find(trak, *b"mdhd")? else {
        return Ok(0);
    };
    let version = *elst.first().ok_or_else(|| media("elst"))?;
    let n = be_u32(elst, 4)? as usize;
    let mut pos = 8usize;
    let mut start = 0i64;
    for _ in 0..n {
        let (mt, next) = if version == 1 {
            let end = pos.checked_add(16).ok_or_else(|| media("elst"))?;
            let slice = elst.get(pos + 8..end).ok_or_else(|| media("elst"))?;
            let bytes: [u8; 8] = slice.try_into().map_err(|_| media("elst"))?;
            (i64::from_be_bytes(bytes), pos.saturating_add(20))
        } else {
            (
                i64::from(be_u32(elst, pos.saturating_add(4))? as i32),
                pos.saturating_add(12),
            )
        };
        if mt >= 0 {
            start = mt;
            break;
        }
        pos = next;
    }
    if start <= 0 || sample_rate == 0 {
        return Ok(0);
    }
    let mdhd_ver = *mdhd.first().ok_or_else(|| media("mdhd"))?;
    let ts_off = if mdhd_ver == 1 { 20 } else { 12 };
    let timescale = be_u32(mdhd, ts_off)?;
    if timescale == 0 {
        return Ok(0);
    }
    let n = u64::try_from(start)
        .map_err(|_| media("elst"))?
        .saturating_mul(u64::from(sample_rate))
        / u64::from(timescale);
    usize::try_from(n).map_err(|_| media("elst"))
}

fn handler_of(trak: &[u8]) -> Result<[u8; 4], SyomError> {
    let hdlr = find(trak, *b"hdlr")?.ok_or_else(|| media("hdlr"))?;
    fourcc(hdlr, 8)
}

fn sample_format(trak: &[u8]) -> Result<[u8; 4], SyomError> {
    let stsd = find(trak, *b"stsd")?.ok_or_else(|| media("stsd"))?;
    fourcc(stsd, 12)
}

fn audio_params(trak: &[u8]) -> Result<(u16, u16, u32), SyomError> {
    let stsd = find(trak, *b"stsd")?.ok_or_else(|| media("stsd"))?;
    let ch = be_u16(stsd, 32)?;
    let bits = be_u16(stsd, 34)?;
    let rate = be_u32(stsd, 40)? >> 16;
    if ch == 0 || bits == 0 || rate == 0 {
        return Err(media("zero audio params"));
    }
    Ok((ch, bits, rate))
}

fn sample_bytes(trak: &[u8], file: &[u8]) -> Result<Vec<u8>, SyomError> {
    let sizes = stsz(trak)?;
    let locs = sample_locs(&sizes, &stsc(trak)?, &chunk_offs(trak)?)?;
    let mut out = Vec::new();
    for (off, size) in locs {
        let start = usize::try_from(off).map_err(|_| media("sample"))?;
        let end = start
            .checked_add(usize::try_from(size).map_err(|_| media("sample"))?)
            .ok_or_else(|| media("sample"))?;
        out.extend_from_slice(file.get(start..end).ok_or_else(|| media("sample"))?);
    }
    Ok(out)
}

fn stsc(trak: &[u8]) -> Result<Vec<(u32, u32)>, SyomError> {
    let raw = find(trak, *b"stsc")?.ok_or_else(|| media("stsc"))?;
    let count = be_u32(raw, 4)? as usize;
    let mut out = Vec::new();
    let mut i = 8usize;
    for _ in 0..count {
        let first = be_u32(raw, i)?;
        let n = be_u32(raw, i.checked_add(4).ok_or_else(|| media("stsc"))?)?;
        out.push((first, n));
        i = i.checked_add(12).ok_or_else(|| media("stsc"))?;
    }
    Ok(out)
}

fn chunk_offs(trak: &[u8]) -> Result<Vec<u64>, SyomError> {
    if let Some(raw) = find(trak, *b"stco")? {
        return stco_u32(raw);
    }
    if let Some(raw) = find(trak, *b"co64")? {
        return co64(raw);
    }
    Err(media("stco"))
}

fn stco_u32(raw: &[u8]) -> Result<Vec<u64>, SyomError> {
    let count = be_u32(raw, 4)? as usize;
    let mut offs = Vec::new();
    let mut i = 8usize;
    for _ in 0..count {
        offs.push(u64::from(be_u32(raw, i)?));
        i = i.checked_add(4).ok_or_else(|| media("stco"))?;
    }
    Ok(offs)
}

fn co64(raw: &[u8]) -> Result<Vec<u64>, SyomError> {
    let count = be_u32(raw, 4)? as usize;
    let mut offs = Vec::new();
    let mut i = 8usize;
    for _ in 0..count {
        let end = i.checked_add(8).ok_or_else(|| media("co64"))?;
        let slice = raw.get(i..end).ok_or_else(|| media("co64"))?;
        let bytes: [u8; 8] = slice.try_into().map_err(|_| media("co64"))?;
        offs.push(u64::from_be_bytes(bytes));
        i = end;
    }
    Ok(offs)
}

fn stsz(trak: &[u8]) -> Result<Vec<u32>, SyomError> {
    let raw = find(trak, *b"stsz")?.ok_or_else(|| media("stsz"))?;
    let default = be_u32(raw, 4)?;
    let count = be_u32(raw, 8)? as usize;
    if default != 0 {
        return Ok(vec![default; count.max(1)]);
    }
    let mut sizes = Vec::new();
    let mut i = 12usize;
    for _ in 0..count {
        sizes.push(be_u32(raw, i)?);
        i = i.checked_add(4).ok_or_else(|| media("stsz"))?;
    }
    Ok(sizes)
}

fn pcm_to_f32(
    data: &[u8],
    channels: u16,
    bits: u16,
    le: bool,
    unsigned: bool,
) -> Result<Vec<f32>, SyomError> {
    let ch = usize::from(channels);
    let width = usize::from(bits) / 8;
    if width != 1 && width != 2 {
        return Err(media("pcm bits"));
    }
    let frame = ch.saturating_mul(width);
    if frame == 0 {
        return Err(media("pcm frame"));
    }
    let n = data.len() / frame;
    let mut out = Vec::with_capacity(n.saturating_mul(ch));
    for i in 0..n {
        for c in 0..ch {
            let off = i
                .saturating_mul(frame)
                .saturating_add(c.saturating_mul(width));
            let s = data
                .get(off..off.saturating_add(width))
                .ok_or_else(|| media("pcm"))?;
            out.push(sample_f32(s, le, unsigned)?);
        }
    }
    Ok(out)
}

fn sample_f32(s: &[u8], le: bool, unsigned: bool) -> Result<f32, SyomError> {
    match s {
        [b] if unsigned => Ok((f32::from(*b) - 128.0) / 128.0),
        [b] => Ok(f32::from(i8::from_le_bytes([*b])) / 128.0),
        [a, b] if le => Ok(f32::from(i16::from_le_bytes([*a, *b])) / 32768.0),
        [a, b] => Ok(f32::from(i16::from_be_bytes([*a, *b])) / 32768.0),
        _ => Err(media("pcm sample")),
    }
}
