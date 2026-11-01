//! Per-frame invariant math must preserve the original rate-loop decisions.
use super::*;
use crate::engine::enc_quant::{QuantChannel, raw_scalefactors};

fn random(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *seed
}

#[test]
fn cached_targets_and_scalefactors_match_original_all_offsets() {
    let mut seed = 413;
    let mut a = AllocCache::new();
    eprintln!(
        "long_alloc_bytes={}",
        std::mem::size_of::<[AllocCache; 2]>()
    );
    for frame in 0..40 {
        let n = 1 + random(&mut seed) as usize % MAX_BANDS;
        for b in 0..n {
            a.coded[b] = !random(&mut seed).is_multiple_of(4);
            a.peaks[b] = if b % 11 == 0 {
                0.0
            } else if b % 13 == 0 {
                f32::from_bits(1 + random(&mut seed) % 0x007f_ffff)
            } else {
                0.01 + (random(&mut seed) % 1_000_000) as f32
            };
            a.width[b] = (random(&mut seed) % 100) as f32;
            a.energy[b] = (random(&mut seed) % 100_000) as f32;
            a.noise[b] = if b % 9 == 0 {
                0.0
            } else {
                (random(&mut seed) % 10_000) as f32 * 0.00001
            };
            a.sum_sqrt[b] = (random(&mut seed) % 10_000) as f32 * 0.01;
            a.cap[b] = (random(&mut seed) % 8000) as f32;
        }
        a.finish_invariants(n);
        for offset in -60..=240 {
            let (mut cached_mask, mut reference_mask) = (a.coded, a.coded);
            let (mut cached_tq, mut reference_tq) = ([0.0; MAX_BANDS], [0.0; MAX_BANDS]);
            a.targets(offset, n, &mut cached_mask, &mut cached_tq);
            noise_targets(
                &a.peaks,
                &a.energy,
                &a.sum_sqrt,
                &a.noise,
                &a.width,
                &a.cap,
                offset,
                n,
                &mut reference_mask,
                &mut reference_tq,
            );
            assert_eq!(cached_mask, reference_mask, "frame={frame} offset={offset}");
            assert_eq!(cached_tq.map(f32::to_bits), reference_tq.map(f32::to_bits));
            let mut cached = QuantChannel::new(n);
            cached.coded = cached_mask;
            let mut reference = cached.clone();
            a.raw_scalefactors(&cached_tq, &mut cached);
            raw_scalefactors(&a.peaks, &reference_tq, 0, &mut reference);
            assert_eq!(cached.sf, reference.sf, "frame={frame} offset={offset}");
        }
    }
}

#[test]
fn cached_scalefactors_match_boundary_targets() {
    let mut a = AllocCache::new();
    a.coded.fill(true);
    for (b, peak) in a.peaks.iter_mut().enumerate() {
        *peak = match b % 5 {
            0 => 0.0,
            1 => f32::MIN_POSITIVE,
            2 => f32::from_bits(1),
            3 => 1.0,
            _ => 3e7,
        };
    }
    a.finish_invariants(MAX_BANDS);
    for target in [0.0, 0.99, 1.0, 1.01, 2048.0, 7600.0, 8191.0, f32::MAX] {
        let tq = [target; MAX_BANDS];
        let mut cached = QuantChannel::new(MAX_BANDS);
        cached.coded = a.coded;
        let mut reference = cached.clone();
        a.raw_scalefactors(&tq, &mut cached);
        raw_scalefactors(&a.peaks, &tq, 0, &mut reference);
        assert_eq!(cached.sf, reference.sf, "target={target}");
    }
}
