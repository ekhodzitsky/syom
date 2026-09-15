//! Section planning and element emission for the LC encoder — mirrors
//! [`super::section`] (Table 4.5), [`super::ics`] (Table 4.6),
//! [`super::sf`] (Table 4.53), and the spectral walk of
//! [`super::spectrum::parse_quant_into`]. Long windows: one group,
//! `section_data` escape 31 with 5-bit increments. The short-window twins
//! live in [`super::enc_short`].

use super::bits::BitWriter;
use super::enc_huff::{sf_delta_bits, sf_emit_delta, spectral_emit};
use super::enc_ms::MsBands;
use super::enc_quant::{BOOKS, MAX_BANDS, QuantChannel, UNREPRESENTABLE};
use super::enc_tns::EncTns;
use super::ics::WindowSequence;
use super::section::{NOISE_HCB, has_spectral, is_intensity, is_noise};
use super::sf::{NOISE_OFFSET, NOISE_PCM_BITS};

#[path = "enc_section_dp.rs"]
mod exact;
#[cfg(test)]
pub(super) use exact::{plan_books_dp, section_spectral_cost};

/// Section-header cost for a run of `len` bands (4-bit cb + 5-bit
/// increments, escape 31).
pub(super) fn section_header_bits(len: usize) -> usize {
    4 + 5 * (len / 31 + 1)
}

/// Spectral bits of bands `lo..hi` under one book (`UNREPRESENTABLE` if any
/// band cannot use it).
pub(super) fn range_book_bits(
    coded: &[bool],
    bits: &[[u32; BOOKS]],
    lo: usize,
    hi: usize,
    cb: usize,
) -> u32 {
    let mut total = 0u32;
    for b in lo..hi {
        if !coded[b] {
            continue;
        }
        let bits = bits[b][cb];
        if bits == UNREPRESENTABLE {
            return UNREPRESENTABLE;
        }
        total = total.saturating_add(bits);
    }
    total
}

/// Cheapest book for bands `lo..hi` (`None` when nothing represents them —
/// impossible in practice: book 11 escapes any quantized value).
pub(super) fn best_book(
    coded: &[bool],
    bits: &[[u32; BOOKS]],
    lo: usize,
    hi: usize,
) -> Option<(u8, u32)> {
    let mut best: Option<(u8, u32)> = None;
    for cb in 1..BOOKS {
        let bits = range_book_bits(coded, bits, lo, hi, cb);
        if bits == UNREPRESENTABLE {
            continue;
        }
        if best.is_none_or(|(_, b)| bits < b) {
            best = Some((cb as u8, bits));
        }
    }
    best
}

/// Choose `sfb_cb` for `n` bands: per-band cheapest book, then greedily
/// merge adjacent coded runs while saved headers outweigh extra spectral
/// bits. ZERO_HCB on uncoded bands. Exact DP is [`plan_books_dp`] (TASK-73
/// no-go as default: < 2% section+spectral savings).
pub(super) fn plan_books_into(
    coded: &[bool],
    bits: &[[u32; BOOKS]],
    n: usize,
    sfb_cb: &mut [u8],
    header_bits: fn(usize) -> usize,
) {
    for b in 0..n {
        sfb_cb[b] = if coded[b] {
            best_book(coded, bits, b, b + 1)
                .map(|(cb, _)| cb)
                .unwrap_or(11)
        } else {
            0
        };
    }
    let mut improved = true;
    while improved {
        improved = false;
        let mut lo = 0usize;
        while lo < n {
            if !coded[lo] {
                lo += 1;
                continue;
            }
            let mut hi = lo + 1;
            while hi < n && coded[hi] {
                hi += 1;
            }
            let mut i = lo;
            while i < hi {
                let mut j = i + 1;
                while j < hi && sfb_cb[j] == sfb_cb[i] {
                    j += 1;
                }
                if j < hi {
                    let mut k = j + 1;
                    while k < hi && sfb_cb[k] == sfb_cb[j] {
                        k += 1;
                    }
                    let saved = header_bits(j - i) as i64 + header_bits(k - j) as i64
                        - header_bits(k - i) as i64;
                    if let Some((cb, merged)) = best_book(coded, bits, i, k) {
                        let cur = range_book_bits(coded, bits, i, j, usize::from(sfb_cb[i]))
                            .saturating_add(range_book_bits(
                                coded,
                                bits,
                                j,
                                k,
                                usize::from(sfb_cb[j]),
                            ));
                        if i64::from(merged) - i64::from(cur) < saved {
                            sfb_cb[i..k].fill(cb);
                            improved = true;
                        }
                    }
                }
                i = j;
            }
            lo = hi;
        }
    }
    #[cfg(debug_assertions)]
    {
        let mut alt = [0u8; MAX_BANDS];
        exact::plan_books_dp(coded, bits, n, &mut alt, header_bits);
        debug_assert!(
            exact::section_spectral_cost(sfb_cb, coded, bits, n, header_bits)
                >= exact::section_spectral_cost(&alt, coded, bits, n, header_bits)
        );
    }
}

