//! Fast DCT-IV factorization of the ISO SBR QMF modulation kernels.
//!
//! The Figure 4.42 / 4.43 matrix-vector products are algebraically
//! equivalent to length-32 and length-64 type-IV DCTs (plus a DST-IV
//! that is a sign-flip + reversal of a DCT-IV) and a handful of
//! pre/post twiddles. Tables are interned spec constants (libm at
//! init). A future encoder reuse of this path must rebuild those
//! tables with `det_math` and keep butterflies FMA-free.

use super::Complex;
use std::sync::OnceLock;

struct FftPlan {
    bitrev: Vec<u16>,
    tw_re: Vec<f64>,
    tw_im: Vec<f64>,
}

struct DctPlan {
    fft: FftPlan,
    /// `cos(π n / (2N))`, `n = 0..N`.
    pre_c: Vec<f64>,
    /// `−sin(π n / (2N))`.
    pre_s: Vec<f64>,
    /// `cos(π (k+½) / (2N))`, `k = 0..N`.
    post_c: Vec<f64>,
    /// `sin(π (k+½) / (2N))`.
    post_s: Vec<f64>,
}

fn bitrev_table(n: usize) -> Vec<u16> {
    let mut t = vec![0u16; n];
    let mut j = 0usize;
    for slot in t.iter_mut() {
        *slot = j as u16;
        let mut bit = n >> 1;
        while bit != 0 && j >= bit {
            j -= bit;
            bit >>= 1;
        }
        j += bit;
    }
    t
}

fn fft_plan(n: usize) -> FftPlan {
    let mut tw_re = Vec::new();
    let mut tw_im = Vec::new();
    let mut len = 2usize;
    while len <= n {
        let half = len / 2;
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        for k in 0..half {
            let a = ang * k as f64;
            tw_re.push(a.cos());
            tw_im.push(a.sin());
        }
        len *= 2;
    }
    FftPlan {
        bitrev: bitrev_table(n),
        tw_re,
        tw_im,
    }
}

fn dct_plan(n: usize) -> DctPlan {
    let n2 = 2 * n;
    let mut pre_c = Vec::with_capacity(n);
    let mut pre_s = Vec::with_capacity(n);
    let mut post_c = Vec::with_capacity(n);
    let mut post_s = Vec::with_capacity(n);
    for i in 0..n {
        let a = std::f64::consts::PI * i as f64 / n2 as f64;
        pre_c.push(a.cos());
        pre_s.push(-a.sin());
        let b = std::f64::consts::PI * (i as f64 + 0.5) / n2 as f64;
        post_c.push(b.cos());
        post_s.push(b.sin());
    }
    DctPlan {
        fft: fft_plan(n2),
        pre_c,
        pre_s,
        post_c,
        post_s,
    }
}

fn plan32() -> &'static DctPlan {
    static P: OnceLock<DctPlan> = OnceLock::new();
    P.get_or_init(|| dct_plan(32))
}

fn plan64() -> &'static DctPlan {
    static P: OnceLock<DctPlan> = OnceLock::new();
    P.get_or_init(|| dct_plan(64))
}

