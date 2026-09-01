//! Minimal ISOBMFF (MP4/M4A) demuxer for the in-tree AAC path.
#![allow(clippy::all)]
//!
//! Scope: locate the first AAC audio track and extract its
//! AudioSpecificConfig plus the byte ranges of every compressed sample, which
//! the AAC decoder then feeds to the AAC decoder. Everything else the container
//! can carry is skipped, never interpreted:
//!
//! * `meta` / `ilst` / `udta` (tags, artwork) — walked past, not parsed;
//! * video / subtitle / hint tracks — ignored (the first usable `soun` track
//!   with an `mp4a` sample entry wins, mirroring symphonia's default-track
//!   pick; broken or non-AAC sound tracks are skipped);
//! * fragmented MP4 (`moof` / `mvex`) — rejected with a clean error;
//! * encrypted content — `enca` sample entries are rejected with a clean
//!   error; CENC-style files hide the `esds` inside a `sinf` wrapper that is
//!   never descended into, so such tracks carry no usable esds and are
//!   skipped like any other unusable track;
//! * non-AAC sample entries (`alac`, `twos`, `sowt`, …) — the track is
//!   skipped; if no usable AAC track remains, a clean error is returned.
//!
//! Boxes parsed: `ftyp` (sniff only), `moov/trak/mdia/minf/stbl`, `hdlr`
//! (handler `soun`), `mdhd` (media timescale), `edts`/`elst` (encoder delay),
//! `stsd` (`mp4a` entry → `esds` → ES_Descriptor(0x03) →
//! DecoderConfigDescriptor(0x04) → DecoderSpecificInfo(0x05) = ASC),
//! `stts` (total sample count for the duration budget), `stsc`, `stsz`,
//! `stco`/`co64` (sample → byte-range mapping).
//!
//! All arithmetic is checked and every offset is bounds-validated against the
//! input before indexing: untrusted input must produce errors, never panics
//! and never out-of-bounds reads.

use crate::error::{AacError, Result};

/// One extracted AAC audio track: the codec configuration and the location of
/// every compressed sample (each sample is one AAC `raw_data_block()`).
pub struct AacTrack {
    /// AudioSpecificConfig bytes (esds DecoderSpecificInfo).
    pub asc: Vec<u8>,
    /// Total samples per channel declared by `stts` (each AAC frame covers
    /// 1024 core samples); used for the duration budget before allocation.
    pub total_samples: u64,
    /// `(offset, len)` of every audio sample in decode order.
    pub frames: Vec<(u64, u32)>,
    /// First non-empty `elst.media_time` (ISO-BMFF media timescale), or 0.
    pub edit_start: u64,
    /// `mdhd` timescale for `edit_start`. 0 if the box is missing.
    pub media_timescale: u32,
}

impl AacTrack {
    /// Native-rate samples to drop after decode (AAC encoder delay).
    #[must_use]
    pub fn skip_samples(&self, sample_rate: u32) -> usize {
        if self.edit_start == 0 || self.media_timescale == 0 || sample_rate == 0 {
            return 0;
        }
        let n = self.edit_start.saturating_mul(u64::from(sample_rate))
            / u64::from(self.media_timescale);
        usize::try_from(n).unwrap_or(usize::MAX)
    }
}

/// 4-byte box type.
type FourCc = [u8; 4];

const BOX_MOOV: FourCc = *b"moov";
const BOX_TRAK: FourCc = *b"trak";
const BOX_MOOF: FourCc = *b"moof";
const BOX_MVEX: FourCc = *b"mvex";

/// A parsed box header: content range within the input.
#[derive(Debug, Clone, Copy)]
struct BoxHdr {
    /// Offset of the box content (after the 8/16-byte header).
    content_start: usize,
    /// End of the box content (exclusive).
    content_end: usize,
}

/// Read the box header at `pos`; returns the header and the box type, or
/// `None` when fewer than 8 bytes remain (trailing padding). Errors on
/// malformed sizes (truncated header, size overflowing the input).
fn read_box(data: &[u8], pos: usize) -> Result<Option<(BoxHdr, FourCc)>> {
    let Some(hdr) = data.get(pos..pos.saturating_add(8)) else {
        return Ok(None);
    };
    let size32 = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]) as u64;
    let typ: FourCc = [hdr[4], hdr[5], hdr[6], hdr[7]];
    let (size, head_len) = match size32 {
        1 => {
            // 64-bit largesize follows the type.
            let ext = data
                .get(pos + 8..pos.saturating_add(16))
                .ok_or_else(|| AacError::format("isomp4: truncated 64-bit box size"))?;
            (u64::from_be_bytes(ext.try_into().expect("8 bytes")), 16u64)
        }
        0 => {
            // "to end of file" box: only legal at top level; treat the rest
            // of the input as the content.
            ((data.len() - pos) as u64, 8u64)
        }
        s => (s, 8u64),
    };
    if size < head_len {
        return Err(AacError::format(format!(
            "isomp4: box size {size} smaller than its header"
        )));
    }
    let end = pos
        .checked_add(size as usize)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| AacError::format("isomp4: box extends past end of input"))?;
    Ok(Some((
        BoxHdr {
            content_start: pos + head_len as usize,
            content_end: end,
        },
        typ,
    )))
}

/// Iterate the child boxes of `data[start..end]`.
struct BoxIter<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
}

