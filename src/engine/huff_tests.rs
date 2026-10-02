//! Huffman 1–11 and Table 4.A.1 scalefactor codebook.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::bits::{BitReader, BitWriter};
use super::huff;
use super::sf::decode_dpcm;

fn emit(len: u8, code: u32) -> Vec<u8> {
    let mut w = BitWriter::new();
    w.write(code, u32::from(len));
    w.finish()
}

#[test]
fn book1_zero_tuple_is_single_bit_zero() -> Result<(), super::Error> {
    // Table 4.A.2 index 40 = (0,0,0,0), 1-bit `0`.
    let bytes = emit(1, 0);
    let mut br = BitReader::new(&bytes);
    let mut out = [0i32; 4];
    let n = huff::decode_tuple(&mut br, 1, &mut out)?;
    assert_eq!(n, 4);
    assert_eq!(out, [0, 0, 0, 0]);
    Ok(())
}

#[test]
fn book1_roundtrip_index_zero() -> Result<(), super::Error> {
    // Index 0 = (−1,−1,−1,−1). Length/code from Table 4.A.2.
    let (len, code) = (super::huff_quad::H1_LEN[0], super::huff_quad::H1_CODE[0]);
    let bytes = emit(len, u32::from(code));
    let mut br = BitReader::new(&bytes);
    let mut out = [0i32; 4];
    huff::decode_tuple(&mut br, 1, &mut out)?;
    assert_eq!(out, [-1, -1, -1, -1]);
    Ok(())
}

#[test]
fn scalefactor_delta_zero_is_one_bit() -> Result<(), super::Error> {
    // Table 4.A.1 index 60 (delta 0) is the single bit `0`.
    let bytes = emit(1, 0);
    let mut br = BitReader::new(&bytes);
    assert_eq!(decode_dpcm(&mut br)?, 0);
    Ok(())
}

fn decode_cb(cb: u8, idx: usize, signs: &[bool]) -> Result<(usize, [i32; 4], u64), super::Error> {
    let (len, code) = match cb {
        1 => (
            super::huff_quad::H1_LEN[idx],
            super::huff_quad::H1_CODE[idx],
        ),
        2 => (
            super::huff_quad::H2_LEN[idx],
            super::huff_quad::H2_CODE[idx],
        ),
        3 => (
            super::huff_quad::H3_LEN[idx],
            super::huff_quad::H3_CODE[idx],
        ),
        4 => (
            super::huff_quad::H4_LEN[idx],
            super::huff_quad::H4_CODE[idx],
        ),
        5 => (
            super::huff_pair::H5_LEN[idx],
            super::huff_pair::H5_CODE[idx],
        ),
        6 => (
            super::huff_pair::H6_LEN[idx],
            super::huff_pair::H6_CODE[idx],
        ),
        7 => (
            super::huff_pair::H7_LEN[idx],
            super::huff_pair::H7_CODE[idx],
        ),
        8 => (
            super::huff_pair::H8_LEN[idx],
            super::huff_pair::H8_CODE[idx],
        ),
        9 => (
            super::huff_pair::H9_LEN[idx],
            super::huff_pair::H9_CODE[idx],
        ),
        10 => (
            super::huff_pair::H10_LEN[idx],
            super::huff_pair::H10_CODE[idx],
        ),
        11 => (
            super::huff_esc::H11_LEN[idx],
            super::huff_esc::H11_CODE[idx],
        ),
        _ => unreachable!(),
    };
    let mut w = BitWriter::new();
    w.write(u32::from(code), u32::from(len));
    for &s in signs {
        w.write_bit(s);
    }
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let mut out = [0i32; 4];
    let n = huff::decode_tuple(&mut br, cb, &mut out)?;
    Ok((n, out, br.bit_position()))
}

