//! Fast forward MDCT vs the naive analysis sum, plus the TDAC roundtrip
//! (window → MDCT → IMDCT → window + overlap-add reconstructs the input).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::filterbank::window_left;
use super::super::ics::WindowShape;
use super::super::imdct::imdct;
use super::{mdct_into_f32, mdct_naive};

const N: usize = 2048;

/// Deterministic smooth-ish signal in [-1, 1].
fn test_signal(len: usize) -> Vec<f64> {
    (0..len)
        .map(|i| {
            let t = i as f64;
            0.6 * (t * 0.017).sin() + 0.3 * (t * 0.111).cos() + 0.1 * (t * 0.005).sin()
        })
        .collect()
}

fn sine_window() -> Vec<f64> {
    let half = window_left(N, WindowShape::Sine);
    (0..N)
        .map(|i| {
            if i < N / 2 {
                f64::from(half[i])
            } else {
                f64::from(half[N - 1 - i])
            }
        })
        .collect()
}

fn mdct_fast_f64(time: &[f64]) -> Vec<f64> {
    let t32: Vec<f32> = time.iter().map(|&x| x as f32).collect();
    let mut spec = vec![0.0f32; time.len() / 2];
    mdct_into_f32(&t32, &mut spec);
    spec.iter().map(|&x| f64::from(x)).collect()
}

#[test]
fn fast_matches_naive_long() {
    let sig = test_signal(N);
    let a = mdct_naive(&sig, N);
    let b = mdct_fast_f64(&sig);
    let err = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    assert!(err < 1e-3, "fast MDCT drifted from the naive sum: {err}");
}

#[test]
fn tdac_reconstructs_input() {
    let sig = test_signal(3 * (N / 2));
    let w = sine_window();
    let frame = |off: usize| -> Vec<f64> { (0..N).map(|i| sig[off + i] * w[i]).collect() };
    let spec0 = mdct_fast_f64(&frame(0));
    let spec1 = mdct_fast_f64(&frame(N / 2));
    let y0 = imdct(&spec0, N);
    let y1 = imdct(&spec1, N);
    let mut err = 0.0f64;
    for i in 0..N / 2 {
        let rec = y0[N / 2 + i] * w[N / 2 + i] + y1[i] * w[i];
        err = err.max((rec - sig[N / 2 + i]).abs());
    }
    assert!(err < 1e-3, "TDAC reconstruction error {err}");
}

#[test]
fn zero_input_is_silence() {
    let spec = mdct_fast_f64(&vec![0.0; N]);
    assert!(spec.iter().all(|&x| x.abs() < 1e-9));
}

const N_S: usize = 256;

fn short_window() -> Vec<f64> {
    let half = window_left(N_S, WindowShape::Sine);
    (0..N_S)
        .map(|i| {
            if i < N_S / 2 {
                f64::from(half[i])
            } else {
                f64::from(half[N_S - 1 - i])
            }
        })
        .collect()
}

#[test]
fn fast_matches_naive_short() {
    let sig = test_signal(N_S);
    let a = mdct_naive(&sig, N_S);
    let b = mdct_fast_f64(&sig);
    let err = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    assert!(
        err < 1e-3,
        "fast short MDCT drifted from the naive sum: {err}"
    );
}

const N_LD: usize = 1024;

fn ld_sine_window() -> &'static [f32; 1024] {
    super::super::filterbank::ld_analysis_window(WindowShape::Sine)
}

#[test]
fn fast_matches_naive_ld() {
    let sig = test_signal(N_LD);
    let a = mdct_naive(&sig, N_LD);
    let b = mdct_fast_f64(&sig);
    let err = a
        .iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max);
    assert!(err < 1e-3, "fast LD MDCT drifted from the naive sum: {err}");
}

fn ld_ics() -> super::super::ics::IcsInfo {
    super::super::ics::IcsInfo {
        window_sequence: super::super::ics::WindowSequence::OnlyLong,
        window_shape: WindowShape::Sine,
        max_sfb: 0,
        num_windows: 1,
        num_window_groups: 1,
        window_group_length: [1, 0, 0, 0, 0, 0, 0, 0],
        num_swb: 0,
        ld: true,
    }
}

/// Analysis-window + forward MDCT, then the decoder's LD filterbank.
fn ld_decode_frames(padded: &[f32], frames: usize) -> Vec<f32> {
    let w = ld_sine_window();
    let ics = ld_ics();
    let mut fb = super::super::filterbank::Filterbank::new();
    let mut out = Vec::new();
    for frame in 0..frames {
        let off = frame * 512;
        let mut time = vec![0.0f32; N_LD];
        for i in 0..N_LD {
            time[i] = padded[off + i] * w[i];
        }
        let mut spec = vec![0.0f32; 512];
        mdct_into_f32(&time, &mut spec);
        let mut pcm = Vec::new();
        fb.synthesize_into(&spec, &ics, &mut pcm).unwrap();
        out.extend_from_slice(&pcm);
    }
    out
}

#[test]
fn ld_sine_window_roundtrips_through_the_decoder() {
    // 512 zeros of encoder priming, then the source. Reconstruction of the
    // source starts at decode sample 512 (FDK encoder `nDelay`).
    let mut padded = vec![0.0f32; 512 + 2048];
    for i in 0..1536 {
        padded[512 + i] = 0.2 * (i as f32 * 0.07).sin() + 0.1 * (i as f32 * 0.013).cos();
    }
    let out = ld_decode_frames(&padded, 4);
    let mut err = 0.0f32;
    for i in 0..1024 {
        err = err.max((out[512 + i] - padded[512 + i]).abs());
    }
    assert!(err < 2e-3, "LD TDAC error {err}");

    let mut impulse = vec![0.0f32; 512 + 2048];
    impulse[512] = 1.0;
    let out = ld_decode_frames(&impulse, 4);
    assert!(
        (out[512] - 1.0).abs() < 2e-3,
        "impulse at decode {}: {}",
        512,
        out[512]
    );
    let leaked = out[513..1536].iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(leaked < 2e-3, "impulse leaked {leaked}");
}

#[test]
fn tdac_reconstructs_input_short() {
    // Eight-short hop is 128: each output sample is the overlap of two
    // 256-sample windows (like the decoder's `short_windowed`).
    let sig = test_signal(N_S + 3 * (N_S / 2));
    let w = short_window();
    let frame = |off: usize| -> Vec<f64> { (0..N_S).map(|i| sig[off + i] * w[i]).collect() };
    let mut prev = imdct(&mdct_fast_f64(&frame(0)), N_S);
    let mut err = 0.0f64;
    for hop in 1..=3 {
        let cur = imdct(&mdct_fast_f64(&frame(hop * N_S / 2)), N_S);
        for i in 0..N_S / 2 {
            let rec = prev[N_S / 2 + i] * w[N_S / 2 + i] + cur[i] * w[i];
            err = err.max((rec - sig[(hop - 1) * (N_S / 2) + N_S / 2 + i]).abs());
        }
        prev = cur;
    }
    assert!(err < 1e-3, "short TDAC reconstruction error {err}");
}