impl<'a> BoxIter<'a> {
    fn new(data: &'a [u8], start: usize, end: usize) -> Self {
        BoxIter {
            data,
            pos: start,
            end,
        }
    }
}

impl Iterator for BoxIter<'_> {
    type Item = Result<(BoxHdr, FourCc)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end {
            return None;
        }
        match read_box(&self.data[..self.end], self.pos) {
            Ok(Some((hdr, typ))) => {
                self.pos = hdr.content_end;
                Some(Ok((hdr, typ)))
            }
            Ok(None) => {
                // Trailing slack (e.g. `free` padding to a 4-byte boundary):
                // stop without error.
                self.pos = self.end;
                None
            }
            Err(e) => {
                self.pos = self.end;
                Some(Err(e))
            }
        }
    }
}

/// Variable-length descriptor size (ISO/IEC 14496-1 §8.3.3): up to 4 bytes,
/// 7 bits each, MSB = continuation.
fn read_desc_len(data: &[u8], pos: usize) -> Result<(usize, usize)> {
    let mut len = 0usize;
    for i in 0..4 {
        let &b = data
            .get(pos + i)
            .ok_or_else(|| AacError::format("isomp4: truncated descriptor length"))?;
        len = (len << 7) | usize::from(b & 0x7F);
        if b & 0x80 == 0 {
            return Ok((len, pos + i + 1));
        }
    }
    Err(AacError::format(
        "isomp4: descriptor length exceeds 4 bytes",
    ))
}

/// Extract the DecoderSpecificInfo (AudioSpecificConfig) bytes from an `esds`
/// box body (after the 8-byte box header: 4 bytes version/flags, then the
/// descriptor tree). Only the descriptor *lengths* are walked; descriptor
/// payloads other than tag 0x05 are never interpreted.
fn parse_esds(data: &[u8]) -> Result<Vec<u8>> {
    let pos = 4usize; // skip version/flags
    // ES_Descriptor (0x03).
    if data.get(pos) != Some(&0x03) {
        return Err(AacError::format(
            "isomp4: esds does not start with ES_Descriptor",
        ));
    }
    let (es_len, mut p) = read_desc_len(data, pos + 1)?;
    let es_end = p
        .checked_add(es_len)
        .filter(|&e| e <= data.len())
        .ok_or_else(|| AacError::format("isomp4: ES_Descriptor overruns esds box"))?;
    // ES_ID(2) + flags(1).
    p = p
        .checked_add(3)
        .filter(|&e| e <= es_end)
        .ok_or_else(|| AacError::format("isomp4: truncated ES_Descriptor"))?;
    let flags = data[p - 1];
    if flags & 0x80 != 0 {
        p = p
            .checked_add(2)
            .filter(|&e| e <= es_end)
            .ok_or_else(|| AacError::format("isomp4: truncated streamDependence"))?;
    }
    if flags & 0x40 != 0 {
        let url_len = *data
            .get(p)
            .ok_or_else(|| AacError::format("isomp4: truncated URL flag"))?
            as usize;
        p = p
            .checked_add(1 + url_len)
            .filter(|&e| e <= es_end)
            .ok_or_else(|| AacError::format("isomp4: truncated URL string"))?;
    }
    if flags & 0x20 != 0 {
        p = p
            .checked_add(2)
            .filter(|&e| e <= es_end)
            .ok_or_else(|| AacError::format("isomp4: truncated OCR flag"))?;
    }
    // DecoderConfigDescriptor (0x04).
    if data.get(p) != Some(&0x04) {
        return Err(AacError::format(
            "isomp4: no DecoderConfigDescriptor in esds",
        ));
    }
    let (dc_len, mut p) = read_desc_len(data, p + 1)?;
    let dc_end = p
        .checked_add(dc_len)
        .filter(|&e| e <= es_end)
        .ok_or_else(|| AacError::format("isomp4: DecoderConfigDescriptor overruns"))?;
    // objectTypeIndication(1) + streamType(1) + bufferSizeDB(3) +
    // maxBitrate(4) + avgBitrate(4) = 13 bytes of fixed fields.
    let object_type = *data
        .get(p)
        .ok_or_else(|| AacError::format("isomp4: truncated DecoderConfigDescriptor"))?;
    if object_type != 0x40 {
        return Err(AacError::format(format!(
            "isomp4: unsupported MPEG-4 object type 0x{object_type:02x} (only 0x40 = AAC is handled)"
        )));
    }
    p = p
        .checked_add(13)
        .filter(|&e| e <= dc_end)
        .ok_or_else(|| AacError::format("isomp4: truncated DecoderConfigDescriptor body"))?;
    // Sub-descriptors: DecoderSpecificInfo (0x05) is the ASC.
    while p < dc_end {
        let tag = data[p];
        let (len, body) = read_desc_len(data, p + 1)?;
        let end = body
            .checked_add(len)
            .filter(|&e| e <= dc_end)
            .ok_or_else(|| AacError::format("isomp4: sub-descriptor overruns"))?;
        if tag == 0x05 {
            if len == 0 {
                return Err(AacError::format("isomp4: empty AudioSpecificConfig"));
            }
            return Ok(data[body..end].to_vec());
        }
        p = end;
    }
    Err(AacError::format(
        "isomp4: no DecoderSpecificInfo (AudioSpecificConfig) in esds",
    ))
}