#[test]
fn books_1_through_11_exact_tuples() -> Result<(), super::Error> {
    // Signed quads (1–2): index maps to −1..=1, no extra bits.
    let (_, t, pos) = decode_cb(2, 0, &[])?;
    assert_eq!(&t[..4], &[-1, -1, -1, -1]);
    assert_eq!(pos, u64::from(super::huff_quad::H2_LEN[0]));

    // Unsigned quads (3–4): magnitudes then one sign per non-zero.
    let (_, t, pos) = decode_cb(3, 1, &[true])?;
    assert_eq!(&t[..4], &[0, 0, 0, -1]);
    assert_eq!(pos, u64::from(super::huff_quad::H3_LEN[1]) + 1);

    let (_, t, pos) = decode_cb(4, 1, &[false])?;
    assert_eq!(&t[..4], &[0, 0, 0, 1]);
    assert_eq!(pos, u64::from(super::huff_quad::H4_LEN[1]) + 1);

    // Signed pairs (5–6).
    let (_, t, pos) = decode_cb(5, 0, &[])?;
    assert_eq!(&t[..2], &[-4, -4]);
    assert_eq!(pos, u64::from(super::huff_pair::H5_LEN[0]));

    let (_, t, _) = decode_cb(6, 40, &[])?;
    assert_eq!(&t[..2], &[0, 0]);

    // Unsigned pairs (7–10).
    let (_, t, pos) = decode_cb(7, 1, &[true])?;
    assert_eq!(&t[..2], &[0, -1]);
    assert_eq!(pos, u64::from(super::huff_pair::H7_LEN[1]) + 1);

    let (_, t, pos) = decode_cb(8, 1, &[false])?;
    assert_eq!(&t[..2], &[0, 1]);
    assert_eq!(pos, u64::from(super::huff_pair::H8_LEN[1]) + 1);

    let (_, t, pos) = decode_cb(9, 1, &[true])?;
    assert_eq!(&t[..2], &[0, -1]);
    assert_eq!(pos, u64::from(super::huff_pair::H9_LEN[1]) + 1);

    let (_, t, pos) = decode_cb(10, 1, &[false])?;
    assert_eq!(&t[..2], &[0, 1]);
    assert_eq!(pos, u64::from(super::huff_pair::H10_LEN[1]) + 1);

    // Book 11 unsigned pair, no ESC.
    let (_, t, pos) = decode_cb(11, 1, &[true])?;
    assert_eq!(&t[..2], &[0, -1]);
    assert_eq!(pos, u64::from(super::huff_esc::H11_LEN[1]) + 1);
    Ok(())
}

#[test]
fn book11_signs_before_escape() -> Result<(), super::Error> {
    // ISO/IEC 14496-3 §4.6.3.3: book 11 is unsigned. After the Huffman
    // pair, sign bits for each non-zero (including the 16 flag), then
    // escape_sequence for each magnitude-16. Encoder (lavc) writes the
    // same order. Reversing it desyncs the next channel.
    // Pair (16, 0): idx = 16*17 = 272. Sign=1 → negative. ESC n=4, off=0 → 16.
    let idx = 16 * 17;
    let (len, code) = (
        super::huff_esc::H11_LEN[idx],
        super::huff_esc::H11_CODE[idx],
    );
    let mut w = BitWriter::new();
    w.write(u32::from(code), u32::from(len));
    w.write_bit(true); // sign of 16
    w.write_bit(false); // ESC prefix ends (n stays 4)
    w.write(0, 4); // offset → magnitude 16
    w.write(0b1010, 4); // must remain unread
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let mut out = [0i32; 4];
    huff::decode_tuple(&mut br, 11, &mut out)?;
    assert_eq!(out[0], -16);
    assert_eq!(out[1], 0);
    let expect = u64::from(len) + 1 + 1 + 4;
    assert_eq!(
        br.bit_position(),
        expect,
        "sign-then-ESC must consume {expect} bits"
    );
    Ok(())
}

