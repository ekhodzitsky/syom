//! TASK-87: SBR parameter estimation — invariants, determinism, bits.
//! The reference reconstruction lives in `enc_sbr_est_recon_tests.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{SLOTS, SbrEstimator, SbrFrameParams, he_header};
use crate::engine::enc_sbr_prep::{OUT_FRAME, SbrPrep};
use crate::engine::enc_sbr_qmf::{BANDS, EncSlot};
use crate::engine::sbr_reconstruct::{EnvelopeScalefactors, NoiseScalefactors};

pub(super) const SR: f32 = 48_000.0;
/// Envelope slot `t` of decoder frame `n` reads `XHigh` column
/// `2t + tHFAdj` where the frame's own analysis starts at column
/// `tHFGen = 8`: the grid begins 6 analysis slots before the frame.
pub(super) const GRID_LEAD: usize = 6;

pub(super) fn zero_slot() -> EncSlot {
    EncSlot {
        re: [0.0; BANDS],
        im: [0.0; BANDS],
    }
}

pub(super) fn analyse(pcm: &[f32]) -> (Vec<f32>, Vec<EncSlot>) {
    let mut prep = SbrPrep::new();
    let mut core = Vec::new();
    let mut slots = Vec::new();
    prep.push(pcm, &mut core, |s| slots.push(*s)).unwrap();
    prep.finish(&mut core, |s| slots.push(*s)).unwrap();
    (core, slots)
}

/// Frame `n` of the estimator input: slots `[32n − 6, 32n + 26)`.
pub(super) fn frames(slots: &[EncSlot]) -> Vec<Vec<EncSlot>> {
    let mut all = vec![zero_slot(); GRID_LEAD];
    all.extend_from_slice(slots);
    all.chunks_exact(SLOTS).map(<[EncSlot]>::to_vec).collect()
}

pub(super) fn lcg(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) - 0.5
}

/// Zero-phase band-pass by FFT-free means: sum of many random-phase
/// tones is avoided; instead filter white noise with a long windowed
/// sinc so the band edges are ≥ 60 dB steep.
pub(super) fn band_noise(n: usize, lo: f32, hi: f32, amp: f32, seed: u32) -> Vec<f32> {
    let mut s = seed;
    let white: Vec<f32> = (0..n + 512).map(|_| lcg(&mut s)).collect();
    let taps = 511usize;
    let mid = (taps / 2) as f32;
    let h: Vec<f32> = (0..taps)
        .map(|i| {
            let t = i as f32 - mid;
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (taps - 1) as f32).cos();
            let sinc = |f: f32| {
                if t == 0.0 {
                    2.0 * f / SR
                } else {
                    (2.0 * std::f32::consts::PI * f * t / SR).sin() / (std::f32::consts::PI * t)
                }
            };
            (sinc(hi) - sinc(lo)) * w
        })
        .collect();
    (0..n)
        .map(|i| {
            amp * 3.0
                * h.iter()
                    .enumerate()
                    .map(|(j, &c)| c * white[i + j])
                    .sum::<f32>()
        })
        .collect()
}

/// f64 phase, wrapped per sample: an f32 argument would add a phase-jitter
/// noise floor that rises with time.
pub(super) fn tone(freq: f32, n: usize, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let ph = (f64::from(freq) * i as f64 / f64::from(SR)).fract();
            amp * (2.0 * std::f64::consts::PI * ph).sin() as f32
        })
        .collect()
}

pub(super) fn add(a: &mut [f32], b: &[f32]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += *y;
    }
}

pub(super) fn run(pcm: &[f32]) -> Vec<SbrFrameParams> {
    let mut est = SbrEstimator::new(48_000).unwrap();
    frames(&analyse(pcm).1)
        .iter()
        .map(|f| est.estimate(f).unwrap())
        .collect()
}

#[test]
fn header_and_band_tables_are_pinned_per_rate() {
    // (fs_sbr, k0, k2, n_high, n_low, n_q) — determinism tripwire for
    // the libm-derived master table.
    let expect = [
        (16_000u32, 28, 60, 10, 5, 2),
        (22_050, 24, 52, 12, 6, 2),
        (24_000, 25, 52, 10, 5, 2),
        (32_000, 25, 52, 10, 5, 2),
        (44_100, 19, 43, 12, 6, 2),
        (48_000, 18, 41, 12, 6, 2),
    ];
    let got: Vec<_> = expect
        .iter()
        .map(|&(fs, ..)| {
            let est = SbrEstimator::new(fs).unwrap();
            let b = est.bands();
            assert_eq!(est.header().start_freq, he_header(fs).unwrap().start_freq);
            assert!(b.k_x <= 32, "{fs}: k_x {} must sit inside the core", b.k_x);
            (fs, b.k_x, b.k_x + b.m, b.n_high(), b.n_low(), b.n_q())
        })
        .collect();
    assert_eq!(got, expect.to_vec());
    assert!(he_header(64_000).is_err());
    assert!(SbrEstimator::new(8_000).is_err());
}