/// Choose `sfb_cb` per band of a long-window channel.
pub fn plan_books(q: &QuantChannel) -> [u8; MAX_BANDS] {
    let mut coded = q.coded;
    for ((c, &p), &is) in coded
        .iter_mut()
        .zip(q.pns.iter())
        .zip(q.intensity.iter())
        .take(q.n_bands)
    {
        if p || is {
            *c = false;
        }
    }
    let mut sfb_cb = [0u8; MAX_BANDS];
    plan_books_into(&coded, &q.bits, q.n_bands, &mut sfb_cb, section_header_bits);
    for ((slot, &p), (&is, &hcb)) in sfb_cb
        .iter_mut()
        .zip(q.pns.iter())
        .zip(q.intensity.iter().zip(q.is_hcb.iter()))
        .take(q.n_bands)
    {
        if p {
            *slot = NOISE_HCB;
        } else if is {
            *slot = hcb;
        }
    }
    sfb_cb
}

/// `ics_info()` for an LC stream (Table 4.6, KBD window — the encoder's
/// analysis window — no predictor). Short frames carry `max_sfb` on 4 bits
/// plus 7-bit `scale_factor_grouping` (`grouping = 0` → 8 groups of 1).
pub fn emit_ics_info(w: &mut BitWriter, seq: WindowSequence, max_sfb: u8, grouping: u8) {
    w.write_bit(false); // ics_res
    w.write(u32::from(seq as u8), 2);
    w.write_bit(true); // window_shape: KBD
    if seq.is_eight_short() {
        w.write(u32::from(max_sfb), 4);
        w.write(u32::from(grouping), 7);
    } else {
        w.write(u32::from(max_sfb), 6);
        w.write_bit(false); // predictor_data_present
    }
}

/// `section_data()` for one group (Table 4.5 long-window branch).
pub fn emit_section_data(w: &mut BitWriter, sfb_cb: &[u8; MAX_BANDS], max_sfb: usize) {
    let mut k = 0usize;
    while k < max_sfb {
        let cb = sfb_cb[k];
        let mut len = 1usize;
        while k + len < max_sfb && sfb_cb[k + len] == cb {
            len += 1;
        }
        w.write(u32::from(cb), 4);
        let mut rem = len;
        while rem >= 31 {
            w.write(31, 5);
            rem -= 31;
        }
        w.write(rem as u32, 5);
        k += len;
    }
}

/// `scale_factor_data()` (Table 4.53): DPCM over spectral, PNS, and IS.
pub fn emit_scale_factors(
    w: &mut BitWriter,
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
    global_gain: u8,
) {
    let mut prev_sf = i32::from(global_gain);
    let mut prev_nrg = i32::from(global_gain) - NOISE_OFFSET - 256;
    let mut prev_is = 0i32;
    let mut noise_first = true;
    let bands = sfb_cb
        .iter()
        .zip(q.sf.iter())
        .zip(q.noise_nrg.iter())
        .zip(q.is_pos.iter())
        .take(q.n_bands);
    for (((&cb, &sf), &nrg), &pos) in bands {
        if is_noise(cb) {
            if noise_first {
                w.write((nrg - prev_nrg).clamp(0, 511) as u32, NOISE_PCM_BITS);
                noise_first = false;
            } else {
                sf_emit_delta(w, nrg - prev_nrg);
            }
            prev_nrg = nrg;
        } else if is_intensity(cb) {
            sf_emit_delta(w, pos - prev_is);
            prev_is = pos;
        } else if has_spectral(cb) {
            sf_emit_delta(w, sf - prev_sf);
            prev_sf = sf;
        }
    }
}

/// Huffman spectral data, mirroring the `parse_quant_into` long-window walk.
pub fn emit_spectral(
    w: &mut BitWriter,
    offsets: &[u16],
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
) {
    let bands = sfb_cb.iter().zip(offsets.windows(2)).take(q.n_bands);
    for (&cb, win) in bands {
        if !has_spectral(cb) {
            continue;
        }
        let lo = usize::from(win[0]);
        let hi = usize::from(win[1]);
        let step = if cb <= 4 { 4 } else { 2 };
        let mut i = lo;
        while i < hi {
            spectral_emit(cb, &q.quant[i..i + step], w);
            i += step;
        }
    }
}

