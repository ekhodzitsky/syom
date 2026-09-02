//! LATM mux config / payload helper branches.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{MuxCfg, latm_value, read_frame_len, skip_mux_tail};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::error::Error;
use crate::engine::latm::decode_loas_planar;

#[test]
fn decode_loas_resync_and_empty_is_error() {
    let mut data = vec![0u8, 0, 0];
    data.extend_from_slice(&[0x2B, 0xE0, 0x00]);
    assert!(decode_loas_planar(&[0, 1, 2], false).is_err());
    assert!(decode_loas_planar(&[], false).is_err());
}

#[test]
fn mux_version_a_reserved_and_program_layer() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write_bit(true);
    w.write_bit(true);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(matches!(
        MuxCfg::parse(&mut br),
        Err(Error::LatmAudioMuxVersionAReserved)
    ));

    let mut w = BitWriter::new();
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 6);
    w.write(1, 4);
    w.write(0, 3);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(matches!(
        MuxCfg::parse(&mut br),
        Err(Error::LatmConfigOutOfRange)
    ));
    Ok(())
}

#[test]
fn frame_len_types_and_mux_tail_and_latm_value() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0, 8);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert_eq!(read_frame_len(&mut br, 0)?, 0);

    let mut w = BitWriter::new();
    w.write(8, 9);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert_eq!(read_frame_len(&mut br, 1)?, 8);
    let mut br = BitReader::new(&[0xFF]);
    assert!(read_frame_len(&mut br, 2).is_err());

    let mut w = BitWriter::new();
    w.write_bit(true);
    w.write_bit(false);
    w.write(0, 8);
    w.write_bit(true);
    w.write(0x5A, 8);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    skip_mux_tail(&mut br)?;

    let mut w = BitWriter::new();
    w.write_bit(true);
    w.write_bit(true);
    w.write_bit(false);
    w.write(1, 8);
    w.write_bit(false);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    skip_mux_tail(&mut br)?;

    let mut w = BitWriter::new();
    w.write(1, 8);
    w.write_bit(true);
    w.write(2, 8);
    w.write_bit(false);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert_eq!(latm_value(&mut br)?, (1 << 8) | 2);
    Ok(())
}
