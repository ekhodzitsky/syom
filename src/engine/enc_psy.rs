//! Minimal psychoacoustic model: per-band energies, a Bark-domain spreading
//! matrix (computed once per encoder), flat 18 dB signal-to-mask ratio.
//!
//! Output is a per-band quantization-precision target: bands whose energy
//! clears the masked threshold get full precision, fully masked bands are
//! dropped (ZERO_HCB). The rate loop's global offset still applies on top.
//! Default is not a hearing model — just enough masking to stop coding
//! inaudible sidelobes at full price. Opt-in Terhardt ATH (TASK-68) raises
//! the floor at insensitive frequencies; off by default so goldens match.

use super::det_math;
use super::enc_quant::MAX_BANDS;
use super::swb::LONG_WINDOW_LEN;

/// Flat signal-to-mask ratio (18 dB).
const SMR: f32 = 63.1;
/// Spreading slopes: 25 dB/Bark upward from the masker, 60 dB/Bark down,
/// only within ±8 Bark.
const SPREAD_UP_DB: f32 = 25.0;
const SPREAD_DOWN_DB: f32 = 60.0;
const SPREAD_RANGE_BARK: f32 = 8.0;

/// Per-rate psy state: Bark center per band + spreading matrix.
pub struct Psy {
    n_bands: usize,
    /// `spread[i][j]`: fraction of band `j`'s energy masking band `i`.
    spread: Box<[[f32; MAX_BANDS]; MAX_BANDS]>,
    energy: [f32; MAX_BANDS],
    /// Absolute-threshold energy per band (Terhardt, 0 dBFS = 96 dB SPL).
    ath_energy: [f32; MAX_BANDS],
    ath: bool,
}

/// Approximate Bark scale of `f` Hz. The `atan`s go through
/// [`det_math`](super::det_math): libm `atan` is not bit-identical across
/// platforms, and a 1-ulp band-center drift can flip a masking decision.
fn bark(f: f32) -> f32 {
    13.0 * det_math::atan(0.00076 * f) + 3.5 * det_math::atan((f / 7500.0).powi(2))
}

/// Terhardt (1979) simplified ATH in dB SPL. `f_hz` is clamped to 20 Hz
/// .. 24 kHz so the `f^4` term stays finite. Via [`det_math`], not libm.
fn ath_db_spl(f_hz: f32) -> f32 {
    let f = (f_hz * 0.001).clamp(0.02, 24.0);
    let inv = det_math::exp2(-0.8 * det_math::log2(f));
    let d = f - 3.3;
    let gauss = det_math::exp2(-0.6 * d * d * std::f32::consts::LOG2_E);
    3.64 * inv - 6.5 * gauss + 0.001 * f * f * f * f
}

/// 0 dBFS peak PCM = 96 dB SPL. One MDCT bin of a full-scale sine: window
/// peak 32768, 2048-point coherent gain ≈ 512.
const FS_SPL: f32 = 96.0;
const FS_BIN: f32 = 32768.0 * 512.0;

fn ath_band_energy(f_hz: f32) -> f32 {
    let rel = det_math::exp2((ath_db_spl(f_hz) - FS_SPL) / 10.0 * det_math::LOG2_10);
    FS_BIN * FS_BIN * rel
}

impl Psy {
    /// Build the spreading matrix for this rate's long-window bands.
    pub fn new(offsets: &[u16], sample_rate: u32) -> Self {
        Self::with_transform(offsets, sample_rate, 2 * LONG_WINDOW_LEN)
    }

    /// Spreading matrix for short-window bands (128-bin spectra, 8× wider
    /// bins than the long transform).
    pub fn new_short(offsets: &[u16], sample_rate: u32) -> Self {
        Self::with_transform(offsets, sample_rate, 256)
    }

    /// Terhardt absolute-threshold floor (default off). When on, a band
    /// must also clear ATH energy at its center; silence stays uncoded.
    pub fn enable_ath(&mut self, on: bool) {
        self.ath = on;
    }

    fn with_transform(offsets: &[u16], sample_rate: u32, transform: usize) -> Self {
        let n_bands = offsets.len() - 1;
        let bin_hz = sample_rate as f32 / transform as f32;
        let mut zb = [0.0f32; MAX_BANDS];
        let mut ath_energy = [0.0f32; MAX_BANDS];
        for (b, z) in zb.iter_mut().enumerate().take(n_bands) {
            let center = (f32::from(offsets[b]) + f32::from(offsets[b + 1])) * 0.5 * bin_hz;
            let f = center.max(20.0);
            *z = bark(f);
            ath_energy[b] = ath_band_energy(f);
        }
        let mut spread = Box::new([[0.0f32; MAX_BANDS]; MAX_BANDS]);
        for (i, row) in spread.iter_mut().enumerate().take(n_bands) {
            for (j, cell) in row.iter_mut().enumerate().take(n_bands) {
                let dz = zb[i] - zb[j];
                if dz.abs() > SPREAD_RANGE_BARK {
                    continue;
                }
                let att = if dz >= 0.0 {
                    SPREAD_UP_DB * dz
                } else {
                    SPREAD_DOWN_DB * (-dz)
                };
                *cell = det_math::exp2(-att / 10.0 * det_math::LOG2_10);
            }
        }
        Self {
            n_bands,
            spread,
            energy: [0.0; MAX_BANDS],
            ath_energy,
            ath: false,
        }
    }

