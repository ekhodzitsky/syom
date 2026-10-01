//! Quantizer / scalefactor-normalization unit tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::swb::long_offsets;
use super::{
    MAX_BANDS, QuantChannel, band_peaks, dpcm_ok, normalize_sf, quantize, raw_scalefactors,
    requant_band, sf_for_peak,
};

#[test]
fn sf_for_peak_hits_target() {
    for &peak in &[1.0f32, 100.0, 1e4, 1e6, 3e7] {
        for &tq in &[256.0f32, 2048.0, 4095.0] {
            let sf = sf_for_peak(peak, tq);
            let q = peak.powf(0.75) * (-0.1875f32 * (sf - 100) as f32).exp2();
            let ratio = q / tq;
            assert!(
                (0.85..=1.15).contains(&ratio),
                "peak {peak} tq {tq}: sf {sf} gives q {q}"
            );
        }
    }
}

#[test]
fn normalize_sf_respects_wire_bounds() {
    // Deterministic pseudo-random raw scalefactors, including wild swings.
    let mut state = 0x1234_5678u32;
    let mut rng = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state >> 8) as i32
    };
    for _ in 0..50 {
        let n_bands = 49;
        let mut q = QuantChannel::new(n_bands);
        for b in 0..n_bands {
            q.coded[b] = rng() % 4 != 0;
            q.sf[b] = (rng() % 400) - 80;
        }
        let gg = normalize_sf(&mut q);
        let mut prev = i32::from(gg);
        let mut first = true;
        for b in 0..n_bands {
            if !q.coded[b] {
                continue;
            }
            let sf = q.sf[b];
            assert!((0..=255).contains(&sf), "sf {sf} out of 8-bit range");
            if first {
                assert_eq!(sf, i32::from(gg), "first coded sf is global_gain");
                first = false;
            } else {
                assert!(
                    (sf - prev).abs() <= 60,
                    "DPCM delta {} exceeds ±60",
                    sf - prev
                );
            }
            prev = sf;
        }
    }
}

#[test]
fn silent_band_quantizes_to_zero() {
    let offsets = long_offsets(3).expect("48k long offsets");
    let n_bands = offsets.len() - 1;
    let mut q = QuantChannel::new(n_bands);
    let mut peaks = [0.0f32; MAX_BANDS];
    peaks[10] = 1e6; // one live band, rest silent
    let tq = [2048.0f32; MAX_BANDS];
    q.coded[10] = true; // the psy model's decision, mocked
    raw_scalefactors(&peaks, &tq, 0, &mut q);
    let gg = normalize_sf(&mut q);
    let mut spec = [0.0f32; 1024];
    spec[usize::from(offsets[10])] = 1e6;
    quantize(&spec, offsets, &mut q);
    assert!(q.quant[..usize::from(offsets[10])].iter().all(|&v| v == 0));
    assert!(q.quant[usize::from(offsets[11])..].iter().all(|&v| v == 0));
    let peak_q = q.quant[usize::from(offsets[10])].abs();
    assert!(peak_q > 1000, "peak band quantized to {peak_q}");
    let _ = gg;
}

#[test]
fn band_peaks_tracks_spectrum() {
    let offsets = long_offsets(3).expect("48k long offsets");
    let mut spec = [0.0f32; 1024];
    spec[100] = -7.5;
    spec[600] = 3.0;
    let mut peaks = [0.0f32; MAX_BANDS];
    band_peaks(&spec, offsets, &mut peaks);
    let b100 = offsets.iter().position(|&o| o > 100).expect("band") - 1;
    assert_eq!(peaks[b100], 7.5);
    assert_eq!(peaks[0], 0.0);
}

#[test]
fn dpcm_ok_enforces_range_and_delta() {
    let mut sf = [0i32; 4];
    let coded = [true, true, false, true];
    sf[0] = 100;
    sf[1] = 160;
    sf[3] = 160;
    assert!(dpcm_ok(&sf, &coded, 4));
    sf[1] = 161;
    assert!(!dpcm_ok(&sf, &coded, 4));
    sf[1] = 160;
    sf[0] = -1;
    assert!(!dpcm_ok(&sf, &coded, 4));
}

#[test]
fn requant_band_matches_full_quantize_on_that_band() {
    let offsets = long_offsets(3).expect("48k");
    let n = offsets.len() - 1;
    let mut spec = [0.0f32; 1024];
    for (i, x) in spec.iter_mut().enumerate() {
        *x = ((i % 17) as f32 - 8.0) * 100.0;
    }
    let mut q = QuantChannel::new(n);
    q.coded[..n].fill(true);
    q.sf[..n].fill(100);
    quantize(&spec, offsets, &mut q);
    let b = 10usize;
    q.sf[b] = 99;
    requant_band(&spec, offsets, &mut q, b);
    let mut q2 = QuantChannel::new(n);
    q2.coded[..n].fill(true);
    q2.sf[..n].fill(100);
    q2.sf[b] = 99;
    quantize(&spec, offsets, &mut q2);
    let lo = usize::from(offsets[b]);
    let hi = usize::from(offsets[b + 1]);
    assert_eq!(&q.quant[lo..hi], &q2.quant[lo..hi]);
    assert_eq!(q.bits[b], q2.bits[b]);
}

