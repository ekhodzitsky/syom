//! Minimal ISOBMFF writer for AAC-LC: `ftyp` + `mdat` + `moov` (after
//! `mdat`, so `stco` needs no patching). One sound track, one chunk,
//! `stts` (n, 1024), per-frame `stsz`, absolute `stco`.
//!
//! Timeline (Apple QA1636): movie timescale = sample rate so counts are
//! exact. `elst.media_time` = priming; `elst.segment_duration` /
//! `mvhd`/`tkhd` duration = valid source samples; `mdhd` duration =
//! coded `n_frames * 1024`. Remainder is the unplayed media tail
//! `coded − priming − valid`. Decode still skips only `media_time`
//! (TASK-43 trims the tail).
//!
//! The reader side ([`crate::isomp4`]) is the structural oracle: its
//! `parse_esds` walk defines exactly what this writer emits.

use crate::engine::asc::write_lc;
use crate::error::{AacError, Result};

/// AAC-LC `objectTypeIndication`.
const OBJECT_TYPE_AAC: u8 = 0x40;
/// streamType: audio (5) << 2 | upstream flag 0 | reserved 1.
const STREAM_TYPE_AUDIO: u8 = 0x15;

fn u32_field(v: u64, what: &str) -> Result<u32> {
    u32::try_from(v).map_err(|_| AacError::encode(format!("m4a: {what} exceeds u32")))
}

fn be32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn be16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn be64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_be_bytes());
}

/// Start a box; returns the position of its size field for [`end_box`].
fn start_box(out: &mut Vec<u8>, typ: &[u8; 4]) -> usize {
    let at = out.len();
    be32(out, 0);
    out.extend_from_slice(typ);
    at
}

fn end_box(out: &mut [u8], at: usize) {
    let size = (out.len() - at) as u32;
    out[at..at + 4].copy_from_slice(&size.to_be_bytes());
}

/// `ftyp M4A ` + `mdat` + `moov` for LC payloads (one `raw_data_block` per
/// sample entry). `valid_samples` is source length N; `priming` is encoder
/// delay in media units (1024 for LC). Remainder is implied:
/// `n_frames*1024 − priming − valid_samples`.
pub fn mux_aac_lc(
    payloads: &[Vec<u8>],
    fs_index: u8,
    channels: usize,
    sample_rate: u32,
    valid_samples: u64,
    priming: u64,
) -> Result<Vec<u8>> {
    if payloads.is_empty() {
        return Err(AacError::encode("m4a: no access units"));
    }
    if sample_rate == 0 {
        return Err(AacError::encode("m4a: sample rate is 0"));
    }
    let n_frames = u32_field(payloads.len() as u64, "frame count")?;
    let coded = u64::from(n_frames)
        .checked_mul(1024)
        .ok_or_else(|| AacError::encode("m4a: coded duration overflow"))?;
    let play_end = priming
        .checked_add(valid_samples)
        .ok_or_else(|| AacError::encode("m4a: priming + valid overflow"))?;
    if play_end > coded {
        return Err(AacError::encode(
            "m4a: valid samples exceed coded duration after priming",
        ));
    }
    let presentation = u32_field(valid_samples, "presentation duration")?;
    let media_duration = u32_field(coded, "media duration")?;
    let media_time = u32_field(priming, "priming")?;
    let payload_bytes: u64 = payloads.iter().map(|p| p.len() as u64).sum();
    let bitrate = payload_bytes
        .checked_mul(8)
        .and_then(|b| b.checked_mul(u64::from(sample_rate)))
        .and_then(|b| b.checked_div(coded))
        .and_then(|b| u32::try_from(b).ok())
        .unwrap_or(u32::MAX);
    let asc = write_lc(fs_index, channels as u8);

    let mut out = Vec::new();
    // ftyp
    let ftyp = start_box(&mut out, b"ftyp");
    out.extend_from_slice(b"M4A ");
    be32(&mut out, 0); // minor version
    out.extend_from_slice(b"M4A mp42isom");
    end_box(&mut out, ftyp);
    // mdat: payloads contiguous; the single stco entry points here.
    let mdat = start_box(&mut out, b"mdat");
    let media_offset = u32_field(out.len() as u64, "mdat offset")?;
    for p in payloads {
        out.extend_from_slice(p);
    }
    end_box(&mut out, mdat);
    // moov: movie timescale = sample rate so elst duration is N exactly.
    let moov = start_box(&mut out, b"moov");
    write_mvhd(&mut out, sample_rate, presentation);
    let trak = start_box(&mut out, b"trak");
    write_tkhd(&mut out, presentation);
    write_edts(&mut out, presentation, media_time);
    let mdia = start_box(&mut out, b"mdia");
    write_mdhd(&mut out, sample_rate, media_duration);
    write_hdlr(&mut out);
    let minf = start_box(&mut out, b"minf");
    let smhd = start_box(&mut out, b"smhd");
    be32(&mut out, 0); // version/flags
    be16(&mut out, 0); // balance
    be16(&mut out, 0);
    end_box(&mut out, smhd);
    write_dinf(&mut out);
    let stbl = start_box(&mut out, b"stbl");
    write_stsd(&mut out, &asc, channels, sample_rate, bitrate);
    write_stts(&mut out, n_frames);
    write_stsc(&mut out, n_frames);
    write_stsz(&mut out, payloads);
    let stco = start_box(&mut out, b"stco");
    be32(&mut out, 0);
    be32(&mut out, 1);
    be32(&mut out, media_offset);
    end_box(&mut out, stco);
    end_box(&mut out, stbl);
    end_box(&mut out, minf);
    end_box(&mut out, mdia);
    end_box(&mut out, trak);
    end_box(&mut out, moov);
    Ok(out)
}

