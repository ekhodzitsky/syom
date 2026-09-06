//! Short-window (`EIGHT_SHORT`) section planning and element emission for
//! the LC encoder — mirrors the short branches of [`super::section`]
//! (Table 4.5), [`super::ics`] (Table 4.6), [`super::sf`] (Table 4.53), and
//! the grouped spectral walk of [`super::spectrum::parse_quant_into`].
//!
//! v1 grouping: no `scale_factor_grouping` — each of the 8 short windows is
//! its own group (group length 1), so the grouped/interleaved spectral walk
//! degenerates to contiguous per-window runs. Scalefactor DPCM still runs
//! continuously across groups, exactly as the decoder accumulates `last_sf`.

use super::bits::BitWriter;
use super::enc_huff::{sf_delta_bits, sf_emit_delta, spectral_emit};
use super::enc_quant::{MAX_FLAT_SHORT, MAX_GROUPS, QuantShort};
use super::enc_section::{emit_ics_info, plan_books_into};
use super::ics::WindowSequence;
use super::section::has_spectral;
use super::swb::SHORT_WINDOW_LEN;

/// Short-window section-header cost (4-bit cb + 3-bit increments, escape 7).
fn section_header_bits_short(len: usize) -> usize {
    4 + 3 * (len / 7 + 1)
}

/// Choose `sfb_cb` per (window, band): planning is per group (window), the
/// result is flattened `g * n_sfb + b`.
pub fn plan_books_short(q: &QuantShort) -> [u8; MAX_FLAT_SHORT] {
    let mut sfb_cb = [0u8; MAX_FLAT_SHORT];
    for g in 0..MAX_GROUPS {
        let lo = g * q.n_sfb;
        plan_books_into(
            &q.coded[lo..lo + q.n_sfb],
            &q.bits[lo..lo + q.n_sfb],
            q.n_sfb,
            &mut sfb_cb[lo..lo + q.n_sfb],
            section_header_bits_short,
        );
    }
    sfb_cb
}

/// `section_data()` for a short frame (Table 4.5 short-window branch):
/// `MAX_GROUPS` groups of `n_sfb` bands each, 3-bit increments, escape 7.
pub fn emit_section_data_short(w: &mut BitWriter, sfb_cb: &[u8; MAX_FLAT_SHORT], n_sfb: usize) {
    for g in 0..MAX_GROUPS {
        let row = &sfb_cb[g * n_sfb..(g + 1) * n_sfb];
        let mut k = 0usize;
        while k < n_sfb {
            let cb = row[k];
            let mut len = 1usize;
            while k + len < n_sfb && row[k + len] == cb {
                len += 1;
            }
            w.write(u32::from(cb), 4);
            let mut rem = len;
            while rem >= 7 {
                w.write(7, 3);
                rem -= 7;
            }
            w.write(rem as u32, 3);
            k += len;
        }
    }
}

/// `scale_factor_data()` for a short frame: DPCM over the flattened
/// (group, band) order — the decoder's `last_sf` continues across groups.
pub fn emit_scale_factors_short(
    w: &mut BitWriter,
    sfb_cb: &[u8; MAX_FLAT_SHORT],
    q: &QuantShort,
    global_gain: u8,
) {
    let mut prev = i32::from(global_gain);
    let n = MAX_GROUPS * q.n_sfb;
    let bands = sfb_cb.iter().zip(q.sf.iter()).take(n);
    for (&cb, &sf) in bands {
        if !has_spectral(cb) {
            continue;
        }
        sf_emit_delta(w, sf - prev);
        prev = sf;
    }
}

/// Huffman spectral data for a short frame, mirroring the
/// `parse_quant_into` grouped walk with group length 1: per group (window)
/// and band the bins are contiguous in the window-major layout.
pub fn emit_spectral_short(
    w: &mut BitWriter,
    offsets: &[u16],
    sfb_cb: &[u8; MAX_FLAT_SHORT],
    q: &QuantShort,
) {
    for g in 0..MAX_GROUPS {
        for b in 0..q.n_sfb {
            let cb = sfb_cb[g * q.n_sfb + b];
            if !has_spectral(cb) {
                continue;
            }
            let lo = g * SHORT_WINDOW_LEN + usize::from(offsets[b]);
            let hi = g * SHORT_WINDOW_LEN + usize::from(offsets[b + 1]);
            let step = if cb <= 4 { 4 } else { 2 };
            let mut i = lo;
            while i < hi {
                spectral_emit(cb, &q.quant[i..i + step], w);
                i += step;
            }
        }
    }
}

/// Total bits of one short-window channel body (8 groups of one window).
pub fn channel_body_bits_short(
    sfb_cb: &[u8; MAX_FLAT_SHORT],
    q: &QuantShort,
    global_gain: u8,
    standalone: bool,
) -> usize {
    let mut bits = 8; // global_gain
    if standalone {
        bits += 15; // ics_info (short)
    }
    // section_data, per group
    for g in 0..MAX_GROUPS {
        let row = &sfb_cb[g * q.n_sfb..(g + 1) * q.n_sfb];
        let mut k = 0usize;
        while k < q.n_sfb {
            let mut len = 1usize;
            while k + len < q.n_sfb && row[k + len] == row[k] {
                len += 1;
            }
            bits += section_header_bits_short(len);
            k += len;
        }
    }
    // scale_factor_data + spectral data, flattened (group, band) order
    let n = MAX_GROUPS * q.n_sfb;
    let mut prev = i32::from(global_gain);
    let bands = sfb_cb.iter().zip(q.sf.iter()).zip(q.bits.iter()).take(n);
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

/// Emit one short-window channel body (`individual_channel_stream`):
/// global_gain, then `ics_info` when `standalone`, then section_data +
/// scale_factor_data + flags + spectral data.
pub fn emit_channel_body_short(
    w: &mut BitWriter,
    offsets: &[u16],
    sfb_cb: &[u8; MAX_FLAT_SHORT],
    q: &QuantShort,
    global_gain: u8,
    standalone: bool,
) {
    w.write(u32::from(global_gain), 8);
    if standalone {
        emit_ics_info(w, WindowSequence::EightShort, q.n_sfb as u8);
    }
    emit_section_data_short(w, sfb_cb, q.n_sfb);
    emit_scale_factors_short(w, sfb_cb, q, global_gain);
    w.write_bit(false); // pulse_data_present
    w.write_bit(false); // tns_data_present
    w.write_bit(false); // gain_control_data_present
    emit_spectral_short(w, offsets, sfb_cb, q);
}

#[cfg(test)]
#[path = "enc_short_tests.rs"]
mod enc_short_tests;
