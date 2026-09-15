//! Fast IMDCT vs the naive §4.6.11.3.1 sum.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{bitrev_table, ifft_soa, ifft_soa_scalar, imdct, imdct_naive, twiddle_table};

fn max_err(n: usize, spec: &[f64]) -> f64 {
    let a = imdct_naive(spec, n);
    let b = imdct(spec, n);
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f64::max)
}

#[test]
fn fast_matches_naive_short() {
    let spec: Vec<f64> = (0..128)
        .map(|k| ((k * 13) % 17) as f64 * 0.1 - 0.8)
        .collect();
    assert!(
        max_err(256, &spec) < 1e-3,
        "short IMDCT drifted from §4.6.11.3.1"
    );
}

#[test]
fn fast_matches_naive_long_impulse() {
    let mut spec = vec![0.0f64; 1024];
    spec[4] = 1.0;
    assert!(
        max_err(2048, &spec) < 1e-3,
        "long IMDCT drifted from §4.6.11.3.1"
    );
}

#[test]
fn naive_zero_is_silence() {
    let out = imdct_naive(&[0.0; 1024], 2048);
    assert!(out.iter().all(|&x| x.abs() < 1e-18));
}

fn fill_soa(n: usize, seed: u32) -> (Vec<f32>, Vec<f32>) {
    let mut s = seed;
    let mut re = vec![0.0f32; n];
    let mut im = vec![0.0f32; n];
    for i in 0..n {
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        re[i] = (s as f32 / u32::MAX as f32) * 2.0 - 1.0;
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        im[i] = (s as f32 / u32::MAX as f32) * 2.0 - 1.0;
    }
    (re, im)
}

#[cfg(target_arch = "x86_64")]
#[test]
fn declared_x86_minimum_is_sse2() {
    assert!(
        std::is_x86_feature_detected!("sse2"),
        "declared x86_64 minimum is SSE2; AVX is runtime-probed; scalar is ifft_soa_scalar"
    );
}

/// SSE2/NEON 4-wide butterflies match scalar IEEE bits (no FMA).
#[test]
fn simd_fft_matches_scalar_bits() {
    for n in [8usize, 16, 64, 512] {
        let bitrev = bitrev_table(n);
        let (tw_re, tw_im) = twiddle_table(n);
        for seed in [1u32, 0x9e3779b9, 7] {
            let (mut re_s, mut im_s) = fill_soa(n, seed);
            let mut re_v = re_s.clone();
            let mut im_v = im_s.clone();
            ifft_soa_scalar(&mut re_s, &mut im_s, &bitrev, &tw_re, &tw_im);
            ifft_soa(&mut re_v, &mut im_v, &bitrev, &tw_re, &tw_im);
            assert_eq!(re_s, re_v, "re n={n} seed={seed}");
            assert_eq!(im_s, im_v, "im n={n} seed={seed}");
        }
        // Impulse / silence / Nyquist-ish boundary.
        for (re0, im0) in [
            (
                vec![1.0f32]
                    .into_iter()
                    .chain(std::iter::repeat_n(0.0, n - 1))
                    .collect::<Vec<_>>(),
                vec![0.0f32; n],
            ),
            (vec![0.0f32; n], vec![0.0f32; n]),
            (
                (0..n).map(|i| if i + 1 == n { 1.0 } else { 0.0 }).collect(),
                vec![0.0f32; n],
            ),
        ] {
            let mut re_s = re0.clone();
            let mut im_s = im0.clone();
            let mut re_v = re0;
            let mut im_v = im0;
            ifft_soa_scalar(&mut re_s, &mut im_s, &bitrev, &tw_re, &tw_im);
            ifft_soa(&mut re_v, &mut im_v, &bitrev, &tw_re, &tw_im);
            assert_eq!(re_s, re_v);
            assert_eq!(im_s, im_v);
        }
    }
}

/// Isolated FFT wall: scalar vs dispatched SIMD, 2048-point IMDCT size (n=512).
#[test]
#[ignore = "manual TASK-82 transform CPU probe"]
fn fft_cpu_probe() {
    let n = 512usize;
    let bitrev = bitrev_table(n);
    let (tw_re, tw_im) = twiddle_table(n);
    let (re0, im0) = fill_soa(n, 1);
    const REPS: usize = 8000;
    let mut re = re0.clone();
    let mut im = im0.clone();
    for _ in 0..32 {
        ifft_soa(&mut re, &mut im, &bitrev, &tw_re, &tw_im);
    }
    let t0 = std::time::Instant::now();
    for _ in 0..REPS {
        re.copy_from_slice(&re0);
        im.copy_from_slice(&im0);
        ifft_soa_scalar(&mut re, &mut im, &bitrev, &tw_re, &tw_im);
        std::hint::black_box((re[0], im[0]));
    }
    let scalar_ns = t0.elapsed().as_nanos();
    let t1 = std::time::Instant::now();
    for _ in 0..REPS {
        re.copy_from_slice(&re0);
        im.copy_from_slice(&im0);
        ifft_soa(&mut re, &mut im, &bitrev, &tw_re, &tw_im);
        std::hint::black_box((re[0], im[0]));
    }
    let simd_ns = t1.elapsed().as_nanos();
    println!(
        "fft_cpu_probe n={n} reps={REPS} scalar_ns={scalar_ns} simd_ns={simd_ns} ratio={}",
        simd_ns as f64 / scalar_ns as f64
    );
    assert!(
        simd_ns <= scalar_ns,
        "SIMD FFT should not be slower than scalar"
    );
}