/// Same bits and the same `invquant(q) * gain` as the integer tuple path.
fn assert_fill(cb: u8, bytes: &[u8], n: usize, expect_bits: u64) -> Result<(), super::Error> {
    let gain = super::spectrum::sf_gain(137);
    let mut slow_br = BitReader::new(bytes);
    let mut slow = vec![0.0f32; n];
    let mut filled = 0usize;
    let mut tuple = [0i32; 4];
    while filled < n {
        let k = huff::decode_tuple(&mut slow_br, cb, &mut tuple)?;
        let take = k.min(n - filled);
        for (slot, &q) in slow[filled..filled + take].iter_mut().zip(tuple.iter()) {
            *slot = super::spectrum::invquant(q) * gain;
        }
        filled += take;
        if take < k {
            break;
        }
    }
    let mut fast_br = BitReader::new(bytes);
    let mut fast = vec![0.0f32; n];
    huff::SpectralBook::open(cb)?.fill(&mut fast_br, gain, &mut fast)?;
    for (i, (got, want)) in fast.iter().zip(&slow).enumerate() {
        assert_eq!(got.to_bits(), want.to_bits(), "cb={cb} i={i}");
    }
    assert_eq!(slow_br.bit_position(), expect_bits, "cb={cb} slow");
    assert_eq!(fast_br.bit_position(), expect_bits, "cb={cb} fast");
    Ok(())
}

#[test]
fn fill_band_matches_invquant_including_spill_and_escape() -> Result<(), super::Error> {
    let gain_bits = |len: u8| u64::from(len);

    // Signed pair, two tuples, contiguous band.
    let (l0, c0) = (super::huff_pair::H5_LEN[0], super::huff_pair::H5_CODE[0]);
    let (l40, c40) = (super::huff_pair::H5_LEN[40], super::huff_pair::H5_CODE[40]);
    let mut w = BitWriter::new();
    w.write(u32::from(c0), u32::from(l0));
    w.write(u32::from(c40), u32::from(l40));
    w.write(0b1010, 4);
    let pair_bits = gain_bits(l0) + gain_bits(l40);
    assert_fill(5, &w.finish(), 4, pair_bits)?;

    // Unsigned pair: one sign bit, full tuple.
    let (l1, c1) = (super::huff_pair::H7_LEN[1], super::huff_pair::H7_CODE[1]);
    let mut w = BitWriter::new();
    w.write(u32::from(c1), u32::from(l1));
    w.write_bit(true);
    w.write(0b1010, 4);
    assert_fill(7, &w.finish(), 2, gain_bits(l1) + 1)?;

    // Unsigned quad [1, 1, 0, 0] into a 1-wide band: the second sign is
    // consumed and the coefficient is dropped.
    let idx = 36usize;
    let (lq, cq) = (
        super::huff_quad::H3_LEN[idx],
        super::huff_quad::H3_CODE[idx],
    );
    let mut w = BitWriter::new();
    w.write(u32::from(cq), u32::from(lq));
    w.write_bit(true);
    w.write_bit(false);
    w.write(0b1010, 4);
    assert_fill(3, &w.finish(), 1, gain_bits(lq) + 2)?;

    // Signed quad spilled across two tuples (band length 5).
    let (la, ca) = (super::huff_quad::H1_LEN[0], super::huff_quad::H1_CODE[0]);
    let mut w = BitWriter::new();
    w.write(u32::from(ca), u32::from(la));
    w.write(u32::from(ca), u32::from(la));
    w.write(0b1010, 4);
    assert_fill(1, &w.finish(), 5, gain_bits(la) * 2)?;

    // Book 11: sign, then escape past the POW43 table (n=13, off=5 → 8197).
    let esc_idx = 16 * 17;
    let (le, ce) = (
        super::huff_esc::H11_LEN[esc_idx],
        super::huff_esc::H11_CODE[esc_idx],
    );
    let mut w = BitWriter::new();
    w.write(u32::from(ce), u32::from(le));
    w.write_bit(true);
    for _ in 0..9 {
        w.write_bit(true);
    }
    w.write_bit(false);
    w.write(5, 13);
    w.write(0b1010, 4);
    let esc_bits = gain_bits(le) + 1 + 9 + 1 + 13;
    assert_fill(11, &w.finish(), 2, esc_bits)?;
    // Same stream, one written sample: the escape is still consumed.
    let mut w = BitWriter::new();
    w.write(u32::from(ce), u32::from(le));
    w.write_bit(true);
    for _ in 0..9 {
        w.write_bit(true);
    }
    w.write_bit(false);
    w.write(5, 13);
    w.write(0b1010, 4);
    assert_fill(11, &w.finish(), 1, esc_bits)?;
    Ok(())
}

