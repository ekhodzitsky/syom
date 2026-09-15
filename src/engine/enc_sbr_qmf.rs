//! Encoder 32-band analysis QMF — ISO Figure 4.42 windowing, DCT-IV via
//! [`super::det_math`] (decision path uses det_math sincos, not platform sin).

use super::det_math;
use super::error::{Error, Result};
use super::sbr_qmf::QMF_WINDOW;
use std::sync::OnceLock;

const N: usize = 32;
const N2: usize = 64;
const HIST: usize = 320;

/// One analysis slot: 32 complex subband samples `W[k]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EncSlot {
    pub re: [f32; N],
    pub im: [f32; N],
}

impl EncSlot {
    pub(crate) fn energy(&self, k: usize) -> f32 {
        self.re[k] * self.re[k] + self.im[k] * self.im[k]
    }

    pub(crate) fn band_energy(&self, lo: usize, hi: usize) -> f32 {
        (lo..hi.min(N)).map(|k| self.energy(k)).sum()
    }
}

struct Plan {
    bitrev: [u16; N2],
    tw_re: Vec<f32>,
    tw_im: Vec<f32>,
    pre_c: [f32; N],
    pre_s: [f32; N],
    post_c: [f32; N],
    post_s: [f32; N],
    phi_c: [f32; N],
    phi_s: [f32; N],
}

fn plan() -> &'static Plan {
    static P: OnceLock<Plan> = OnceLock::new();
    P.get_or_init(|| {
        let mut bitrev = [0u16; N2];
        let mut j = 0usize;
        for slot in &mut bitrev {
            *slot = j as u16;
            let mut bit = N2 / 2;
            while bit != 0 && j >= bit {
                j -= bit;
                bit >>= 1;
            }
            j += bit;
        }
        let mut tw_re = Vec::new();
        let mut tw_im = Vec::new();
        let mut len = 2usize;
        while len <= N2 {
            let half = len / 2;
            let ang = -2.0 * core::f32::consts::PI / len as f32;
            for k in 0..half {
                let mut a = ang * k as f32;
                let tau = core::f32::consts::TAU;
                a %= tau;
                if a < 0.0 {
                    a += tau;
                }
                let (s, c) = det_math::sincos(a);
                tw_re.push(c);
                tw_im.push(s);
            }
            len *= 2;
        }
        let mut pre_c = [0.0f32; N];
        let mut pre_s = [0.0f32; N];
        let mut post_c = [0.0f32; N];
        let mut post_s = [0.0f32; N];
        let mut phi_c = [0.0f32; N];
        let mut phi_s = [0.0f32; N];
        for i in 0..N {
            let (s, c) = det_math::sincos(core::f32::consts::PI * i as f32 / N2 as f32);
            pre_c[i] = c;
            pre_s[i] = -s;
            let (s, c) = det_math::sincos(core::f32::consts::PI * (i as f32 + 0.5) / N2 as f32);
            post_c[i] = c;
            post_s[i] = s;
            let (s, c) = det_math::sincos(3.0 * core::f32::consts::PI / 128.0 * (i as f32 + 0.5));
            phi_c[i] = c;
            phi_s[i] = s;
        }
        Plan {
            bitrev,
            tw_re,
            tw_im,
            pre_c,
            pre_s,
            post_c,
            post_s,
            phi_c,
            phi_s,
        }
    })
}

fn fft64(re: &mut [f32; N2], im: &mut [f32; N2], p: &Plan) {
    for (i, &rev) in p.bitrev.iter().enumerate() {
        let j = rev as usize;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2usize;
    let mut off = 0usize;
    while len <= N2 {
        let half = len / 2;
        for i in (0..N2).step_by(len) {
            for k in 0..half {
                let wr = p.tw_re[off + k];
                let wi = p.tw_im[off + k];
                let j = i + k + half;
                let wrj = wr * re[j];
                let wij = wi * im[j];
                let tr = wrj - wij;
                let wir = wi * re[j];
                let wjr = wr * im[j];
                let ti = wir + wjr;
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

fn dct4(x: &[f32; N], out: &mut [f32; N], p: &Plan) {
    let mut re = [0.0f32; N2];
    let mut im = [0.0f32; N2];
    for i in 0..N {
        re[i] = x[i] * p.pre_c[i];
        im[i] = x[i] * p.pre_s[i];
    }
    fft64(&mut re, &mut im, p);
    for k in 0..N {
        let a = re[k] * p.post_c[k];
        let b = im[k] * p.post_s[k];
        out[k] = a + b;
    }
}

fn dst4(x: &[f32; N], out: &mut [f32; N], p: &Plan) {
    let mut y = [0.0f32; N];
    let mut t = [0.0f32; N];
    for (i, &v) in x.iter().enumerate() {
        y[i] = if i % 2 == 0 { v } else { -v };
    }
    dct4(&y, &mut t, p);
    for k in 0..N {
        out[k] = t[N - 1 - k];
    }
}

fn modulate(u: &[f32; N2]) -> EncSlot {
    let p = plan();
    let mut u0 = [0.0f32; N];
    let mut u1 = [0.0f32; N];
    u0.copy_from_slice(&u[..N]);
    u1.copy_from_slice(&u[N..]);
    let mut c0 = [0.0f32; N];
    let mut s0 = [0.0f32; N];
    let mut c1 = [0.0f32; N];
    let mut s1 = [0.0f32; N];
    dct4(&u0, &mut c0, p);
    dst4(&u0, &mut s0, p);
    dct4(&u1, &mut c1, p);
    dst4(&u1, &mut s1, p);
    let mut slot = EncSlot {
        re: [0.0; N],
        im: [0.0; N],
    };
    for k in 0..N {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        let c = c0[k] - sign * s1[k];
        let s = s0[k] + sign * c1[k];
        let cp = p.phi_c[k];
        let sp = p.phi_s[k];
        slot.re[k] = 2.0 * (cp * c + sp * s);
        slot.im[k] = 2.0 * (cp * s - sp * c);
    }
    slot
}

/// Figure 4.42 analysis bank (encoder path, f32 + det_math).
pub(crate) struct EncAnalysisQmf {
    x: [f32; HIST],
}

impl EncAnalysisQmf {
    pub(crate) fn new() -> Self {
        Self { x: [0.0; HIST] }
    }

    pub(crate) fn reset(&mut self) {
        self.x = [0.0; HIST];
    }

    pub(crate) fn push_slot(&mut self, samples: &[f32]) -> Result<EncSlot> {
        if samples.len() != N {
            return Err(Error::SbrQmfInvalid);
        }
        self.x.copy_within(0..288, 32);
        for (n, s) in samples.iter().enumerate() {
            self.x[31 - n] = *s;
        }
        let mut u = [0.0f32; N2];
        for (n, un) in u.iter_mut().enumerate() {
            let mut acc = 0.0f32;
            for j in 0..5 {
                let idx = n + j * 64;
                acc += self.x[idx] * (QMF_WINDOW[2 * idx] as f32);
            }
            *un = acc;
        }
        Ok(modulate(&u))
    }
}

impl Default for EncAnalysisQmf {
    fn default() -> Self {
        Self::new()
    }
}
