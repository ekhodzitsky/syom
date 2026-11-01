//! BitReader helpers used only from tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::error::Error;
use super::{BitReader, BitWriter};

#[test]
fn wide_and_signed_helpers() -> Result<(), Error> {
    let bytes = [0xA5, 0x5A, 0xFF, 0x00, 0x80, 0x11];
    let mut br = BitReader::with_position(&bytes, 0);
    assert_eq!(br.read_u1()?, 1);
    assert!(br.peek_u32(7)? > 0);
    let _ = br.read_u32(7)?;
    let mut br = BitReader::new(&bytes);
    let _ = br.read_i32(16)?;
    let mut br = BitReader::new(&bytes);
    let _ = br.read_u64(40)?;
    let mut br = BitReader::new(&bytes);
    br.consume(3)?;
    br.align_to_byte()?;
    assert!(br.is_byte_aligned());
    assert_eq!(br.byte_position(), 1);
    Ok(())
}

fn reference(fields: &[(u32, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut at = 0;
    for &(value, width) in fields {
        for bit in (0..width).rev() {
            if at % 8 == 0 {
                bytes.push(0);
            }
            let last = bytes.len() - 1;
            bytes[last] |= (((value >> bit) & 1) as u8) << (7 - at % 8);
            at += 1;
        }
    }
    bytes
}

#[test]
fn writer_fields_match_independent_bits_at_every_byte_boundary() {
    for prefix in 0..8 {
        for width in 0..=32 {
            for value in [0, 1, u32::MAX, 0x8000_0000, 0xA5A5_5A5A] {
                let fields = [(0x55, prefix), (value, width), (0x3C, 6)];
                let mut writer = BitWriter::new();
                for &(v, n) in &fields {
                    writer.write(v, n);
                }
                assert_eq!(writer.bit_len(), u64::from(prefix + width + 6));
                assert_eq!(
                    writer.finish(),
                    reference(&fields),
                    "prefix={prefix} width={width} value={value:x}"
                );
            }
        }
    }
}

#[test]
fn writer_mixed_fields_reuse_capacity_without_retaining_bits() {
    let mut seed = 0xC0FF_EE42u32;
    let mut buffer = vec![0xFF; 4096];
    for round in 0..4 {
        let capacity = buffer.capacity();
        let mut writer = BitWriter::from_vec(buffer);
        assert_eq!(writer.bit_len(), 0);
        let mut fields = Vec::new();
        let mut total = 0u64;
        for i in 0..5000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let width = if i % 7 == 0 { 1 } else { seed % 33 };
            let value = if i % 7 == 0 { (seed >> 31) & 1 } else { seed };
            if i % 7 == 0 {
                writer.write_bit(value != 0);
            } else if i % 2 == 0 {
                writer.write_u32(value, width);
            } else {
                writer.write(value, width);
            }
            fields.push((value, width));
            total += u64::from(width);
            assert_eq!(writer.bit_len(), total);
        }
        buffer = writer.finish();
        assert!(buffer.capacity() >= capacity);
        assert_eq!(buffer, reference(&fields), "reuse round={round}");
    }
    assert_eq!(BitWriter::from_vec(buffer).finish(), Vec::<u8>::new());
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn writer_preserves_checked_shift_failure_above_u32_width() {
    BitWriter::new().write(0, 33);
}

#[test]
fn writer_resumes_every_prefix_without_preserving_discarded_tail() {
    for source in [b"".as_slice(), &[0xA5, 0x3C, 0xFF, 0x80, 0x69][..]] {
        for prefix in 0..=source.len() * 8 {
            let prefix_fields: Vec<_> = (0..prefix)
                .map(|i| (u32::from((source[i / 8] >> (7 - i % 8)) & 1), 1))
                .collect();
            for width in 0..=32 {
                let mut storage = Vec::with_capacity(128);
                storage.extend_from_slice(source);
                let pointer = storage.as_ptr();
                let capacity = storage.capacity();
                let mut resumed = BitWriter::resume_vec(storage, prefix);
                assert_eq!(resumed.bit_len(), prefix as u64);
                assert_eq!(resumed.buf.capacity(), capacity);
                assert_eq!(resumed.buf.as_ptr(), pointer);
                let mut expected = prefix_fields.clone();
                for field in [(u32::MAX, width), (0, 7), (0x1234_5678, 32), (1, 1)] {
                    resumed.write(field.0, field.1);
                    expected.push(field);
                }
                assert_eq!(
                    resumed.finish(),
                    reference(&expected),
                    "prefix={prefix} width={width}"
                );
            }
            let untouched = BitWriter::resume_vec(source.to_vec(), prefix);
            assert_eq!(untouched.finish(), reference(&prefix_fields));
        }
    }
}

#[test]
#[should_panic(expected = "bit prefix exceeds supplied bytes")]
fn writer_resume_rejects_partial_byte_beyond_buffer() {
    let _ = BitWriter::resume_vec(vec![0, 0], 17);
}

#[test]
#[should_panic(expected = "bit prefix exceeds supplied bytes")]
fn writer_resume_rejects_overflowing_length() {
    let _ = BitWriter::resume_vec(Vec::new(), usize::MAX);
}