fn fft_in_place(re: &mut [f64], im: &mut [f64], plan: &FftPlan) {
    let n = re.len();
    for (i, &rev) in plan.bitrev.iter().enumerate() {
        let j = rev as usize;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2usize;
    let mut off = 0usize;
    while len <= n {
        let half = len / 2;
        for i in (0..n).step_by(len) {
            for k in 0..half {
                let wr = plan.tw_re[off + k];
                let wi = plan.tw_im[off + k];
                let j = i + k + half;
                let tr = wr * re[j] - wi * im[j];
                let ti = wr * im[j] + wi * re[j];
                let ur = re[i + k];
                let ui = im[i + k];
                re[j] = ur - tr;
                im[j] = ui - ti;
                re[i + k] = ur + tr;
                im[i + k] = ui + ti;
            }
        }
        off += half;
        len *= 2;
    }
}

fn dct4(x: &[f64], out: &mut [f64], plan: &DctPlan) {
    let n = x.len();
    let n2 = n * 2;
    let mut re = [0.0f64; 128];
    let mut im = [0.0f64; 128];
    for i in 0..n {
        re[i] = x[i] * plan.pre_c[i];
        im[i] = x[i] * plan.pre_s[i];
    }
    fft_in_place(&mut re[..n2], &mut im[..n2], &plan.fft);
    for k in 0..n {
        out[k] = re[k] * plan.post_c[k] + im[k] * plan.post_s[k];
    }
}

fn dst4(x: &[f64], out: &mut [f64], plan: &DctPlan) {
    let n = x.len();
    let mut y = [0.0f64; 64];
    let mut t = [0.0f64; 64];
    for (i, &v) in x.iter().enumerate() {
        y[i] = if i % 2 == 0 { v } else { -v };
    }
    dct4(&y[..n], &mut t[..n], plan);
    for k in 0..n {
        out[k] = t[n - 1 - k];
    }
}

/// Figure 4.42 modulation: `W[k] = Σ_n u[n] · 2·exp(i·π/64·(k+½)·(2n−½))`.
pub(super) fn analysis_modulate(u: &[f64; 64]) -> [Complex; 32] {
    let plan = plan32();
    let mut u0 = [0.0f64; 32];
    let mut u1 = [0.0f64; 32];
    u0.copy_from_slice(&u[..32]);
    u1.copy_from_slice(&u[32..]);
    let mut c0 = [0.0f64; 32];
    let mut s0 = [0.0f64; 32];
    let mut c1 = [0.0f64; 32];
    let mut s1 = [0.0f64; 32];
    dct4(&u0, &mut c0, plan);
    dst4(&u0, &mut s0, plan);
    dct4(&u1, &mut c1, plan);
    dst4(&u1, &mut s1, plan);
    let phi = analysis_phi();
    let mut w = [Complex::default(); 32];
    for k in 0..32 {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        let c = c0[k] - sign * s1[k];
        let s = s0[k] + sign * c1[k];
        let (cp, sp) = phi[k];
        w[k] = Complex::new(2.0 * (cp * c + sp * s), 2.0 * (cp * s - sp * c));
    }
    w
}

fn analysis_phi() -> &'static [(f64, f64); 32] {
    static P: OnceLock<[(f64, f64); 32]> = OnceLock::new();
    P.get_or_init(|| {
        let mut t = [(0.0, 0.0); 32];
        for (k, slot) in t.iter_mut().enumerate() {
            let phi = 3.0 * std::f64::consts::PI / 128.0 * (k as f64 + 0.5);
            *slot = (phi.cos(), phi.sin());
        }
        t
    })
}

/// Figure 4.43 modulation: `v[n] = Σ_k Re(X[k]/64 · exp(i·π/128·(k+½)·(2n−255)))`.
pub(super) fn synthesis_modulate(bands: &[Complex]) -> [f64; 128] {
    let plan = plan64();
    let mut re = [0.0f64; 64];
    let mut im = [0.0f64; 64];
    for (k, x) in bands.iter().enumerate() {
        re[k] = x.re;
        im[k] = x.im;
    }
    let mut cr = [0.0f64; 64];
    let mut si = [0.0f64; 64];
    dct4(&re, &mut cr, plan);
    dst4(&im, &mut si, plan);
    let mut v = [0.0f64; 128];
    for n in 0..64 {
        v[n] = (-cr[n] + si[n]) * (1.0 / 64.0);
        v[64 + n] = (cr[63 - n] + si[63 - n]) * (1.0 / 64.0);
    }
    v
}

#[cfg(test)]
pub(super) fn analysis_modulate_ref(u: &[f64; 64], m: &[Complex]) -> [Complex; 32] {
    let mut w = [Complex::default(); 32];
    for (k, wk) in w.iter_mut().enumerate() {
        let row = &m[k * 64..(k + 1) * 64];
        let mut acc = Complex::default();
        for (n, cell) in row.iter().enumerate() {
            acc += *cell * u[n];
        }
        *wk = acc;
    }
    w
}

#[cfg(test)]
pub(super) fn synthesis_modulate_ref(bands: &[Complex], n_mat: &[Complex]) -> [f64; 128] {
    let mut v = [0.0f64; 128];
    for (n, vn) in v.iter_mut().enumerate() {
        let row = &n_mat[n * 64..(n + 1) * 64];
        let mut acc = 0.0;
        for (k, cell) in row.iter().enumerate() {
            let x = bands[k];
            acc += x.re * cell.re - x.im * cell.im;
        }
        *vn = acc;
    }
    v
}
