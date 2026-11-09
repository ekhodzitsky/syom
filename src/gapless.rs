//! Apple/ffmpeg `iTunSMPB` as an ID3v2.4 `TXXX` frame.
//!
//! ADTS has no edit list. A one-shot ADTS encode prepends one tag whose
//! priming, remainder and source length are the encoder tallies (HE
//! priming is 3018 output-rate samples). One-shot decode drops those
//! edges only when the decoded sample count equals the tag sum, and
//! leaves a stale tag untouched. A push feed applies a tag whose edges
//! are each at most [`MAX_PUSH_GAP`] as frames arrive, and ignores a
//! larger edge. `finish` reports the edges only when the count matches.

use crate::budgets::{BudgetExceeded, BudgetKind};
use crate::engine::adts::AdtsHeader;
use crate::engine::enc_he::HE_PRIMING_OUT;
use crate::engine::enc_sbr_prep::{CORE_FRAME, FIR_DELAY, OUT_FRAME};
use crate::error::{AacError, Result};

/// Push decode applies a tag immediately when both edges fit in this many
/// samples (LC 1024, HE 3018, the common Apple delay 2112). A larger edge
/// is ignored on a push feed so the resident buffer stays one access unit.
/// One-shot decode still checks the tag against the decoded length.
pub(crate) const MAX_PUSH_GAP: u64 = 4096;

#[cfg(test)]
mod audit_tests;
#[cfg(test)]
#[path = "gapless/fixtures_tests.rs"]
pub(crate) mod test_support;

#[cfg(test)]
mod text_tests;

/// Encoder delay record, in decoded samples at the output rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Delay {
    pub priming: u64,
    pub remainder: u64,
    pub source: u64,
}

impl Delay {
    pub(crate) fn coded_samples(self) -> Option<u64> {
        self.priming
            .checked_add(self.source)?
            .checked_add(self.remainder)
    }

    /// Exact arithmetic: an overflowing record can never validate a window.
    pub(crate) fn matches(self, coded: u64) -> bool {
        coded > 0 && self.coded_samples() == Some(coded)
    }

    pub(crate) fn matches_frames(self, frames: u64, samples_per_frame: u64) -> bool {
        frames
            .checked_mul(samples_per_frame)
            .is_some_and(|n| self.matches(n))
    }
}

mod text;

mod tags;
pub(crate) use crate::isomp4::itunsmpb_in;
pub(crate) use tags::{Tag, Tags};
pub(crate) use text::{itunsmpb_value, parse_text};

/// What a leading ID3 tag is, without copying its body.
pub(crate) enum Id3At {
    /// Not an ID3 tag (or not one we can treat as one).
    Absent,
    /// `ID3` started but the header or the declared body is short.
    Need,
    /// Full tag. `delay` is set only for an `iTunSMPB` text we could read.
    Ready { len: usize, delay: Option<Delay> },
}

/// LC: one 1024-sample block of priming, tail pad inside the last content
/// frame. Matches [`crate::EncodeInfo`] for LC and surround.
pub(crate) fn delay_lc(samples: u64) -> Delay {
    let block = 1024u64;
    let remainder = (block - (samples % block)) % block;
    Delay {
        priming: block,
        remainder,
        source: samples,
    }
}

/// HE v1/v2 output-rate delay. Access-unit count follows `HeEncoder::finish`:
/// content frames, then at least one silent drain, until coded samples
/// cover priming plus the source. Lookahead still emits one AU per core frame.
pub(crate) fn delay_he(samples: u64) -> Delay {
    let priming = HE_PRIMING_OUT;
    let core = (samples + FIR_DELAY as u64).div_ceil(2);
    let content = core.div_ceil(CORE_FRAME as u64);
    let need = priming.saturating_add(samples);
    let out = OUT_FRAME as u64;
    let mut frames = content;
    loop {
        frames = frames.saturating_add(1);
        if frames.saturating_mul(out) >= need {
            break;
        }
    }
    let coded = frames.saturating_mul(out);
    Delay {
        priming,
        remainder: coded.saturating_sub(need),
        source: samples,
    }
}

/// ID3 bytes for `samples` of LC (`he == false`) or HE output-rate PCM.
pub(crate) fn tag_bytes(samples: u64, he: bool) -> Vec<u8> {
    build_id3(&if he {
        delay_he(samples)
    } else {
        delay_lc(samples)
    })
}

/// Prepend an LC tag. The ADTS access units themselves are not rewritten.
pub(crate) fn prefix_lc(adts: Vec<u8>, samples: u64) -> Vec<u8> {
    prefix(adts, &delay_lc(samples))
}

/// Prepend a tag built from the encoder's own tallies (HE uses these).
pub(crate) fn prefix_delay(adts: Vec<u8>, priming: u64, remainder: u64, source: u64) -> Vec<u8> {
    prefix(
        adts,
        &Delay {
            priming,
            remainder,
            source,
        },
    )
}

fn prefix(mut adts: Vec<u8>, delay: &Delay) -> Vec<u8> {
    let tag = build_id3(delay);
    let mut out = Vec::with_capacity(tag.len() + adts.len());
    out.extend_from_slice(&tag);
    out.append(&mut adts);
    out
}

/// Drop one valid leading tag. Anything else is returned unchanged.
/// Does not allocate the tag body. Test-only: production decode peels in
/// the stream pump.
#[cfg(test)]
pub(crate) fn strip_id3(data: &[u8]) -> &[u8] {
    match id3_at(data, u64::MAX) {
        Ok(Id3At::Ready { len, .. }) if len <= data.len() => &data[len..],
        _ => data,
    }
}