#[test]
fn two_band_one_step_picks_higher_error() {
    // Exhaustive 0/1 decrement on two isolated coded bands: the higher
    // energy/(qmax+1) band is the unique best single step.
    let offsets = long_offsets(3).expect("48k");
    let n = offsets.len() - 1;
    let mut spec = [0.0f32; 1024];
    let b0 = 4usize;
    let b1 = 20usize;
    let lo0 = usize::from(offsets[b0]);
    let hi0 = usize::from(offsets[b0 + 1]);
    let lo1 = usize::from(offsets[b1]);
    let hi1 = usize::from(offsets[b1 + 1]);
    for x in spec[lo0..hi0].iter_mut() {
        *x = 800.0;
    }
    for x in spec[lo1..hi1].iter_mut() {
        *x = 80.0;
    }
    let mut q = QuantChannel::new(n);
    q.coded[b0] = true;
    q.coded[b1] = true;
    q.sf[b0] = 100;
    q.sf[b1] = 100;
    quantize(&spec, offsets, &mut q);
    let score = |q: &QuantChannel, lo: usize, hi: usize| {
        let e: f32 = spec[lo..hi].iter().map(|x| x * x).sum();
        let qmax = q.quant[lo..hi]
            .iter()
            .map(|v| v.unsigned_abs())
            .max()
            .unwrap_or(0);
        e / (qmax as f32 + 1.0)
    };
    let s0 = score(&q, lo0, hi0);
    let s1 = score(&q, lo1, hi1);
    assert!(s0 > s1, "loud band must score higher: {s0} vs {s1}");
    assert!(dpcm_ok(&q.sf, &q.coded, n));
}

/// The gain table is `det_math::exp2`, not libm, for every wire scalefactor.
#[test]
fn quant_gain_table_matches_det_math_for_every_scalefactor() {
    for sf in 0..256 {
        let want = crate::engine::det_math::exp2(-0.1875 * (sf - super::SF_OFFSET) as f32);
        assert_eq!(
            super::bits::quant_gain(sf).to_bits(),
            want.to_bits(),
            "sf {sf}"
        );
    }
    for sf in [-8, 256, 300] {
        let want = crate::engine::det_math::exp2(-0.1875 * (sf - super::SF_OFFSET) as f32);
        assert_eq!(super::bits::quant_gain(sf).to_bits(), want.to_bits());
    }
}

/// TASK-83: the one-pass cost table equals walking `spectral_bits` book by
/// book, for every magnitude class (LAV 1, 2, 4, 7, 12, escapes, clip).
#[test]
fn one_pass_fill_bits_matches_the_per_book_reference() {
    use super::{BOOKS, UNREPRESENTABLE};
    use crate::engine::enc_huff::spectral_bits;
    let reference = |vals: &[i32]| {
        let mut bits = [7u32; BOOKS];
        for (cb, slot) in bits.iter_mut().enumerate().skip(1) {
            let step = if cb <= 4 { 4 } else { 2 };
            let mut total = 0u32;
            for t in vals.chunks(step) {
                match spectral_bits(cb as u8, t) {
                    Some(n) => total = total.saturating_add(n as u32),
                    None => {
                        total = UNREPRESENTABLE;
                        break;
                    }
                }
            }
            *slot = total;
        }
        bits
    };
    let mut s = 0x1234_5678u32;
    let mut next = move || {
        s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        s >> 8
    };
    for &lim in &[
        0i32, 1, 2, 3, 4, 5, 7, 8, 12, 13, 15, 16, 17, 40, 1000, 8191,
    ] {
        for width in [4usize, 8, 12, 16, 32, 96] {
            for _ in 0..40 {
                let mut vals = vec![0i32; width];
                for v in &mut vals {
                    let m = (next() % (lim as u32 + 1)) as i32;
                    *v = if next() & 1 == 0 { m } else { -m };
                }
                if lim > 0 {
                    vals[(next() as usize) % width] = lim; // hit the class edge
                }
                let mut got = [7u32; BOOKS];
                super::bits::fill_bits(&mut got, &vals);
                assert_eq!(got, reference(&vals), "lim {lim} width {width} {vals:?}");
            }
        }
    }
}
