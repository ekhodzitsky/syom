//! TASK-68: opt-in Terhardt ATH. Default off = production bytes unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, EncodeOptions, Encoder, decode_with, encode, encode_with};

const RATE: u32 = 48_000;
const PRIME: usize = 1024;

fn sine(n: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin())
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
    // A tagged decode is already presentation-aligned. An untagged buffer
    // still carries the 1024-sample delay in front of that window.
    let start = if got.len() == want.len() {
        PRIME
    } else {
        2 * PRIME
    };
    snr_db(w, &got[start..start + w.len()])
}

fn rms(x: &[f32]) -> f32 {
    let s: f32 = x.iter().map(|v| v * v).sum();
    (s / x.len().max(1) as f32).sqrt()
}

#[test]
fn default_encode_matches_explicit_ath_off() {
    let pcm = vec![sine(4096, 440.0, 0.5)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_ath(false)).unwrap();
    assert_eq!(a, b, "with_ath(false) must be the production path");
}

#[test]
fn ath_encode_is_deterministic() {
    let pcm = vec![sine(4096, 440.0, 0.5)];
    let opts = EncodeOptions::adts().with_ath(true);
    let a = encode_with(&pcm, RATE, &opts).unwrap();
    let b = encode_with(&pcm, RATE, &opts).unwrap();
    assert_eq!(a, b);
}

#[test]
fn ath_silence_stays_silent() {
    let pcm = vec![vec![0.0f32; 4096]];
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts().with_ath(true)).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let peak = dec.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak < 1e-4, "ATH silence peak {peak}");
}

#[test]
fn ath_changes_bytes_on_quiet_hf() {
    let n = 4096;
    let pcm: Vec<Vec<f32>> = vec![
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.5 * (2.0 * std::f32::consts::PI * 1000.0 * t).sin()
                    + 0.01 * (2.0 * std::f32::consts::PI * 16_000.0 * t).sin()
            })
            .collect(),
    ];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_ath(true)).unwrap();
    assert_ne!(
        off, on,
        "ATH must change coding of −40 dB 16 kHz vs 0 dB 1 kHz"
    );
}

#[test]
fn ath_stream_matches_oneshot() {
    let pcm = vec![sine(3000, 440.0, 0.4)];
    let opts = EncodeOptions::adts().with_ath(true);
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
fn ath_keeps_midband_sine_roundtrip() {
    let pcm = vec![sine(8192, 440.0, 0.5)];
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts().with_ath(true)).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let snr = snr_aligned(&pcm[0], &dec.channels[0]);
    assert!(snr > 20.0, "ATH 440 Hz SNR {snr:.1} dB");
}

#[test]
fn ath_reduces_inaudible_hf_tone_energy() {
    let n = 8192;
    let pcm = vec![sine(n, 16_000.0, 0.01)];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_ath(true)).unwrap();
    let d_off = decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let mid = 2 * PRIME;
    let hi = n;
    let rms_off = rms(&d_off.channels[0][mid..hi]);
    let rms_on = rms(&d_on.channels[0][mid..hi]);
    assert!(
        rms_on < rms_off * 0.5,
        "ATH should suppress −40 dB 16 kHz (off {rms_off:.3e} on {rms_on:.3e})"
    );
}
