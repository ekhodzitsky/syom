//! TASK-87: decode the estimated SBR parameters with the in-tree SBR
//! decoder over the ideal (halfband) core and compare HF energies.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::SbrEstimator;
use super::enc_sbr_est_tests::{add, analyse, band_noise, frames, lcg, run, tone};
use crate::engine::enc_sbr_prep::OUT_FRAME;
use crate::engine::enc_sbr_qmf::{BANDS, EncAnalysisQmf};
use crate::engine::sbr_decoder::SbrDecoder;
use crate::engine::sbr_element::{SbrChannel, SbrElement};
use crate::engine::sbr_extension::SbrExtensionData;

/// Decode the estimated parameters with the in-tree SBR decoder over
/// the ideal (halfband) core and return per-band HF energies of the
/// original and the reconstruction plus the HF energy per 64-sample
/// block of both.
fn reconstruct(pcm: &[f32]) -> ([f64; BANDS], [f64; BANDS], Vec<f32>, Vec<f32>) {
    let (core, slots) = analyse(pcm);
    let fr = frames(&slots);
    let mut est = SbrEstimator::new(48_000, 24).unwrap();
    let mut dec = SbrDecoder::new(48_000, 1).unwrap();
    let mut out = Vec::new();
    for (n, f) in fr.iter().enumerate() {
        let p = est.estimate(f).unwrap();
        let ext = SbrExtensionData {
            crc: None,
            header_present: true,
            header: *est.header(),
            element: SbrElement {
                coupling: false,
                channels: vec![SbrChannel {
                    grid: p.grid,
                    dtdf: p.dtdf,
                    invf: p.invf,
                    envelope: p.envelope,
                    noise: p.noise,
                    add_harmonic: p.add_harmonic,
                }],
                extension: None,
            },
            num_sbr_bits: 0,
        };
        let start = n * 1024;
        if start + 1024 > core.len() {
            break;
        }
        let frame: Vec<f64> = core[start..start + 1024]
            .iter()
            .map(|&x| f64::from(x) * 32768.0)
            .collect();
        let o = dec.process_frame(&ext, &[&frame]).unwrap();
        out.extend(o[0].iter().map(|&v| (v / 32768.0) as f32));
    }
    let kx = est.bands().k_x as usize;
    let hf = |x: &[f32]| -> Vec<f32> {
        let mut q = EncAnalysisQmf::new();
        x.chunks_exact(64)
            .map(|c| q.push_slot(c).unwrap().band_energy(kx, BANDS))
            .collect()
    };
    // The bank runs from sample 0 (a cold start mid-signal would splatter
    // one transient slot across every band); only the sums skip.
    let bands = |x: &[f32], skip: usize| -> [f64; BANDS] {
        let mut q = EncAnalysisQmf::new();
        let mut e = [0.0f64; BANDS];
        for (i, c) in x.chunks_exact(64).enumerate() {
            let s = q.push_slot(c).unwrap();
            if i >= skip {
                for (k, v) in e.iter_mut().enumerate() {
                    *v += f64::from(s.energy(k));
                }
            }
        }
        e
    };
    let n = out.len().min(pcm.len());
    (
        bands(&pcm[..n], 96),
        bands(&out[..n], 96),
        hf(pcm),
        hf(&out),
    )
}

fn db(a: f64, b: f64) -> f64 {
    10.0 * (a.max(1e-30) / b.max(1e-30)).log10()
}

