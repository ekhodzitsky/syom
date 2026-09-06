//! `LcEncoder` — per-frame LC pipeline: window + forward MDCT → psy model
//! (per-band precision targets) → quantize → section plan → rate loop (one
//! global sf offset) → `raw_data_block()` bytes.
//!
//! v1 scope: long windows only (no block switching — pre-echo on attacks is
//! accepted; TODO attack detector), no TNS/PNS/intensity, mono/stereo,
//! no bit reservoir (`adts_buffer_fullness = 0x7FF` VBR marker).

use super::adts::ADTS_SAMPLE_RATES_HZ;
use super::bits::BitWriter;
use super::enc_psy::Psy;
use super::enc_quant::{self, MAX_BANDS, QuantChannel};
use super::enc_section;
use super::error::{Error, Result};
use super::filterbank::window_left;
use super::ics::{WindowSequence, WindowShape};
use super::mdct::mdct_into_f32;
use super::swb::{LONG_WINDOW_LEN, long_offsets};

/// ADTS `raw_data_block` payload ceiling (8191-byte frame − 7-byte header).
pub const MAX_PAYLOAD_BYTES: usize = 8184;
/// Flat quality target: band peaks quantize to about this magnitude before
/// the rate loop trades it against the bit budget.
const TARGET_Q: f32 = 2048.0;
/// Rate-loop search bounds for the global scalefactor offset. Lower offset
/// = larger quantized values = finer quantization = more bits; the floor
/// stays clear of the escape-saturation plateau.
const OFFSET_LO: i32 = -8;
const OFFSET_HI: i32 = 80;

/// AAC-LC encoder state (push-shaped: one `raw_data_block` per 1024
/// samples, so a streaming wrapper is additive later).
pub struct LcEncoder {
    fs_index: u8,
    sample_rate: u32,
    channels: usize,
    bitrate_bps: u32,
    offsets: &'static [u16],
    /// Full 2048-tap KBD window pre-scaled by 32768 (decoder filterbank
    /// output is s16-scaled before the public 1/32768 mapping).
    window: Box<[f32; 2 * LONG_WINDOW_LEN]>,
    prev: [[f32; LONG_WINDOW_LEN]; 2],
    chans: Box<[QuantChannel; 2]>,
    books: [[u8; MAX_BANDS]; 2],
    gains: [u8; 2],
    psy: Psy,
    target_q: [[f32; MAX_BANDS]; 2],
    /// Unspent bits carried forward (bounded at one frame's budget): the
    /// integer sf-offset step undershoots by up to ~11%, so frames may
    /// spend accumulated savings. ADTS carries the VBR fullness marker.
    credit: i64,
    /// This frame's CPE is M/S-coded (`ms_mask_present = 2`).
    ms_used: bool,
}

impl LcEncoder {
    /// `channels` must be 1 or 2; `sample_rate` must be in the ADTS table.
    pub fn new(sample_rate: u32, channels: usize, bitrate_bps: u32) -> Result<Self> {
        let fs_index = ADTS_SAMPLE_RATES_HZ
            .iter()
            .position(|&r| r == sample_rate)
            .ok_or(Error::UnsupportedSampleRateIndex(0xFF))? as u8;
        if !(1..=2).contains(&channels) {
            return Err(Error::Format("LC encoder: channels must be 1 or 2"));
        }
        if bitrate_bps == 0 {
            return Err(Error::Format("LC encoder: bitrate must be > 0"));
        }
        let offsets = long_offsets(fs_index)?;
        // KBD both halves (alpha 4, matching the decoder table): far better
        // sidelobe rejection than sine, so masked-band decisions aren't
        // fighting analysis leakage. The ics_info shape bit matches.
        let half = window_left(2 * LONG_WINDOW_LEN, WindowShape::Kbd);
        let mut window = Box::new([0.0f32; 2 * LONG_WINDOW_LEN]);
        for i in 0..LONG_WINDOW_LEN {
            window[i] = half[i] * 32768.0;
            window[2 * LONG_WINDOW_LEN - 1 - i] = half[i] * 32768.0;
        }
        let n_bands = offsets.len() - 1;
        Ok(Self {
            fs_index,
            sample_rate,
            channels,
            bitrate_bps,
            offsets,
            window,
            prev: [[0.0; LONG_WINDOW_LEN]; 2],
            chans: Box::new([QuantChannel::new(n_bands), QuantChannel::new(n_bands)]),
            books: [[0; MAX_BANDS]; 2],
            gains: [100; 2],
            psy: Psy::new(offsets, sample_rate),
            target_q: [[0.0; MAX_BANDS]; 2],
            credit: 0,
            ms_used: false,
        })
    }

