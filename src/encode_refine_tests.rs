//! TASK-74: opt-in bandwise leftover-bit sf refine. Default off.

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

fn snr_aligned(want: &[f32], got: &[f32]) -> f64 {
    let n = want.len();
    if got.len() < n + PRIME {
        return 0.0;
    }
    let w = &want[PRIME..n.saturating_sub(PRIME)];
    let g = &got[2 * PRIME..2 * PRIME + w.len()];
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for i in 0..w.len().min(g.len()) {
        let s = f64::from(w[i]);
        let e = s - f64::from(g[i]);
        ps += s * s;
        pe += e * e;
    }
    if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    }
}

#[test]
fn default_encode_matches_explicit_band_refine_off() {
    let pcm = vec![sine(4096, 440.0, 0.5)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_band_refine(false)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn band_refine_is_deterministic() {
    let pcm = vec![noise(4096, 0.4)];
    let opts = EncodeOptions::adts().with_band_refine(true);
    assert_eq!(
        encode_with(&pcm, RATE, &opts).unwrap(),
        encode_with(&pcm, RATE, &opts).unwrap()
    );
}

#[test]
fn band_refine_silence_stays_silent() {
    let pcm = vec![vec![0.0f32; 4096]];
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts().with_band_refine(true)).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let peak = dec.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak < 1e-4, "refine silence peak {peak}");
}

#[test]
fn band_refine_stream_matches_oneshot() {
    let pcm = vec![noise(3000, 0.3)];
    let opts = EncodeOptions::adts().with_band_refine(true);
    let want = encode_with(&pcm, RATE, &opts).unwrap();
    let mut enc = Encoder::new(RATE, 1, &opts).unwrap();
    let mut got = Vec::new();
    let mut cb = |f: crate::EncodedFrame<'_>| {
        got.extend_from_slice(f.au);
        Ok(())
    };
    enc.feed(&[&pcm[0]], &mut cb).unwrap();
    enc.finish(&mut cb).unwrap();
    assert_eq!(got, want);
}

#[test]
fn band_refine_snr_vs_off() {
    let n = 10 * RATE as usize / 10; // 1 s
    let pcm = vec![noise(n, 0.5)];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_band_refine(true)).unwrap();
    let d_off = decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let snr_off = snr_aligned(&pcm[0], &d_off.channels[0]);
    let snr_on = snr_aligned(&pcm[0], &d_on.channels[0]);
    let db = snr_on - snr_off;
    eprintln!(
        "TASK-74 band refine noise 1s: off {snr_off:.2} on {snr_on:.2} Δ {db:.2} dB; bytes {} vs {}",
        off.len(),
        on.len()
    );
    let _ = db;
    let sine_pcm = vec![sine(n, 440.0, 0.5)];
    let s_off = encode_with(&sine_pcm, RATE, &EncodeOptions::adts()).unwrap();
    let s_on = encode_with(
        &sine_pcm,
        RATE,
        &EncodeOptions::adts().with_band_refine(true),
    )
    .unwrap();
    let ds_off = decode_with(&s_off, &DecodeOptions::unbounded()).unwrap();
    let ds_on = decode_with(&s_on, &DecodeOptions::unbounded()).unwrap();
    let sine_off = snr_aligned(&sine_pcm[0], &ds_off.channels[0]);
    let sine_on = snr_aligned(&sine_pcm[0], &ds_on.channels[0]);
    let sine_db = sine_on - sine_off;
    eprintln!(
        "TASK-74 band refine sine 1s: off {sine_off:.2} on {sine_on:.2} Δ {sine_db:.2} dB; bytes {} vs {}; dec_len {} vs {}; peak {} vs {}",
        s_off.len(),
        s_on.len(),
        ds_off.channels[0].len(),
        ds_on.channels[0].len(),
        ds_off.channels[0]
            .iter()
            .fold(0.0f32, |m, x| m.max(x.abs())),
        ds_on.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs())),
    );
}