/// Find the `esds` box inside an `mp4a` sample entry and return the ASC.
/// Direct children are scanned first; a `wave` wrapper (QuickTime) is looked
/// through one level deep. Anything else is ignored.
fn extract_asc(data: &[u8], children: BoxHdr) -> Result<Vec<u8>> {
    for child in BoxIter::new(data, children.content_start, children.content_end) {
        let (hdr, typ) = child?;
        match &typ {
            b"esds" => return parse_esds(&data[hdr.content_start..hdr.content_end]),
            b"wave" => {
                for sub in BoxIter::new(data, hdr.content_start, hdr.content_end) {
                    let (shdr, styp) = sub?;
                    if &styp == b"esds" {
                        return parse_esds(&data[shdr.content_start..shdr.content_end]);
                    }
                }
            }
            _ => {}
        }
    }
    Err(AacError::format("isomp4: mp4a entry carries no esds box"))
}

/// Parsed sample-table boxes of one track.
struct SampleTable {
    /// Per-sample byte sizes from `stsz`.
    sizes: Vec<u32>,
    /// `stsc` entries: (first_chunk 1-based, samples_per_chunk).
    stsc: Vec<(u32, u32)>,
    /// Chunk byte offsets from `stco`/`co64`.
    chunk_offsets: Vec<u64>,
    /// Total per-channel samples from `stts`.
    total_samples: u64,
}

/// Parse one `stts` box body: total sample count is Σ entry.sample_count.
fn parse_stts(data: &[u8]) -> Result<u64> {
    let count = read_u32(data, 4)? as usize;
    let mut total = 0u64;
    for i in 0..count {
        let sample_count = read_u32(
            data,
            8 + i
                .checked_mul(8)
                .ok_or_else(|| AacError::format("isomp4: stts overflow"))?,
        )?;
        total = total
            .checked_add(u64::from(sample_count))
            .ok_or_else(|| AacError::format("isomp4: stts total overflow"))?;
    }
    Ok(total)
}

fn read_u32(data: &[u8], pos: usize) -> Result<u32> {
    let b = data
        .get(pos..pos + 4)
        .ok_or_else(|| AacError::format("isomp4: truncated table"))?;
    Ok(u32::from_be_bytes(b.try_into().expect("4 bytes")))
}

fn parse_stsz(data: &[u8], file_len: usize) -> Result<Vec<u32>> {
    let default_size = read_u32(data, 4)?;
    let count = read_u32(data, 8)? as usize;
    if default_size != 0 {
        // No per-entry table follows, so `count` is bounded only by the
        // declaration itself: a crafted header could force a multi-GiB
        // allocation before the duration budget fires. The samples are
        // disjoint byte ranges inside the file, so their total size cannot
        // exceed the whole input length — fence against `file_len`, not the
        // stsz body (always 12 bytes here, which would reject any legit
        // CBR file with more than 12 frames; ffmpeg movenc writes exactly
        // this shape). The byte-range walk in build_frame_index re-checks
        // every sample against the real offsets.
        let total = u64::from(default_size)
            .checked_mul(count as u64)
            .ok_or_else(|| AacError::format("isomp4: stsz overflow"))?;
        if total > file_len as u64 {
            return Err(AacError::format(format!(
                "isomp4: stsz declares {count} samples of {default_size} bytes, more than the input holds"
            )));
        }
        return Ok(vec![default_size; count]);
    }
    let mut sizes = Vec::with_capacity(count.min(1 << 20));
    for i in 0..count {
        sizes.push(read_u32(
            data,
            12 + i
                .checked_mul(4)
                .ok_or_else(|| AacError::format("isomp4: stsz overflow"))?,
        )?);
    }
    Ok(sizes)
}

fn parse_stsc(data: &[u8]) -> Result<Vec<(u32, u32)>> {
    let count = read_u32(data, 4)? as usize;
    let mut out = Vec::with_capacity(count.min(1 << 20));
    for i in 0..count {
        let base = 8 + i
            .checked_mul(12)
            .ok_or_else(|| AacError::format("isomp4: stsc overflow"))?;
        let first_chunk = read_u32(data, base)?;
        let samples_per_chunk = read_u32(data, base + 4)?;
        // sample_description_index (base + 8) is read but only the first
        // entry's mp4a is ever used; a mid-file switch is rejected later by
        // the byte-range walk producing garbage — ffmpeg never emits it.
        out.push((first_chunk, samples_per_chunk));
    }
    if out.is_empty() {
        return Err(AacError::format("isomp4: empty stsc"));
    }
    Ok(out)
}

fn parse_stco(data: &[u8], wide: bool) -> Result<Vec<u64>> {
    let count = read_u32(data, 4)? as usize;
    let mut out = Vec::with_capacity(count.min(1 << 20));
    for i in 0..count {
        let off = if wide {
            let base = 8 + i
                .checked_mul(8)
                .ok_or_else(|| AacError::format("isomp4: co64 overflow"))?;
            let hi = read_u32(data, base)? as u64;
            let lo = read_u32(data, base + 4)? as u64;
            (hi << 32) | lo
        } else {
            read_u32(
                data,
                8 + i
                    .checked_mul(4)
                    .ok_or_else(|| AacError::format("isomp4: stco overflow"))?,
            )? as u64
        };
        out.push(off);
    }
    Ok(out)
}

