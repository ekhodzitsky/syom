//! TASK-86: downsample + 64-band analysis QMF — delay, alias, decoder
//! grid/scale agreement, chunk identity.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{CORE_FRAME, FIR_DELAY, OUT_FRAME, QMF_DRAIN, QMF_SLOT, SbrPrep, he_core_rate};
use crate::engine::enc_sbr_qmf::{BANDS, EncAnalysisQmf};
use crate::engine::sbr_qmf::{AnalysisQmf, QMF_WINDOW};
use crate::{EncodeOptions, ProbeProfile, encode_with, probe};
use std::time::Instant;

fn tone(freq: f32, n: usize, sr: f32) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * freq * i as f32 / sr).sin())
        .collect()
}

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

fn run(pcm: &[f32]) -> (Vec<f32>, Vec<f32>, super::PrepCounts) {
    let mut prep = SbrPrep::new();
    let mut core = Vec::new();
    let mut e_low = Vec::new();
    prep.push(pcm, &mut core, |s| {
        e_low.push(s.band_energy(0, 8));
    })
    .unwrap();
    let c = prep
        .finish(&mut core, |s| {
            e_low.push(s.band_energy(0, 8));
        })
        .unwrap();
    (core, e_low, c)
}

#[test]
fn he_core_rate_v1_set() {
    assert_eq!(he_core_rate(48_000).unwrap(), 24_000);
    assert_eq!(he_core_rate(44_100).unwrap(), 22_050);
    assert_eq!(he_core_rate(16_000).unwrap(), 8_000);
    assert!(he_core_rate(64_000).is_err());
    assert!(he_core_rate(8_000).is_err());
}

#[test]
fn impulse_core_delay_is_four_samples() {
    let mut pcm = vec![0.0f32; OUT_FRAME];
    pcm[0] = 1.0;
    let (core, _, c) = run(&pcm);
    assert_eq!(c, {
        let mut p = SbrPrep::new();
        p.push(&pcm, &mut Vec::new(), |_| {}).unwrap();
        p.finish(&mut Vec::new(), |_| {}).unwrap();
        p.counts()
    });
    assert_eq!(c.source, (OUT_FRAME + FIR_DELAY + QMF_DRAIN) as u64);
    let peak = core
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap();
    assert_eq!(peak.0, FIR_DELAY / 2, "FIR delay 8 output → 4 core");
    assert!(*peak.1 > 0.4, "peak={}", peak.1);
}

#[test]
fn tone_1k_passes_core_18k_is_rejected() {
    let sr = 48_000f32;
    let n = OUT_FRAME * 4;
    let low = tone(1_000.0, n, sr);
    let high = tone(18_000.0, n, sr);
    let (c_lo, slots_lo, _) = run(&low);
    let (c_hi, slots_hi, _) = run(&high);
    let skip = 64;
    let g_lo = goertzel(&c_lo[skip..], 1_000.0, 24_000.0);
    let g_hi = goertzel(&c_hi[skip..], 6_000.0, 24_000.0);
    assert!(
        g_lo > 10.0 * g_hi.max(1e-20),
        "1 kHz must pass, 18 kHz alias rejected: lo={g_lo} hi={g_hi}"
    );
    let e_lo: f32 = slots_lo.iter().sum();
    let e_hi: f32 = slots_hi.iter().sum();
    assert!(
        e_lo > e_hi,
        "QMF low-band energy 1 kHz vs 18 kHz {e_lo} {e_hi}"
    );
}

#[test]
fn qmf_puts_1k_in_low_bands_and_16k_in_high() {
    let sr = 48_000f32;
    let n = OUT_FRAME * 2;
    let mut prep_lo = SbrPrep::new();
    let mut prep_hi = SbrPrep::new();
    let mut core = Vec::new();
    let mut lo_low = 0.0f32;
    let mut lo_high = 0.0f32;
    let mut hi_low = 0.0f32;
    let mut hi_high = 0.0f32;
    prep_lo
        .push(&tone(1_000.0, n, sr), &mut core, |s| {
            lo_low += s.band_energy(0, 12);
            lo_high += s.band_energy(40, 64);
        })
        .unwrap();
    core.clear();
    prep_hi
        .push(&tone(16_000.0, n, sr), &mut core, |s| {
            hi_low += s.band_energy(0, 12);
            hi_high += s.band_energy(40, 64);
        })
        .unwrap();
    assert!(
        lo_low > 8.0 * lo_high.max(1e-12),
        "1 kHz low/high {lo_low}/{lo_high}"
    );
    assert!(
        hi_high > 8.0 * hi_low.max(1e-12),
        "16 kHz high/low {hi_high}/{hi_low}"
    );
}

