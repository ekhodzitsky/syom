//! M4A writer structure: the shipped isomp4 reader is the oracle.
//! Timeline fields are also read from the raw boxes so assertions do not
//! depend only on a self-roundtrip decode.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::mux_aac_lc;
use crate::engine::adts::ADTS_SAMPLE_RATES_HZ;
use crate::engine::asc::AudioSpecificConfig;
use crate::error::Result;
use crate::{DecodeOptions, EncodeOptions, decode_with, encode_with};

/// Three fake payloads; the muxer treats them as opaque sample bytes.
fn payloads() -> Vec<Vec<u8>> {
    vec![vec![0xAA; 100], vec![0xBB; 137], vec![0xCC; 90]]
}

/// Nested box walk: first body whose type matches `want`.
fn box_body<'a>(data: &'a [u8], want: &[u8; 4]) -> Option<&'a [u8]> {
    fn walk<'a>(data: &'a [u8], start: usize, end: usize, want: &[u8; 4]) -> Option<&'a [u8]> {
        let mut pos = start;
        while pos + 8 <= end {
            let size = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as usize;
            if size < 8 || pos + size > end {
                return None;
            }
            let typ: [u8; 4] = data[pos + 4..pos + 8].try_into().ok()?;
            if &typ == want {
                return Some(&data[pos + 8..pos + size]);
            }
            if let Some(b) = walk(data, pos + 8, pos + size, want) {
                return Some(b);
            }
            pos += size;
        }
        None
    }
    walk(data, 0, data.len(), want)
}

fn be_u32(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

#[test]
fn mux_parses_back_structurally() -> Result<()> {
    let p = payloads();
    let m4a = mux_aac_lc(&p, 3, 2, 48_000, 2048, 1024)?;
    assert!(crate::isomp4::sniff_is_isobmff(&m4a));
    let track = crate::isomp4::parse_aac_track(&m4a)?;
    let (asc, _) = AudioSpecificConfig::parse(&track.asc)?;
    assert_eq!(asc.aot, 2);
    assert_eq!(asc.sampling_frequency_index, 3);
    assert_eq!(asc.channel_configuration, 2);
    assert_eq!(asc.sample_rate, 48_000);
    assert_eq!(track.frames.len(), p.len());
    // `total_samples` is Σ stts.sample_count — the frame count for the
    // single-entry table we (and ffmpeg) write.
    assert_eq!(track.total_samples, 3);
    assert_eq!(track.edit_start, 1024, "priming via elst media_time");
    assert_eq!(track.edit_duration, 2048);
    assert_eq!(track.movie_timescale, 48_000);
    assert_eq!(track.media_timescale, 48_000);
    assert_eq!(track.media_duration, 3 * 1024);
    assert_eq!(track.presentation_samples(), Some(2048));
    assert_eq!(track.remainder_samples(), Some(0));
    for (frame, want) in track.frames.iter().zip(&p) {
        let (off, len) = *frame;
        assert_eq!(len as usize, want.len());
        assert_eq!(
            &m4a[off as usize..off as usize + len as usize],
            want.as_slice(),
            "frame bytes must be the payload, in order"
        );
    }
    Ok(())
}

#[test]
fn muxed_frame_bytes_roundtrip_bit_exact() -> Result<()> {
    let p = payloads();
    let m4a = mux_aac_lc(&p, 7, 1, 22_050, 2048, 1024)?;
    let track = crate::isomp4::parse_aac_track(&m4a)?;
    let joined: Vec<u8> = track
        .frames
        .iter()
        .flat_map(|&(off, len)| m4a[off as usize..off as usize + len as usize].to_vec())
        .collect();
    let want: Vec<u8> = p.iter().flatten().copied().collect();
    assert_eq!(joined, want);
    assert_eq!(track.media_timescale, ADTS_SAMPLE_RATES_HZ[7]);
    assert_eq!(track.movie_timescale, ADTS_SAMPLE_RATES_HZ[7]);
    Ok(())
}

#[test]
fn independent_boxes_distinguish_coded_from_presentation() -> Result<()> {
    // Two access units, valid = 1: coded 2048, priming 1024, remainder 1023.
    // Movie-timescale 1000 would truncate N=1 at 48 kHz to 0 ms.
    let p = vec![vec![0xAAu8; 8], vec![0xBB; 8]];
    let m4a = mux_aac_lc(&p, 3, 1, 48_000, 1, 1024)?;
    let elst = box_body(&m4a, b"elst").expect("elst");
    assert_eq!(be_u32(elst, 4), 1, "one edit");
    assert_eq!(be_u32(elst, 8), 1, "segment_duration = valid N");
    assert_eq!(be_u32(elst, 12), 1024, "media_time = priming");
    let mdhd = box_body(&m4a, b"mdhd").expect("mdhd");
    assert_eq!(be_u32(mdhd, 12), 48_000);
    assert_eq!(be_u32(mdhd, 16), 2048, "mdhd is coded duration");
    let mvhd = box_body(&m4a, b"mvhd").expect("mvhd");
    assert_eq!(be_u32(mvhd, 12), 48_000, "movie timescale = sample rate");
    assert_eq!(be_u32(mvhd, 16), 1, "mvhd is presentation, not coded");
    let tkhd = box_body(&m4a, b"tkhd").expect("tkhd");
    assert_eq!(be_u32(tkhd, 20), 1);
    let track = crate::isomp4::parse_aac_track(&m4a)?;
    assert_eq!(track.presentation_samples(), Some(1));
    assert_eq!(track.remainder_samples(), Some(1023));
    assert_ne!(
        track.media_duration,
        track.presentation_samples().unwrap(),
        "coded duration must not be used as presentation"
    );
    Ok(())
}

#[test]
fn mux_rejects_valid_past_coded_and_overflow() {
    let p = payloads();
    let err = mux_aac_lc(&p, 3, 2, 48_000, 2049, 1024).unwrap_err();
    assert!(err.to_string().contains("exceed"), "{err}");
    let err = mux_aac_lc(&p, 3, 2, 48_000, u64::MAX, 1024).unwrap_err();
    assert!(err.to_string().contains("overflow"), "{err}");
    let err = mux_aac_lc(&[], 3, 2, 48_000, 0, 1024).unwrap_err();
    assert!(err.to_string().contains("no access"), "{err}");
    let err = mux_aac_lc(&p, 3, 2, 0, 1, 1024).unwrap_err();
    assert!(err.to_string().contains("sample rate"), "{err}");
}

fn impulse(n: usize, at: usize) -> Vec<Vec<f32>> {
    let mut v = vec![0.0f32; n];
    if at < n {
        v[at] = 0.9;
    }
    vec![v]
}

fn peak(x: &[f32]) -> (usize, f32) {
    x.iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, a)| (i, a.abs()))
        .unwrap_or((0, 0.0))
}

