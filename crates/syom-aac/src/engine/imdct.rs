//! Fast AAC IMDCT: N/4 complex IFFT + pre/post twiddles.
//!
//! The naive §4.6.11.3.1 sum is O(N²) `cos` calls (~2e6 per long
//! window). This path is O(N log N). Same formula, same 2/N scale.

use rustfft::Fft;
use rustfft::FftPlanner;
use rustfft::num_complex::Complex;
use std::cell::RefCell;
use std::sync::{Arc, LazyLock};

thread_local! {
    static FFT_BUF: RefCell<Vec<Complex<f64>>> = const { RefCell::new(Vec::new()) };
    static FFT_SCRATCH: RefCell<Vec<Complex<f64>>> = const { RefCell::new(Vec::new()) };
    static FFT_BUF_F32: RefCell<Vec<Complex<f32>>> = const { RefCell::new(Vec::new()) };
    static FFT_SCRATCH_F32: RefCell<Vec<Complex<f32>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn imdct(spec: &[f64], n_transform: usize) -> Vec<f64> {
    let mut out = vec![0.0f64; n_transform];
    imdct_into(spec, &mut out);
    out
}

pub(crate) fn imdct_into(spec: &[f64], out: &mut [f64]) {
    let n_transform = out.len();
    if spec.len() != n_transform / 2 {
        let v = imdct_naive(spec, n_transform);
        out.copy_from_slice(&v);
        return;
    }
    match n_transform {
        8 => PLAN_8.apply_into(spec, out),
        256 => PLAN_256_F32.apply_into(spec, out),
        2048 => PLAN_2048_F32.apply_into(spec, out),
        _ => {
            let v = imdct_naive(spec, n_transform);
            out.copy_from_slice(&v);
        }
    }
}

pub(crate) fn imdct_naive(spec: &[f64], n_transform: usize) -> Vec<f64> {
    let half = n_transform / 2;
    let n0 = (half + 1) as f64 / 2.0;
    let scale = 2.0 / n_transform as f64;
    let phase_step = 2.0 * core::f64::consts::PI / n_transform as f64;
    let mut out = vec![0.0f64; n_transform];
    for (n, slot) in out.iter_mut().enumerate() {
        let np = n as f64 + n0;
        let mut acc = 0.0f64;
        for (k, &c) in spec.iter().enumerate() {
            acc += c * (phase_step * np * (k as f64 + 0.5)).cos();
        }
        *slot = scale * acc;
    }
    out
}

struct Plan {
    n: usize,
    fft: Arc<dyn Fft<f64>>,
    pre: Vec<Complex<f64>>,
    post: Vec<Complex<f64>>,
}

static PLAN_8: LazyLock<Plan> = LazyLock::new(|| Plan::new(8));
static PLAN_256_F32: LazyLock<PlanF32> = LazyLock::new(|| PlanF32::new(256));
static PLAN_2048_F32: LazyLock<PlanF32> = LazyLock::new(|| PlanF32::new(2048));

impl Plan {
    fn new(n: usize) -> Self {
        let n4 = n / 4;
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_inverse(n4);
        let two_pi_n = 2.0 * core::f64::consts::PI / n as f64;
        let mut pre = Vec::with_capacity(n4);
        let mut post = Vec::with_capacity(n4);
        for k in 0..n4 {
            let a = two_pi_n * (k as f64 + 0.125);
            pre.push(Complex::new(a.cos(), a.sin()));
            post.push(Complex::new(a.cos(), a.sin()));
        }
        Self { n, fft, pre, post }
    }

    fn apply_into(&self, spec: &[f64], out: &mut [f64]) {
        let n = self.n;
        debug_assert_eq!(out.len(), n);
        let n2 = n / 2;
        let n4 = n / 4;
        let scratch_len = self.fft.get_inplace_scratch_len();
        FFT_BUF.with(|cell| {
            let mut buf = cell.borrow_mut();
            buf.resize(n4, Complex::<f64>::new(0.0, 0.0));
            for k in 0..n4 {
                let re = spec[n2 - 2 * k - 1];
                let im = spec[2 * k];
                buf[k] = Complex::new(re, im) * self.pre[k];
            }
            FFT_SCRATCH.with(|sc| {
                let mut scratch = sc.borrow_mut();
                scratch.resize(scratch_len, Complex::<f64>::new(0.0, 0.0));
                self.fft.process_with_scratch(&mut buf, &mut scratch);
            });
            for (slot, tw) in buf.iter_mut().zip(self.post.iter()) {
                *slot *= *tw;
            }
            let scale = 2.0 / n as f64;
            let half = n4 / 2;
            for i in 0..n4 {
                if i % 2 == 0 {
                    let p = i / 2;
                    let y = buf[half + p];
                    out[i] = scale * y.im;
                    out[n2 + i] = scale * y.re;
                } else {
                    let p = (i - 1) / 2;
                    let y = buf[half - 1 - p];
                    out[i] = -scale * y.re;
                    out[n2 + i] = -scale * y.im;
                }
            }
            for n_i in 0..n4 {
                out[n2 - 1 - n_i] = -out[n_i];
                out[n - 1 - n_i] = out[n2 + n_i];
            }
        });
    }
}

/// Same N/4 IFFT IMDCT in f32 (rustfft NEON). Product long/short
/// windows; N=8 stays f64.
struct PlanF32 {
    n: usize,
    fft: Arc<dyn Fft<f32>>,
    pre: Vec<Complex<f32>>,
    post: Vec<Complex<f32>>,
}

impl PlanF32 {
    fn new(n: usize) -> Self {
        let n4 = n / 4;
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_inverse(n4);
        let two_pi_n = 2.0 * core::f32::consts::PI / n as f32;
        let mut pre = Vec::with_capacity(n4);
        let mut post = Vec::with_capacity(n4);
        for k in 0..n4 {
            let a = two_pi_n * (k as f32 + 0.125);
            pre.push(Complex::new(a.cos(), a.sin()));
            post.push(Complex::new(a.cos(), a.sin()));
        }
        Self { n, fft, pre, post }
    }

    fn apply_into(&self, spec: &[f64], out: &mut [f64]) {
        let n = self.n;
        debug_assert_eq!(out.len(), n);
        let n2 = n / 2;
        let n4 = n / 4;
        let scratch_len = self.fft.get_inplace_scratch_len();
        FFT_BUF_F32.with(|cell| {
            let mut buf = cell.borrow_mut();
            buf.resize(n4, Complex::<f32>::new(0.0, 0.0));
            for k in 0..n4 {
                let re = spec[n2 - 2 * k - 1] as f32;
                let im = spec[2 * k] as f32;
                buf[k] = Complex::new(re, im) * self.pre[k];
            }
            FFT_SCRATCH_F32.with(|sc| {
                let mut scratch = sc.borrow_mut();
                scratch.resize(scratch_len, Complex::<f32>::new(0.0, 0.0));
                self.fft.process_with_scratch(&mut buf, &mut scratch);
            });
            for (slot, tw) in buf.iter_mut().zip(self.post.iter()) {
                *slot *= *tw;
            }
            let scale = 2.0 / n as f32;
            let half = n4 / 2;
            for p in 0..half {
                let i = p * 2;
                let y = buf[half + p];
                out[i] = f64::from(scale * y.im);
                out[n2 + i] = f64::from(scale * y.re);
            }
            for p in 0..half {
                let i = p * 2 + 1;
                let y = buf[half - 1 - p];
                out[i] = f64::from(-scale * y.re);
                out[n2 + i] = f64::from(-scale * y.im);
            }
            for n_i in 0..n4 {
                out[n2 - 1 - n_i] = -out[n_i];
                out[n - 1 - n_i] = out[n2 + n_i];
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{imdct, imdct_naive};

    fn max_err(n: usize, spec: &[f64]) -> f64 {
        let a = imdct_naive(spec, n);
        let b = imdct(spec, n);
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn test_fast_matches_naive_n8() {
        let spec = [1.0, 0.2, -0.3, 0.4];
        assert!(max_err(8, &spec) < 1e-12);
    }

    #[test]
    fn test_fast_matches_naive_n256() {
        let spec: Vec<f64> = (0..128)
            .map(|i| ((i * 17) % 50) as f64 / 25.0 - 1.0)
            .collect();
        assert!(max_err(256, &spec) < 1e-4);
    }

    #[test]
    fn test_fast_matches_naive_n2048() {
        let spec: Vec<f64> = (0..1024)
            .map(|i| ((i * 31) % 100) as f64 / 50.0 - 1.0)
            .collect();
        assert!(max_err(2048, &spec) < 1e-4);
    }
}