#[test]
fn reference_reconstruction_matches_hf_band_energies() {
    let n = OUT_FRAME * 12;
    let est = SbrEstimator::new(48_000, 24).unwrap();
    let (kx, k2) = (
        est.bands().k_x as usize,
        (est.bands().k_x + est.bands().m) as usize,
    );
    // Stationary cells: LF harmonics + HF noise (invf on), white noise
    // (patch only), LF noise + HF tone (energy lands, tone becomes noise).
    // The LF content must reach k0 (6.75 kHz): an empty patch-source
    // subband gives E_curr ≈ 0, the limiter caps its gain and the noise
    // floor is limited with it (Q_M_lim = Q_M·G_max/G), so the HF band
    // stays empty. 150 Hz comb: 2–3 harmonics in every source band.
    let mut harm = band_noise(n, 8_000.0, 15_000.0, 0.12, 51);
    for h in 1..=44 {
        add(&mut harm, &tone(150.0 * h as f32, n, 0.03));
    }
    let mut s = 61u32;
    let white: Vec<f32> = (0..n).map(|_| 0.25 * lcg(&mut s)).collect();
    let mut tonal = band_noise(n, 200.0, 6_700.0, 0.2, 71);
    add(&mut tonal, &tone(12_100.0, n, 0.2));
    for (name, pcm, tol) in [
        ("harmonics+hf-noise", harm, 1.5),
        ("white", white, 1.0),
        ("lf-noise+hf-tone", tonal, 2.0),
    ] {
        let (orig, rec, _, _) = reconstruct(&pcm);
        // Compare per envelope band (fTableHigh): adjacent QMF subbands
        // overlap by design, so a re-analysis resolves no finer than the
        // bands the codec controls. Bands > 60 dB under the loudest one
        // are floored (leakage, not envelope error).
        let fh = &est.bands().f_table_high;
        let sum = |e: &[f64; BANDS], w: &[i32]| -> f64 {
            (w[0] as usize..w[1] as usize).map(|k| e[k]).sum()
        };
        let bo: Vec<f64> = fh.windows(2).map(|w| sum(&orig, w)).collect();
        let br: Vec<f64> = fh.windows(2).map(|w| sum(&rec, w)).collect();
        let peak = bo.iter().cloned().fold(0.0, f64::max);
        let floor = peak * 1e-6;
        let errs: Vec<f64> = bo
            .iter()
            .zip(&br)
            .map(|(&o, &r)| db(r.max(floor), o.max(floor)))
            .collect();
        // A band ≥ 20 dB under a neighbour is a spectral edge: the
        // decoder's subband noise spreads one subband wide and the
        // overlapping QMF re-analysis sees it, so those bands only
        // report; the gate is the interior.
        // Band 0's lower neighbour is the core's top subband (the
        // decoder's XLow/XHigh splice at k_x overlaps the same way).
        let edge: Vec<bool> = (0..bo.len())
            .map(|i| {
                let nb = |j: usize| bo.get(j).copied().unwrap_or(0.0);
                let below = if i == 0 { orig[kx - 1] } else { nb(i - 1) };
                bo[i].max(floor) * 100.0 < below.max(nb(i + 1))
            })
            .collect();
        let interior: Vec<f64> = errs
            .iter()
            .zip(&edge)
            .filter(|&(_, &e)| !e)
            .map(|(&v, _)| v.abs())
            .collect();
        let worst = interior.iter().cloned().fold(0.0, f64::max);
        let mean = interior.iter().sum::<f64>() / interior.len() as f64;
        let total = db(rec[kx..k2].iter().sum(), orig[kx..k2].iter().sum());
        let per: Vec<String> = errs
            .iter()
            .zip(&edge)
            .map(|(e, &x)| format!("{e:+.1}{}", if x { "*" } else { "" }))
            .collect();
        let o: Vec<String> = bo.iter().map(|&v| format!("{:+.0}", db(v, peak))).collect();
        println!(
            "{name}: interior envelope-band |err| mean {mean:.2} dB worst {worst:.2} dB, total HF {total:+.2} dB; per band (* = edge, report only) {}\n  orig re peak: {}",
            per.join(" "),
            o.join(" ")
        );
        let p = &run(&pcm)[4];
        println!(
            "  frame 4: env {:?} Q {:?} invf {:?}",
            p.env_q, p.noise_q, p.invf.invf_mode
        );
        assert!(
            interior.len() >= 6,
            "{name}: {} interior bands",
            interior.len()
        );
        assert!(
            mean <= tol,
            "{name}: interior mean {mean} dB (bands {kx}..{k2})"
        );
        assert!(worst <= 2.0 * tol, "{name}: interior worst {worst} dB");
        assert!(total.abs() <= 1.5, "{name}: total HF energy {total} dB");
    }
}

#[test]
fn reference_reconstruction_keeps_hf_onset_within_one_envelope() {
    let n = OUT_FRAME * 8;
    let mut pcm = band_noise(n, 200.0, 5_500.0, 0.1, 81);
    let burst = band_noise(n, 8_000.0, 15_000.0, 0.3, 82);
    let at = OUT_FRAME * 4 + 64 * 20;
    pcm[at..]
        .iter_mut()
        .zip(&burst[at..])
        .for_each(|(x, b)| *x += *b);
    let (_, _, hf_o, hf_r) = reconstruct(&pcm);
    let onset = |e: &[f32]| {
        let level = e[e.len() - 32..].iter().sum::<f32>() / 32.0;
        e.iter().position(|&v| v > level * 0.1).unwrap()
    };
    let (oo, or) = (onset(&hf_o), onset(&hf_r));
    let lag = or as i64 - oo as i64;
    let level = hf_r[hf_r.len() - 32..].iter().sum::<f32>() / 32.0;
    let pre = hf_r[or.saturating_sub(24)..or.saturating_sub(8)]
        .iter()
        .cloned()
        .fold(0.0f32, f32::max);
    println!(
        "onset original block {oo}, reconstruction block {or}, lag {lag} blocks (64 samples); pre-onset max {:.1} dB",
        db(f64::from(pre), f64::from(level))
    );
    assert!((0..=24).contains(&lag), "pipeline lag {lag} blocks");
    assert!(
        pre < level * 0.01,
        "no HF spread beyond one envelope before the onset: {pre} vs {level}"
    );
}