#[test]
fn silence_is_one_envelope_at_the_floor() {
    let mut est = SbrEstimator::new(48_000).unwrap();
    let f = vec![zero_slot(); SLOTS];
    let p = est.estimate(&f).unwrap();
    assert_eq!(p.grid.num_env, 1);
    assert!(p.grid.freq_res[0] && p.grid.amp_res_override);
    assert!(p.env_q[0].iter().all(|&e| e == 0), "{:?}", p.env_q);
    assert_eq!(p.noise_q.len(), 1);
    assert!(p.add_harmonic.is_empty());
    assert!(p.est_bits < 100, "bits {}", p.est_bits);
    assert!(est.estimate(&f[..31]).is_err(), "31 slots rejected");
}

#[test]
fn hf_tone_over_lf_noise_is_tonal_floor_no_invf() {
    let n = OUT_FRAME * 8;
    let mut pcm = band_noise(n, 200.0, 5_500.0, 0.2, 7);
    add(&mut pcm, &tone(12_000.0, n, 0.3));
    let ps = run(&pcm);
    let p = &ps[4];
    assert_eq!(p.grid.num_env, 1, "stationary → one envelope");
    // 12 kHz = band 32 → the noise band holding it stays tonal.
    let est = SbrEstimator::new(48_000).unwrap();
    let nb = est
        .bands()
        .f_table_noise
        .windows(2)
        .position(|w| w[0] <= 32 && 32 < w[1])
        .unwrap();
    assert!(p.noise_q[0][nb] >= 25, "Q {:?}", p.noise_q);
    assert_eq!(p.invf.invf_mode[nb], 0);
    // The tone's band is the loudest envelope band, ≥ 30 dB (20 steps
    // of 1.5 dB) over the quiet HF bands beside it.
    let hb = est
        .bands()
        .f_table_high
        .windows(2)
        .position(|w| w[0] <= 32 && 32 < w[1])
        .unwrap();
    let e = &p.env_q[0];
    let others = e
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != hb)
        .map(|(_, &v)| v)
        .max()
        .unwrap();
    assert!(e[hb] >= others + 20, "env {:?}", e);
}

#[test]
fn hf_noise_over_lf_harmonics_adds_noise_floor_and_invf() {
    let n = OUT_FRAME * 8;
    let mut pcm = band_noise(n, 8_000.0, 15_000.0, 0.15, 11);
    for h in 1..=11 {
        add(&mut pcm, &tone(500.0 * h as f32, n, 0.05));
    }
    let p = &run(&pcm)[4];
    assert_eq!(p.grid.num_env, 1);
    for (nb, &q) in p.noise_q[0].iter().enumerate() {
        assert!(
            q <= 8,
            "noise band {nb} Q {q} should be noisy: {:?}",
            p.noise_q
        );
        assert!(p.invf.invf_mode[nb] >= 2, "invf {:?}", p.invf.invf_mode);
    }
}

#[test]
fn white_noise_needs_no_added_noise_floor() {
    let n = OUT_FRAME * 8;
    let mut s = 3u32;
    let pcm: Vec<f32> = (0..n).map(|_| 0.3 * lcg(&mut s)).collect();
    let p = &run(&pcm)[4];
    assert!(p.noise_q[0].iter().all(|&q| q >= 20), "{:?}", p.noise_q);
    assert!(p.invf.invf_mode.iter().all(|&m| m == 0));
}

