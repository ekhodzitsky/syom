//! TASK-71: opt-in short-window grouping. Default off = production bytes
//! unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::adts::AdtsHeader;
use crate::engine::bits::BitReader;
use crate::engine::ics::{IcsInfo, WindowSequence};
use crate::engine::section::SectionData;
use crate::engine::sf::{self, ScaleFactors};
use crate::{DecodeOptions, EncodeOptions, encode, encode_with};

const RATE: u32 = 48_000;
const FRAME: usize = 1024;

fn click_pcm() -> Vec<Vec<f32>> {
    let n = 14 * FRAME;
    let mut v = vec![0.0f32; n];
    let at = 5 * FRAME + 700;
    let mut lcg = 0x1234_5678u32;
    for x in &mut v[at..at + 32] {
        lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let u = (lcg >> 9) as f32 / (1u32 << 23) as f32;
        *x = 0.9 * (2.0 * u - 1.0);
    }
    vec![v]
}

fn sine(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / RATE as f32).sin())
        .collect()
}

fn err_around_click(decoded: &[f32], pcm: &[f32]) -> f64 {
    let click = 5 * FRAME + 700;
    let lo = click.saturating_sub(64);
    let hi = (click + 192).min(pcm.len());
    let dec_lo = lo + FRAME;
    let dec_hi = hi + FRAME;
    if dec_hi > decoded.len() {
        return f64::INFINITY;
    }
    pcm[lo..hi]
        .iter()
        .zip(decoded[dec_lo..dec_hi].iter())
        .map(|(&a, &b)| {
            let d = f64::from(a) - f64::from(b);
            d * d
        })
        .sum()
}

/// Grouping + section + scale-factor bits of every EightShort frame.
fn short_syntax_bits(adts: &[u8]) -> (usize, Vec<u8>) {
    let adts = crate::gapless::strip_id3(adts);
    let mut i = 0usize;
    let mut bits = 0usize;
    let mut groups = Vec::new();
    while i + 7 <= adts.len() {
        let Ok((hdr, off)) = AdtsHeader::parse(&adts[i..]) else {
            break;
        };
        let n = hdr.aac_frame_length as usize;
        if n < off || i + n > adts.len() {
            break;
        }
        let payload = &adts[i + off..i + n];
        let mut br = BitReader::new(payload);
        let id = br.read(3).unwrap_or(7);
        let _tag = br.read(4);
        if id == 0 {
            let gg = br.read(8).unwrap_or(0) as u8;
            if let Ok(ics) = IcsInfo::parse(&mut br, 3, false)
                && ics.window_sequence == WindowSequence::EightShort
            {
                groups.push(ics.num_window_groups);
                let t0 = br.bit_position();
                if let Ok(sec) = SectionData::parse(&mut br, &ics) {
                    let mut sfs = ScaleFactors::default();
                    let _ = sf::parse_into(&mut br, &ics, &sec.sfb_cb, gg, &mut sfs);
                    let t2 = br.bit_position();
                    bits += 7;
                    bits += t2.saturating_sub(t0) as usize;
                }
            }
        }
        i += n;
    }
    (bits, groups)
}

#[test]
fn default_matches_short_group_off() {
    let pcm = vec![sine(4096)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_group(false)).unwrap();
    assert_eq!(a, b);
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_group(true)).unwrap();
    assert_eq!(a, on, "steady sine has no short windows");
}

#[test]
fn short_group_off_matches_default_on_transient() {
    let pcm = click_pcm();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let explicit = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_group(false)).unwrap();
    assert_eq!(off, explicit);
}

#[test]
fn short_group_is_deterministic() {
    let pcm = click_pcm();
    let opts = EncodeOptions::adts().with_short_group(true);
    assert_eq!(
        encode_with(&pcm, RATE, &opts).unwrap(),
        encode_with(&pcm, RATE, &opts).unwrap()
    );
}

#[test]
fn grouped_short_frames_decode_and_reduce_groups() {
    let pcm = click_pcm();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_group(true)).unwrap();
    let d_off = crate::decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = crate::decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(d_off.sample_rate, RATE);
    assert_eq!(d_on.channels[0].len(), d_off.channels[0].len());
    assert!(d_on.channels[0].iter().all(|x| x.is_finite()));
    let (_b_off, g_off) = short_syntax_bits(&off);
    let (_b_on, g_on) = short_syntax_bits(&on);
    assert!(g_off.contains(&8), "baseline is 8×1: {g_off:?}");
    assert!(
        g_on.iter().any(|&n| n < 8),
        "grouped click should merge windows: {g_on:?}"
    );
}

#[test]
fn grouping_overhead_and_click_error() {
    let pcm = click_pcm();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_group(true)).unwrap();
    let (syn_off, _) = short_syntax_bits(&off);
    let (syn_on, groups) = short_syntax_bits(&on);
    let d_off = crate::decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = crate::decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let e_off = err_around_click(&d_off.channels[0], &pcm[0]);
    let e_on = err_around_click(&d_on.channels[0], &pcm[0]);
    let save = if syn_off == 0 {
        0.0
    } else {
        100.0 * (syn_off as f64 - syn_on as f64) / syn_off as f64
    };
    let db = 10.0 * (e_off / e_on.max(1e-12)).log10();
    eprintln!(
        "TASK-71 grouping: syntax {syn_off} → {syn_on} bits ({save:.1}%); \
         click Δ {db:.1} dB; bytes {} vs {}; groups {groups:?}",
        off.len(),
        on.len()
    );
    // Recorded in lab/quality/GROUPING.md; default stays off unless ≥5%.
    let _ = (save, db);
}
