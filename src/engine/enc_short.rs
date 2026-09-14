//! Short-window (`EIGHT_SHORT`) section planning and element emission for
//! the LC encoder — mirrors the short branches of [`super::section`]
//! (Table 4.5), [`super::ics`] (Table 4.6), [`super::sf`] (Table 4.53), and
//! the grouped spectral walk of [`super::spectrum::parse_quant_into`].
//!
//! Default grouping is 8×1 (`scale_factor_grouping = 0`). Opt-in grouping
//! shares scale factors / sections across similar consecutive windows.
//! Scalefactor DPCM still runs continuously across groups.

use super::bits::BitWriter;
use super::enc_group::{self, Grouping};
use super::enc_huff::{sf_delta_bits, sf_emit_delta};
use super::enc_ms::MsBands;
use super::enc_psy::Psy;
use super::enc_quant::{self, MAX_FLAT_SHORT, MAX_GROUPS, QuantShort};
use super::enc_section::{emit_ics_info, plan_books_into};
use super::enc_tns::EncTns;
use super::filterbank::window_left;
use super::ics::WindowSequence;
use super::mdct::mdct_into_f32;
use super::section::has_spectral;
use super::swb::{LONG_WINDOW_LEN, SHORT_WINDOW_LEN};

/// Block offset of the first short window (`(2048 − 256) / 4`) and the hop
/// between short windows — mirrors the decoder's `short_windowed`.
const SHORT_START: usize = (2 * LONG_WINDOW_LEN - 2 * SHORT_WINDOW_LEN) / 4;
const SHORT_HOP: usize = SHORT_WINDOW_LEN;

/// The encoder's analysis windows, pre-scaled by 32768 like the long
/// `LcEncoder::window` and matching the decoder's synthesis shapes exactly
/// (`filterbank::apply_long_window` / `short_windowed`).
pub struct ShortWindows {
    /// Long-start: long KBD left, flat 1.0, short KBD right, zeros.
    pub start: Box<[f32; 2 * LONG_WINDOW_LEN]>,
    /// Long-stop: zeros, short KBD left, flat 1.0, long KBD right.
    pub stop: Box<[f32; 2 * LONG_WINDOW_LEN]>,
    /// One 256-tap short KBD window.
    pub short: Box<[f32; 2 * SHORT_WINDOW_LEN]>,
}

impl ShortWindows {
    pub fn new() -> Self {
        let long = window_left(2 * LONG_WINDOW_LEN, super::ics::WindowShape::Kbd);
        let short = window_left(2 * SHORT_WINDOW_LEN, super::ics::WindowShape::Kbd);
        let mut start = Box::new([0.0f32; 2 * LONG_WINDOW_LEN]);
        let mut stop = Box::new([0.0f32; 2 * LONG_WINDOW_LEN]);
        for i in 0..LONG_WINDOW_LEN {
            start[i] = long[i] * 32768.0;
            stop[LONG_WINDOW_LEN + i] = long[LONG_WINDOW_LEN - 1 - i] * 32768.0;
        }
        let flat = SHORT_START; // 448 samples of flat 1.0 on each side
        for i in 0..flat {
            start[LONG_WINDOW_LEN + i] = 32768.0;
            stop[SHORT_START + SHORT_WINDOW_LEN + i] = 32768.0;
        }
        for m in 0..SHORT_WINDOW_LEN {
            // Descending short right half at 1472..1600 of the start window.
            start[LONG_WINDOW_LEN + flat + m] = short[SHORT_WINDOW_LEN - 1 - m] * 32768.0;
            // Ascending short left half at 448..576 of the stop window.
            stop[SHORT_START + m] = short[m] * 32768.0;
        }
        let mut sw = Box::new([0.0f32; 2 * SHORT_WINDOW_LEN]);
        for i in 0..SHORT_WINDOW_LEN {
            sw[i] = short[i] * 32768.0;
            sw[2 * SHORT_WINDOW_LEN - 1 - i] = short[i] * 32768.0;
        }
        Self {
            start,
            stop,
            short: sw,
        }
    }
}

