//! TASK-69: opt-in Johnston SFM tonality. Default off = production bytes unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, EncodeOptions, Encoder, decode_with, encode, encode_with};

const RATE: u32 = 48_000;
const PRIME: usize = 1024;

fn sine(n: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin())
        .collect()
}

fn noise(n: usize, amp: f32) -> Vec<f32> {
    let mut s = 0x0BAD_F00Du32;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn snr_db(want: &[f32], got: &[f32]) -> f64 {
    let n = want.len().min(got.len());
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for i in 0..n {
        let s = f64::from(want[i]);
        let e = s - f64::from(got[i]);
        ps += s * s;
        pe += e * e;
    }
    if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    }
}

fn snr_aligned(want: &[f32], got: &[f32]) -> f64 {
    let w = &want[PRIME..want.len() - PRIME];
    let start = if got.len() == want.len() {
        PRIME
    } else {
        2 * PRIME
    };
    snr_db(w, &got[start..start + w.len()])
}

#[test]
fn default_encode_matches_explicit_tonality_off() {
    let pcm = vec![sine(4096, 440.0, 0.5)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_tonality(false)).unwrap();
    assert_eq!(a, b, "with_tonality(false) must be the production path");
}

#[test]
fn tonality_encode_is_deterministic() {
    let pcm = vec![sine(4096, 440.0, 0.5)];
    let opts = EncodeOptions::adts().with_tonality(true);
    let a = encode_with(&pcm, RATE, &opts).unwrap();
    let b = encode_with(&pcm, RATE, &opts).unwrap();
    assert_eq!(a, b);
}

#[test]
fn tonality_silence_stays_silent() {
    let pcm = vec![vec![0.0f32; 4096]];
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts().with_tonality(true)).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let peak = dec.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak < 1e-4, "tonality silence peak {peak}");
}

#[test]
fn tonality_changes_bytes_on_tone_plus_noise() {
    let n = 4096;
    let nse = noise(n, 0.2);
    let pcm: Vec<Vec<f32>> = vec![
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() + nse[i]
            })
            .collect(),
    ];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_tonality(true)).unwrap();
    assert_ne!(off, on, "tonality must change mixed tone+noise coding");
}

#[test]
fn tonality_stream_matches_oneshot() {
    let pcm = vec![sine(3000, 440.0, 0.4)];
    let opts = EncodeOptions::adts().with_tonality(true);
    let want = encode_with(&pcm, RATE, &opts).unwrap();
    let mut enc = Encoder::new(RATE, 1, &opts).unwrap();
    let mut got = Vec::new();
    let mut cb = |f: crate::EncodedFrame<'_>| {
        got.extend_from_slice(f.au);
        Ok(())
    };
    enc.feed(&[&pcm[0]], &mut cb).unwrap();
    enc.finish(&mut cb).unwrap();
    assert_eq!(got.as_slice(), crate::gapless::strip_id3(&want));
}

#[test]
fn tonality_keeps_midband_sine_roundtrip() {
    let pcm = vec![sine(8192, 440.0, 0.5)];
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts().with_tonality(true)).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let snr = snr_aligned(&pcm[0], &dec.channels[0]);
    assert!(snr > 20.0, "tonality 440 Hz SNR {snr:.1} dB");
}

#[test]
fn tonality_is_not_the_ath_path() {
    let n = 4096;
    let nse = noise(n, 0.2);
    let pcm: Vec<Vec<f32>> = vec![
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin() + nse[i]
            })
            .collect(),
    ];
    let ton = encode_with(&pcm, RATE, &EncodeOptions::adts().with_tonality(true)).unwrap();
    let ath = encode_with(&pcm, RATE, &EncodeOptions::adts().with_ath(true)).unwrap();
    assert_ne!(ton, ath, "tonality-only must not equal ATH-only");
}