    /// Wire `sampling_frequency_index` (for the ADTS header / ASC).
    #[must_use]
    pub fn fs_index(&self) -> u8 {
        self.fs_index
    }

    /// Per-frame bit budget from the target bitrate (no reservoir).
    fn budget_bits(&self) -> usize {
        let bits = u64::from(self.bitrate_bps) * 1024 / u64::from(self.sample_rate);
        (bits as usize).min(MAX_PAYLOAD_BYTES * 8)
    }

    /// Encode 1024 samples per channel into one `raw_data_block`.
    pub fn encode_frame(&mut self, pcm: &[&[f32]]) -> Result<Vec<u8>> {
        if pcm.len() != self.channels || pcm.iter().any(|c| c.len() != LONG_WINDOW_LEN) {
            return Err(Error::Format("LC encoder: bad frame shape"));
        }
        let mut specs = [[0.0f32; LONG_WINDOW_LEN]; 2];
        for ((spec, prev), plane) in specs.iter_mut().zip(self.prev.iter_mut()).zip(pcm.iter()) {
            let mut buf = [0.0f32; 2 * LONG_WINDOW_LEN];
            buf[..LONG_WINDOW_LEN].copy_from_slice(prev);
            buf[LONG_WINDOW_LEN..].copy_from_slice(plane);
            for (b, &w) in buf.iter_mut().zip(self.window.iter()) {
                *b *= w;
            }
            mdct_into_f32(&buf, spec);
            prev.copy_from_slice(plane);
        }
        self.ms_used = self.channels == 2 && self.decide_ms(&mut specs);
        let budget = self.budget_bits();
        let spend = budget + (self.credit.min(budget as i64 / 2)) as usize;
        let offset = self.search_offset(&specs, spend);
        let bits = self.build(&specs, offset);
        if bits > spend {
            self.drop_bands_until(spend);
        }
        let out = self.emit();
        self.credit =
            (self.credit + budget as i64 - (out.len() * 8) as i64).clamp(0, budget as i64);
        Ok(out)
    }

    /// Per-frame M/S decision (whole-frame, `ms_mask_present` 0 or 2): code
    /// (m, s) = ((l+r)/2, (l−r)/2) when the side channel is quieter than
    /// the right channel (correlated content concentrates into mid). On a
    /// decision to use M/S, `specs` is transformed in place. TODO: per-band
    /// ms_used bits, binaural masking margin.
    fn decide_ms(&self, specs: &mut [[f32; LONG_WINDOW_LEN]; 2]) -> bool {
        let mut e_side = 0.0f64;
        let mut e_right = 0.0f64;
        {
            let [l, r] = specs;
            for (&lv, &rv) in l.iter().zip(r.iter()) {
                let s = f64::from((lv - rv) * 0.5);
                e_side += s * s;
                e_right += f64::from(rv) * f64::from(rv);
            }
        }
        if e_side >= e_right {
            return false;
        }
        let [l, r] = specs;
        for (lv, rv) in l.iter_mut().zip(r.iter_mut()) {
            let (a, b) = (*lv, *rv);
            *lv = (a + b) * 0.5;
            *rv = (a - b) * 0.5;
        }
        true
    }