fn build_id3(delay: &Delay) -> Vec<u8> {
    let value = itunsmpb_value(delay);
    let mut body = Vec::with_capacity(1 + 9 + value.len());
    body.push(3); // UTF-8
    body.extend_from_slice(b"iTunSMPB\0");
    body.extend_from_slice(value.as_bytes());
    let mut tag = Vec::with_capacity(10 + 10 + body.len());
    tag.extend_from_slice(b"ID3");
    tag.push(4);
    tag.push(0);
    tag.push(0);
    let frame = 10 + body.len();
    tag.extend_from_slice(&syncsafe(frame as u32));
    tag.extend_from_slice(b"TXXX");
    tag.extend_from_slice(&syncsafe(body.len() as u32));
    tag.extend_from_slice(&[0, 0]);
    tag.extend_from_slice(&body);
    tag
}

fn syncsafe(n: u32) -> [u8; 4] {
    [
        ((n >> 21) & 0x7f) as u8,
        ((n >> 14) & 0x7f) as u8,
        ((n >> 7) & 0x7f) as u8,
        (n & 0x7f) as u8,
    ]
}

fn unsyncsafe(b: &[u8]) -> Option<u32> {
    if b.len() < 4 || b[..4].iter().any(|x| x & 0x80 != 0) {
        return None;
    }
    Some(
        (u32::from(b[0]) << 21)
            | (u32::from(b[1]) << 14)
            | (u32::from(b[2]) << 7)
            | u32::from(b[3]),
    )
}

/// Parse a leading ID3v2.3/2.4 tag. A declared size above `budget` is a
/// [`AacError::Limit`] and is never allocated.
pub(crate) fn id3_at(data: &[u8], budget: u64) -> Result<Id3At> {
    if data.len() < 3 {
        if !data.is_empty() && data == &b"ID3"[..data.len()] {
            return Ok(Id3At::Need);
        }
        return Ok(Id3At::Absent);
    }
    if &data[..3] != b"ID3" {
        return Ok(Id3At::Absent);
    }
    if data.len() < 10 {
        return Ok(Id3At::Need);
    }
    let major = data[3];
    if major != 3 && major != 4 {
        return Ok(Id3At::Absent);
    }
    let flags = data[5];
    let Some(body) = unsyncsafe(&data[6..10]) else {
        return Ok(Id3At::Absent);
    };
    // Both versions use a syncsafe tag size; frame sizes differ below.
    let mut total = 10u64.saturating_add(u64::from(body));
    if major == 4 && flags & 0x10 != 0 {
        total = total.saturating_add(10);
    }
    if total > budget {
        return Err(AacError::from(BudgetExceeded {
            kind: BudgetKind::Input,
            observed: total,
            max: budget,
        }));
    }
    let Ok(len) = usize::try_from(total) else {
        return Err(AacError::from(BudgetExceeded {
            kind: BudgetKind::Input,
            observed: total,
            max: budget,
        }));
    };
    if (data.len() as u64) < total {
        return Ok(Id3At::Need);
    }
    let delay = if flags & 0xC0 != 0 {
        // Unsynchronisation or an extended header: skip the tag, do not guess.
        None
    } else {
        let end = 10 + body as usize;
        gapless_in_tag(&data[10..end], major)
    };
    Ok(Id3At::Ready { len, delay })
}

fn gapless_in_tag(body: &[u8], major: u8) -> Option<Delay> {
    let mut tags = Tags::default();
    let mut pos = 0usize;
    while pos + 10 <= body.len() {
        let id = &body[pos..pos + 4];
        let bytes = body[pos + 4..pos + 8].try_into().ok()?;
        let size = if major == 3 {
            u32::from_be_bytes(bytes)
        } else {
            unsyncsafe(&bytes)?
        };
        let format_flags = body[pos + 9];
        pos += 10;
        let end = pos.checked_add(usize::try_from(size).ok()?)?;
        let frame = body.get(pos..end)?;
        pos = end;
        if format_flags != 0 {
            continue;
        }
        if (id == b"TXXX" || id == b"COMM")
            && let Some(tag) = text_delay(id, frame)
        {
            tags.add(tag);
        }
    }
    tags.delay()
}

fn text_delay(id: &[u8], frame: &[u8]) -> Option<Tag> {
    let enc = *frame.first()?;
    if enc != 0 && enc != 3 {
        return None;
    }
    let mut p = 1usize;
    if id == b"COMM" {
        p = 4; // encoding + 3-byte language
        if frame.len() < p {
            return None;
        }
    }
    let rel = frame[p..].iter().position(|&b| b == 0)?;
    if &frame[p..p + rel] != b"iTunSMPB" {
        return None;
    }
    Some(Tag {
        delay: parse_text(&frame[p + rel + 1..]),
    })
}

/// Complete ADTS frame count, or [`None`] when a byte falls outside a frame.
pub(crate) fn exact_adts_frames(data: &[u8]) -> Option<u64> {
    let mut pos = 0usize;
    let mut n = 0u64;
    while pos < data.len() {
        let (hdr, _) = AdtsHeader::parse(&data[pos..]).ok()?;
        let len = usize::from(hdr.aac_frame_length);
        if len == 0 || pos.saturating_add(len) > data.len() {
            return None;
        }
        pos += len;
        n += 1;
    }
    Some(n)
}

/// Decoded samples per access unit, including 960/480 cores and SBR rate ratios.
pub(crate) fn output_frame_samples(asc: &crate::engine::asc::AudioSpecificConfig) -> u64 {
    let scaled = u64::from(asc.frame_samples) * u64::from(asc.output_sample_rate);
    let rate = u64::from(asc.sample_rate);
    if rate == 0 || scaled % rate != 0 {
        0
    } else {
        scaled / rate
    }
}
