//! Huffman 1–11 and Table 4.A.1 scalefactor codebook.

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

#[test]
fn books_1_through_11_decode_zeroish_prefix() -> Result<(), super::Error> {
    // Each book's shortest/zero-ish codeword must decode without UnexpectedEnd.
    for cb in 1u8..=11 {
        let (len, code) = match cb {
            1 => (super::huff_quad::H1_LEN[40], super::huff_quad::H1_CODE[40]),
            2 => (super::huff_quad::H2_LEN[40], super::huff_quad::H2_CODE[40]),
            3 => (super::huff_quad::H3_LEN[0], super::huff_quad::H3_CODE[0]),
            4 => (super::huff_quad::H4_LEN[0], super::huff_quad::H4_CODE[0]),
            5 => (super::huff_pair::H5_LEN[40], super::huff_pair::H5_CODE[40]),
            6 => (super::huff_pair::H6_LEN[40], super::huff_pair::H6_CODE[40]),
            7 => (super::huff_pair::H7_LEN[0], super::huff_pair::H7_CODE[0]),
            8 => (super::huff_pair::H8_LEN[0], super::huff_pair::H8_CODE[0]),
            9 => (super::huff_pair::H9_LEN[0], super::huff_pair::H9_CODE[0]),
            10 => (super::huff_pair::H10_LEN[0], super::huff_pair::H10_CODE[0]),
            11 => (super::huff_esc::H11_LEN[0], super::huff_esc::H11_CODE[0]),
            _ => unreachable!(),
        };
        let bytes = emit(len, u32::from(code));
        let mut br = BitReader::new(&bytes);
        let mut out = [0i32; 4];
        let n = huff::decode_tuple(&mut br, cb, &mut out)?;
        assert!(n == 2 || n == 4, "cb {cb} dim {n}");
    }
    Ok(())
}