/// Parse the sample-table boxes of one track's `stbl`.
fn parse_stbl(data: &[u8], stbl: BoxHdr) -> Result<(Vec<u8>, SampleTable)> {
    let mut asc = None;
    let mut stts_total = None;
    let mut sizes = None;
    let mut stsc = None;
    let mut chunk_offsets = None;
    for child in BoxIter::new(data, stbl.content_start, stbl.content_end) {
        let (hdr, typ) = child?;
        let body = &data[hdr.content_start..hdr.content_end];
        match &typ {
            b"stsd" => {
                // version/flags(4) + entry_count(4), then sample entries as boxes.
                let entries = read_u32(body, 4)? as usize;
                if entries == 0 {
                    return Err(AacError::format("isomp4: stsd with no sample entries"));
                }
                let (ehdr, etyp) = read_box(body, 8)?
                    .ok_or_else(|| AacError::format("isomp4: truncated stsd entry"))?;
                match &etyp {
                    b"mp4a" => {}
                    b"enca" => {
                        return Err(AacError::format(
                            "isomp4: encrypted (DRM) audio is not supported",
                        ));
                    }
                    other => {
                        return Err(AacError::format(format!(
                            "isomp4: unsupported audio sample entry '{}'",
                            String::from_utf8_lossy(other)
                        )));
                    }
                }
                // AudioSampleEntry fixed fields: 6 reserved + 2 data-ref +
                // 2 version + 2 revision + 4 vendor + 2 channels + 2 sample
                // size + 2 compression id + 2 packet size + 4 rate = 28 bytes
                // past the entry box header; version 1 adds 16, version 2
                // adds 36 more before the child boxes.
                let body_start = ehdr.content_start;
                let version = u16::from_be_bytes([
                    *body
                        .get(body_start + 8)
                        .ok_or_else(|| AacError::format("isomp4: truncated mp4a entry"))?,
                    *body
                        .get(body_start + 9)
                        .ok_or_else(|| AacError::format("isomp4: truncated mp4a entry"))?,
                ]);
                let fixed = match version {
                    0 => 28usize,
                    1 => 28 + 16,
                    2 => 28 + 36,
                    v => {
                        return Err(AacError::format(format!(
                            "isomp4: unsupported sound sample entry version {v}"
                        )));
                    }
                };
                let children = BoxHdr {
                    content_start: body_start
                        .checked_add(fixed)
                        .filter(|&e| e <= ehdr.content_end)
                        .ok_or_else(|| AacError::format("isomp4: truncated mp4a entry"))?,
                    content_end: ehdr.content_end,
                };
                asc = Some(extract_asc(body, children)?);
            }
            b"stts" => stts_total = Some(parse_stts(body)?),
            b"stsz" => sizes = Some(parse_stsz(body, data.len())?),
            b"stsc" => stsc = Some(parse_stsc(body)?),
            b"stco" => chunk_offsets = Some(parse_stco(body, false)?),
            b"co64" => chunk_offsets = Some(parse_stco(body, true)?),
            _ => {}
        }
    }
    let asc =
        asc.ok_or_else(|| AacError::format("isomp4: no AAC sample description (stsd/mp4a)"))?;
    let table = SampleTable {
        sizes: sizes.ok_or_else(|| AacError::format("isomp4: missing stsz"))?,
        stsc: stsc.ok_or_else(|| AacError::format("isomp4: missing stsc"))?,
        chunk_offsets: chunk_offsets
            .ok_or_else(|| AacError::format("isomp4: missing stco/co64"))?,
        total_samples: stts_total.ok_or_else(|| AacError::format("isomp4: missing stts"))?,
    };
    Ok((asc, table))
}

/// First `elst` entry with `media_time >= 0` (media timescale), else 0.
fn parse_elst_start(body: &[u8]) -> Result<u64> {
    let version = *body
        .first()
        .ok_or_else(|| AacError::format("isomp4: truncated elst"))?;
    let count = read_u32(body, 4)? as usize;
    let mut pos = 8usize;
    for _ in 0..count {
        let (media_time, next) = if version == 1 {
            let hi = u64::from(read_u32(body, pos + 8)?);
            let lo = u64::from(read_u32(body, pos + 12)?);
            (
                i64::from_be_bytes(((hi << 32) | lo).to_be_bytes()),
                pos + 20,
            )
        } else if version == 0 {
            (i64::from(read_u32(body, pos + 4)? as i32), pos + 12)
        } else {
            return Err(AacError::format("isomp4: unsupported elst version"));
        };
        if media_time >= 0 {
            return Ok(media_time as u64);
        }
        pos = next;
    }
    Ok(0)
}

fn parse_mdhd_timescale(body: &[u8]) -> Result<u32> {
    let version = *body
        .first()
        .ok_or_else(|| AacError::format("isomp4: truncated mdhd"))?;
    let off = match version {
        0 => 12usize,
        1 => 20usize,
        _ => return Err(AacError::format("isomp4: unsupported mdhd version")),
    };
    read_u32(body, off)
}