/// Total bits of one channel body: global_gain + (`ics_info` when
/// `standalone`, i.e. SCE / `common_window == 0`) + section_data +
/// scale_factor_data + pulse/tns/gain flags + TNS payload + spectral data.
pub fn channel_body_bits(
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
    global_gain: u8,
    standalone: bool,
    tns: &EncTns,
) -> usize {
    let mut bits = 8; // global_gain
    if standalone {
        bits += 11; // ics_info (long)
    }
    // section_data
    let mut k = 0usize;
    while k < q.n_bands {
        let mut len = 1usize;
        while k + len < q.n_bands && sfb_cb[k + len] == sfb_cb[k] {
            len += 1;
        }
        bits += section_header_bits(len);
        k += len;
    }
    // scale_factor_data + spectral data (PNS has energy bits, no Huffman)
    let mut prev_sf = i32::from(global_gain);
    let mut prev_nrg = i32::from(global_gain) - NOISE_OFFSET - 256;
    let mut prev_is = 0i32;
    let mut noise_first = true;
    let bands = sfb_cb
        .iter()
        .zip(q.sf.iter())
        .zip(q.noise_nrg.iter())
        .zip(q.is_pos.iter())
        .zip(q.bits.iter())
        .take(q.n_bands);
    for ((((&cb, &sf), &nrg), &pos), row) in bands {
        if is_noise(cb) {
            if noise_first {
                bits += NOISE_PCM_BITS as usize;
                noise_first = false;
            } else {
                bits += sf_delta_bits(nrg - prev_nrg);
            }
            prev_nrg = nrg;
        } else if is_intensity(cb) {
            bits += sf_delta_bits(pos - prev_is);
            prev_is = pos;
        } else if has_spectral(cb) {
            bits += sf_delta_bits(sf - prev_sf);
            prev_sf = sf;
            bits += row[usize::from(cb)] as usize;
        }
    }
    bits += 2 + tns.bits(); // pulse + gain flags, tns flag + payload
    bits
}

/// Emit one channel body (`individual_channel_stream`): global_gain, then
/// `ics_info` when `standalone` (SCE / `common_window == 0`), then
/// section_data + scale_factor_data + flags + TNS + spectral data.
#[allow(clippy::too_many_arguments)]
pub fn emit_channel_body(
    w: &mut BitWriter,
    offsets: &[u16],
    seq: WindowSequence,
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
    global_gain: u8,
    standalone: bool,
    tns: &EncTns,
) {
    w.write(u32::from(global_gain), 8);
    if standalone {
        emit_ics_info(w, seq, q.n_bands as u8, 0);
    }
    emit_section_data(w, sfb_cb, q.n_bands);
    emit_scale_factors(w, sfb_cb, q, global_gain);
    w.write_bit(false); // pulse_data_present
    tns.emit(w); // tns_data_present + payload
    w.write_bit(false); // gain_control_data_present
    emit_spectral(w, offsets, sfb_cb, q);
}

/// Emit a long-family (`OnlyLong` / `LongStart` / `LongStop`) `raw_data_block`.
#[allow(clippy::too_many_arguments)]
pub fn emit_frame(
    offsets: &[u16],
    seq: WindowSequence,
    chans: &[QuantChannel],
    books: &[[u8; MAX_BANDS]],
    gains: &[u8],
    ms: &MsBands,
    tns: &[EncTns],
    channels: usize,
    fill: &[u8],
    out: &mut Vec<u8>,
) {
    let mut w = BitWriter::from_vec(std::mem::take(out));
    if channels == 1 {
        w.write(0, 3); // SCE
        w.write(0, 4); // tag
        emit_channel_body(
            &mut w, offsets, seq, &books[0], &chans[0], gains[0], true, &tns[0],
        );
    } else {
        w.write(1, 3); // CPE
        w.write(0, 4); // tag
        w.write_bit(true); // common_window
        emit_ics_info(&mut w, seq, chans[0].n_bands as u8, 0);
        ms.emit(&mut w); // ms_mask_present + optional per-band ms_used bits
        for ch in 0..channels {
            emit_channel_body(
                &mut w, offsets, seq, &books[ch], &chans[ch], gains[ch], false, &tns[ch],
            );
        }
    }
    if !fill.is_empty() {
        let _ = super::enc_sbr_bits::write_fill_element(&mut w, fill); // HE FIL
    }
    w.write(7, 3); // END
    *out = w.finish();
}

#[cfg(test)]
#[path = "enc_section_tests.rs"]
mod enc_section_tests;
