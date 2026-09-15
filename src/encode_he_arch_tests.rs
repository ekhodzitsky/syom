//! TASK-85: HE encode architecture — shipped LC gap, signalling, tables.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::sbr_huffman::{T_HUFFMAN_ENV_1_5DB, T_HUFFMAN_ENV_3_0DB};
use crate::engine::sbr_qmf::QMF_WINDOW;
use crate::{
    DecodeOptions, EncodeOptions, ProbeProfile, decode_with, encode_with, probe, probe_with,
};

const HE_M4A: &[u8] = include_bytes!("goldens/he48.m4a");
const HE_ADTS: &[u8] = include_bytes!("goldens/he48.adts");
const ARCH: &str = include_str!("../lab/quality/HE_ENC.md");

fn tone(freq: f32, n: usize, sr: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
        .collect()
}

/// Goertzel power at `freq` (relative units). Used to show LC 24 kbps
/// keeps 440 Hz and drops 8 kHz — the gap HE is for. Not a HE encoder.
fn goertzel(x: &[f32], freq: f32, sr: f32) -> f64 {
    let w = 2.0 * std::f64::consts::PI * f64::from(freq) / f64::from(sr);
    let coeff = 2.0 * w.cos();
    let mut s1 = 0.0;
    let mut s2 = 0.0;
    for &v in x {
        let s0 = f64::from(v) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - coeff * s1 * s2
}

fn band(x: &[f32], freqs: &[f32], sr: f32) -> f64 {
    freqs.iter().map(|&f| goertzel(x, f, sr)).sum()
}

fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((s >> 33) as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .map(|v| v * 0.5)
        .collect()
}

#[test]
fn architecture_doc_names_scope_targets_and_go() {
    for needle in [
        "## Decision",
        "**Go HE v1 encode**",
        "**No-go as a 0.x default**",
        "## Scope (v1)",
        "2× dual-rate",
        "## Signal path",
        "## Delay / drain",
        "## Envelope / noise / grid",
        "## Bit budget",
        "## Profile signalling",
        "## Quality / CPU / memory targets",
        "## Tables: reuse vs original",
        "QMF_WINDOW",
        "sbr_huffman",
        "Forbidden sources",
        "HeAacEncoder",
        "## Conformance vectors",
        "2b098800",
        "## Staged interfaces",
        "TASK-86",
        "TASK-90",
        "unavailable",
    ] {
        assert!(ARCH.contains(needle), "missing {needle:?}");
    }
    assert!(ARCH.contains("No-go HE v2 (PS) encode"));
    assert!(!ARCH.contains("copy FDK encoder"));
}

#[test]
fn encode_stays_lc_even_at_he_bitrates() {
    let pcm = vec![tone(440.0, 48_000, 48_000.0, 0.5)];
    let opts = EncodeOptions {
        bitrate_bps: 24_000,
        ..EncodeOptions::adts()
    };
    let adts = encode_with(&pcm, 48_000, &opts).unwrap();
    let info = probe(&adts).unwrap();
    assert_eq!(info.profile, ProbeProfile::Lc);
    assert_eq!(info.meta.core_rate, 48_000);
    assert_eq!(info.meta.output_rate, 48_000);
    assert_eq!(info.container, crate::ProbeContainer::Adts);
}

#[test]
fn m4a_he_golden_is_the_signalling_target() {
    let p = probe(HE_M4A).unwrap();
    assert_eq!(p.container, crate::ProbeContainer::M4a);
    assert_ne!(p.profile, ProbeProfile::Lc);
    assert_eq!(p.meta.core_rate, 24_000);
    assert_eq!(p.meta.output_rate, 48_000);
    let adts = probe(HE_ADTS).unwrap();
    assert_eq!(adts.container, crate::ProbeContainer::Adts);
    assert_eq!(adts.profile, ProbeProfile::Lc);
    assert_eq!(adts.meta.core_rate, 24_000);
}

#[test]
fn lc_24k_keeps_low_tone_and_darkens_noise() {
    let sr = 48_000f32;
    let n = 48_000usize;
    let opts = EncodeOptions {
        bitrate_bps: 24_000,
        ..EncodeOptions::adts()
    };
    let audio = DecodeOptions::audio();
    let skip = 1024;

    let low = vec![tone(440.0, n, sr, 0.5)];
    let adts = encode_with(&low, 48_000, &opts).unwrap();
    assert_eq!(probe(&adts).unwrap().profile, ProbeProfile::Lc);
    let dec = decode_with(&adts, &audio).unwrap();
    let src = &low[0][..n - skip];
    let out = &dec.channels[0][skip..skip + src.len()];
    let keep = goertzel(out, 440.0, sr) / goertzel(src, 440.0, sr);
    assert!(keep >= 0.5, "LC 24k must keep 440 Hz, ratio={keep}");

    let nse = vec![noise(n, 0x5EED_5EED_5EED_5EED)];
    let adts = encode_with(&nse, 48_000, &opts).unwrap();
    let dec = decode_with(&adts, &audio).unwrap();
    let src = &nse[0][..n - skip];
    let out = &dec.channels[0][skip..skip + src.len()];
    let lf = [250.0, 500.0, 1_000.0, 2_000.0];
    let hf = [7_000.0, 8_000.0, 10_000.0, 12_000.0];
    let src_tilt = band(src, &hf, sr) / band(src, &lf, sr).max(1e-20);
    let out_tilt = band(out, &hf, sr) / band(out, &lf, sr).max(1e-20);
    assert!(
        out_tilt < 0.8 * src_tilt,
        "LC 24k noise must darken vs source: out_tilt={out_tilt} src_tilt={src_tilt}"
    );
}

#[test]
fn permitted_iso_tables_are_in_tree_and_complete() {
    assert_eq!(QMF_WINDOW.len(), 640);
    assert!(QMF_WINDOW[0].abs() < 1e-12);
    assert_eq!(T_HUFFMAN_ENV_1_5DB.len(), 121);
    assert_eq!(T_HUFFMAN_ENV_3_0DB.len(), 63);
    let bands = include_str!("engine/sbr_freq_bands.rs");
    assert!(bands.contains("fn k0"));
    assert!(bands.contains("fn k2"));
    let qmf = include_str!("engine/sbr_qmf.rs");
    assert!(qmf.contains("det_math"));
    assert!(qmf.contains("FMA-free"));
}

#[test]
fn product_encode_path_is_still_lc_only() {
    let encode = include_str!("encode.rs");
    assert!(encode.contains("AAC-LC only"));
    assert!(!encode.contains("SbrEncoder"));
    let opts = format!("{:?}", EncodeOptions::default());
    assert!(!opts.to_ascii_lowercase().contains("sbr"));
    assert!(!opts.to_ascii_lowercase().contains("he_aac"));
}

#[test]
fn he_decode_golden_is_full_band_not_core() {
    let pcm = decode_with(HE_ADTS, &DecodeOptions::audio()).unwrap();
    assert_eq!(pcm.sample_rate, 48_000);
    assert_eq!(pcm.core_rate, 24_000);
    assert_eq!(pcm.channels.len(), 2);
    assert!(pcm.channels[0].iter().any(|s| s.abs() > 1e-4));
}

#[test]
fn metadata_budget_still_fences_he_m4a_probe() {
    let tiny = crate::MemoryBudgets {
        max_metadata_bytes: 8,
        ..crate::MemoryBudgets::default()
    };
    let err = probe_with(HE_M4A, &tiny).unwrap_err();
    assert!(
        matches!(
            err,
            crate::AacError::Limit {
                kind: crate::BudgetKind::Metadata,
                ..
            }
        ),
        "{err:?}"
    );
}
