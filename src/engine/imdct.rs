//! IMDCT — ISO/IEC 14496-3 §4.6.11.3.1, plus an N/4 IFFT fast path.
//!
//! Pre/post twiddles live here. The fast path is proven against the naive sum
//! in `imdct_tests.rs`.

use std::cell::RefCell;
use std::sync::LazyLock;

#[derive(Clone, Copy)]
struct C {
    re: f64,
    im: f64,
}

impl C {
    const ZERO: Self = Self { re: 0.0, im: 0.0 };

    fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    #[inline(always)]
    fn mul(self, o: Self) -> Self {
        Self {
            re: self.re * o.re - self.im * o.im,
            im: self.re * o.im + self.im * o.re,
        }
    }
}

/// Unnormalized inverse radix-2 FFT (same contract as rustfft inverse).
fn ifft_radix2(a: &mut [C]) {
    let n = a.len();
    if n < 2 {
        return;
    }
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j >= bit {
            j -= bit;
            bit >>= 1;
        }
        j += bit;
        if i < j {
            a.swap(i, j);
        }
    }
    let mut len = 2usize;
    while len <= n {
        let ang = 2.0 * std::f64::consts::PI / len as f64;
        let wlen = C::new(ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let mut w = C::new(1.0, 0.0);
            let half = len / 2;
            for k in 0..half {
                let u = a[i + k];
                let v = a[i + k + half].mul(w);
                a[i + k] = C::new(u.re + v.re, u.im + v.im);
                a[i + k + half] = C::new(u.re - v.re, u.im - v.im);
                w = w.mul(wlen);
            }
        }
        len *= 2;
    }
}

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
    pre: Vec<C>,
    post: Vec<C>,
}

static PLAN_256: LazyLock<Plan> = LazyLock::new(|| Plan::new(256));
static PLAN_2048: LazyLock<Plan> = LazyLock::new(|| Plan::new(2048));

impl Plan {
    fn new(n: usize) -> Self {
        let n4 = n / 4;
        let two_pi_n = 2.0 * std::f64::consts::PI / n as f64;
        let mut pre = Vec::with_capacity(n4);
        let mut post = Vec::with_capacity(n4);
        for k in 0..n4 {
            let a = two_pi_n * (k as f64 + 0.125);
            let c = C::new(a.cos(), a.sin());
            pre.push(c);
            post.push(c);
        }
        Self { n, pre, post }
    }

    fn apply_into(&self, spec: &[f64], out: &mut [f64]) {
        let n = self.n;
        let n2 = n / 2;
        let n4 = n / 4;
        thread_local! {
            static BUF: RefCell<Vec<C>> = const { RefCell::new(Vec::new()) };
        }
        BUF.with(|cell| {
            let mut buf = cell.borrow_mut();
            if buf.len() < n4 {
                buf.resize(n4, C::ZERO);
            }
            let buf = &mut buf[..n4];
            for k in 0..n4 {
                let re = spec[n2 - 2 * k - 1];
                let im = spec[2 * k];
                buf[k] = C::new(re, im).mul(self.pre[k]);
            }
            ifft_radix2(buf);
            for (slot, tw) in buf.iter_mut().zip(self.post.iter()) {
                *slot = slot.mul(*tw);
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
        });
    }
}

#[cfg(test)]
#[path = "imdct_tests.rs"]
mod imdct_tests;
