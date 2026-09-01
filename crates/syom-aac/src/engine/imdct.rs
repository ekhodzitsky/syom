//! IMDCT — ISO/IEC 14496-3 §4.6.11.3.1, plus an N/4 IFFT fast path.
//!
//! Pre/post twiddles live here. The fast path is proven against the naive sum
//! in `imdct_tests.rs`.

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::{Arc, LazyLock};

/// Naive §4.6.11.3.1 sum.
///
/// `x[n] = (2/N) · Σ_k spec[k] · cos((2π/N)·(n + n0)·(k + 1/2))`
/// with `n0 = (N/2 + 1)/2`, `N = n_transform`.
#[cfg(test)]
#[must_use]
pub fn imdct_naive(spec: &[f64], n_transform: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; n_transform];
    imdct_naive_into(spec, &mut out);
    out
}

fn imdct_naive_into(spec: &[f64], out: &mut [f64]) {
    let n = out.len();
    if n == 0 {
        return;
    }
    let n0 = (n / 2 + 1) as f64 / 2.0;
    let scale = 2.0 / n as f64;
    let step = 2.0 * std::f64::consts::PI / n as f64;
    for (n_i, slot) in out.iter_mut().enumerate() {
        let np = n_i as f64 + n0;
        let mut acc = 0.0f64;
        for (k, &c) in spec.iter().enumerate() {
            acc += c * (step * np * (k as f64 + 0.5)).cos();
        }
        *slot = scale * acc;
    }
}

/// Fast IMDCT into `out`. Falls back to the naive sum for odd lengths.
pub fn imdct_into(spec: &[f64], out: &mut [f64]) {
    let n = out.len();
    if spec.len() != n / 2 {
        imdct_naive_into(spec, out);
        return;
    }
    match n {
        256 => PLAN_256.apply_into(spec, out),
        2048 => PLAN_2048.apply_into(spec, out),
        _ => imdct_naive_into(spec, out),
    }
}

/// Allocate and run [`imdct_into`].
#[cfg(test)]
#[must_use]
pub fn imdct(spec: &[f64], n_transform: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; n_transform];
    imdct_into(spec, &mut out);
    out
}

struct Plan {
    n: usize,
    fft: Arc<dyn Fft<f64>>,
    pre: Vec<Complex<f64>>,
    post: Vec<Complex<f64>>,
}

static PLAN_256: LazyLock<Plan> = LazyLock::new(|| Plan::new(256));
static PLAN_2048: LazyLock<Plan> = LazyLock::new(|| Plan::new(2048));

impl Plan {
    fn new(n: usize) -> Self {
        let n4 = n / 4;
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_inverse(n4);
        let two_pi_n = 2.0 * std::f64::consts::PI / n as f64;
        let mut pre = Vec::with_capacity(n4);
        let mut post = Vec::with_capacity(n4);
        for k in 0..n4 {
            let a = two_pi_n * (k as f64 + 0.125);
            let c = Complex::new(a.cos(), a.sin());
            pre.push(c);
            post.push(c);
        }
        Self { n, fft, pre, post }
    }

    fn apply_into(&self, spec: &[f64], out: &mut [f64]) {
        let n = self.n;
        let n2 = n / 2;
        let n4 = n / 4;
        let mut buf = vec![Complex::<f64>::new(0.0, 0.0); n4];
        for k in 0..n4 {
            let re = spec[n2 - 2 * k - 1];
            let im = spec[2 * k];
            buf[k] = Complex::new(re, im) * self.pre[k];
        }
        self.fft.process(&mut buf);
        for (slot, tw) in buf.iter_mut().zip(self.post.iter()) {
            *slot *= *tw;
        }
        let scale = 2.0 / n as f64;
        let half = n4 / 2;
        for p in 0..half {
            let i = p * 2;
            let y = buf[half + p];
            out[i] = scale * y.im;
            out[n2 + i] = scale * y.re;
        }
        for p in 0..half {
            let i = p * 2 + 1;
            let y = buf[half - 1 - p];
            out[i] = -scale * y.re;
            out[n2 + i] = -scale * y.im;
        }
        for n_i in 0..n4 {
            out[n2 - 1 - n_i] = -out[n_i];
            out[n - 1 - n_i] = out[n2 + n_i];
        }
    }
}

#[cfg(test)]
#[path = "imdct_tests.rs"]
mod imdct_tests;