/// One short group of two windows, book 11, compared with the integer
/// quant path. Standard short bands are multiples of 4, so a pair stays
/// inside one window; the group is still one coefficient stream.
#[test]
fn grouped_short_book11_matches_integer_rescale() -> Result<(), super::Error> {
    use super::ics::{IcsInfo, WindowSequence, WindowShape};
    use super::section::SectionData;
    use super::sf::ScaleFactors;

    let mut lens = [0u8; 8];
    lens[0] = 2;
    for slot in lens.iter_mut().take(7).skip(1) {
        *slot = 1;
    }
    let ics = IcsInfo {
        window_sequence: WindowSequence::EightShort,
        window_shape: WindowShape::Sine,
        max_sfb: 1,
        num_windows: 8,
        num_window_groups: 7,
        window_group_length: lens,
        num_swb: 14,
        ld: false,
    };
    let sections = SectionData {
        sfb_cb: vec![
            vec![11],
            vec![0],
            vec![0],
            vec![0],
            vec![0],
            vec![0],
            vec![0],
        ],
    };
    let sf = ScaleFactors {
        sf: vec![vec![137]; 7],
        ..ScaleFactors::default()
    };

    let (l0, c0) = (super::huff_esc::H11_LEN[0], super::huff_esc::H11_CODE[0]);
    let (l1, c1) = (super::huff_esc::H11_LEN[1], super::huff_esc::H11_CODE[1]);
    let esc_idx = 16 * 17;
    let (le, ce) = (
        super::huff_esc::H11_LEN[esc_idx],
        super::huff_esc::H11_CODE[esc_idx],
    );
    let mut w = BitWriter::new();
    // (0, 1) with a sign, then (0, 0): first window, band width 4.
    w.write(u32::from(c1), u32::from(l1));
    w.write_bit(true);
    w.write(u32::from(c0), u32::from(l0));
    // Escape pair into the second window of the group, then (0, 0).
    w.write(u32::from(ce), u32::from(le));
    w.write_bit(true);
    for _ in 0..9 {
        w.write_bit(true);
    }
    w.write_bit(false);
    w.write(5, 13);
    w.write(u32::from(c0), u32::from(l0));
    w.write(0b1010, 4);
    let bytes = w.finish();
    let expect = u64::from(l1) + 1 + u64::from(l0) + u64::from(le) + 1 + 9 + 1 + 13 + u64::from(l0);

    let mut slow_br = BitReader::new(&bytes);
    let mut quant = Vec::new();
    super::spectrum::parse_quant_into(&mut slow_br, &ics, &sections, 3, &mut quant)?;
    let mut slow = Vec::new();
    super::spectrum::rescale_into(&quant, &ics, &sections, &sf, 3, &mut slow)?;
    let mut fast_br = BitReader::new(&bytes);
    let mut fast = Vec::new();
    super::spectrum_scaled::decode_scaled_into(&mut fast_br, &ics, &sections, &sf, 3, &mut fast)?;

    assert_eq!(slow_br.bit_position(), expect);
    assert_eq!(fast_br.bit_position(), expect);
    assert_eq!(fast.len(), slow.len());
    for (i, (got, want)) in fast.iter().zip(&slow).enumerate() {
        assert_eq!(got.to_bits(), want.to_bits(), "i={i}");
    }
    let gain = super::spectrum::sf_gain(137);
    assert_eq!(
        fast[1].to_bits(),
        (super::spectrum::invquant(-1) * gain).to_bits()
    );
    assert_eq!(
        fast[128].to_bits(),
        (super::spectrum::invquant(-8197) * gain).to_bits()
    );
    assert_eq!(fast[4].to_bits(), 0f32.to_bits());
    assert_eq!(fast[256].to_bits(), 0f32.to_bits());
    Ok(())
}
