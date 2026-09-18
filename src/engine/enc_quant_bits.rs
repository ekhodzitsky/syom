//! Per-band Huffman cost under every spectral book (TASK-83 hot path).
//!
//! One pass over the band: the band's largest magnitude decides which
//! books can represent it (LAV 1 / 2 / 4 / 7 / 12, book 11 escapes), and
//! book pairs that share a tuple ordinal (1–2, 3–4, 5–6, 7–8, 9–10) are
//! costed from one index. Bit-for-bit the same table as walking
//! [`crate::engine::enc_huff::spectral_bits`] book by book.

use super::{BOOKS, UNREPRESENTABLE};
use crate::engine::enc_huff::esc_bits;
use crate::engine::{huff_esc, huff_pair, huff_quad};

/// Fill `bits[1..=11]` for the quantized band `vals` (width a multiple of
/// 4, as every AAC scalefactor band is). Slot 0 is left untouched.
pub(super) fn fill_bits(bits: &mut [u32; BOOKS], vals: &[i32]) {
    let max = vals.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0);
    let mut t = [0u32; BOOKS];
    if max <= 2 {
        for v in vals.chunks_exact(4) {
            let a = [v[0].abs(), v[1].abs(), v[2].abs(), v[3].abs()];
            let nz = a.iter().filter(|&&x| x != 0).count() as u32;
            let u = (a[0] * 27 + a[1] * 9 + a[2] * 3 + a[3]) as usize;
            t[3] += u32::from(huff_quad::H3_LEN[u]) + nz;
            t[4] += u32::from(huff_quad::H4_LEN[u]) + nz;
            if max <= 1 {
                let s = ((v[0] + 1) * 27 + (v[1] + 1) * 9 + (v[2] + 1) * 3 + (v[3] + 1)) as usize;
                t[1] += u32::from(huff_quad::H1_LEN[s]);
                t[2] += u32::from(huff_quad::H2_LEN[s]);
            }
        }
    }
    for v in vals.chunks_exact(2) {
        let (a, b) = (v[0].abs(), v[1].abs());
        let nz = u32::from(a != 0) + u32::from(b != 0);
        if max <= 4 {
            let s = ((v[0] + 4) * 9 + (v[1] + 4)) as usize;
            t[5] += u32::from(huff_pair::H5_LEN[s]);
            t[6] += u32::from(huff_pair::H6_LEN[s]);
        }
        if max <= 7 {
            let u = (a * 8 + b) as usize;
            t[7] += u32::from(huff_pair::H7_LEN[u]) + nz;
            t[8] += u32::from(huff_pair::H8_LEN[u]) + nz;
        }
        if max <= 12 {
            let u = (a * 13 + b) as usize;
            t[9] += u32::from(huff_pair::H9_LEN[u]) + nz;
            t[10] += u32::from(huff_pair::H10_LEN[u]) + nz;
        }
        let u = (a.min(16) * 17 + b.min(16)) as usize;
        t[11] += u32::from(huff_esc::H11_LEN[u]) + nz;
        for m in [a, b] {
            if m >= 16 {
                t[11] += esc_bits(m as u32) as u32;
            }
        }
    }
    let lav = [0u32, 1, 1, 2, 2, 4, 4, 7, 7, 12, 12, u32::MAX];
    for cb in 1..BOOKS {
        bits[cb] = if max <= lav[cb] {
            t[cb]
        } else {
            UNREPRESENTABLE
        };
    }
}

/// `true` if consecutive coded scalefactors are in `[0, 255]` with DPCM ±60.
#[must_use]
pub fn dpcm_ok(sf: &[i32], coded: &[bool], n: usize) -> bool {
    let mut prev: Option<i32> = None;
    for b in 0..n {
        if !coded[b] {
            continue;
        }
        let v = sf[b];
        if !(0..=255).contains(&v) {
            return false;
        }
        if let Some(p) = prev
            && (v - p).abs() > 60
        {
            return false;
        }
        prev = Some(v);
    }
    true
}