fn pad_remainder(n: u64) -> u64 {
    (1024 - (n % 1024)) % 1024
}

fn coded_len(n: u64) -> u64 {
    (n.div_ceil(1024) + 1) * 1024
}

#[test]
fn encode_elst_matches_source_length_short_and_unaligned() -> Result<()> {
    for n in [1u64, 1023, 1024, 1025, 2048, 2049, 28800] {
        let pcm = vec![vec![0.1f32; n as usize]];
        let m4a = encode_with(&pcm, 48_000, &EncodeOptions::m4a())?;
        let track = crate::isomp4::parse_aac_track(&m4a)?;
        assert_eq!(track.edit_start, 1024, "n={n}");
        assert_eq!(track.presentation_samples(), Some(n), "n={n}");
        assert_eq!(track.remainder_samples(), Some(pad_remainder(n)), "n={n}");
        assert_eq!(track.media_duration, coded_len(n), "n={n}");
        assert_eq!(track.movie_timescale, 48_000);
        let elst = box_body(&m4a, b"elst").expect("elst");
        assert_eq!(be_u32(elst, 8), n as u32, "raw elst duration n={n}");
        assert_eq!(be_u32(elst, 12), 1024);
        let mdhd = box_body(&m4a, b"mdhd").expect("mdhd");
        assert_eq!(be_u32(mdhd, 16), coded_len(n) as u32);
        let mvhd = box_body(&m4a, b"mvhd").expect("mvhd");
        assert_eq!(be_u32(mvhd, 16), n as u32);
    }
    Ok(())
}

#[test]
fn encode_elst_retains_first_and_last_for_causal_and_lookahead() -> Result<()> {
    for lookahead in [false, true] {
        let opts = EncodeOptions::m4a().with_lookahead(lookahead);
        let first = encode_with(&impulse(2048, 0), 48_000, &opts)?;
        let last = encode_with(&impulse(2048, 2047), 48_000, &opts)?;
        let t0 = crate::isomp4::parse_aac_track(&first)?;
        let t1 = crate::isomp4::parse_aac_track(&last)?;
        assert_eq!(t0.presentation_samples(), Some(2048));
        assert_eq!(t1.presentation_samples(), Some(2048));
        assert_eq!(t0.remainder_samples(), Some(0));
        let d0 = decode_with(&first, &DecodeOptions::unbounded())?;
        let d1 = decode_with(&last, &DecodeOptions::unbounded())?;
        let (i0, a0) = peak(&d0.channels[0]);
        let (i1, a1) = peak(&d1.channels[0]);
        assert_eq!(i0, 0, "lookahead={lookahead}");
        assert!(a0 > 0.4, "first {a0} lookahead={lookahead}");
        assert!((i1 as i32 - 2047).abs() <= 8, "last i={i1}");
        assert!(a1 > 0.2, "last {a1} lookahead={lookahead}");
    }
    Ok(())
}