/// 9 × u32 identity matrix (0x10000 / 0x40000000 fixed point).
fn write_matrix(out: &mut Vec<u8>) {
    for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        be32(out, v);
    }
}

fn write_mvhd(out: &mut Vec<u8>, timescale: u32, duration: u32) {
    let b = start_box(out, b"mvhd");
    be32(out, 0); // version 0 + flags
    be32(out, 0); // creation_time
    be32(out, 0); // modification_time
    be32(out, timescale);
    be32(out, duration);
    be32(out, 0x0001_0000); // rate 1.0
    be16(out, 0x0100); // volume 1.0
    be16(out, 0);
    be64(out, 0);
    write_matrix(out);
    for _ in 0..6 {
        be32(out, 0); // pre_defined
    }
    be32(out, 2); // next_track_id
    end_box(out, b);
}

fn write_tkhd(out: &mut Vec<u8>, duration: u32) {
    let b = start_box(out, b"tkhd");
    be32(out, 0x0000_0003); // version 0, flags: enabled | in_movie
    be32(out, 0); // creation_time
    be32(out, 0); // modification_time
    be32(out, 1); // track_id
    be32(out, 0);
    be32(out, duration);
    be64(out, 0);
    be16(out, 0); // layer
    be16(out, 0); // alternate_group
    be16(out, 0x0100); // volume 1.0
    be16(out, 0);
    write_matrix(out);
    be32(out, 0); // width
    be32(out, 0); // height
    end_box(out, b);
}

/// `edts`/`elst` v0: play `segment_duration` movie units from `media_time`.
fn write_edts(out: &mut Vec<u8>, segment_duration: u32, media_time: u32) {
    let edts = start_box(out, b"edts");
    let elst = start_box(out, b"elst");
    be32(out, 0); // version 0 + flags
    be32(out, 1); // entry_count
    be32(out, segment_duration);
    be32(out, media_time);
    be16(out, 1); // media_rate_integer
    be16(out, 0); // media_rate_fraction
    end_box(out, elst);
    end_box(out, edts);
}

fn write_mdhd(out: &mut Vec<u8>, sample_rate: u32, duration: u32) {
    let b = start_box(out, b"mdhd");
    be32(out, 0); // version 0 + flags
    be32(out, 0); // creation_time
    be32(out, 0); // modification_time
    be32(out, sample_rate); // timescale
    be32(out, duration);
    be16(out, 0x55C4); // language: und
    be16(out, 0);
    end_box(out, b);
}

fn write_hdlr(out: &mut Vec<u8>) {
    let b = start_box(out, b"hdlr");
    be32(out, 0); // version/flags
    be32(out, 0); // pre_defined
    out.extend_from_slice(b"soun");
    be32(out, 0);
    be32(out, 0);
    be32(out, 0); // reserved
    out.push(0); // empty name
    end_box(out, b);
}

