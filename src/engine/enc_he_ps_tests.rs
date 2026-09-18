//! TASK-93 (engine): HE v2 access units decode to two channels with the
//! source's spatial image, on the HE v1 timeline.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::PsHeEncoder;
use crate::engine::det_math;
use crate::engine::enc_he::HE_PRIMING_OUT;
use crate::{DecodeOptions, decode_with};

const RATE: u32 = 48_000;

fn programme(n: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    for f in [150u32, 420, 900, 1800, 3500, 7000, 12_000] {
        for (i, o) in v.iter_mut().enumerate() {
            let cycles = (i as u64 * u64::from(f)) % u64::from(RATE);
            *o += 0.08 * det_math::sincos(cycles as f32 / RATE as f32 * std::f32::consts::TAU).0;
        }
    }
    v
}

/// Encode to ADTS (mono core header, implicit SBR + PS).
pub(crate) fn encode_adts(l: &[f32], r: &[f32], bps: u32, chunk: usize) -> Vec<u8> {
    let mut enc = PsHeEncoder::new(RATE, bps, false).unwrap();
    let fs = enc.fs_index();
    let mut adts = Vec::new();
    let mut sink = |au: &[u8]| {
        crate::encode::adts_frame_into(au, fs, 1, &mut adts);
        Ok(())
    };
    let mut at = 0usize;
    while at < l.len() {
        let end = (at + chunk).min(l.len());
        enc.push(&[&l[at..end], &r[at..end]], &mut sink).unwrap();
        at = end;
    }
    enc.finish(&mut sink).unwrap();
    adts
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| f64::from(*v).powi(2)).sum()
}

#[test]
#[ignore]
fn explore_alignment() {
    let n = 40 * 2048;
    let s = programme(n);
    let half = 20 * 2048 + 700;
    let l: Vec<f32> = s
        .iter()
        .enumerate()
        .map(|(i, x)| if i < half { *x } else { 0.05 * x })
        .collect();
    let r: Vec<f32> = s
        .iter()
        .enumerate()
        .map(|(i, x)| if i < half { 0.05 * x } else { *x })
        .collect();
    let adts = encode_adts(&l, &r, 32_000, 4096);
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    println!(
        "channels {} rate {} len {}",
        dec.channels.len(),
        dec.sample_rate,
        dec.channels[0].len()
    );
    let p = HE_PRIMING_OUT as usize;
    for w in (half - 3000..half + 4000).step_by(256) {
        let a = energy(&dec.channels[0][p + w..p + w + 256]);
        let b = energy(&dec.channels[1][p + w..p + w + 256]);
        println!(
            "src {:6} (rel {:5}): L/R {:6.1} dB",
            w,
            w as i64 - half as i64,
            10.0 * (a / b.max(1e-12)).log10()
        );
    }
}