#[test]
fn hf_burst_selects_four_envelopes_without_smearing() {
    let n = OUT_FRAME * 6;
    let mut pcm = band_noise(n, 200.0, 5_500.0, 0.1, 5);
    let burst = band_noise(n, 8_000.0, 15_000.0, 0.3, 9);
    let at = OUT_FRAME * 3 + 64 * (20 - GRID_LEAD);
    pcm[at..]
        .iter_mut()
        .zip(&burst[at..])
        .for_each(|(x, b)| *x += *b);
    let ps = run(&pcm);
    let p = &ps[3];
    assert_eq!(
        p.grid.num_env, 4,
        "burst at slot 20 of frame 3: {:?}",
        p.grid
    );
    assert!(
        !p.grid.freq_res[0],
        "transient frames use the low resolution"
    );
    assert_eq!(p.grid.num_noise, 2);
    let quiet = *p.env_q[1].iter().max().unwrap();
    let loud = *p.env_q[2].iter().max().unwrap();
    assert!(loud >= quiet + 8, "3 dB steps: quiet {quiet} loud {loud}");
    assert_eq!(
        ps[5].grid.num_env, 1,
        "steady burst returns to one envelope"
    );
}

#[test]
fn deltas_round_trip_through_the_decoder_dpcm_and_time_direction_saves_bits() {
    let n = OUT_FRAME * 6;
    let mut pcm = band_noise(n, 200.0, 5_500.0, 0.2, 21);
    add(&mut pcm, &band_noise(n, 7_000.0, 15_000.0, 0.1, 22));
    let ps = run(&pcm);
    let est = SbrEstimator::new(48_000).unwrap();
    let bands = est.bands();
    let mut prev: Option<EnvelopeScalefactors> = None;
    let mut prev_n: Option<NoiseScalefactors> = None;
    let mut time_used = false;
    for p in &ps {
        let mut env = EnvelopeScalefactors {
            eq: Vec::new(),
            freq_res: Vec::new(),
        };
        env.reconstruct_into(
            &p.envelope,
            &p.grid,
            &p.dtdf,
            bands,
            false,
            false,
            prev.as_ref(),
        )
        .unwrap();
        assert_eq!(env.eq, p.env_q, "envelope DPCM round trip");
        let mut noise = NoiseScalefactors { q: Vec::new() };
        noise
            .reconstruct_into(
                &p.noise,
                &p.grid,
                &p.dtdf,
                bands.n_q(),
                false,
                false,
                prev_n.as_ref(),
            )
            .unwrap();
        assert_eq!(noise.q, p.noise_q, "noise DPCM round trip");
        time_used |= p.dtdf.df_env.iter().any(|&t| t);
        prev = Some(env);
        prev_n = Some(noise);
    }
    assert!(
        time_used,
        "stationary frames should pick the time direction"
    );
    assert!(!ps[0].dtdf.df_env[0], "first frame has no reference");
    assert!(
        ps[3].est_bits <= ps[0].est_bits,
        "{} vs {}",
        ps[3].est_bits,
        ps[0].est_bits
    );
}

#[test]
fn reset_and_repeat_are_identical() {
    let n = OUT_FRAME * 4;
    let mut pcm = band_noise(n, 200.0, 5_500.0, 0.2, 31);
    add(&mut pcm, &tone(11_000.0, n, 0.2));
    let fr = frames(&analyse(&pcm).1);
    let mut est = SbrEstimator::new(48_000).unwrap();
    let a: Vec<_> = fr.iter().map(|f| est.estimate(f).unwrap()).collect();
    est.reset();
    let b: Vec<_> = fr.iter().map(|f| est.estimate(f).unwrap()).collect();
    assert_eq!(a, b);
    assert_ne!(
        est.estimate(&fr[1]).unwrap(),
        a[1],
        "history matters (no reset)"
    );
}

#[test]
fn bit_estimate_fits_the_he_side_budget() {
    // HE_ENC.md: ~25% of 24 kbps stereo for SBR = 128 bits per
    // channel-frame at 23.4 frames/s.
    let n = OUT_FRAME * 8;
    let mut pcm = band_noise(n, 200.0, 5_500.0, 0.2, 41);
    add(&mut pcm, &band_noise(n, 7_000.0, 15_000.0, 0.05, 42));
    for h in 1..=6 {
        add(&mut pcm, &tone(700.0 * h as f32, n, 0.04));
    }
    let ps = run(&pcm);
    let steady: Vec<u32> = ps[2..].iter().map(|p| p.est_bits).collect();
    let mean = steady.iter().sum::<u32>() as f32 / steady.len() as f32;
    assert!(mean <= 128.0, "mean {mean} bits/frame: {steady:?}");
}

#[test]
fn no_libm_on_the_decision_path() {
    let src = include_str!("enc_sbr_est.rs");
    for pat in [
        "powf(", ".sin()", ".cos()", ".ln()", ".log2()", ".exp2()", "f32::sin",
    ] {
        assert!(!src.contains(pat), "{pat}");
    }
    assert!(src.contains("det_math::log2"));
}
