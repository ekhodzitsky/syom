//! Compare cached rate probes to the original uncached short-window path.
#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn cached_short_analysis_matches_uncached_rate_probes() {
    eprintln!(
        "short_alloc_bytes={}",
        std::mem::size_of::<[ShortAlloc; 2]>()
    );
    for fs_index in [0, 3, 4, 8, 11] {
        let rate = crate::engine::adts::ADTS_SAMPLE_RATES_HZ[fs_index];
        let offsets = crate::engine::swb::short_offsets(fs_index as u8).unwrap();
        let n_sfb = offsets.len() - 1;
        let mut cached = Box::new(QuantShort::new(n_sfb));
        let mut uncached = Box::new(QuantShort::new(n_sfb));
        let mut cache = ShortAlloc::new();
        let mut psy_cached = Psy::new_short(offsets, rate);
        let mut psy_ref = Psy::new_short(offsets, rate);
        for frame in 0..3 {
            let mut spec = [0.0; LONG_WINDOW_LEN];
            for (i, x) in spec.iter_mut().enumerate() {
                *x = (i as f32 * (0.13 + frame as f32 * 0.07)).sin() * (i % 17 + 1) as f32 * 30.0;
            }
            let grouping = [
                Grouping::from_bits(0),
                Grouping::from_bits(127),
                Grouping::from_bits(85),
            ][frame];
            let mut tns = EncTns::off();
            if frame == 1 {
                for (i, x) in spec.iter_mut().enumerate() {
                    let t = (i % SHORT_WINDOW_LEN) as f32 / SHORT_WINDOW_LEN as f32;
                    *x = 400.0 * (2.0 * std::f32::consts::PI * 2.0 * t).sin()
                        + 120.0 * (2.0 * std::f32::consts::PI * 5.0 * t).sin();
                }
                tns.set_short(crate::engine::enc_tns::decide_short(
                    &mut spec,
                    offsets,
                    fs_index as u8,
                ));
                if fs_index == 3 {
                    assert!(
                        tns.short().is_on(),
                        "reference must exercise active short TNS"
                    );
                }
            }
            psy_cached.enable_ath(frame == 1);
            psy_ref.enable_ath(frame == 1);
            psy_cached.enable_tonality(frame == 2);
            psy_ref.enable_tonality(frame == 2);
            enc_quant::cache_mags(&spec, &mut cached.mag);
            uncached.mag = cached.mag;
            cache.prepare(&mut psy_cached, offsets, &spec, n_sfb, &tns, grouping);
            let (mut tq_c, mut tq_r) = ([0.0; MAX_FLAT_SHORT], [0.0; MAX_FLAT_SHORT]);
            for offset in [-60, 0, 20, 21, 20, 80, 240, -60] {
                let (mut books_c, mut books_r) = ([0; MAX_FLAT_SHORT], [0; MAX_FLAT_SHORT]);
                let (mut gain_c, mut gain_r) = (0, 0);
                let got = channel_build(
                    &cache,
                    offsets,
                    &spec,
                    offset,
                    &mut cached,
                    &mut tq_c,
                    &mut books_c,
                    &mut gain_c,
                    true,
                    &tns,
                );
                let want = reference_channel_build(
                    &mut psy_ref,
                    offsets,
                    &spec,
                    offset,
                    &mut uncached,
                    &mut tq_r,
                    &mut books_r,
                    &mut gain_r,
                    true,
                    &tns,
                    grouping,
                );
                assert_eq!(got, want, "rate={rate} frame={frame} offset={offset}");
                assert_eq!(cached.quant, uncached.quant);
                assert_eq!(cached.coded, uncached.coded);
                assert_eq!(cached.sf, uncached.sf);
                assert_eq!(cached.bits, uncached.bits);
                assert_eq!(books_c, books_r);
                assert_eq!(gain_c, gain_r);
                let mut w_c = BitWriter::new();
                let mut w_r = BitWriter::new();
                emit_channel_body_short(&mut w_c, offsets, &books_c, &cached, gain_c, true, &tns);
                emit_channel_body_short(&mut w_r, offsets, &books_r, &uncached, gain_r, true, &tns);
                assert_eq!(w_c.finish(), w_r.finish());
            }
        }
    }
}

