//! Minimal psychoacoustic model: per-band energies, a Bark-domain spreading
//! matrix (computed once per encoder), flat 18 dB signal-to-mask ratio.
//!
//! Output is a per-band quantization-precision target: bands whose energy
//! clears the masked threshold get full precision, partially masked bands
//! get proportionally less, fully masked bands are dropped (ZERO_HCB). The
//! rate loop's global offset still applies on top. Not a hearing model —
//! just enough masking to stop coding inaudible sidelobes at full price.

use super::enc_quant::MAX_BANDS;

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
    /// scratch for band energies
    energy: [f32; MAX_BANDS],
}

/// Approximate Bark scale of `f` Hz.
fn bark(f: f32) -> f32 {
    13.0 * (0.00076 * f).atan() + 3.5 * (f / 7500.0).powi(2).atan()
}

impl Psy {
    /// Build the spreading matrix for this rate's long-window bands.
    pub fn new(offsets: &[u16], sample_rate: u32) -> Self {
        let n_bands = offsets.len() - 1;
        let bin_hz = sample_rate as f32 / 2048.0;
        let mut zb = [0.0f32; MAX_BANDS];
        for (b, z) in zb.iter_mut().enumerate().take(n_bands) {
            let center = (f32::from(offsets[b]) + f32::from(offsets[b + 1])) * 0.5 * bin_hz;
            *z = bark(center.max(20.0));
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
                *cell = 10f32.powf(-att / 10.0);
            }
        }
        Self {
            n_bands,
            spread,
            energy: [0.0; MAX_BANDS],
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
    /// clears both the masked threshold (18 dB SMR after spreading) and a
    /// −60 dB relative-to-loudest floor. Coded bands get the full precision
    /// target `max_q` — the rate loop's global offset trades precision for
    /// bits uniformly on top.
    /// TODO: scale `target_q` by band tonality / partial masking.
    pub fn analyze(
        &mut self,
        spec: &[f32; 1024],
        offsets: &[u16],
        max_q: f32,
        coded: &mut [bool; MAX_BANDS],
        target_q: &mut [f32; MAX_BANDS],
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
        let bands = self
            .energy
            .iter()
            .zip(thresh.iter())
            .zip(coded.iter_mut())
            .zip(target_q.iter_mut())
            .take(self.n_bands);
        for (((&e, &t), c), tq) in bands {
            *c = e > t.max(abs_floor);
            *tq = if *c { max_q } else { 0.0 };
        }
    }
}

#[cfg(test)]
#[path = "enc_psy_tests.rs"]
mod enc_psy_tests;
