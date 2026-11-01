//! Independent quantizer-cache reference checks.
#![allow(clippy::unwrap_used)]
use super::*;

fn check(
    cache: &mut BandReuse,
    spec: &[f32; 1024],
    offsets: &[u16],
    cached: &mut QuantChannel,
    reference: &mut QuantChannel,
) {
    cache.quantize(spec, offsets, cached);
    quantize_cached(spec, offsets, reference);
    assert_eq!(cached.quant, reference.quant);
    assert_eq!(cached.bits, reference.bits);
    assert_eq!(cached.zero, reference.zero);
    assert_eq!(cached.sf, reference.sf);
    assert_eq!(cached.coded, reference.coded);
}

#[test]
fn reuse_matches_uncached_full_arrays_across_frames_and_band_transitions() {
    for rate in [0, 3, 4, 8, 11] {
        let offsets = crate::engine::swb::long_offsets(rate).unwrap();
        let n = offsets.len() - 1;
        let mut cache = BandReuse::new();
        let mut cached = Box::new(QuantChannel::new(n));
        let mut reference = cached.clone();
        for frame in 0..4 {
            let mut spec = [0.0; 1024];
            for (i, x) in spec.iter_mut().enumerate() {
                *x = (i as f32 * (0.13 + frame as f32 * 0.07)).sin() * (i % 19) as f32 * 500.0;
            }
            cache.reset();
            cache_mags(&spec, &mut cached.mag);
            reference.mag = cached.mag;
            for pass in 0..40 {
                for b in 0..n {
                    let coded = (b + pass / 3) % 5 != 0;
                    let sf = 70 + (b * 7 + pass / 2) as i32 % 160;
                    cached.coded[b] = coded;
                    reference.coded[b] = coded;
                    cached.sf[b] = sf;
                    reference.sf[b] = sf;
                }
                // Mimic the PNS/IS flag stamps: they change neither bins nor costs.
                cached.pns.fill(pass % 2 == 0);
                reference.pns = cached.pns;
                cached.intensity.fill(pass % 3 == 0);
                reference.intensity = cached.intensity;
                check(&mut cache, &spec, offsets, &mut cached, &mut reference);
                // Repeating exactly the same normalized sf must also preserve
                // all arrays, including uncoded bands and the book-0 slot.
                check(&mut cache, &spec, offsets, &mut cached, &mut reference);
            }
            // Inactive cost/zero slots retain their old values, while ALL bins
            // outside the active prefix must become zero just as in the oracle.
            for bands in [n / 2, n] {
                cached.n_bands = bands;
                reference.n_bands = bands;
                check(&mut cache, &spec, offsets, &mut cached, &mut reference);
            }
        }
    }
}

#[test]
fn external_band_mutations_are_invalidated_for_accepted_and_reverted_refinement() {
    let offsets = crate::engine::swb::long_offsets(3).unwrap();
    let n = offsets.len() - 1;
    let spec = [400.0; 1024];
    let mut cache = BandReuse::new();
    let mut cached = QuantChannel::new(n);
    cached.coded.fill(true);
    cached.sf.fill(110);
    cache_mags(&spec, &mut cached.mag);
    let mut reference = cached.clone();
    check(&mut cache, &spec, offsets, &mut cached, &mut reference);
    for reverted in [false, true] {
        for band in 0..n {
            cache.invalidate(band);
            let snapshot = cached.clone();
            cached.sf[band] -= 1;
            requant_band(&spec, offsets, &mut cached, band);
            if reverted {
                cached.quant = snapshot.quant;
                cached.bits = snapshot.bits;
                cached.sf = snapshot.sf;
                // Deliberately leave zero as an external mutation may do;
                // the next quantization must compute it afresh.
            }
            // The subsequent rate build re-derives sf rather than using the
            // external refinement's modified sf/quantized state.
            cached.sf = reference.sf;
            check(&mut cache, &spec, offsets, &mut cached, &mut reference);
        }
    }
}
