//! Forward quantization, per-band scalefactors, and codebook cost table —
//! the inverse of [`super::spectrum::invquant`] / [`super::spectrum::sf_gain`]
//! (ISO/IEC 14496-3 §4.6.2–4.6.3).

use super::enc_huff::spectral_bits;
use super::spectrum::SF_OFFSET;

/// Largest quantized magnitude the bitstream carries (§4.6.2).
pub const QUANT_MAX: i32 = 8191;
/// Spectral books 1..=11; slot 0 is unused.
pub const BOOKS: usize = 12;
/// Long-window band ceiling (the 32 kHz table has 51 bands).
pub const MAX_BANDS: usize = 51;
/// `bits` sentinel: book cannot represent the band.
pub const UNREPRESENTABLE: u32 = u32::MAX;

/// One quantized long-window channel.
#[derive(Clone)]
pub struct QuantChannel {
    pub quant: [i32; 1024],
    /// `bits[band][book]` — spectral bits under each book, or
    /// [`UNREPRESENTABLE`].
    pub bits: [[u32; BOOKS]; MAX_BANDS],
    /// Absolute scalefactors actually quantized with (the transmitted ones).
    pub sf: [i32; MAX_BANDS],
    /// Bands with signal worth coding (the rest get ZERO_HCB).
    pub coded: [bool; MAX_BANDS],
    pub n_bands: usize,
}

impl QuantChannel {
    pub fn new(n_bands: usize) -> Self {
        Self {
            quant: [0; 1024],
            bits: [[UNREPRESENTABLE; BOOKS]; MAX_BANDS],
            sf: [0; MAX_BANDS],
            coded: [false; MAX_BANDS],
            n_bands,
        }
    }
}

/// Per-band peak |coefficient| of a 1024-line long-window spectrum.
pub fn band_peaks(spec: &[f32; 1024], offsets: &[u16], out: &mut [f32; MAX_BANDS]) {
    let n_bands = offsets.len() - 1;
    for b in 0..n_bands {
        let lo = usize::from(offsets[b]);
        let hi = usize::from(offsets[b + 1]);
        out[b] = spec[lo..hi].iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    }
}

/// Desired scalefactor for a band peak so the peak quantizes to about
/// `target_q`: `q = |x|^0.75 · 2^(−0.1875·(sf−100))` solved for `sf`.
pub fn sf_for_peak(peak: f32, target_q: f32) -> i32 {
    if peak <= 0.0 || target_q <= 0.0 {
        return 0;
    }
    let sf = SF_OFFSET as f32 + 4.0 * peak.log2() - (16.0 / 3.0) * target_q.log2();
    sf.round() as i32
}

/// Raw per-band scalefactors from peaks and the psy model's per-band
/// precision targets. `out.coded` is set by the psy model (a coded band
/// always has `target_q >= 1` and a nonzero peak).
pub fn raw_scalefactors(
    peaks: &[f32; MAX_BANDS],
    target_q: &[f32; MAX_BANDS],
    global_offset: i32,
    out: &mut QuantChannel,
) {
    let bands = peaks
        .iter()
        .zip(target_q.iter())
        .zip(out.coded.iter())
        .zip(out.sf.iter_mut())
        .take(out.n_bands);
    for (((&peak, &tq), &coded), sf) in bands {
        *sf = if coded && tq >= 1.0 {
            sf_for_peak(peak, tq) + global_offset
        } else {
            0
        };
    }
}

/// Clamp `out.sf` onto the wire format: global_gain is 8-bit, DPCM deltas
/// are ±60. Returns `global_gain`. Values actually transmitted are written
/// back into `out.sf` so quantization matches what the decoder applies.
pub fn normalize_sf(out: &mut QuantChannel) -> u8 {
    let mut global_gain = 100u8;
    let mut prev: Option<i32> = None;
    let bands = out.coded.iter().zip(out.sf.iter_mut()).take(out.n_bands);
    for (&coded, sf) in bands {
        if !coded {
            continue;
        }
        let want = (*sf).clamp(0, 255);
        let v = match prev {
            None => {
                global_gain = want as u8;
                want
            }
            Some(p) => (p + (want - p).clamp(-60, 60)).clamp(0, 255),
        };
        *sf = v;
        prev = Some(v);
    }
    global_gain
}

/// Quantize `spec` with the (normalized) `out.sf` and fill the per-band,
/// per-book bit-cost table.
pub fn quantize(spec: &[f32; 1024], offsets: &[u16], out: &mut QuantChannel) {
    out.quant = [0; 1024];
    let n_bands = out.n_bands.min(offsets.len() - 1);
    let bands = out
        .coded
        .iter()
        .zip(out.sf.iter())
        .zip(out.bits.iter_mut())
        .zip(offsets.windows(2))
        .take(n_bands);
    for (((&coded, &sf), bits), w) in bands {
        let (lo, hi) = (usize::from(w[0]), usize::from(w[1]));
        if !coded {
            *bits = [UNREPRESENTABLE; BOOKS];
            continue;
        }
        let gain = (-0.1875f32 * (sf - SF_OFFSET) as f32).exp2();
        for (q, &x) in out.quant[lo..hi].iter_mut().zip(spec[lo..hi].iter()) {
            let mag = (x.abs().powf(0.75) * gain).round() as i32;
            let mag = mag.min(QUANT_MAX);
            *q = if x < 0.0 { -mag } else { mag };
        }
        fill_bits(bits, &out.quant, lo, hi);
    }
}

/// Cost of one band under every book: quad books 1–4 walk 4-tuples, pair
/// books 5–11 walk pairs (long-window band widths are multiples of 4).
fn fill_bits(bits: &mut [u32; BOOKS], quant: &[i32; 1024], lo: usize, hi: usize) {
    for (cb, slot) in bits.iter_mut().enumerate().skip(1) {
        let step = if cb <= 4 { 4 } else { 2 };
        let mut total = 0u32;
        let mut i = lo;
        while i < hi {
            let tuple = &quant[i..i + step];
            let Some(n) = spectral_bits(cb as u8, tuple) else {
                total = UNREPRESENTABLE;
                break;
            };
            total = total.saturating_add(n as u32);
            i += step;
        }
        *slot = total;
    }
}

#[cfg(test)]
#[path = "enc_quant_tests.rs"]
mod enc_quant_tests;
