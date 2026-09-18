//! Per-frame rate control of [`LcEncoder`] (line cap): the ABR loop
//! (search the allowed-noise offset that fits `bitrate_bps` plus one
//! frame of credit, refine, pad — decision-8 / TASK-66) or quality VBR
//! (TASK-67: one fixed offset, no target rate, no padding, only the
//! 6144 bits/channel cap).

use super::LcEncoder;
use super::rate::max_frame_bits;
use crate::engine::error::{Error, Result};
use crate::engine::swb::LONG_WINDOW_LEN;

/// Allowed-noise offset steps (0.75 dB of energy each) per quality level:
/// level 5 is the psy target; every level above refines 6 dB, every level
/// below raises the water level 6 dB (quiet bands leave first).
const STEPS_PER_LEVEL: i32 = 8;
/// Highest quality level.
pub const QUALITY_MAX: u8 = 10;

impl LcEncoder {
    /// Quality VBR level 0..=10 (`None` = ABR). Level 5 codes at the psy
    /// model's noise-to-mask target; each level is 6 dB of allowed noise.
    #[must_use]
    pub(crate) fn with_quality(mut self, level: Option<u8>) -> Self {
        self.quality = level.map(|q| (5 - i32::from(q.min(QUALITY_MAX))) * STEPS_PER_LEVEL);
        self
    }

    /// `|x|^0.75` once per frame: every `build` re-quantizes these spectra.
    pub(super) fn cache_mags(&mut self, specs: &[[f32; LONG_WINDOW_LEN]; 2]) {
        for (ch, spec) in specs.iter().enumerate().take(self.channels) {
            if self.seq.is_eight_short() {
                crate::engine::enc_quant::cache_mags(spec, &mut self.chans_s[ch].mag);
            } else {
                crate::engine::enc_quant::cache_mags(spec, &mut self.chans[ch].mag);
            }
        }
    }

    /// Quantize the built spectra into `payload` under the active mode.
    pub(super) fn rate_control(&mut self, specs: &[[f32; LONG_WINDOW_LEN]; 2]) -> Result<()> {
        let cap = max_frame_bits(self.channels);
        self.cache_mags(specs);
        let mut payload = std::mem::take(&mut self.payload);
        if let Some(offset) = self.quality {
            // The level's offset, coarsened uniformly (never by dropping
            // top bands) when a frame would exceed the LC cap.
            let offset = if self.build(specs, offset) > cap {
                self.search_offset_from(offset, specs, cap)
            } else {
                offset
            };
            self.build(specs, offset);
            self.drop_bands_until(cap);
            self.emit_into(&mut payload);
            self.credit = 0;
        } else {
            let budget = self.budget_bits();
            let spend = budget + (self.credit.min(budget as i64 / 2)) as usize;
            let offset = self.search_offset(specs, spend);
            self.build(specs, offset);
            self.refine_bands(specs, spend);
            let coded = self.fit_budget(spend.min(cap), &mut payload);
            self.credit = (self.credit + budget as i64 - coded as i64).clamp(0, budget as i64);
        }
        self.payload = payload;
        if self.payload.len().saturating_mul(8) > cap {
            return Err(Error::Format("LC encoder: frame exceeds 6144 bits/channel"));
        }
        self.prev_seq = self.seq;
        Ok(())
    }
}
