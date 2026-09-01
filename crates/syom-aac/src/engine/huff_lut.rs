//! Two-level (8 + extra) prefix lookup for AAC Huffman books.
//!
//! A flat `2^max_len` table for codebook 3 is 64K entries and misses
//! L1 on every codeword. Almost all AAC spectrum codes are ≤ 8 bits;
//! those resolve from a 256-slot table that stays in L1. Longer codes
//! go to a per-prefix subtable of `2^(max_len-8)` (or tighter).

use crate::engine::bits::BitReader;
use crate::engine::{Error, Result};

const FIRST_BITS: u8 = 8;

#[derive(Clone, Copy)]
struct Leaf {
    len: u8,
    idx: u16,
    v: [i16; 4],
}

/// `extra == 0` → this 8-bit peek is a complete code (`len`, `idx`, `v`).
/// `extra > 0` → consume nothing yet; `idx` is the offset into `second`
/// of a `2^extra`-entry subtable.
#[derive(Clone, Copy)]
struct First {
    extra: u8,
    len: u8,
    idx: u32,
    v: [i16; 4],
}

/// Huffman acceleration table.
pub struct HuffLut {
    max_len: u8,
    first_bits: u8,
    first: Box<[First]>,
    second: Box<[Leaf]>,
}

impl HuffLut {
    /// Build a LUT from a `(length, right-aligned codeword)` table
    /// whose codewords fit in `u16` (`max_len` ≤ 16).
    pub fn from_u16(table: &[(u8, u16)], max_len: u8) -> Self {
        build(
            table.iter().map(|&(len, cw)| (len, u32::from(cw))),
            max_len,
            &[],
        )
    }

    /// Like [`Self::from_u16`], with the §4.6.3.3 n-tuple for each index.
    pub fn from_u16_tuples(table: &[(u8, u16)], tuples: &[[i16; 4]], max_len: u8) -> Self {
        build(
            table.iter().map(|&(len, cw)| (len, u32::from(cw))),
            max_len,
            tuples,
        )
    }

    /// Scalefactor book (`max_len` = 19).
    pub fn from_u32(table: &[(u8, u32)], max_len: u8) -> Self {
        build(table.iter().copied(), max_len, &[])
    }

    /// Decode one prefix codeword; returns the table index.
    #[inline(always)]
    pub fn decode(&self, reader: &mut BitReader<'_>) -> Result<u32> {
        Ok(u32::from(self.lookup(reader)?.idx))
    }

    /// Decode one prefix codeword; returns the n-tuple (low `dim` used).
    #[inline(always)]
    pub fn decode_vals(&self, reader: &mut BitReader<'_>) -> Result<[i32; 4]> {
        let v = self.lookup(reader)?.v;
        Ok([
            i32::from(v[0]),
            i32::from(v[1]),
            i32::from(v[2]),
            i32::from(v[3]),
        ])
    }

    #[inline(always)]
    fn lookup(&self, reader: &mut BitReader<'_>) -> Result<Leaf> {
        let fb = u32::from(self.first_bits);
        // Need first-level bits plus up to `max_len-first` extra for the
        // second table. 32 covers spectrum (≤16) and scalefactor (19).
        if reader.buffered_bits() < 48 {
            reader.refill_only();
        }
        let have = reader.buffered_bits();
        if have == 0 {
            return Err(Error::UnexpectedEnd);
        }
        let peeked = if have >= fb {
            reader.peek_from_acc(fb) as usize
        } else {
            (reader.peek_from_acc(have) as usize) << (fb - have) as usize
        };
        let f = self.first[peeked];
        if f.extra == 0 {
            if f.len == 0 || u32::from(f.len) > have {
                return Err(Error::UnexpectedEnd);
            }
            reader.consume_unchecked(u32::from(f.len));
            return Ok(Leaf {
                len: f.len,
                idx: f.idx as u16,
                v: f.v,
            });
        }
        let extra = u32::from(f.extra);
        let need = fb + extra;
        let wide = if have >= need {
            reader.peek_from_acc(need)
        } else {
            reader.peek_from_acc(have) << (need - have)
        };
        let low = (wide as usize) & ((1usize << extra) - 1);
        let leaf = self.second[f.idx as usize + low];
        if leaf.len == 0 || u32::from(leaf.len) > have {
            return Err(Error::UnexpectedEnd);
        }
        reader.consume_unchecked(u32::from(leaf.len));
        Ok(leaf)
    }
}

fn empty_first() -> First {
    First {
        extra: 0,
        len: 0,
        idx: 0,
        v: [0; 4],
    }
}

fn empty_leaf() -> Leaf {
    Leaf {
        len: 0,
        idx: 0,
        v: [0; 4],
    }
}

fn build(table: impl Iterator<Item = (u8, u32)>, max_len: u8, tuples: &[[i16; 4]]) -> HuffLut {
    debug_assert!(max_len > 0 && max_len <= 24);
    let first_bits = FIRST_BITS.min(max_len);
    let nfirst = 1usize << first_bits;
    let mut first = vec![empty_first(); nfirst];
    let mut long: [Vec<(u8, u32, u16, [i16; 4])>; 256] = core::array::from_fn(|_| Vec::new());
    let rows: Vec<(u8, u32)> = table.collect();
    for (idx, &(len, cw)) in rows.iter().enumerate() {
        debug_assert!(len > 0 && len <= max_len);
        let v = tuples.get(idx).copied().unwrap_or([0; 4]);
        if len <= first_bits {
            let shift = first_bits - len;
            let base = (cw as usize) << shift;
            let count = 1usize << shift;
            let slot = First {
                extra: 0,
                len,
                idx: idx as u32,
                v,
            };
            for j in 0..count {
                first[base + j] = slot;
            }
        } else {
            let extra = len - first_bits;
            let prefix = (cw >> extra) as usize;
            long[prefix].push((len, cw, idx as u16, v));
        }
    }

    let mut second = Vec::new();
    for (prefix, codes) in long.iter().enumerate() {
        if codes.is_empty() {
            continue;
        }
        let mut max_extra = 0u8;
        for &(len, _, _, _) in codes {
            max_extra = max_extra.max(len - first_bits);
        }
        let size = 1usize << max_extra;
        let offset = second.len();
        second.resize(offset + size, empty_leaf());
        for &(len, cw, idx, v) in codes {
            let extra_len = len - first_bits;
            let low = cw & ((1u32 << extra_len) - 1);
            let sub_shift = max_extra - extra_len;
            let base = offset + ((low as usize) << sub_shift);
            let count = 1usize << sub_shift;
            let leaf = Leaf { len, idx, v };
            for j in 0..count {
                second[base + j] = leaf;
            }
        }
        first[prefix] = First {
            extra: max_extra,
            len: 0,
            idx: offset as u32,
            v: [0; 4],
        };
    }

    HuffLut {
        max_len,
        first_bits,
        first: first.into_boxed_slice(),
        second: second.into_boxed_slice(),
    }
}