/// Quantize + plan one short channel at `offset`; returns its body bits.
/// Psy scores each window; [`Grouping`] then folds windows into groups.
#[allow(clippy::too_many_arguments)]
fn reference_channel_build(
    psy: &mut Psy,
    offsets: &[u16],
    spec: &[f32; LONG_WINDOW_LEN],
    offset: i32,
    q: &mut QuantShort,
    tq: &mut [f32; MAX_FLAT_SHORT],
    books: &mut [u8; MAX_FLAT_SHORT],
    gain: &mut u8,
    standalone: bool,
    tns: &EncTns,
    grouping: Grouping,
) -> usize {
    let n_sfb = q.n_sfb;
    let mut win_coded = [false; MAX_FLAT_SHORT];
    let mut win_tq = [0.0f32; MAX_FLAT_SHORT];
    let mut win_forced = [false; MAX_FLAT_SHORT];
    // Per-window energy / allowed noise / Σ√|x| for the noise targets.
    let mut win_e = [0.0f32; MAX_FLAT_SHORT];
    let mut win_n = [0.0f32; MAX_FLAT_SHORT];
    let mut win_s = [0.0f32; MAX_FLAT_SHORT];
    for w in 0..MAX_GROUPS {
        let (lo, hi) = (w * n_sfb, (w + 1) * n_sfb);
        psy.analyze(
            &spec[w * SHORT_WINDOW_LEN..(w + 1) * SHORT_WINDOW_LEN],
            offsets,
            crate::engine::enc_frame::TARGET_Q,
            &mut win_coded[lo..hi],
            &mut win_tq[lo..hi],
        );
        let (e, n, s) = psy.bands();
        win_e[lo..hi].copy_from_slice(&e[..n_sfb]);
        win_n[lo..hi].copy_from_slice(&n[..n_sfb]);
        win_s[lo..hi].copy_from_slice(&s[..n_sfb]);
        if tns.short().window_on(w) {
            for b in 0..n_sfb.min(14) {
                win_forced[lo + b] = !win_coded[lo + b];
                win_coded[lo + b] = true;
                win_tq[lo + b] = crate::engine::enc_frame::TARGET_Q;
            }
        }
    }
    let mut peaks = [0.0f32; MAX_FLAT_SHORT];
    enc_quant::band_peaks_short(spec, offsets, n_sfb, &mut peaks);
    q.set_grouping(grouping);
    let mut gpeaks = [0.0f32; MAX_FLAT_SHORT];
    enc_group::fold_windows(
        grouping,
        n_sfb,
        &win_coded,
        &win_tq,
        &peaks,
        &mut q.coded,
        tq,
        &mut gpeaks,
    );
    // Fold energy / noise / Σ√|x| (sums) and the TNS forcing (any) per group.
    let n = q.n_groups * n_sfb;
    let (mut ge, mut gn, mut gs) = (
        [0.0f32; MAX_FLAT_SHORT],
        [0.0f32; MAX_FLAT_SHORT],
        [0.0f32; MAX_FLAT_SHORT],
    );
    let mut gw = [0.0f32; MAX_FLAT_SHORT];
    let mut gforced = [false; MAX_FLAT_SHORT];
    let mut wbase = 0usize;
    for g in 0..q.n_groups {
        for k in 0..q.group_len[g] as usize {
            for b in 0..n_sfb {
                let (o, i) = (g * n_sfb + b, (wbase + k) * n_sfb + b);
                ge[o] += win_e[i];
                gn[o] += win_n[i];
                gs[o] += win_s[i];
                gw[o] += f32::from(offsets[b + 1] - offsets[b]);
                gforced[o] |= win_forced[i];
            }
        }
        wbase += q.group_len[g] as usize;
    }
    let cap = *tq;
    crate::engine::enc_alloc::noise_targets(
        &gpeaks,
        &ge,
        &gs,
        &gn,
        &gw,
        &cap,
        offset,
        n,
        &mut q.coded,
        tq,
    );
    for o in 0..n {
        if gforced[o] && !q.coded[o] && gpeaks[o] > 0.0 {
            q.coded[o] = true;
            tq[o] = 1.0;
        }
    }
    enc_quant::raw_scalefactors_short(&gpeaks, tq, 0, q);
    *gain = enc_quant::normalize_sf_short(q);
    enc_quant::quantize_short_cached(spec, offsets, q);
    *books = plan_books_short(q);
    channel_body_bits_short(books, q, *gain, standalone, tns)
}
