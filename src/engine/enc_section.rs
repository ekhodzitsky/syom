//! Section planning and element emission for the LC encoder — mirrors
//! [`super::section`] (Table 4.5), [`super::ics`] (Table 4.6),
//! [`super::sf`] (Table 4.53), and the spectral walk of
//! [`super::spectrum::parse_quant_into`]. Long windows only: one group,
//! `section_data` escape 31 with 5-bit increments.

use super::bits::BitWriter;
use super::enc_huff::{sf_delta_bits, sf_emit_delta, spectral_emit};
use super::enc_quant::{BOOKS, MAX_BANDS, QuantChannel, UNREPRESENTABLE};
use super::section::has_spectral;

/// Section-header cost for a run of `len` bands (4-bit cb + 5-bit
/// increments, escape 31).
fn section_header_bits(len: usize) -> usize {
    4 + 5 * (len / 31 + 1)
}

/// Spectral bits of bands `lo..hi` under one book (`UNREPRESENTABLE` if any
/// band cannot use it).
fn range_book_bits(q: &QuantChannel, lo: usize, hi: usize, cb: usize) -> u32 {
    let mut total = 0u32;
    for b in lo..hi {
        if !q.coded[b] {
            continue;
        }
        let bits = q.bits[b][cb];
        if bits == UNREPRESENTABLE {
            return UNREPRESENTABLE;
        }
        total = total.saturating_add(bits);
    }
    total
}

/// Cheapest book for bands `lo..hi` (`None` when nothing represents them —
/// impossible in practice: book 11 escapes any quantized value).
fn best_book(q: &QuantChannel, lo: usize, hi: usize) -> Option<(u8, u32)> {
    let mut best: Option<(u8, u32)> = None;
    for cb in 1..BOOKS {
        let bits = range_book_bits(q, lo, hi, cb);
        if bits == UNREPRESENTABLE {
            continue;
        }
        if best.is_none_or(|(_, b)| bits < b) {
            best = Some((cb as u8, bits));
        }
    }
    best
}

/// Choose `sfb_cb` per band: per-band cheapest book, then greedily merge
/// adjacent same-range runs into one section while the saved section header
/// outweighs the extra spectral bits. ZERO_HCB (0) on uncoded bands.
pub fn plan_books(q: &QuantChannel) -> [u8; MAX_BANDS] {
    let n = q.n_bands;
    let mut sfb_cb = [0u8; MAX_BANDS];
    for (b, slot) in sfb_cb.iter_mut().enumerate().take(n) {
        if q.coded[b] {
            *slot = best_book(q, b, b + 1).map(|(cb, _)| cb).unwrap_or(11);
        }
    }
    // Greedy merge of adjacent runs (ZERO bands keep ZERO_HCB, so only
    // directly adjacent coded bands can share a section).
    let mut improved = true;
    while improved {
        improved = false;
        let mut lo = 0usize;
        while lo < n {
            if !q.coded[lo] {
                lo += 1;
                continue;
            }
            let mut hi = lo + 1;
            while hi < n && q.coded[hi] {
                hi += 1;
            }
            // Runs are maximal coded stretches; merge inside them.
            if merge_run(q, &mut sfb_cb, lo, hi) {
                improved = true;
            }
            lo = hi;
        }
    }
    sfb_cb
}

/// One greedy merge pass over `lo..hi`; true if any merge happened.
fn merge_run(q: &QuantChannel, sfb_cb: &mut [u8; MAX_BANDS], lo: usize, hi: usize) -> bool {
    let mut merged = false;
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
            // Cost now: two sections. Cost merged: one section, one book.
            let now = section_header_bits(j - i) + section_header_bits(k - j);
            let one = section_header_bits(k - i);
            let saved_headers = now as i64 - one as i64;
            if let Some((cb, bits_merged)) = best_book(q, i, k) {
                let cur = range_book_bits(q, i, j, usize::from(sfb_cb[i]))
                    .saturating_add(range_book_bits(q, j, k, usize::from(sfb_cb[j])));
                if i64::from(bits_merged) - i64::from(cur) < saved_headers {
                    sfb_cb[i..k].fill(cb);
                    merged = true;
                }
            }
        }
        i = j;
    }
    merged
}

/// `ics_info()` for a long-window LC stream (Table 4.6, KBD window — the
/// encoder's analysis window — no predictor).
pub fn emit_ics_info(w: &mut BitWriter, max_sfb: u8) {
    w.write_bit(false); // ics_res
    w.write(0, 2); // ONLY_LONG
    w.write_bit(true); // window_shape: KBD
    w.write(u32::from(max_sfb), 6);
    w.write_bit(false); // predictor_data_present
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

/// `scale_factor_data()` (Table 4.53): DPCM over spectral bands.
pub fn emit_scale_factors(
    w: &mut BitWriter,
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
    global_gain: u8,
) {
    let mut prev = i32::from(global_gain);
    let bands = sfb_cb.iter().zip(q.sf.iter()).take(q.n_bands);
    for (&cb, &sf) in bands {
        if !has_spectral(cb) {
            continue;
        }
        sf_emit_delta(w, sf - prev);
        prev = sf;
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
/// scale_factor_data + pulse/tns/gain flags + spectral data.
pub fn channel_body_bits(
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
    global_gain: u8,
    standalone: bool,
) -> usize {
    let mut bits = 8; // global_gain
    if standalone {
        bits += 11; // ics_info
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
    // scale_factor_data + spectral data
    let mut prev = i32::from(global_gain);
    let bands = sfb_cb
        .iter()
        .zip(q.sf.iter())
        .zip(q.bits.iter())
        .take(q.n_bands);
    for ((&cb, &sf), row) in bands {
        if !has_spectral(cb) {
            continue;
        }
        bits += sf_delta_bits(sf - prev);
        prev = sf;
        bits += row[usize::from(cb)] as usize;
    }
    bits += 3; // pulse + tns + gain flags
    bits
}

/// Emit one channel body (`individual_channel_stream`): global_gain, then
/// `ics_info` when `standalone` (SCE / `common_window == 0`), then
/// section_data + scale_factor_data + flags + spectral data.
pub fn emit_channel_body(
    w: &mut BitWriter,
    offsets: &[u16],
    sfb_cb: &[u8; MAX_BANDS],
    q: &QuantChannel,
    global_gain: u8,
    standalone: bool,
) {
    w.write(u32::from(global_gain), 8);
    if standalone {
        emit_ics_info(w, q.n_bands as u8);
    }
    emit_section_data(w, sfb_cb, q.n_bands);
    emit_scale_factors(w, sfb_cb, q, global_gain);
    w.write_bit(false); // pulse_data_present
    w.write_bit(false); // tns_data_present
    w.write_bit(false); // gain_control_data_present
    emit_spectral(w, offsets, sfb_cb, q);
}

#[cfg(test)]
#[path = "enc_section_tests.rs"]
mod enc_section_tests;