/// Walk one `mdia` box: is it sound (`hdlr` = 'soun'), and if so parse its
/// `minf/stbl` and `mdhd` timescale. Returns `None` for non-sound tracks.
fn parse_mdia(data: &[u8], mdia: BoxHdr) -> Result<Option<(Vec<u8>, SampleTable, u32)>> {
    let mut is_sound = false;
    let mut table = None;
    let mut timescale = 0u32;
    for child in BoxIter::new(data, mdia.content_start, mdia.content_end) {
        let (hdr, typ) = child?;
        match &typ {
            b"hdlr" => {
                // version/flags(4) + pre_defined(4) + handler_type(4).
                let handler = data
                    .get(hdr.content_start + 8..hdr.content_start + 12)
                    .ok_or_else(|| AacError::format("isomp4: truncated hdlr"))?;
                is_sound = handler == b"soun";
            }
            b"mdhd" => {
                timescale = parse_mdhd_timescale(&data[hdr.content_start..hdr.content_end])?;
            }
            b"minf" => {
                for sub in BoxIter::new(data, hdr.content_start, hdr.content_end) {
                    let (shdr, styp) = sub?;
                    if &styp == b"stbl" {
                        table = Some(parse_stbl(data, shdr)?);
                    }
                }
            }
            _ => {}
        }
    }
    if is_sound {
        let table = table.ok_or_else(|| AacError::format("isomp4: sound track without stbl"))?;
        Ok(Some((table.0, table.1, timescale)))
    } else {
        Ok(None)
    }
}

/// Map the sample table to absolute byte ranges, one per AAC frame.
fn build_frame_index(data_len: usize, table: &SampleTable) -> Result<Vec<(u64, u32)>> {
    let n_samples = table.sizes.len();
    let mut frames = Vec::with_capacity(n_samples.min(1 << 22));
    let mut sample_idx = 0usize;
    let mut stsc_idx = 0usize;
    for (chunk_i, &chunk_off) in table.chunk_offsets.iter().enumerate() {
        // stsc entries are sorted by first_chunk (1-based); advance while the
        // next entry starts at or before this chunk.
        let chunk_no = chunk_i as u32 + 1;
        while stsc_idx + 1 < table.stsc.len() && table.stsc[stsc_idx + 1].0 <= chunk_no {
            stsc_idx += 1;
        }
        let per_chunk = table.stsc[stsc_idx].1 as usize;
        let mut off = chunk_off;
        for _ in 0..per_chunk {
            let Some(&size) = table.sizes.get(sample_idx) else {
                return Err(AacError::format(
                    "isomp4: stsc/stsz disagree on the sample count",
                ));
            };
            let end = off
                .checked_add(u64::from(size))
                .filter(|&e| e <= data_len as u64)
                .ok_or_else(|| AacError::format("isomp4: sample extends past end of input"))?;
            frames.push((off, size));
            off = end;
            sample_idx += 1;
        }
    }
    if sample_idx != n_samples {
        return Err(AacError::format(format!(
            "isomp4: stsz declares {n_samples} samples but the chunk walk placed {sample_idx}"
        )));
    }
    if frames.is_empty() {
        return Err(AacError::format("isomp4: track has no audio samples"));
    }
    Ok(frames)
}

/// Parse `data` as an ISOBMFF file and extract its first AAC audio track.
///
/// Errors are all clean `AacError` returns; no input can panic or index out
/// of bounds.
pub fn parse_aac_track(data: &[u8]) -> Result<AacTrack> {
    let mut moov = None;
    for top in BoxIter::new(data, 0, data.len()) {
        let (hdr, typ) = top?;
        if typ == BOX_MOOV {
            moov = Some(hdr);
        } else if typ == BOX_MOOF || typ == BOX_MVEX {
            return Err(AacError::format("isomp4: fragmented MP4 is not supported"));
        }
    }
    let moov = moov.ok_or_else(|| AacError::format("isomp4: no moov box"))?;

    // Nested containers contain no moof/mvex at their level; fragmentation
    // also shows up as an `mvex` child of moov.
    let mut track = None;
    for child in BoxIter::new(data, moov.content_start, moov.content_end) {
        let (hdr, typ) = child?;
        if typ == BOX_MVEX {
            return Err(AacError::format("isomp4: fragmented MP4 is not supported"));
        } else if typ == BOX_TRAK {
            if track.is_some() {
                continue;
            }
            let mut edit_start = 0u64;
            let mut mdia_track = None;
            for sub in BoxIter::new(data, hdr.content_start, hdr.content_end) {
                let (shdr, styp) = sub?;
                match &styp {
                    b"edts" => {
                        for ed in BoxIter::new(data, shdr.content_start, shdr.content_end) {
                            let (ehdr, etyp) = ed?;
                            if &etyp == b"elst" {
                                edit_start =
                                    parse_elst_start(&data[ehdr.content_start..ehdr.content_end])?;
                            }
                        }
                    }
                    b"mdia" => {
                        // A broken or non-AAC sound track must not sink the
                        // whole file: skip it and keep looking for a usable
                        // AAC track. Structural box-walk errors stay fatal.
                        if let Ok(Some(t)) = parse_mdia(data, shdr) {
                            mdia_track = Some(t);
                        }
                    }
                    _ => {}
                }
            }
            if let Some((asc, table, timescale)) = mdia_track {
                track = Some((asc, table, timescale, edit_start));
            }
        }
    }
    let (asc, table, media_timescale, edit_start) =
        track.ok_or_else(|| AacError::format("isomp4: no AAC audio track found"))?;
    let frames = build_frame_index(data.len(), &table)?;
    Ok(AacTrack {
        asc,
        total_samples: table.total_samples,
        frames,
        edit_start,
        media_timescale,
    })
}

