//! Packed costs against the independent tuple encoder's reference costs.
use super::*;

fn reference(vals: &[i32], cb: u8) -> u32 {
    let width = if cb <= 4 { 4 } else { 2 };
    let mut sum = 0;
    for tuple in vals.chunks_exact(width) {
        let Some(cost) = crate::engine::enc_huff::spectral_bits(cb, tuple) else {
            return UNREPRESENTABLE;
        };
        sum += cost as u32;
    }
    // fill_bits deliberately uses the entire band's max, including a
    // partial trailing tuple, to decide representability.
    let lav = [0, 1, 1, 2, 2, 4, 4, 7, 7, 12, 12, u32::MAX][cb as usize];
    if vals.iter().any(|v| v.unsigned_abs() > lav) {
        UNREPRESENTABLE
    } else {
        sum
    }
}

#[test]
fn packed_and_fallback_costs_match_all_books_and_preserve_slot_zero() {
    let mut seed = 0x9d42_e113u32;
    for max in [0, 1, 2, 3, 4, 5, 7, 8, 12, 13, 16, 31, 8191] {
        for n in [
            0, 1, 2, 3, 4, 7, 16, 64, 128, 256, 512, 1023, 1024, 1025, 1032,
        ] {
            let mut vals = Vec::with_capacity(n);
            for i in 0..n {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let value = if i == 0 {
                    max
                } else {
                    (seed % (max as u32 + 1)) as i32
                };
                vals.push(if i % 3 == 0 { -value } else { value });
            }
            let mut bits = [0xace1; BOOKS];
            super::super::fill_bits(&mut bits, &vals);
            assert_eq!(bits[0], 0xace1);
            for (cb, &cost) in bits.iter().enumerate().skip(1) {
                assert_eq!(cost, reference(&vals, cb as u8), "max={max} n={n} cb={cb}");
            }
        }
    }
}

#[test]
fn packed_costs_cannot_carry_between_lanes() {
    for max in [1, 2, 4, 7] {
        for pattern in [[max, -max, max, -max], [max, 0, 0, 0], [-max; 4]] {
            let vals = pattern.repeat(256);
            let mut got = [0; BOOKS];
            fill_small(&mut got, &vals, max as u32);
            for (cb, &cost) in got.iter().enumerate().skip(1) {
                assert_eq!(cost, reference(&vals, cb as u8));
            }
        }
    }
}

#[test]
fn every_small_quad_and_signed_pair_matches_the_tuple_reference() {
    for ordinal in 0..625 {
        let mut value = ordinal;
        let mut vals = [0; 4];
        for slot in &mut vals {
            *slot = value % 5 - 2;
            value /= 5;
        }
        let mut bits = [0; BOOKS];
        super::super::fill_bits(&mut bits, &vals);
        for (cb, &cost) in bits.iter().enumerate().skip(1) {
            assert_eq!(cost, reference(&vals, cb as u8));
        }
    }
    for a in -12..=12 {
        for b in -12..=12 {
            let vals = [a, b, a, b];
            let mut bits = [0; BOOKS];
            super::super::fill_bits(&mut bits, &vals);
            for (cb, &cost) in bits.iter().enumerate().skip(1) {
                assert_eq!(cost, reference(&vals, cb as u8));
            }
        }
    }
}
