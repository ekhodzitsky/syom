//! Public encode API: roundtrips through the shipped decoder, error paths.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AacError, EncodeOptions, decode_with, encode, encode_with};

/// SNR (dB) of `got` vs `want` over their common prefix.
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
        return 200.0;
    }
    10.0 * (ps / pe).log10()
}

fn sine(rate: u32, secs: f64, freq: f32, amp: f32) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
        .collect()
}

/// Encoder delay: decoded sample `i` corresponds to input `i − 1024` past
/// the windowed fade-in (the first decoded frame). Compare from frame 2 on.
const PRIME: usize = 1024;

/// SNR of the roundtrip with the one-frame encoder delay accounted for.
fn snr_aligned(want: &[f32], got: &[f32]) -> f64 {
    snr_db(
        &want[PRIME..want.len() - PRIME],
        &got[2 * PRIME..want.len()],
    )
}

#[test]
fn sine_roundtrip_mono() {
    let pcm = vec![sine(48_000, 0.25, 440.0, 0.5)];
    let adts = encode(&pcm, 48_000).expect("encode");
    let dec = decode_with(&adts, &crate::DecodeOptions::unbounded()).expect("decode");
    assert_eq!(dec.sample_rate, 48_000);
    assert_eq!(dec.channels.len(), 1);
    let got = &dec.channels[0];
    let want = &pcm[0];
    assert!(got.len() >= want.len());
    let snr = snr_aligned(want, got);
    assert!(snr >= 45.0, "mono sine roundtrip SNR {snr:.1} dB");
}

#[test]
fn sine_roundtrip_stereo() {
    let l = sine(48_000, 0.25, 440.0, 0.5);
    let r = sine(48_000, 0.25, 660.0, 0.25);
    let pcm = vec![l, r];
    let adts = encode(&pcm, 48_000).expect("encode");
    let dec = decode_with(&adts, &crate::DecodeOptions::unbounded()).expect("decode");
    assert_eq!(dec.channels.len(), 2);
    for (ch, (want, got)) in pcm.iter().zip(dec.channels.iter()).enumerate() {
        let snr = snr_aligned(want, got);
        assert!(snr >= 40.0, "stereo ch{ch} roundtrip SNR {snr:.1} dB");
    }
}

#[test]
fn silence_stays_silent() {
    let pcm = vec![vec![0.0f32; 8192]];
    let adts = encode(&pcm, 48_000).expect("encode");
    let dec = decode_with(&adts, &crate::DecodeOptions::unbounded()).expect("decode");
    let peak = dec.channels[0]
        .iter()
        .map(|x| x.abs())
        .fold(0.0f32, f32::max);
    assert!(peak < 1e-4, "silence decoded with peak {peak}");
}

#[test]
fn partial_tail_frame_is_padded() {
    let pcm = vec![sine(48_000, 0.05, 440.0, 0.5)[..1000].to_vec()];
    let adts = encode(&pcm, 48_000).expect("encode");
    let dec = decode_with(&adts, &crate::DecodeOptions::unbounded()).expect("decode");
    assert_eq!(dec.channels[0].len(), 1024, "one padded frame");
}

#[test]
fn bitrate_knob_changes_size() {
    let pcm = vec![sine(48_000, 1.0, 440.0, 0.5)];
    let lo = encode_with(
        &pcm,
        48_000,
        &EncodeOptions::adts().with_bitrate_bps(32_000),
    )
    .expect("32k");
    let hi = encode_with(
        &pcm,
        48_000,
        &EncodeOptions::adts().with_bitrate_bps(192_000),
    )
    .expect("192k");
    assert!(
        hi.len() > lo.len(),
        "192k ({} B) should beat 32k ({} B)",
        hi.len(),
        lo.len()
    );
}

