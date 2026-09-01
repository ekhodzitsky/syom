//! Filterbank: windows + overlap-add — ISO/IEC 14496-3 §4.6.11.3.2 / §4.6.11.3.3.

use super::error::{Error, Result};
use super::ics::{IcsInfo, WindowSequence, WindowShape};
use super::imdct::imdct_into;
use super::swb::{LONG_WINDOW_LEN, SHORT_WINDOW_LEN};
use std::sync::LazyLock;

const N_L: usize = 2048;
const N_S: usize = 256;

static SINE_LONG: LazyLock<Vec<f64>> = LazyLock::new(|| sine_left(1024));
static SINE_SHORT: LazyLock<Vec<f64>> = LazyLock::new(|| sine_left(128));
static KBD_LONG: LazyLock<Vec<f64>> = LazyLock::new(|| kbd_left(1024, 4.0));
static KBD_SHORT: LazyLock<Vec<f64>> = LazyLock::new(|| kbd_left(128, 6.0));

fn bessel_i0(x: f64) -> f64 {
    let half_x = x / 2.0;
    let mut term = 1.0f64;
    let mut sum = 1.0f64;
    let mut k = 1.0f64;
    loop {
        term *= (half_x / k) * (half_x / k);
        sum += term;
        if term <= sum * 1e-18 || k > 256.0 {
            break;
        }
        k += 1.0;
    }
    sum
}

fn kbd_left(half: usize, alpha: f64) -> Vec<f64> {
    let quarter = half as f64 / 2.0;
    let denom = bessel_i0(std::f64::consts::PI * alpha);
    let kernel: Vec<f64> = (0..=half)
        .map(|n| {
            let t = (n as f64 - quarter) / quarter;
            let rad = (1.0 - t * t).max(0.0);
            bessel_i0(std::f64::consts::PI * alpha * rad.sqrt()) / denom
        })
        .collect();
    let total: f64 = kernel.iter().sum();
    let mut running = 0.0f64;
    let mut out = Vec::with_capacity(half);
    for &w in kernel.iter().take(half) {
        running += w;
        out.push((running / total).sqrt());
    }
    out
}

fn sine_left(half: usize) -> Vec<f64> {
    let n = (2 * half) as f64;
    (0..half)
        .map(|i| (std::f64::consts::PI / n * (i as f64 + 0.5)).sin())
        .collect()
}

fn half(n: usize, shape: WindowShape) -> &'static [f64] {
    match (n, shape) {
        (N_L, WindowShape::Sine) => SINE_LONG.as_slice(),
        (N_L, WindowShape::Kbd) => KBD_LONG.as_slice(),
        (N_S, WindowShape::Sine) => SINE_SHORT.as_slice(),
        (N_S, WindowShape::Kbd) => KBD_SHORT.as_slice(),
        _ => SINE_LONG.as_slice(),
    }
}

/// Per-channel overlap-add state.
#[derive(Clone, Debug)]
pub struct Filterbank {
    overlap: Vec<f64>,
    prev_shape: Option<WindowShape>,
    scratch: Vec<f64>,
}

impl Default for Filterbank {
    fn default() -> Self {
        Self::new()
    }
}

impl Filterbank {
    /// Zero overlap, no previous shape (first frame uses its own shape on both halves).
    #[must_use]
    pub fn new() -> Self {
        Self {
            overlap: vec![0.0; LONG_WINDOW_LEN],
            prev_shape: None,
            scratch: vec![0.0; N_L],
        }
    }

    /// Synthesize 1024 PCM samples from a 1024-bin window-major spectrum.
    pub fn synthesize(&mut self, spec: &[f64], ics: &IcsInfo) -> Result<Vec<f64>> {
        let mut out = Vec::new();
        self.synthesize_into(spec, ics, &mut out)?;
        Ok(out)
    }