/// `dinf` with a self-contained `url ` data reference.
fn write_dinf(out: &mut Vec<u8>) {
    let dinf = start_box(out, b"dinf");
    let dref = start_box(out, b"dref");
    be32(out, 0); // version/flags
    be32(out, 1); // entry_count
    let url = start_box(out, b"url ");
    be32(out, 1); // version 0, flags 1: media data is in this file
    end_box(out, url);
    end_box(out, dref);
    end_box(out, dinf);
}

/// `stsd` with one `mp4a` AudioSampleEntry v0 carrying `esds`.
fn write_stsd(out: &mut Vec<u8>, asc: &[u8], channels: usize, sample_rate: u32, bitrate: u32) {
    let stsd = start_box(out, b"stsd");
    be32(out, 0); // version/flags
    be32(out, 1); // entry_count
    let mp4a = start_box(out, b"mp4a");
    for _ in 0..6 {
        out.push(0); // reserved
    }
    be16(out, 1); // data_reference_index
    be16(out, 0); // version 0
    be16(out, 0); // revision
    be32(out, 0); // vendor
    be16(out, channels as u16);
    be16(out, 16); // sample size
    be16(out, 0); // compression id
    be16(out, 0); // packet size
    be32(out, sample_rate << 16); // 16.16 fixed rate
    write_esds(out, asc, bitrate);
    end_box(out, mp4a);
    end_box(out, stsd);
}

/// `esds`: version/flags, then the descriptor tree ES_Descriptor 0x03 →
/// DecoderConfig 0x04 (objectType 0x40, streamType 0x15) →
/// DecoderSpecificInfo 0x05 = ASC, → SLConfig 0x06 predefined 2. All
/// descriptor bodies are under 128 bytes, so lengths are single bytes.
fn write_esds(out: &mut Vec<u8>, asc: &[u8], bitrate: u32) {
    let esds = start_box(out, b"esds");
    be32(out, 0); // version/flags
    let dc_len = 13 + 2 + asc.len() + 2 + 1;
    let es_len = 3 + 2 + dc_len;
    out.push(0x03);
    out.push(es_len as u8);
    be16(out, 1); // ES_ID
    out.push(0); // flags: no stream dependence / URL / OCR
    out.push(0x04);
    out.push(dc_len as u8);
    out.push(OBJECT_TYPE_AAC);
    out.push(STREAM_TYPE_AUDIO);
    // bufferSizeDB
    out.push(0);
    out.push(0);
    out.push(0);
    be32(out, bitrate); // maxBitrate
    be32(out, bitrate); // avgBitrate
    out.push(0x05);
    out.push(asc.len() as u8);
    out.extend_from_slice(asc);
    out.push(0x06);
    out.push(1);
    out.push(2); // predefined = 2
    end_box(out, esds);
}

/// `stts` v0: every sample is 1024 media units.
fn write_stts(out: &mut Vec<u8>, n_frames: u32) {
    let b = start_box(out, b"stts");
    be32(out, 0);
    be32(out, 1); // entry_count
    be32(out, n_frames);
    be32(out, 1024);
    end_box(out, b);
}

/// `stsc` v0: one chunk holding all samples.
fn write_stsc(out: &mut Vec<u8>, n_frames: u32) {
    let b = start_box(out, b"stsc");
    be32(out, 0);
    be32(out, 1); // entry_count
    be32(out, 1); // first_chunk (1-based)
    be32(out, n_frames); // samples_per_chunk
    be32(out, 1); // sample_description_index
    end_box(out, b);
}

/// `stsz` v0 with per-frame sizes.
fn write_stsz(out: &mut Vec<u8>, payloads: &[Vec<u8>]) {
    let b = start_box(out, b"stsz");
    be32(out, 0);
    be32(out, 0); // sample_size 0 ⇒ per-entry table
    be32(out, payloads.len() as u32);
    for p in payloads {
        be32(out, p.len() as u32);
    }
    end_box(out, b);
}

#[cfg(test)]
#[path = "m4a_write_tests.rs"]
mod m4a_write_tests;
