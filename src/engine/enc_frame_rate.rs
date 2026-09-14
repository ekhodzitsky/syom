//! Rate loop + `raw_data_block` emission for [`LcEncoder`] — split from
//! `enc_frame.rs` for the line cap. A child module of `enc_frame`, so the
//! `impl` below keeps using the encoder's private fields; no new
//! abstraction boundary.

use super::{LcEncoder, MAX_BITS_PER_CHANNEL, MAX_PAYLOAD_BYTES, TARGET_Q};
use crate::engine::enc_quant::{self, MAX_BANDS};
use crate::engine::enc_section;
use crate::engine::enc_short;
use crate::engine::swb::LONG_WINDOW_LEN;

/// Hard LC frame bit ceiling for `channels` (1 or 2).
#[must_use]
pub fn max_frame_bits(channels: usize) -> usize {
    MAX_BITS_PER_CHANNEL.saturating_mul(channels)
}

/// Highest target bitrate that still fits [`MAX_BITS_PER_CHANNEL`].
#[must_use]
pub fn max_bitrate_bps(sample_rate: u32, channels: usize) -> u32 {
    let bits = (MAX_BITS_PER_CHANNEL as u64)
        .saturating_mul(channels as u64)
        .saturating_mul(u64::from(sample_rate))
        / 1024;
    bits.min(u64::from(u32::MAX)) as u32
}

/// Rate-loop search bounds for the global scalefactor offset. Lower offset
/// = larger quantized values = finer quantization = more bits; the floor
/// stays clear of the escape-saturation plateau.
const OFFSET_LO: i32 = -8;
const OFFSET_HI: i32 = 80;

impl LcEncoder {
    pub(crate) fn with_short_tns(mut self, on: bool) -> Self {
        self.short_tns = on;
        self
    }

    pub(crate) fn with_short_group(mut self, on: bool) -> Self {
        self.short_group = on;
        self
    }

    pub(crate) fn with_band_refine(mut self, on: bool) -> Self {
        self.band_refine = on;
        self
    }

    pub(crate) fn with_pns(mut self, on: bool) -> Self {
        self.pns = on;
        self
    }

    pub(crate) fn with_intensity(mut self, on: bool) -> Self {
        self.intensity = on;
        self
    }

    /// Per-band M/S (and optional IS skip) once per frame before the rate loop.
    pub(super) fn decide_stereo(
        &mut self,
        specs: &mut [[f32; LONG_WINDOW_LEN]; 2],
        psy_specs: &mut [[f32; LONG_WINDOW_LEN]; 2],
        long: bool,
    ) {
        self.ms = if self.channels == 2 {
            #[cfg(test)]
            let per_band = self.ms_per_band;
            #[cfg(not(test))]
            let per_band = true;
            if self.seq.is_eight_short() {
                self.is_band.fill(false);
                crate::engine::enc_ms::decide_short(
                    specs,
                    self.short_offsets,
                    per_band,
                    self.grouping,
                )
            } else {
                if self.intensity {
                    crate::engine::enc_is::plan(
                        specs,
                        self.offsets,
                        self.sample_rate,
                        &mut self.is_band,
                    );
                } else {
                    self.is_band.fill(false);
                }
                crate::engine::enc_ms::decide_long_skip(
                    specs,
                    self.offsets,
                    per_band,
                    &self.is_band,
                )
            }
        } else {
            crate::engine::enc_ms::MsBands::off()
        };
        if long && self.channels == 2 {
            self.ms.apply_long_to(psy_specs, self.offsets);
        }
    }

    pub(super) fn apply_tns(
        &mut self,
        specs: &mut [[f32; LONG_WINDOW_LEN]; 2],
        coded: &[[bool; crate::engine::enc_quant::MAX_BANDS]; 2],
        enabled: bool,
    ) {
        let offsets = if self.seq.is_eight_short() {
            self.short_offsets
        } else {
            self.offsets
        };
        self.tns = crate::engine::enc_tns::decide_frame(
            specs,
            self.channels,
            self.seq,
            offsets,
            self.fs_index,
            coded,
            enabled,
            self.short_tns,
        );
    }

