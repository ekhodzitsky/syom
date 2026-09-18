//! TASK-91: PS analysis on spatial reference signals — parameter ranges,
//! model reconstruction diagnostics, downmix behaviour, reset and
//! determinism.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{PS_BANDS, PS_BLOCK, PsAnalysis, PsEstimator, PsFrameParams};
use crate::engine::det_math;
use crate::engine::enc_sbr_qmf::EncAnalysisQmf;

const RATE: f32 = 48_000.0;
const IID_DB: [f64; 15] = [
    -25.0, -18.0, -14.0, -10.0, -7.0, -4.0, -2.0, 0.0, 2.0, 4.0, 7.0, 10.0, 14.0, 18.0, 25.0,
];

fn tone(f: u32, amp: f32, n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let cycles = (i as u64 * u64::from(f)) % 48_000;
            amp * det_math::sincos(cycles as f32 / RATE * std::f32::consts::TAU).0
        })
        .collect()
}

fn noise(seed: u32, amp: f32, n: usize) -> Vec<f32> {
    let mut s = 0x0BAD_F00Du32 ^ seed.wrapping_mul(0x9E37_79B9);
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

/// Multi-tone programme covering low, mid and high stereo bands.
fn programme(n: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    for f in [150u32, 420, 900, 1800, 3500, 7000, 12_000] {
        for (o, t) in v.iter_mut().zip(tone(f, 0.08, n)) {
            *o += t;
        }
    }
    v
}

struct Run {
    params: Vec<PsFrameParams>,
    mono: Vec<f32>,
    /// Source band energies (L, R) of the last frame.
    src: ([f64; PS_BANDS], [f64; PS_BANDS]),
    /// Mono band energy of the last frame.
    mono_e: [f64; PS_BANDS],
}

fn analyse(l: &[f32], r: &[f32]) -> Run {
    let mut ps = PsAnalysis::new();
    let mut params = Vec::new();
    let mut mono = Vec::new();
    for (bl, br) in l.chunks_exact(PS_BLOCK).zip(r.chunks_exact(PS_BLOCK)) {
        let mut m = [0.0f32; PS_BLOCK];
        if let Some(p) = ps
            .block(bl.try_into().unwrap(), br.try_into().unwrap(), &mut m)
            .unwrap()
        {
            params.push(p);
        }
        mono.extend_from_slice(&m);
    }
    // Band energies of the mono signal through the same analysis.
    let mut est = PsEstimator::new();
    let mut qmf = EncAnalysisQmf::new();
    let mut qmf2 = EncAnalysisQmf::new();
    for s in mono.chunks_exact(64) {
        let _ = est.push(qmf.push_slot(s).unwrap(), qmf2.push_slot(s).unwrap());
    }
    Run {
        params,
        mono,
        src: ps.est.last_energies(),
        mono_e: est.last_energies().0,
    }
}

/// PS model reconstruction: `e_L' = 2·e_M·c/(1+c)`, `e_R' = 2·e_M/(1+c)`
/// with `c = 10^(IID/10)`. Returns the worst |error| in dB over bands
/// holding at least 1% of the frame energy, for (L, R, L+R).
fn model_error_db(run: &Run) -> (f64, f64, f64) {
    let p = run.params.last().unwrap();
    let total: f64 = run.src.0.iter().chain(run.src.1.iter()).sum();
    let (mut wl, mut wr, mut ws) = (0.0f64, 0.0f64, 0.0f64);
    for b in 0..PS_BANDS {
        let (el, er) = (run.src.0[b], run.src.1[b]);
        if el + er < 0.01 * total {
            continue;
        }
        let c = 10f64.powf(IID_DB[(p.iid[b] + 7) as usize] / 10.0);
        let (pl, pr) = (
            2.0 * run.mono_e[b] * c / (1.0 + c),
            2.0 * run.mono_e[b] / (1.0 + c),
        );
        let db = |a: f64, b: f64| 10.0 * ((a + 1e-12) / (b + 1e-12)).log10();
        if el > 0.005 * total {
            wl = wl.max(db(pl, el).abs());
        }
        if er > 0.005 * total {
            wr = wr.max(db(pr, er).abs());
        }
        ws = ws.max(db(pl + pr, el + er).abs());
    }
    (wl, wr, ws)
}

fn active_bands(run: &Run) -> Vec<usize> {
    let total: f64 = run.src.0.iter().chain(run.src.1.iter()).sum();
    (0..PS_BANDS)
        .filter(|&b| run.src.0[b] + run.src.1[b] >= 0.01 * total)
        .collect()
}

fn scaled(s: &[f32], g: f32) -> Vec<f32> {
    s.iter().map(|x| g * x).collect()
}

#[test]
fn panned_tones_give_the_expected_iid_full_coherence_and_a_faithful_model() {
    let s = programme(8 * PS_BLOCK);
    // (right gain, expected coarse IID index): 0, 6.02, 13.98 dB, hard left.
    for (g, iid) in [(1.0f32, 0i8), (0.5, 3), (0.2, 5), (0.0, 7)] {
        let run = analyse(&s, &scaled(&s, g));
        let p = run.params.last().unwrap();
        for b in active_bands(&run) {
            assert_eq!(p.iid[b], iid, "gain {g} band {b}");
            assert_eq!(p.icc[b], 0, "gain {g} band {b}: in-phase is fully coherent");
        }
        let (el, er, sum) = model_error_db(&run);
        assert!(
            el <= 1.0 && er <= 1.0 && sum <= 0.1,
            "gain {g}: {el:.2} {er:.2} {sum:.2} dB"
        );
        // Mirror image: right louder gives the negated index.
        let mirrored = analyse(&scaled(&s, g), &s);
        for b in active_bands(&mirrored) {
            assert_eq!(mirrored.params.last().unwrap().iid[b], -iid);
        }
    }
}

#[test]
fn decorrelated_ambience_is_near_zero_coherence_and_keeps_its_energy() {
    let n = 8 * PS_BLOCK;
    let run = analyse(&noise(1, 0.3, n), &noise(2, 0.3, n));
    for p in &run.params[1..] {
        assert!(p.iid.iter().all(|&i| i.abs() <= 2), "{:?}", p.iid);
        assert!(p.icc.iter().all(|&c| (3..=6).contains(&c)), "{:?}", p.icc);
    }
    // The passive downmix would lose 3 dB here; the compensated one keeps
    // the total within 1 dB and every band within 4 dB.
    let (src, mono): (f64, f64) = (
        run.src.0.iter().chain(run.src.1.iter()).sum(),
        run.mono_e.iter().sum::<f64>() * 2.0,
    );
    assert!(
        (10.0 * (mono / src).log10()).abs() <= 1.0,
        "total {mono} vs {src}"
    );
    let (el, er, sum) = model_error_db(&run);
    assert!(
        el <= 4.0 && er <= 4.0 && sum <= 4.0,
        "{el:.2} {er:.2} {sum:.2}"
    );
}

#[test]
fn anti_phase_is_reported_and_is_the_irreversible_downmix_loss() {
    let s = programme(8 * PS_BLOCK);
    let run = analyse(&s, &scaled(&s, -1.0));
    let p = run.params.last().unwrap();
    for b in active_bands(&run) {
        assert_eq!((p.iid[b], p.icc[b]), (0, 7), "band {b}: ICC = −1");
    }
    assert!(
        run.mono.iter().all(|&m| m.abs() < 1e-6),
        "the mono core is empty"
    );
    // A partial inversion (R = −0.5·L) survives with compensation.
    let part = analyse(&s, &scaled(&s, -0.5));
    assert!(part.mono.iter().any(|&m| m.abs() > 0.01));
    assert!(
        active_bands(&part)
            .iter()
            .all(|&b| part.params.last().unwrap().icc[b] == 7)
    );
}

#[test]
fn natural_stereo_stays_in_range_without_clipping_and_is_stable() {
    let dec = crate::decode_with(
        include_bytes!("../goldens/lecture.m4a"),
        &crate::DecodeOptions::unbounded(),
    )
    .unwrap();
    let peak = dec
        .channels
        .iter()
        .flatten()
        .fold(0.0f32, |m, &x| m.max(x.abs()));
    // Loud, then widened: R gets an independent noise bed.
    let n = dec.channels[0].len() / PS_BLOCK * PS_BLOCK;
    let l = scaled(&dec.channels[0][..n], 0.9 / peak);
    let bed = noise(7, 0.2, n);
    let r: Vec<f32> = dec.channels[1][..n]
        .iter()
        .zip(bed.iter())
        .map(|(x, b)| (0.9 / peak * x + b).clamp(-1.0, 1.0))
        .collect();
    let run = analyse(&l, &r);
    assert!(run.params.len() >= 3);
    assert!(run.mono.iter().all(|m| m.abs() <= 1.0), "downmix headroom");
    for w in run.params.windows(2) {
        assert!(w[1].iid.iter().all(|i| (-7..=7).contains(i)));
        assert!(w[1].icc.iter().all(|&c| c <= 7));
        let jump = (0..PS_BANDS)
            .map(|b| (w[1].iid[b] - w[0].iid[b]).abs())
            .max()
            .unwrap();
        assert!(jump <= 6, "IID jumped {jump} steps between frames");
    }
}

#[test]
fn reset_repeats_the_run_and_the_parameters_are_pinned() {
    let n = 6 * PS_BLOCK;
    let l: Vec<f32> = programme(n)
        .iter()
        .zip(noise(3, 0.1, n))
        .map(|(a, b)| a + b)
        .collect();
    let r: Vec<f32> = programme(n)
        .iter()
        .zip(noise(4, 0.1, n))
        .map(|(a, b)| 0.5 * a + b)
        .collect();
    let mut ps = PsAnalysis::new();
    let go = |ps: &mut PsAnalysis| {
        let mut h = 0xcbf2_9ce4_8422_2325u64;
        for (bl, br) in l.chunks_exact(PS_BLOCK).zip(r.chunks_exact(PS_BLOCK)) {
            let mut m = [0.0f32; PS_BLOCK];
            let p = ps
                .block(bl.try_into().unwrap(), br.try_into().unwrap(), &mut m)
                .unwrap();
            for v in p
                .iter()
                .flat_map(|p| p.iid.iter().map(|&i| i as u8).chain(p.icc.iter().copied()))
            {
                h = (h ^ u64::from(v)).wrapping_mul(0x0000_0100_0000_01b3);
            }
            for v in m {
                h = (h ^ u64::from(v.to_bits())).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    };
    let first = go(&mut ps);
    ps.reset();
    assert_eq!(go(&mut ps), first, "reset restores the initial state");
    // Cross-architecture tripwire: det_math only, fixed summation order.
    assert_eq!(first, PINNED, "parameter / downmix hash {first:#x}");
}

const PINNED: u64 = 0x5751_0735_3ea9_c66e;

#[test]
#[ignore = "prints CPU / workspace numbers for lab/quality/PS_EST.md"]
fn cost_report() {
    let n = 48 * PS_BLOCK;
    let (l, r) = (noise(1, 0.3, n), noise(2, 0.3, n));
    let mut ps = PsAnalysis::new();
    let t = std::time::Instant::now();
    for (bl, br) in l.chunks_exact(PS_BLOCK).zip(r.chunks_exact(PS_BLOCK)) {
        let mut m = [0.0f32; PS_BLOCK];
        let _ = ps
            .block(bl.try_into().unwrap(), br.try_into().unwrap(), &mut m)
            .unwrap();
    }
    let secs = t.elapsed().as_secs_f64();
    println!(
        "PS analysis: {:.1} us per 2048-sample block, {:.0}x realtime, state {} bytes",
        secs * 1e6 / 48.0,
        (n as f64 / 48_000.0) / secs,
        std::mem::size_of::<PsAnalysis>()
            + 13 * 2 * std::mem::size_of::<crate::engine::enc_sbr_qmf::EncSlot>()
    );
}