/// True when `data` opens with a plausible ISOBMFF `ftyp` box.
pub fn sniff_is_m4a(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    if &data[4..8] != b"ftyp" {
        return false;
    }
    let size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    if size < 8 {
        return false;
    }
    if data.len() >= 12 {
        return data[8..12]
            .iter()
            .all(|b| b.is_ascii_graphic() || *b == b' ');
    }
    true
}

#[inline]
pub fn sniff_is_isobmff(data: &[u8]) -> bool {
    sniff_is_m4a(data)
}

#[cfg(test)]
mod tests {
    //! Crafted-structure tests: every malformed/adversarial shape must be a
    //! clean error, never a panic and never an oversized allocation.

    use super::*;

    /// Build a box: size + type + body.
    fn bx(typ: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        v.extend_from_slice(typ);
        v.extend_from_slice(body);
        v
    }

    fn ftyp() -> Vec<u8> {
        bx(b"ftyp", b"M4A \x00\x00\x00\x00M4A mp42")
    }

    /// stbl containing only an stsz with the given default size and count.
    fn stbl_with_stsz(default_size: u32, count: u32) -> Vec<u8> {
        let mut stsz = Vec::new();
        stsz.extend_from_slice(&[0u8; 4]); // version/flags
        stsz.extend_from_slice(&default_size.to_be_bytes());
        stsz.extend_from_slice(&count.to_be_bytes());
        let mut stbl_body = bx(b"stsz", &stsz);
        // parse_stsz is called with the box body directly.
        stbl_body.drain(0..8);
        stbl_body
    }