    /// Smallest global sf offset whose frame fits the bit budget (frame
    /// bits decrease as the offset grows; the smallest fitting offset is
    /// the finest quantization we can afford).
    fn search_offset(&mut self, specs: &[[f32; LONG_WINDOW_LEN]; 2], spend: usize) -> i32 {
        let budget = spend.saturating_sub(32);
        if self.build(specs, OFFSET_LO) <= budget {
            return OFFSET_LO; // maximum quality fits
        }
        if self.build(specs, OFFSET_HI) > budget {
            return OFFSET_HI; // over budget even at ceiling: cap logic takes over
        }
        let (mut lo, mut hi) = (OFFSET_LO, OFFSET_HI);
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if self.build(specs, mid) <= budget {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    }

    /// Quantize + plan both channels at `offset`; returns total frame bits.
    fn build(&mut self, specs: &[[f32; LONG_WINDOW_LEN]; 2], offset: i32) -> usize {
        let standalone = self.channels == 1;
        let mut total = 3 + 4 + 3; // element id + tag + END
        if self.channels == 2 {
            total += 1 + 11 + 2; // common_window + ics_info + ms_mask
        }
        let channels = self
            .chans
            .iter_mut()
            .zip(self.target_q.iter_mut())
            .zip(specs.iter())
            .zip(self.books.iter_mut())
            .zip(self.gains.iter_mut())
            .take(self.channels);
        for ((((q, tq), spec), books), gain) in channels {
            self.psy
                .analyze(spec, self.offsets, TARGET_Q, &mut q.coded, tq);
            let mut peaks = [0.0f32; MAX_BANDS];
            enc_quant::band_peaks(spec, self.offsets, &mut peaks);
            enc_quant::raw_scalefactors(&peaks, tq, offset, q);
            *gain = enc_quant::normalize_sf(q);
            enc_quant::quantize(spec, self.offsets, q);
            *books = enc_section::plan_books(q);
            total += enc_section::channel_body_bits(books, q, *gain, standalone);
        }
        total + 7 // byte-align pad ceiling
    }

    /// Hard cap: drop the highest coded bands until the frame fits
    /// `limit_bits` (the bit budget, and never above the ADTS payload
    /// limit). Quality collapse is legal, an oversize frame is not.
    fn drop_bands_until(&mut self, limit_bits: usize) {
        let standalone = self.channels == 1;
        let cap = limit_bits.min(MAX_PAYLOAD_BYTES * 8);
        loop {
            let mut total = 3 + 4 + 3 + 7;
            if self.channels == 2 {
                total += 1 + 11 + 2;
            }
            let channels = self
                .chans
                .iter()
                .zip(self.books.iter())
                .zip(self.gains.iter())
                .take(self.channels);
            for ((q, books), gain) in channels {
                total += enc_section::channel_body_bits(books, q, *gain, standalone);
            }
            if total <= cap {
                return;
            }
            // Find the highest coded band across channels and silence it.
            let mut hit: Option<(usize, usize)> = None;
            for (ch, q) in self.chans.iter().enumerate().take(self.channels) {
                if let Some(b) = q.coded[..q.n_bands].iter().rposition(|&c| c) {
                    hit = Some((ch, b));
                    break;
                }
            }
            let Some((ch, b)) = hit else { return };
            self.chans[ch].coded[b] = false;
            self.books[ch][b] = 0;
            self.books[ch] = enc_section::plan_books(&self.chans[ch]);
        }
    }

    /// Emit the `raw_data_block` from the built state.
    fn emit(&self) -> Vec<u8> {
        let max_sfb = self.chans[0].n_bands as u8;
        let mut w = BitWriter::new();
        if self.channels == 1 {
            w.write(0, 3); // SCE
            w.write(0, 4); // tag
            enc_section::emit_channel_body(
                &mut w,
                self.offsets,
                WindowSequence::OnlyLong,
                &self.books[0],
                &self.chans[0],
                self.gains[0],
                true,
            );
        } else {
            w.write(1, 3); // CPE
            w.write(0, 4); // tag
            w.write_bit(true); // common_window
            enc_section::emit_ics_info(&mut w, WindowSequence::OnlyLong, max_sfb);
            w.write(u32::from(self.ms_used) * 2, 2); // ms_mask_present: 0 or 2
            let channels = self
                .chans
                .iter()
                .zip(self.books.iter())
                .zip(self.gains.iter());
            for ((q, books), gain) in channels {
                enc_section::emit_channel_body(
                    &mut w,
                    self.offsets,
                    WindowSequence::OnlyLong,
                    books,
                    q,
                    *gain,
                    false,
                );
            }
        }
        w.write(7, 3); // END
        w.finish()
    }
}

#[cfg(test)]
#[path = "enc_frame_tests.rs"]
mod enc_frame_tests;