#[test]
fn error_paths_are_encode_errors() {
    let pcm = vec![sine(48_000, 0.05, 440.0, 0.5)];
    let cases: Vec<AacError> = vec![
        encode(&pcm, 47_000).unwrap_err(),
        encode(&[], 48_000).unwrap_err(),
        encode(&[vec![], vec![]], 48_000).unwrap_err(),
        encode(&[pcm[0].clone(), pcm[0].clone(), pcm[0].clone()], 48_000).unwrap_err(),
        encode(&[vec![f32::NAN; 2048]], 48_000).unwrap_err(),
        encode_with(&pcm, 48_000, &EncodeOptions::adts().with_bitrate_bps(0)).unwrap_err(),
    ];
    for e in cases {
        assert!(
            matches!(e, AacError::Encode(_)),
            "expected Encode, got {e:?}"
        );
        assert!(!e.to_string().is_empty(), "stable Display");
    }
}

#[test]
fn write_roundtrip_via_file() {
    let pcm = vec![sine(48_000, 0.05, 440.0, 0.5)];
    let path = std::env::temp_dir().join(format!("syom-enc-test-{}.adts", std::process::id()));
    crate::write(&path, &pcm, 48_000).expect("write");
    let dec = crate::read_with(&path, &crate::DecodeOptions::unbounded()).expect("read");
    let _ = std::fs::remove_file(&path);
    assert_eq!(dec.sample_rate, 48_000);
    let snr = snr_aligned(&pcm[0], &dec.channels[0]);
    assert!(snr >= 45.0, "file roundtrip SNR {snr:.1} dB");
}

/// Deterministic white noise (LCG), full-band, moderate level.
fn noise(rate: u32, secs: f64, amp: f32) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    let mut state = 0x2F6E_2B1Du32;
    (0..n)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let u = (state >> 9) as f32 / (1u32 << 23) as f32; // [0, 1)
            amp * (2.0 * u - 1.0)
        })
        .collect()
}

/// Band-limited noise: sum of random-phase sines up to `fmax` (LCG phases).
fn band_noise(rate: u32, secs: f64, amp: f32, fmax: f32) -> Vec<f32> {
    let n = (f64::from(rate) * secs) as usize;
    let mut state = 0x1A2B_3C4Du32;
    let mut phase = [0.0f32; 96];
    let mut freq = [0.0f32; 96];
    for k in 0..96 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        phase[k] = (state >> 9) as f32 / (1u32 << 23) as f32 * 2.0 * std::f32::consts::PI;
        freq[k] = fmax * (k as f32 + 0.5) / 96.0;
    }
    let scale = amp / 96f32.sqrt();
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let mut acc = 0.0f32;
            for k in 0..96 {
                acc += (2.0 * std::f32::consts::PI * freq[k] * t + phase[k]).sin();
            }
            acc * scale
        })
        .collect()
}

#[test]
fn bitrate_accuracy_on_noise() {
    // White noise is budget-limited: achieved bitrate tracks the target.
    let l = noise(48_000, 1.0, 0.3);
    let r = noise(48_000, 1.0, 0.3);
    let stereo = vec![l.clone(), r];
    for (pcm, target) in [
        (vec![l], 32_000u32),
        (stereo.clone(), 64_000),
        (stereo, 128_000),
    ] {
        let opts = EncodeOptions::adts().with_bitrate_bps(target);
        let adts = encode_with(&pcm, 48_000, &opts).expect("encode");
        let achieved = (adts.len() as u64) * 8;
        let ratio = achieved as f64 / f64::from(target);
        assert!(
            (0.90..=1.10).contains(&ratio),
            "target {target}, achieved {achieved} bits/s"
        );
    }
}

#[test]
fn band_noise_quality_floor_stereo_128k() {
    let l = band_noise(48_000, 0.5, 0.3, 4_000.0);
    let r = band_noise(48_000, 0.5, 0.3, 4_000.0);
    let pcm = vec![l, r];
    let adts = encode(&pcm, 48_000).expect("encode");
    let dec = decode_with(&adts, &crate::DecodeOptions::unbounded()).expect("decode");
    for (ch, (want, got)) in pcm.iter().zip(dec.channels.iter()).enumerate() {
        for half in 0..2 {
            let a = &want[1024 + half * 4096..1024 + (half + 1) * 4096];
            let b = &got[2048 + half * 4096..2048 + (half + 1) * 4096];
            eprintln!("ch{ch} half{half} snr {:.2}", snr_db(a, b));
        }
        let snr = snr_aligned(want, got);
        assert!(snr >= 15.0, "noise ch{ch} SNR {snr:.1} dB");
    }
}