/// Direct O(N²) f64 evaluation of the 64-band kernel on the same
/// history: `W[k] = Σ_n u[n]·exp(iπ/128·(k+½)(2n−½))`.
fn reference_slots(pcm: &[f32]) -> Vec<[(f64, f64); BANDS]> {
    let mut x = [0.0f64; 640];
    let mut out = Vec::new();
    for chunk in pcm.chunks_exact(QMF_SLOT) {
        x.copy_within(0..640 - 64, 64);
        for (n, s) in chunk.iter().enumerate() {
            x[63 - n] = f64::from(*s);
        }
        let mut u = [0.0f64; 128];
        for (n, un) in u.iter_mut().enumerate() {
            *un = (0..5)
                .map(|j| x[n + 128 * j] * QMF_WINDOW[n + 128 * j])
                .sum();
        }
        let mut w = [(0.0f64, 0.0f64); BANDS];
        for (k, wk) in w.iter_mut().enumerate() {
            for (n, un) in u.iter().enumerate() {
                let a = std::f64::consts::PI / 128.0 * (k as f64 + 0.5) * (2.0 * n as f64 - 0.5);
                wk.0 += un * a.cos();
                wk.1 += un * a.sin();
            }
        }
        out.push(w);
    }
    out
}

#[test]
fn fft_modulation_matches_direct_kernel() {
    let sr = 48_000f32;
    let mut pcm = tone(2_000.0, QMF_SLOT * 24, sr);
    for (i, v) in pcm.iter_mut().enumerate() {
        *v += 0.3 * (2.0 * std::f32::consts::PI * 17_300.0 * i as f32 / sr).sin();
    }
    let reference = reference_slots(&pcm);
    let mut enc = EncAnalysisQmf::new();
    let mut max_abs = 0.0f64;
    let mut max_mag = 0.0f64;
    for (chunk, r) in pcm.chunks_exact(QMF_SLOT).zip(reference.iter()) {
        let s = enc.push_slot(chunk).unwrap();
        for (k, rk) in r.iter().enumerate() {
            max_abs = max_abs
                .max((f64::from(s.re[k]) - rk.0).abs())
                .max((f64::from(s.im[k]) - rk.1).abs());
            max_mag = max_mag.max(rk.0.hypot(rk.1));
        }
    }
    assert!(max_mag > 1.0, "reference carries signal: {max_mag}");
    assert!(
        max_abs < 2e-5 * max_mag,
        "fft vs direct kernel: max |Δ| {max_abs} of {max_mag}"
    );
}

/// The encoder's 64-band `|W|²` on full-rate PCM sits on the decoder's
/// `XLow` grid and scale: the halfband core through the decoder's
/// 32-band bank lands in the same band index with the same energy.
#[test]
fn encoder_bank_matches_decoder_core_bank_grid_and_scale() {
    let sr = 48_000f32;
    for f in [2_000.0f32, 5_000.0, 8_000.0] {
        let pcm = tone(f, OUT_FRAME * 3, sr);
        let mut prep = SbrPrep::new();
        let mut core = Vec::new();
        let mut enc_e = [0.0f64; BANDS];
        let mut n_slots = 0usize;
        prep.push(&pcm, &mut core, |s| {
            n_slots += 1;
            if n_slots > 16 {
                for (k, e) in enc_e.iter_mut().enumerate() {
                    *e += f64::from(s.energy(k));
                }
            }
        })
        .unwrap();
        let mut dec = AnalysisQmf::new();
        let mut dec_e = [0.0f64; 32];
        let mut n_dec = 0usize;
        for chunk in core.chunks_exact(32) {
            let s = dec
                .push_slot(&chunk.iter().map(|&x| f64::from(x)).collect::<Vec<_>>())
                .unwrap();
            n_dec += 1;
            if n_dec > 16 {
                for (k, e) in dec_e.iter_mut().enumerate() {
                    *e += s[k].norm_sqr();
                }
            }
        }
        let peak = |e: &[f64]| {
            e.iter()
                .enumerate()
                .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
                .unwrap()
                .0
        };
        let (pe, pd) = (peak(&enc_e), peak(&dec_e));
        assert_eq!(pe, pd, "{f} Hz peak band enc {pe} dec {pd}");
        assert_eq!(pe, (f / (sr / 128.0)) as usize);
        let ratio = enc_e[pe] / (n_slots - 16) as f64 / (dec_e[pd] / (n_dec - 16) as f64);
        // Above 1.0 is the halfband's passband droop on the decoder
        // side (−0.27 dB at 8 kHz, SBR_PREP.md), not a kernel-scale gap.
        assert!(
            (0.97..1.08).contains(&ratio),
            "{f} Hz enc/dec energy ratio {ratio}"
        );
    }
}