    /// Per-band masked thresholds (power domain) from the band energies.
    fn masked_thresholds(&self, energy: &[f32; MAX_BANDS], out: &mut [f32; MAX_BANDS]) {
        for (i, slot) in out.iter_mut().enumerate().take(self.n_bands) {
            let mut acc = 0.0f32;
            for (j, &e) in energy.iter().enumerate().take(self.n_bands) {
                acc += e * self.spread[i][j];
            }
            *slot = acc / SMR;
        }
    }

    /// Analyze one channel's MDCT spectrum: a band is coded when its energy
    /// clears the masked threshold (18 dB SMR after spreading) and a −60 dB
    /// relative-to-loudest floor. With ATH on, also the Terhardt floor at
    /// the band center. Coded bands get full-precision `max_q`.
    /// TODO: scale `target_q` by band tonality / partial masking.
    pub fn analyze(
        &mut self,
        spec: &[f32],
        offsets: &[u16],
        max_q: f32,
        coded: &mut [bool],
        target_q: &mut [f32],
    ) {
        for (b, e) in self.energy.iter_mut().enumerate().take(self.n_bands) {
            let lo = usize::from(offsets[b]);
            let hi = usize::from(offsets[b + 1]);
            *e = spec[lo..hi].iter().map(|&x| x * x).sum();
        }
        let mut thresh = [0.0f32; MAX_BANDS];
        self.masked_thresholds(&self.energy, &mut thresh);
        let max_e = self.energy[..self.n_bands]
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        let abs_floor = max_e * 1e-6; // −60 dB below the loudest band
        for b in 0..self.n_bands {
            let t = thresh[b];
            let floor = if self.ath {
                t.max(abs_floor).max(self.ath_energy[b])
            } else {
                t.max(abs_floor)
            };
            coded[b] = self.energy[b] > floor;
            target_q[b] = if coded[b] { max_q } else { 0.0 };
        }
    }
}

/// Attack detector: per-channel high-passed sub-block energy surge vs a
/// running mean.
///
/// Samples are first differenced (`y[n] = x[n] − x[n−1]`, a first-order
/// high-pass) so sustained low-frequency tones don't dominate the energy
/// budget and the surge ratio stays meaningful at any tone level. Each
/// 1024-sample frame is split into 8 sub-blocks of 128 samples; a sub-block
/// whose energy exceeds [`ATTACK_RATIO`] × the mean energy of the previous
/// 16 sub-blocks (two frames) flags a transient. An all-silent history has
/// mean 0, so any sub-block above [`ATTACK_FLOOR`] flags — an onset from
/// digital silence is an attack — while the very first sub-block ever seen
/// never flags (nothing to compare against; stream-start pre-echo lands in
/// the priming frame anyway). Only `+` / `*` / `−` on f32: platform-
/// deterministic by IEEE 754.
pub struct AttackDetector {
    /// Ring of recent sub-block energies (oldest at `pos` while `len` is full).
    hist: [f32; 16],
    /// Last sample of the previous frame (high-pass state).
    prev_sample: f32,
    pos: usize,
    len: usize,
}

/// 128-sample sub-blocks per 1024-sample frame.
pub const SUB_BLOCK_LEN: usize = LONG_WINDOW_LEN / 8;
/// Surge ratio flagging an attack (~9 dB over the running mean).
const ATTACK_RATIO: f32 = 8.0;
/// Absolute high-passed sub-block energy floor: below this the sub-block is
/// silence to the detector (~−70 dBFS for transient content).
const ATTACK_FLOOR: f32 = 1e-6;

impl Default for AttackDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl AttackDetector {
    pub fn new() -> Self {
        Self {
            hist: [0.0; 16],
            prev_sample: 0.0,
            pos: 0,
            len: 0,
        }
    }

    /// Feed one frame of 1024 time-domain samples; `true` when any
    /// sub-block surges. Sub-blocks are scored in order and each joins the
    /// history immediately, so a slow swell that keeps pace with its own
    /// running mean does not flag.
    pub fn push(&mut self, cur: &[f32]) -> bool {
        debug_assert_eq!(cur.len(), LONG_WINDOW_LEN);
        let mut attack = false;
        let mut prev = self.prev_sample;
        for sb in cur.chunks_exact(SUB_BLOCK_LEN) {
            let mut e = 0.0f32;
            for &x in sb {
                let d = x - prev;
                e += d * d;
                prev = x;
            }
            if self.len > 0 && e > ATTACK_FLOOR {
                let mean: f32 = self.hist.iter().take(self.len).sum::<f32>() / self.len as f32;
                if e > ATTACK_RATIO * mean {
                    attack = true;
                }
            }
            self.hist[self.pos] = e;
            self.pos = (self.pos + 1) % self.hist.len();
            self.len = (self.len + 1).min(self.hist.len());
        }
        self.prev_sample = prev;
        attack
    }
}

#[cfg(test)]
#[path = "enc_psy_tests.rs"]
mod enc_psy_tests;