/// Window-major short spectrum of one 2048-sample block: eight 256→128
/// MDCTs at offsets 448 + 128·w (mirrors the decoder's short IMDCT grid).
pub fn spectra(
    block: &[f32; 2 * LONG_WINDOW_LEN],
    windows: &ShortWindows,
    spec: &mut [f32; LONG_WINDOW_LEN],
) {
    let mut seg = [0.0f32; 2 * SHORT_WINDOW_LEN];
    for w in 0..MAX_GROUPS {
        let base = SHORT_START + w * SHORT_HOP;
        for (s, (&x, &win)) in seg.iter_mut().zip(
            block[base..base + 2 * SHORT_WINDOW_LEN]
                .iter()
                .zip(windows.short.iter()),
        ) {
            *s = x * win;
        }
        mdct_into_f32(
            &seg,
            &mut spec[w * SHORT_WINDOW_LEN..(w + 1) * SHORT_WINDOW_LEN],
        );
    }
}

/// Quantize + plan one short channel at `offset`; returns its body bits.
/// Psy scores each window; [`Grouping`] then folds windows into groups.
#[allow(clippy::too_many_arguments)]
pub fn channel_build(
    psy: &mut Psy,
    offsets: &[u16],
    spec: &[f32; LONG_WINDOW_LEN],
    offset: i32,
    q: &mut QuantShort,
    tq: &mut [f32; MAX_FLAT_SHORT],
    books: &mut [u8; MAX_FLAT_SHORT],
    gain: &mut u8,
    standalone: bool,
    tns: &EncTns,
    grouping: Grouping,
) -> usize {
    let n_sfb = q.n_sfb;
    let mut win_coded = [false; MAX_FLAT_SHORT];
    let mut win_tq = [0.0f32; MAX_FLAT_SHORT];
    for w in 0..MAX_GROUPS {
        psy.analyze(
            &spec[w * SHORT_WINDOW_LEN..(w + 1) * SHORT_WINDOW_LEN],
            offsets,
            crate::engine::enc_frame::TARGET_Q,
            &mut win_coded[w * n_sfb..(w + 1) * n_sfb],
            &mut win_tq[w * n_sfb..(w + 1) * n_sfb],
        );
        if tns.short().window_on(w) {
            let hi = n_sfb.min(14);
            for b in 0..hi {
                if !win_coded[w * n_sfb + b] {
                    win_coded[w * n_sfb + b] = true;
                    win_tq[w * n_sfb + b] = crate::engine::enc_frame::TARGET_Q;
                }
            }
        }
    }
    let mut peaks = [0.0f32; MAX_FLAT_SHORT];
    enc_quant::band_peaks_short(spec, offsets, n_sfb, &mut peaks);
    q.set_grouping(grouping);
    let mut gpeaks = [0.0f32; MAX_FLAT_SHORT];
    enc_group::fold_windows(
        grouping,
        n_sfb,
        &win_coded,
        &win_tq,
        &peaks,
        &mut q.coded,
        tq,
        &mut gpeaks,
    );
    enc_quant::raw_scalefactors_short(&gpeaks, tq, offset, q);
    *gain = enc_quant::normalize_sf_short(q);
    enc_quant::quantize_short(spec, offsets, q);
    *books = plan_books_short(q);
    channel_body_bits_short(books, q, *gain, standalone, tns)
}

/// Total frame bits of the built short state (element overhead included).
pub fn frame_bits(
    chans: &[QuantShort],
    books: &[[u8; MAX_FLAT_SHORT]],
    gains: &[u8],
    channels: usize,
    ms_overhead: usize,
) -> usize {
    let mut total = 3 + 4 + 3 + 7; // element id + tag + END + align ceiling
    if channels == 2 {
        total += 1 + 15 + ms_overhead; // common_window + ics_info (short) + ms_mask
    }
    for ch in 0..channels {
        total += channel_body_bits_short(
            &books[ch],
            &chans[ch],
            gains[ch],
            channels == 1,
            &EncTns::off(),
        );
    }
    total
}