#[test]
fn chunking_and_reset_are_identity() {
    let pcm = tone(3_000.0, OUT_FRAME + 17, 48_000.0);
    let hash = |chunks: &[&[f32]]| {
        let mut p = SbrPrep::new();
        let mut core = Vec::new();
        let mut slots = Vec::new();
        for c in chunks {
            p.push(c, &mut core, |s| slots.push(*s)).unwrap();
        }
        p.finish(&mut core, |s| slots.push(*s)).unwrap();
        (core, slots)
    };
    let a = hash(&[&pcm[..]]);
    let b = hash(&[&pcm[..1], &pcm[1..32], &pcm[32..]]);
    let mut p = SbrPrep::new();
    p.push(&pcm[..40], &mut Vec::new(), |_| {}).unwrap();
    p.reset();
    let mut core = Vec::new();
    let mut slots = Vec::new();
    p.push(&pcm, &mut core, |s| slots.push(*s)).unwrap();
    p.finish(&mut core, |s| slots.push(*s)).unwrap();
    assert_eq!(a.0, b.0, "core chunk identity");
    assert_eq!(a.1.len(), b.1.len());
    for (x, y) in a.1.iter().zip(b.1.iter()) {
        assert_eq!(x.re, y.re);
        assert_eq!(x.im, y.im);
    }
    assert_eq!(core, a.0);
    assert_eq!(slots.len(), a.1.len());
}

#[test]
fn encode_stays_lc() {
    let pcm = vec![tone(440.0, CORE_FRAME * 2, 48_000.0)];
    let adts = encode_with(&pcm, 48_000, &EncodeOptions::adts()).unwrap();
    assert_eq!(probe(&adts).unwrap().profile, ProbeProfile::Lc);
}

#[test]
fn encoder_files_have_no_libm_decision_path() {
    let qmf = include_str!("enc_sbr_qmf.rs");
    let prep = include_str!("enc_sbr_prep.rs");
    for src in [qmf, prep] {
        assert!(!src.contains("powf("));
        assert!(!src.contains(".sin()"));
        assert!(!src.contains(".cos()"));
        assert!(!src.contains("f64::sin"));
        assert!(!src.contains("f32::sin"));
    }
    assert!(qmf.contains("det_math"));
}

#[test]
fn workspace_is_bounded_and_prep_is_within_he_cpu_budget() {
    let bytes = std::mem::size_of::<SbrPrep>();
    assert!(bytes < 8 * 1024, "SbrPrep {bytes} bytes");
    let pcm = tone(1_000.0, OUT_FRAME, 48_000.0);
    let mut prep = SbrPrep::new();
    let mut core = Vec::with_capacity(CORE_FRAME + 64);
    let t0 = Instant::now();
    for _ in 0..40 {
        core.clear();
        prep.reset();
        prep.push(&pcm, &mut core, |_| {}).unwrap();
        let _ = prep.finish(&mut core, |_| {}).unwrap();
    }
    let prep_ns = t0.elapsed().as_nanos();
    let planes = vec![pcm.clone()];
    let opts = EncodeOptions::adts();
    let t1 = Instant::now();
    for _ in 0..40 {
        let _ = encode_with(&planes, 48_000, &opts).unwrap();
    }
    let enc_ns = t1.elapsed().as_nanos();
    // HE_ENC.md budget: whole HE encode ≤ 4× LC encode. Release prep alone
    // measures ~1.7× (lab/quality/SBR_PREP.md); debug is far below.
    assert!(
        prep_ns < 4 * enc_ns,
        "prep {prep_ns} ns must be < 4× LC encode {enc_ns} ns over 40 frames"
    );
}

#[test]
fn qmf_slot_is_64_and_frame_is_32_slots() {
    assert_eq!(QMF_SLOT, 64);
    assert_eq!(OUT_FRAME / QMF_SLOT, 32);
    let pcm = vec![0.1f32; OUT_FRAME];
    let mut n = 0u64;
    let mut prep = SbrPrep::new();
    prep.push(&pcm, &mut Vec::new(), |_| n += 1).unwrap();
    assert_eq!(n, 32);
}
