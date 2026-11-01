//! Compare direct-output budget fitting with the original duplicated serializer.
#![allow(clippy::unwrap_used)]
use super::super::rate::take_emits;
use super::*;

#[test]
fn direct_budget_emission_preserves_bytes_credit_debt_and_avoids_reemission() {
    let mut appended_by_mode = [0; 4];
    for (mode, appended_count) in appended_by_mode.iter_mut().enumerate() {
        for channels in [1, 2] {
            let make = || {
                let mut enc = LcEncoder::new(
                    48_000,
                    channels,
                    if mode < 2 {
                        channels as u32 * 192_000
                    } else {
                        24_000
                    },
                )
                .unwrap()
                .with_short_group(true)
                .with_short_tns(true)
                .with_pns(mode == 3)
                .with_intensity(mode == 3)
                .with_band_refine(mode == 3)
                .with_cbr(mode == 1);
                if mode == 2 {
                    enc = enc.with_quality(Some(5));
                }
                enc
            };
            let (mut got, mut reference) = (make(), make());
            for frame in 0..if mode == 1 { 32 } else { 12 } {
                let pcm: Vec<Vec<f32>> = (0..channels)
                    .map(|ch| {
                        (0..1024)
                            .map(|i| {
                                let tone = 0.3
                                    * ((frame * 1024 + i) as f32 * (0.13 + ch as f32 * 0.07)).sin();
                                if i == 100 && frame % 3 == 1 {
                                    0.95
                                } else {
                                    tone
                                }
                            })
                            .collect()
                    })
                    .collect();
                let planes: Vec<_> = pcm.iter().map(Vec::as_slice).collect();
                let attack = frame % 3 == 1;
                let spectra = got.analyze(&planes, attack);
                let reference_spectra = reference.analyze(&planes, attack);
                let _ = take_emits();
                let _ = crate::engine::enc_pad::take_appends();
                got.rate_control(&spectra).unwrap();
                let got_emits = take_emits();
                let appended = crate::engine::enc_pad::take_appends();
                *appended_count += appended;
                reference
                    .reference_rate_control(&reference_spectra)
                    .unwrap();
                let reference_emits = take_emits();
                assert_eq!(
                    got.payload, reference.payload,
                    "mode={mode} channels={channels} frame={frame}"
                );
                assert_eq!(got.credit, reference.credit);
                assert_eq!(got.pad_debt, reference.pad_debt);
                assert_eq!(got.take_cbr_log(), reference.take_cbr_log());
                assert_eq!(
                    got_emits + 1 + appended,
                    reference_emits,
                    "mode={mode} channels={channels} frame={frame}"
                );
            }
        }
    }
    assert!(
        appended_by_mode[0] > 0 && appended_by_mode[1] > 0,
        "reference must exercise actual ABR and CBR padding appends: {appended_by_mode:?}"
    );
}

#[test]
fn direct_band_drops_match_original_including_no_band_fit() {
    for short in [false, true] {
        for limit in [1, 100, 400, 1000, 12_288] {
            let prepare = || {
                let mut enc = LcEncoder::new(48_000, 2, 128_000).unwrap();
                let pcm: Vec<_> = (0..1024).map(|i| 0.4 * (i as f32 * 0.13).sin()).collect();
                if short {
                    enc.prev_seq = crate::engine::ics::WindowSequence::LongStart;
                }
                let spectra = enc.analyze(&[&pcm, &pcm], short);
                enc.cache_mags(&spectra);
                enc.build(&spectra, -60);
                enc
            };
            let (mut got, mut reference) = (prepare(), prepare());
            let _ = take_emits();
            let mut bytes = Vec::new();
            let end = got.drop_bands_until(limit, &mut bytes);
            let got_emits = take_emits();
            reference.reference_drop_bands_until(limit);
            let mut want = Vec::new();
            let reference_end = reference.emit_into(&mut want);
            let reference_emits = take_emits();
            assert_eq!(bytes, want, "short={short} limit={limit}");
            assert_eq!(end, reference_end);
            assert_eq!(got_emits + 1, reference_emits);
        }
    }
}

