//! The generic tuple emitter remains an independent oracle.
#![allow(clippy::expect_used)]
use super::*;

fn compare(cb: u8, values: &[i32], prefix: u32) {
    let mut original = BitWriter::new();
    let mut candidate = BitWriter::new();
    original.write(0x55, prefix);
    candidate.write(0x55, prefix);
    let width = if cb <= 4 { 4 } else { 2 };
    for tuple in values.chunks(width) {
        spectral_emit(cb, tuple, &mut original);
    }
    spectral_emit_band(cb, values, &mut candidate);
    assert_eq!(original.bit_len(), candidate.bit_len());
    assert_eq!(
        original.finish(),
        candidate.finish(),
        "cb={cb} prefix={prefix}"
    );
}

#[test]
fn all_books_preserve_valid_invalid_and_incomplete_tuples() {
    let mut state = 0xC0FF_EE42u32;
    for cb in 0..=15 {
        let limit = match cb {
            1 | 2 => 1,
            3 | 4 => 2,
            5 | 6 => 4,
            7 | 8 => 7,
            9 | 10 => 12,
            _ => 33,
        };
        for len in [0, 1, 2, 3, 4, 7, 8, 31, 32, 127, 128, 1024] {
            let mut values = Vec::new();
            for i in 0..len {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let value = if cb == 11 && i % 13 == 0 {
                    8191
                } else {
                    (state % (2 * limit + 1)) as i32 - limit as i32
                };
                values.push(value);
            }
            for prefix in 0..8 {
                compare(cb, &values, prefix);
                compare(cb, &vec![0; len], prefix);
                compare(cb, &vec![(limit + 1) as i32; len], prefix);
            }
        }
    }
}

#[test]
fn escape_boundaries_and_mixed_signs_preserve_bit_order() {
    let mut values = vec![0, 1, 15, 16, 17, 31, 32, 255, 256, 8191, i32::MAX];
    for exponent in 4..=30 {
        values.extend([(1 << exponent) - 1, 1 << exponent, (1 << exponent) + 1]);
    }
    for value in values {
        for other in [0, 15, 16, -16, -31, -8191] {
            for prefix in 0..8 {
                compare(11, &[value, other, -value, -other], prefix);
            }
        }
    }
}

#[test]
fn every_codeword_and_its_tuple_signs_fit_one_u32() {
    for cb in 1..=11 {
        let (lengths, _) = tables(cb).expect("spectral book");
        let signs = if unsigned_book(cb) {
            if cb <= 4 { 4 } else { 2 }
        } else {
            0
        };
        assert!(
            lengths
                .iter()
                .all(|&length| u32::from(length) + signs <= 32)
        );
    }
}

#[test]
fn extreme_invalid_values_preserve_generic_checked_behavior() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    for cb in 0..=15 {
        for values in [
            [i32::MIN, 0, 0, 0],
            [i32::MIN; 4],
            [i32::MAX; 4],
            [0, i32::MIN, 0, i32::MAX],
        ] {
            let mut original = BitWriter::new();
            let mut candidate = BitWriter::new();
            let width = if cb <= 4 { 4 } else { 2 };
            let original_result = catch_unwind(AssertUnwindSafe(|| {
                for tuple in values.chunks(width) {
                    spectral_emit(cb, tuple, &mut original);
                }
            }));
            let candidate_result = catch_unwind(AssertUnwindSafe(|| {
                spectral_emit_band(cb, &values, &mut candidate);
            }));
            assert_eq!(original_result.is_ok(), candidate_result.is_ok(), "cb={cb}");
            assert_eq!(original.bit_len(), candidate.bit_len());
            assert_eq!(original.finish(), candidate.finish());
        }
    }
}