    /// Write one frame into `dst`.
    pub fn synthesize_into(
        &mut self,
        spec: &[f64],
        ics: &IcsInfo,
        dst: &mut Vec<f64>,
    ) -> Result<()> {
        let z = self.windowed(spec, ics)?;
        let half_n = LONG_WINDOW_LEN;
        dst.clear();
        dst.extend(
            z[..half_n]
                .iter()
                .zip(self.overlap.iter())
                .map(|(&zn, &on)| zn + on),
        );
        self.overlap.copy_from_slice(&z[half_n..]);
        self.prev_shape = Some(ics.window_shape);
        Ok(())
    }

    fn windowed(&mut self, spec: &[f64], ics: &IcsInfo) -> Result<Vec<f64>> {
        let left = self.prev_shape.unwrap_or(ics.window_shape);
        let right = ics.window_shape;
        match ics.window_sequence {
            WindowSequence::OnlyLong | WindowSequence::LongStart | WindowSequence::LongStop => {
                self.long_windowed(spec, left, right, ics.window_sequence)
            }
            WindowSequence::EightShort => self.short_windowed(spec, left, right),
        }
    }

    fn long_windowed(
        &mut self,
        spec: &[f64],
        left: WindowShape,
        right: WindowShape,
        seq: WindowSequence,
    ) -> Result<Vec<f64>> {
        if spec.len() != LONG_WINDOW_LEN {
            return Err(Error::FilterbankInvalid);
        }
        if self.scratch.len() != N_L {
            self.scratch.resize(N_L, 0.0);
        }
        imdct_into(spec, &mut self.scratch);
        let w = long_window(left, right, seq);
        Ok(self
            .scratch
            .iter()
            .zip(w.iter())
            .map(|(&x, &wv)| x * wv)
            .collect())
    }

    fn short_windowed(
        &self,
        spec: &[f64],
        left: WindowShape,
        right: WindowShape,
    ) -> Result<Vec<f64>> {
        if spec.len() != 8 * SHORT_WINDOW_LEN {
            return Err(Error::FilterbankInvalid);
        }
        let mut z = vec![0.0f64; N_L];
        let start = (N_L - N_S) / 4;
        let hop = N_S / 2;
        let mut x = vec![0.0f64; N_S];
        for j in 0..8 {
            let coeffs = &spec[j * SHORT_WINDOW_LEN..(j + 1) * SHORT_WINDOW_LEN];
            imdct_into(coeffs, &mut x);
            let this_left = if j == 0 { left } else { right };
            let lh = half(N_S, this_left);
            let rh = half(N_S, right);
            let base = start + j * hop;
            for n in 0..N_S / 2 {
                z[base + n] += x[n] * lh[n];
            }
            for n in 0..N_S / 2 {
                z[base + N_S / 2 + n] += x[N_S / 2 + n] * rh[N_S / 2 - 1 - n];
            }
        }
        Ok(z)
    }
}

fn long_window(left: WindowShape, right: WindowShape, seq: WindowSequence) -> Vec<f64> {
    let mut w = vec![0.0f64; N_L];
    let half_l = N_L / 2;
    let lh = half(N_L, left);
    let rh = half(N_L, right);
    let sh_l = half(N_S, left);
    let sh_r = half(N_S, right);
    match seq {
        WindowSequence::OnlyLong => {
            w[..half_l].copy_from_slice(lh);
            for i in 0..half_l {
                w[half_l + i] = rh[half_l - 1 - i];
            }
        }
        WindowSequence::LongStart => {
            w[..half_l].copy_from_slice(lh);
            let b = (3 * N_L - N_S) / 4;
            for slot in w.iter_mut().take(b).skip(half_l) {
                *slot = 1.0;
            }
            for (m, i) in (0..N_S / 2).enumerate() {
                w[b + m] = sh_r[N_S / 2 - 1 - i];
            }
        }
        WindowSequence::LongStop => {
            let a = (N_L - N_S) / 4;
            for (m, &sv) in sh_l.iter().enumerate() {
                w[a + m] = sv;
            }
            for slot in w.iter_mut().take(half_l).skip(a + N_S / 2) {
                *slot = 1.0;
            }
            for i in 0..half_l {
                w[half_l + i] = rh[half_l - 1 - i];
            }
        }
        WindowSequence::EightShort => {}
    }
    w
}