impl LcEncoder {
    /// Quantize the built spectra into `payload` under the active mode.
    pub(crate) fn reference_rate_control(
        &mut self,
        specs: &[[f32; LONG_WINDOW_LEN]; 2],
    ) -> Result<()> {
        let cap = max_frame_bits(self.channels);
        self.cache_mags(specs);
        let mut payload = std::mem::take(&mut self.payload);
        if let Some(offset) = self.quality {
            // The level's offset, coarsened uniformly (never by dropping
            // top bands) when a frame would exceed the LC cap.
            if self.build(specs, offset) > cap {
                self.search_offset_from(offset, specs, cap);
            }
            self.reference_drop_bands_until(cap);
            self.emit_into(&mut payload);
            self.credit = 0;
        } else if self.cbr.is_some() {
            // Opt-in reservoir. The ABR arm below is the default path.
            self.reference_rate_control_cbr(specs, &mut payload)?;
        } else {
            let budget = self.budget_bits();
            let spend = budget + (self.credit.min(budget as i64 / 2)) as usize;
            self.search_offset(specs, spend);
            self.refine_bands(specs, spend);
            let coded = self.reference_fit_budget(spend.min(cap), &mut payload);
            self.credit = (self.credit + budget as i64 - coded as i64).clamp(0, budget as i64);
        }
        self.payload = payload;
        if self.payload.len().saturating_mul(8) > cap {
            return Err(Error::Format("LC encoder: frame exceeds 6144 bits/channel"));
        }
        self.prev_seq = self.seq;
        Ok(())
    }

    /// Hard cap: drop the highest coded bands until the frame fits
    /// `limit_bits` (the bit budget, never above 6144 bits/channel or
    /// the ADTS payload limit). Quality collapse is legal, an oversize
    /// frame is not.
    pub(crate) fn reference_drop_bands_until(&mut self, limit_bits: usize) {
        let cap = limit_bits
            .min(max_frame_bits(self.channels))
            .min(MAX_PAYLOAD_BYTES * 8);
        loop {
            let mut tmp = std::mem::take(&mut self.payload);
            self.emit_into(&mut tmp);
            let bits = tmp.len().saturating_mul(8);
            self.payload = tmp;
            if bits <= cap {
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

    /// Pad an undersized frame with EXT_FILL `fill_element()`s before
    /// `ID_END` so payload bits sit in `[97%, 100%]` of `limit` (zero
    /// bytes after `ID_END` are rejected by fdk-aac — TASK-121). Returns
    /// the size before padding — one-frame `credit` must use that, or
    /// stuffing would starve the next frame's rate loop. All-zero spectra
    /// are not padded (silence exception). Not CBR.
    pub(crate) fn reference_fit_budget(&mut self, limit: usize, out: &mut Vec<u8>) -> usize {
        let limit = limit.min(max_frame_bits(self.channels));
        self.reference_drop_bands_until(limit);
        let end_at = self.emit_into(out);
        let coded = out.len().saturating_mul(8);
        if self.quant_all_zero() {
            return coded;
        }
        // Pad to the ABR ceiling, minus stuffing debt from earlier frames
        // that spent credit above `budget`. Search/drop still use credit;
        // this only changes EXT_FILL payload before ID_END.
        let budget = self.budget_bits().min(limit);
        let pad_to = if self.pad_debt > 0 {
            budget.saturating_sub(self.pad_debt as usize)
        } else {
            budget
        };
        if coded < pad_to.saturating_mul(97) / 100 {
            let target = (pad_to / 8).min(MAX_PAYLOAD_BYTES);
            let room = target.saturating_mul(8).saturating_sub(end_at + 3);
            let pad = crate::engine::enc_pad::pad_fill_for_room(room);
            if pad > 0 {
                self.pad_fill = pad;
                self.emit_into(out);
                self.pad_fill = 0;
            }
        }
        let emitted = out.len().saturating_mul(8) as i64;
        self.pad_debt = (self.pad_debt + emitted - budget as i64).max(0);
        coded
    }
}