    #[test]
    fn test_stsz_default_size_count_capped() {
        // A stsz declaring 4G samples at a constant size must not allocate
        // 16 GiB: the total sample bytes are fenced by the file length.
        let body = stbl_with_stsz(1, u32::MAX);
        let err = match parse_stsz(&body, body.len()) {
            Ok(_) => panic!("huge stsz count must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("stsz"));
        // A plausible count passes.
        let body = stbl_with_stsz(1, 4);
        assert_eq!(
            parse_stsz(&body, 64).expect("small count"),
            vec![1, 1, 1, 1]
        );
    }

    #[test]
    fn test_stsz_constant_size_fenced_by_file_len() {
        // Regression: the fence used to measure `count` against the stsz
        // body (always 12 bytes for constant size), rejecting every
        // legitimate CBR file with more than 12 frames.
        let body = stbl_with_stsz(4, 100);
        assert_eq!(parse_stsz(&body, 4096).expect("legit CBR").len(), 100);
        // A crafted count whose samples cannot fit in the file is rejected.
        let body = stbl_with_stsz(4, 2000);
        assert!(parse_stsz(&body, 4096).is_err());
    }

    /// AudioSpecificConfig for AAC-LC, 44100 Hz, stereo.
    const ASC: [u8; 2] = [0x12, 0x10];

    /// Build an `esds` box body (version/flags + descriptor tree) wrapping
    /// the given ASC. All descriptor lengths here fit one length byte.
    fn esds_body(asc: &[u8]) -> Vec<u8> {
        let mut dsi = vec![0x05u8, asc.len() as u8];
        dsi.extend_from_slice(asc);
        let mut dcd = vec![0x04u8, (13 + dsi.len()) as u8];
        dcd.extend_from_slice(&[0x40, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        dcd.extend_from_slice(&dsi);
        let mut esd = vec![0x03u8, (3 + dcd.len()) as u8, 0x00, 0x01, 0x00];
        esd.extend_from_slice(&dcd);
        let mut body = vec![0u8; 4]; // version/flags
        body.extend_from_slice(&esd);
        body
    }

    /// An `mp4a` AudioSampleEntry (version 0) carrying the given child boxes.
    fn mp4a_entry(children: &[u8]) -> Vec<u8> {
        let mut fixed = vec![0u8; 6]; // reserved
        fixed.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
        fixed.extend_from_slice(&0u16.to_be_bytes()); // version 0
        fixed.extend_from_slice(&[0u8; 6]); // revision + vendor
        fixed.extend_from_slice(&2u16.to_be_bytes()); // channelcount
        fixed.extend_from_slice(&16u16.to_be_bytes()); // samplesize
        fixed.extend_from_slice(&[0u8; 4]); // pre_defined + reserved
        fixed.extend_from_slice(&(44100u32 << 16).to_be_bytes()); // samplerate
        let mut entry = fixed;
        entry.extend_from_slice(children);
        bx(b"mp4a", &entry)
    }

    /// A `trak` with a `soun` handler and a complete constant-size sample
    /// table in a single chunk; `entry` is the boxed stsd sample entry.
    fn soun_trak(
        entry: Vec<u8>,
        sample_count: u32,
        sample_size: u32,
        chunk_offset: u32,
    ) -> Vec<u8> {
        let mut stsd_body = vec![0u8; 4]; // version/flags
        stsd_body.extend_from_slice(&1u32.to_be_bytes());
        stsd_body.extend_from_slice(&entry);

        let mut stts = vec![0u8; 4];
        stts.extend_from_slice(&1u32.to_be_bytes());
        stts.extend_from_slice(&sample_count.to_be_bytes());
        stts.extend_from_slice(&1024u32.to_be_bytes());

        let mut stsc = vec![0u8; 4];
        stsc.extend_from_slice(&1u32.to_be_bytes());
        stsc.extend_from_slice(&1u32.to_be_bytes()); // first_chunk
        stsc.extend_from_slice(&sample_count.to_be_bytes()); // samples_per_chunk
        stsc.extend_from_slice(&1u32.to_be_bytes()); // sample_description_index

        let mut stsz = vec![0u8; 4];
        stsz.extend_from_slice(&sample_size.to_be_bytes()); // constant sample size
        stsz.extend_from_slice(&sample_count.to_be_bytes());

        let mut stco = vec![0u8; 4];
        stco.extend_from_slice(&1u32.to_be_bytes());
        stco.extend_from_slice(&chunk_offset.to_be_bytes());

        let mut stbl_body = bx(b"stsd", &stsd_body);
        stbl_body.extend_from_slice(&bx(b"stts", &stts));
        stbl_body.extend_from_slice(&bx(b"stsc", &stsc));
        stbl_body.extend_from_slice(&bx(b"stsz", &stsz));
        stbl_body.extend_from_slice(&bx(b"stco", &stco));

        let mut hdlr = vec![0u8; 8]; // version/flags + pre_defined
        hdlr.extend_from_slice(b"soun");
        hdlr.extend_from_slice(&[0u8; 12]);

        let mut mdhd = vec![0u8; 4];
        mdhd.extend_from_slice(&[0u8; 8]);
        mdhd.extend_from_slice(&48_000u32.to_be_bytes());
        mdhd.extend_from_slice(&0u32.to_be_bytes());
        mdhd.extend_from_slice(&0x55c4_0000u32.to_be_bytes());

        let mut mdia_body = bx(b"mdhd", &mdhd);
        mdia_body.extend_from_slice(&bx(b"hdlr", &hdlr));
        mdia_body.extend_from_slice(&bx(b"minf", &bx(b"stbl", &stbl_body)));
        bx(b"trak", &bx(b"mdia", &mdia_body))
    }

    fn aac_trak_with_elst(
        sample_count: u32,
        sample_size: u32,
        chunk_offset: u32,
        media_time: i32,
    ) -> Vec<u8> {
        let trak = aac_trak(sample_count, sample_size, chunk_offset);
        // Insert edts/elst after the 8-byte trak header, before mdia.
        let mut elst = vec![0u8; 4];
        elst.extend_from_slice(&1u32.to_be_bytes());
        elst.extend_from_slice(&250u32.to_be_bytes());
        elst.extend_from_slice(&(media_time as u32).to_be_bytes());
        elst.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        let edts = bx(b"edts", &bx(b"elst", &elst));
        let mut out = trak[..8].to_vec();
        out.extend_from_slice(&edts);
        out.extend_from_slice(&trak[8..]);
        let size = out.len() as u32;
        out[..4].copy_from_slice(&size.to_be_bytes());
        out
    }

    fn aac_trak(sample_count: u32, sample_size: u32, chunk_offset: u32) -> Vec<u8> {
        soun_trak(
            mp4a_entry(&bx(b"esds", &esds_body(&ASC))),
            sample_count,
            sample_size,
            chunk_offset,
        )
    }

    /// A complete M4A: ftyp + moov(`extra_trak`? + AAC trak) + mdat with
    /// `sample_count * sample_size` payload bytes. The stco offset is patched
    /// in a second pass once the moov length is known.
    fn m4a(sample_count: u32, sample_size: u32, extra_trak: Option<&[u8]>) -> Vec<u8> {
        let build = |off: u32| {
            let mut moov_body = Vec::new();
            if let Some(t) = extra_trak {
                moov_body.extend_from_slice(t);
            }
            moov_body.extend_from_slice(&aac_trak(sample_count, sample_size, off));
            let mut data = ftyp();
            data.extend_from_slice(&bx(b"moov", &moov_body));
            data
        };
        let mdat_off = (build(0).len() + 8) as u32;
        let mut data = build(mdat_off);
        data.extend_from_slice(&bx(
            b"mdat",
            &vec![0xAAu8; (sample_count * sample_size) as usize],
        ));
        data
    }

    #[test]
    fn test_happy_path_full_box_walk() {
        // Always-on (no ffmpeg gate): a programmatically built CBR M4A —
        // stsd→esds→ASC, stts/stsc/stsz/stco → sample offsets — must parse
        // end to end. 100 constant-size samples is also the MAJOR-2
        // regression shape: ffmpeg movenc writes stsz without a table.
        let data = m4a(100, 7, None);
        let track = parse_aac_track(&data).expect("valid constant-size M4A must parse");
        assert_eq!(track.asc, ASC);
        assert_eq!(track.total_samples, 100);
        assert_eq!(track.frames.len(), 100);
        assert_eq!(track.skip_samples(48_000), 0);
        let mdat_payload = (data.len() - 700) as u64;
        for (i, &(off, len)) in track.frames.iter().enumerate() {
            assert_eq!(len, 7);
            assert_eq!(off, mdat_payload + (i * 7) as u64);
        }
    }

    #[test]
    fn test_broken_sound_track_skipped_for_valid_aac() {
        // A non-AAC soun track ahead of a valid AAC track must not sink the
        // file: track selection skips it (symphonia parity).
        let alac = soun_trak(bx(b"alac", &[0u8; 28]), 100, 7, 0);
        let data = m4a(100, 7, Some(&alac));
        let track = parse_aac_track(&data).expect("valid AAC track behind a broken one");
        assert_eq!(track.frames.len(), 100);
    }

    #[test]
    fn test_elst_media_time_is_skip_samples() {
        let build = |off: u32| {
            let mut data = ftyp();
            data.extend_from_slice(&bx(b"moov", &aac_trak_with_elst(4, 7, off, 1024)));
            data
        };
        let mdat_off = (build(0).len() + 8) as u32;
        let mut data = build(mdat_off);
        data.extend_from_slice(&bx(b"mdat", &vec![0xAAu8; 28]));
        let track = parse_aac_track(&data).expect("elst M4A must parse");
        assert_eq!(track.edit_start, 1024);
        assert_eq!(track.media_timescale, 48_000);
        assert_eq!(track.skip_samples(48_000), 1024);
        assert_eq!(track.frames.len(), 4);
    }

    #[test]
    fn test_broken_sound_track_only_is_clean_error() {
        // Only a broken sound track: the same clean error as no audio track.
        let alac = soun_trak(bx(b"alac", &[0u8; 28]), 100, 7, 0);
        let mut data = ftyp();
        data.extend_from_slice(&bx(b"moov", &alac));
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("file with no usable AAC track must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("no AAC audio track"), "{err:?}");
    }

    #[test]
    fn test_sinf_wrapped_esds_is_clean_error() {
        // CENC-style layout: the esds hides inside a sinf wrapper, which is
        // never descended into, so the track is unusable — a clean refusal.
        let sinf = bx(b"sinf", &bx(b"esds", &esds_body(&ASC)));
        let trak = soun_trak(mp4a_entry(&sinf), 4, 7, 0);
        let mut data = ftyp();
        data.extend_from_slice(&bx(b"moov", &trak));
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("sinf-wrapped esds must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("no AAC audio track"), "{err:?}");
    }

    #[test]
    fn test_no_moov_is_clean_error() {
        let mut data = ftyp();
        data.extend_from_slice(&bx(b"mdat", &[0u8; 64]));
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("no moov must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("no moov box"));
    }

    #[test]
    fn test_fragmented_rejected() {
        let mut data = ftyp();
        data.extend_from_slice(&bx(b"moof", &[0u8; 8]));
        data.extend_from_slice(&bx(b"moov", &[]));
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("fragmented must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("fragmented"));
    }

    #[test]
    fn test_truncated_box_is_clean_error() {
        // ftyp declaring a size past the input end.
        let mut data = (0x1000u32.to_be_bytes()).to_vec();
        data.extend_from_slice(b"ftypM4A ");
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("truncated box must fail"),
            Err(e) => e,
        };
        let msg = format!("{err:?}");
        assert!(
            msg.contains("past end") || msg.contains("no moov box"),
            "{msg}"
        );
    }

    #[test]
    fn test_garbage_never_panics() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut rng = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..2000 {
            let len = (rng() % 512) as usize;
            let mut data: Vec<u8> = (0..len).map(|_| (rng() >> 32) as u8).collect();
            // Half the cases get a plausible ftyp prefix so the box walk runs.
            if len >= 20 && rng() % 2 == 0 {
                data[..4].copy_from_slice(&20u32.to_be_bytes());
                data[4..8].copy_from_slice(b"ftyp");
            }
            let _ = parse_aac_track(&data); // Err is fine; a panic is a bug.
        }
    }

    #[test]
    fn test_video_track_only_is_clean_error() {
        // A moov with a vide (not soun) track: hdlr says "vide", so no AAC
        // track exists.
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"vide");
        hdlr.extend_from_slice(&[0u8; 12]);
        let mdia = bx(b"mdia", &bx(b"hdlr", &hdlr));
        let trak = bx(b"trak", &mdia);
        let moov = bx(b"moov", &trak);
        let mut data = ftyp();
        data.extend_from_slice(&moov);
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("video-only must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("no AAC audio track"));
    }

    #[test]
    fn test_drm_sample_entry_rejected() {
        // stsd with an `enca` (encrypted) entry: the DRM track is unusable,
        // so with no other sound track the file fails the same clean way as
        // having no audio track at all.
        let mut stsd = Vec::new();
        stsd.extend_from_slice(&[0u8; 4]); // version/flags
        stsd.extend_from_slice(&1u32.to_be_bytes()); // one entry
        stsd.extend_from_slice(&bx(b"enca", &[0u8; 28]));
        let stbl = bx(b"stbl", &bx(b"stsd", &stsd));
        let minf = bx(b"minf", &stbl);
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(b"soun");
        hdlr.extend_from_slice(&[0u8; 12]);
        let mut mdia_body = bx(b"hdlr", &hdlr);
        mdia_body.extend_from_slice(&minf);
        let mdia = bx(b"mdia", &mdia_body);
        let trak = bx(b"trak", &mdia);
        let moov = bx(b"moov", &trak);
        let mut data = ftyp();
        data.extend_from_slice(&moov);
        let err = match parse_aac_track(&data) {
            Ok(_) => panic!("DRM must fail"),
            Err(e) => e,
        };
        assert!(format!("{err:?}").contains("no AAC audio track"), "{err:?}");
    }
}