    /// Smallest global sf offset whose frame fits the bit budget (frame
    /// bits decrease as the offset grows; the smallest fitting offset is
    /// the finest quantization we can afford).
    pub(super) fn search_offset(
        &mut self,
        specs: &[[f32; LONG_WINDOW_LEN]; 2],
        psy_specs: &[[f32; LONG_WINDOW_LEN]; 2],
        spend: usize,
    ) -> i32 {
        let budget = spend.saturating_sub(32);
        if self.build(specs, psy_specs, OFFSET_LO) <= budget {
            return OFFSET_LO; // maximum quality fits
        }
        if self.build(specs, psy_specs, OFFSET_HI) > budget {
            return OFFSET_HI; // over budget even at ceiling: cap logic takes over
        }
        let (mut lo, mut hi) = (OFFSET_LO, OFFSET_HI);
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if self.build(specs, psy_specs, mid) <= budget {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    }

    /// Quantize + plan both channels at `offset`; returns total frame bits.
    /// Long windows: psy thresholds come from `psy_specs` (the pre-TNS
    /// snapshot, M/S-transformed) while `specs` (the coded, TNS-filtered
    /// spectra) feed peaks and quantization.
    pub(super) fn build(
        &mut self,
        specs: &[[f32; LONG_WINDOW_LEN]; 2],
        psy_specs: &[[f32; LONG_WINDOW_LEN]; 2],
        offset: i32,
    ) -> usize {
        let standalone = self.channels == 1;
        let mut total = 3 + 4 + 3; // element id + tag + END
        if self.seq.is_eight_short() {
            if self.channels == 2 {
                total += 1 + 15 + self.ms.overhead_bits(); // cw + ics_info + ms_mask
            }
            for (ch, spec) in specs.iter().enumerate().take(self.channels) {
                total += enc_short::channel_build(
                    &mut self.psy_short,
                    self.short_offsets,
                    spec,
                    offset,
                    &mut self.chans_s[ch],
                    &mut self.target_q_s[ch],
                    &mut self.books_s[ch],
                    &mut self.gains[ch],
                    standalone,
                    &self.tns[ch],
                    self.grouping,
                );
            }
            return total + 7; // byte-align pad ceiling
        }
        if self.channels == 2 {
            // common_window + ics_info + ms_mask
            total += 1 + 11 + self.ms.overhead_bits();
        }
        for ch in 0..self.channels {
            let q = &mut self.chans[ch];
            q.pns[..q.n_bands].fill(false);
            q.intensity[..q.n_bands].fill(false);
            let tq = &mut self.target_q[ch];
            self.psy
                .analyze(&psy_specs[ch], self.offsets, TARGET_Q, &mut q.coded, tq);
            // TNS whitens its span flat: every band in it carries residual
            // energy the decoder's recursion needs — force them coded.
            if let Some((tns_lo, tns_hi)) = self.tns[ch].band_range() {
                let span = tq.iter_mut().enumerate().take(tns_hi.min(q.n_bands));
                for (b, tq_b) in span.skip(tns_lo) {
                    if !q.coded[b] {
                        q.coded[b] = true;
                        *tq_b = TARGET_Q;
                    }
                }
            }
            Self::quant_one_long(
                self.offsets,
                q,
                tq,
                &specs[ch],
                &mut self.books[ch],
                &mut self.gains[ch],
                offset,
            );
        }
        if self.pns {
            crate::engine::enc_pns::apply_frame(
                specs,
                self.offsets,
                self.sample_rate,
                self.channels,
                &mut self.chans[..],
                &mut self.books,
                &self.gains,
                &self.ms,
                &self.tns,
            );
        }
        if self.intensity && self.channels == 2 {
            crate::engine::enc_is::stamp(
                specs,
                self.offsets,
                &self.is_band,
                &mut self.chans[..],
                &mut self.books,
            );
        }
        for ch in 0..self.channels {
            total += enc_section::channel_body_bits(
                &self.books[ch],
                &self.chans[ch],
                self.gains[ch],
                standalone,
                &self.tns[ch],
            );
        }
        total + 7 // byte-align pad ceiling
    }

    /// Quantize + section-plan one long channel from existing `coded` / `target_q`.
    fn quant_one_long(
        offsets: &[u16],
        q: &mut crate::engine::enc_quant::QuantChannel,
        tq: &[f32; MAX_BANDS],
        spec: &[f32; LONG_WINDOW_LEN],
        books: &mut [u8; MAX_BANDS],
        gain: &mut u8,
        offset: i32,
    ) {
        let mut peaks = [0.0f32; MAX_BANDS];
        enc_quant::band_peaks(spec, offsets, &mut peaks);
        enc_quant::raw_scalefactors(&peaks, tq, offset, q);
        *gain = enc_quant::normalize_sf(q);
        enc_quant::quantize(spec, offsets, q);
        *books = enc_section::plan_books(q);
    }

    /// Hard cap: drop the highest coded bands until the frame fits
    /// `limit_bits` (the bit budget, never above 6144 bits/channel or
    /// the ADTS payload limit). Quality collapse is legal, an oversize
    /// frame is not.
    pub(super) fn drop_bands_until(&mut self, limit_bits: usize) {
        let cap = limit_bits
            .min(max_frame_bits(self.channels))
            .min(MAX_PAYLOAD_BYTES * 8);
        loop {
            if self.emit().len().saturating_mul(8) <= cap {
                return;
            }
            if self.seq.is_eight_short() {
                let mut hit = None;
                for (ch, q) in self.chans_s.iter().enumerate().take(self.channels) {
                    if let Some(b) = q.coded.iter().rposition(|&c| c) {
                        hit = Some((ch, b));
                        break;
                    }
                }
                let Some((ch, b)) = hit else { return };
                self.chans_s[ch].coded[b] = false;
                self.books_s[ch] = enc_short::plan_books_short(&self.chans_s[ch]);
            } else {
                let mut hit = None;
                for (ch, q) in self.chans.iter().enumerate().take(self.channels) {
                    if let Some(b) = q.coded[..q.n_bands].iter().rposition(|&c| c) {
                        hit = Some((ch, b));
                        break;
                    }
                }
                let Some((ch, b)) = hit else { return };
                self.chans[ch].coded[b] = false;
                self.chans[ch].pns[b] = false;
                self.chans[ch].intensity[b] = false;
                self.books[ch][b] = 0;
                self.books[ch] = enc_section::plan_books(&self.chans[ch]);
            }
        }
    }

    /// Emit the `raw_data_block` from the built state.
    pub(super) fn emit(&self) -> Vec<u8> {
        if self.seq.is_eight_short() {
            return enc_short::emit_frame(
                self.short_offsets,
                &self.chans_s[..],
                &self.books_s[..],
                &self.gains,
                &self.ms,
                &self.tns,
                self.channels,
            );
        }
        enc_section::emit_frame(
            self.offsets,
            self.seq,
            &self.chans[..],
            &self.books[..],
            &self.gains,
            &self.ms,
            &self.tns,
            self.channels,
        )
    }

    /// Pad an undersized frame with unused bytes after `ID_END` so payload
    /// bits sit in `[97%, 100%]` of `limit`. Returns `(payload, coded_bits)`
    /// where `coded_bits` is the size before padding — one-frame `credit`
    /// must use that, or stuffing would starve the next frame's rate loop.
    /// The decoder stops at END; trailing zeros are unused ADTS bytes.
    /// All-zero spectra are not padded (silence exception). Not CBR.
    pub(super) fn fit_budget(&mut self, limit: usize) -> (Vec<u8>, usize) {
        let limit = limit.min(max_frame_bits(self.channels));
        self.drop_bands_until(limit);
        let mut out = self.emit();
        let coded = out.len().saturating_mul(8);
        if self.quant_all_zero() {
            return (out, coded);
        }
        // Pad to the ABR ceiling, minus stuffing debt from earlier frames
        // that spent credit above `budget`. Search/drop still use credit;
        // this only changes unused bytes after ID_END.
        let budget = self.budget_bits().min(limit);
        let pad_to = if self.pad_debt > 0 {
            budget.saturating_sub(self.pad_debt as usize)
        } else {
            budget
        };
        if coded < pad_to.saturating_mul(97) / 100 {
            let target = (pad_to / 8).min(MAX_PAYLOAD_BYTES);
            if out.len() < target {
                out.resize(target, 0);
            }
        }
        let emitted = out.len().saturating_mul(8) as i64;
        self.pad_debt = (self.pad_debt + emitted - budget as i64).max(0);
        (out, coded)
    }

    fn quant_all_zero(&self) -> bool {
        if self.seq.is_eight_short() {
            self.chans_s[..self.channels]
                .iter()
                .all(|q| q.quant.iter().all(|&x| x == 0))
        } else {
            self.chans[..self.channels]
                .iter()
                .all(|q| q.quant.iter().all(|&x| x == 0))
        }
    }
}