/// Hard cap, short frames: drop the highest coded (window, band) until the
/// frame fits `cap` bits.
#[allow(dead_code)]
pub fn drop_bands_until(
    chans: &mut [QuantShort],
    books: &mut [[u8; MAX_FLAT_SHORT]],
    gains: &[u8],
    channels: usize,
    cap: usize,
    ms_overhead: usize,
) {
    loop {
        let total = frame_bits(chans, books, gains, channels, ms_overhead);
        if total <= cap {
            return;
        }
        let mut hit: Option<(usize, usize)> = None;
        for (ch, q) in chans.iter().enumerate().take(channels) {
            let n = q.n_groups * q.n_sfb;
            if let Some(b) = q.coded[..n].iter().rposition(|&c| c) {
                hit = Some((ch, b));
                break;
            }
        }
        let Some((ch, b)) = hit else { return };
        chans[ch].coded[b] = false;
        books[ch] = plan_books_short(&chans[ch]);
    }
}

/// Emit the short-frame `raw_data_block` from the built state.
#[allow(clippy::too_many_arguments)]
pub fn emit_frame(
    offsets: &[u16],
    chans: &[QuantShort],
    books: &[[u8; MAX_FLAT_SHORT]],
    gains: &[u8],
    ms: &MsBands,
    tns: &[EncTns],
    channels: usize,
    out: &mut Vec<u8>,
) {
    let mut w = BitWriter::from_vec(std::mem::take(out));
    if channels == 1 {
        w.write(0, 3); // SCE
        w.write(0, 4); // tag
        emit_channel_body_short(
            &mut w, offsets, &books[0], &chans[0], gains[0], true, &tns[0],
        );
    } else {
        w.write(1, 3); // CPE
        w.write(0, 4); // tag
        w.write_bit(true); // common_window
        emit_ics_info(
            &mut w,
            WindowSequence::EightShort,
            chans[0].n_sfb as u8,
            chans[0].grouping_bits(),
        );
        ms.emit(&mut w); // ms_mask_present + optional per-group ms_used bits
        for ch in 0..channels {
            emit_channel_body_short(
                &mut w, offsets, &books[ch], &chans[ch], gains[ch], false, &tns[ch],
            );
        }
    }
    w.write(7, 3); // END
    *out = w.finish();
}

/// Short-window section-header cost (4-bit cb + 3-bit increments, escape 7).
fn section_header_bits_short(len: usize) -> usize {
    4 + 3 * (len / 7 + 1)
}

/// Choose `sfb_cb` per (window, band): planning is per group (window), the
/// result is flattened `g * n_sfb + b`.
pub fn plan_books_short(q: &QuantShort) -> [u8; MAX_FLAT_SHORT] {
    let mut sfb_cb = [0u8; MAX_FLAT_SHORT];
    for g in 0..q.n_groups {
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

/// `section_data()` for a short frame (Table 4.5 short-window branch).
pub fn emit_section_data_short(
    w: &mut BitWriter,
    sfb_cb: &[u8; MAX_FLAT_SHORT],
    n_sfb: usize,
    n_groups: usize,
) {
    for g in 0..n_groups {
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
    let n = q.n_groups * q.n_sfb;
    let bands = sfb_cb.iter().zip(q.sf.iter()).take(n);
    for (&cb, &sf) in bands {
        if !has_spectral(cb) {
            continue;
        }
        sf_emit_delta(w, sf - prev);
        prev = sf;
    }
}

/// Total bits of one short-window channel body.
pub fn channel_body_bits_short(
    sfb_cb: &[u8; MAX_FLAT_SHORT],
    q: &QuantShort,
    global_gain: u8,
    standalone: bool,
    tns: &EncTns,
) -> usize {
    let mut bits = 8; // global_gain
    if standalone {
        bits += 15; // ics_info (short)
    }
    // section_data, per group
    for g in 0..q.n_groups {
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
    let n = q.n_groups * q.n_sfb;
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
    bits += 2 + tns.bits(); // pulse + gain, tns flag + payload
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
    tns: &EncTns,
) {
    w.write(u32::from(global_gain), 8);
    if standalone {
        emit_ics_info(
            w,
            WindowSequence::EightShort,
            q.n_sfb as u8,
            q.grouping_bits(),
        );
    }
    emit_section_data_short(w, sfb_cb, q.n_sfb, q.n_groups);
    emit_scale_factors_short(w, sfb_cb, q, global_gain);
    w.write_bit(false); // pulse_data_present
    tns.emit(w);
    w.write_bit(false); // gain_control_data_present
    enc_group::emit_spectral_short(w, offsets, sfb_cb, q);
}

#[cfg(test)]
#[path = "enc_short_tests.rs"]
mod enc_short_tests;
