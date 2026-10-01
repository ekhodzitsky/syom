//! LATM mux config / payload helper branches.
//!
//! LOAS stream-level behavior (resync, CRC propagation over the public
//! streaming path) is covered by `crate::stream_latm_tests`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{MuxCfg, latm_value, read_frame_len, read_payload, skip_mux_tail};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::crc::stream_mux_config_crc;
use crate::engine::error::Error;

fn push_bits(bits: &mut Vec<bool>, value: u32, n: u32) {
    for i in (0..n).rev() {
        bits.push((value >> i) & 1 != 0);
    }
}

fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    let mut w = BitWriter::new();
    for &b in bits {
        w.write_bit(b);
    }
    w.finish()
}

/// A minimal `StreamMuxConfig()` body, up to but excluding
/// `crcCheckPresent`: LC, 44.1 kHz, stereo, `frameLengthType = 1`.
fn mux_config_bits() -> Vec<bool> {
    let mut b = Vec::new();
    push_bits(&mut b, 0, 1); // audioMuxVersion
    push_bits(&mut b, 1, 1); // allStreamsSameTimeFraming
    push_bits(&mut b, 0, 6); // numSubFrames
    push_bits(&mut b, 0, 4); // numProgram
    push_bits(&mut b, 0, 3); // numLayer
    push_bits(&mut b, 2, 5); // audioObjectType = AAC LC
    push_bits(&mut b, 4, 4); // samplingFrequencyIndex = 44.1 kHz
    push_bits(&mut b, 2, 4); // channelConfiguration = stereo
    push_bits(&mut b, 0, 3); // GASpecificConfig flags
    push_bits(&mut b, 1, 3); // frameLengthType = 1
    push_bits(&mut b, 0, 9); // frameLength
    push_bits(&mut b, 0, 1); // otherDataPresent = 0
    b
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

    // otherDataPresent (bit-length form) + crcCheckPresent, valid CRC.
    let mut b = Vec::new();
    push_bits(&mut b, 1, 1); // otherDataPresent
    push_bits(&mut b, 0, 1); // otherDataLenBits form (bits, not bytes)
    push_bits(&mut b, 0, 8); // otherDataLenBits = 0 → one bit follows
    push_bits(&mut b, 0, 1); // the otherData bit
    let crc = u32::from(stream_mux_config_crc(&b));
    b.push(true); // crcCheckPresent
    push_bits(&mut b, crc, 8);
    let bytes = bits_to_bytes(&b);
    let mut br = BitReader::new(&bytes);
    skip_mux_tail(&mut br, 0, false)?;

    // otherDataPresent (byte-escaped form), crcCheckPresent = 0.
    let mut b = Vec::new();
    push_bits(&mut b, 1, 1);
    push_bits(&mut b, 1, 1); // escaped
    b.push(false); // more = 0
    push_bits(&mut b, 1, 8);
    b.push(false); // crcCheckPresent = 0
    let bytes = bits_to_bytes(&b);
    let mut br = BitReader::new(&bytes);
    skip_mux_tail(&mut br, 0, false)?;

    let mut w = BitWriter::new();
    w.write(0, 2);
    w.write(0xAB, 8);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert_eq!(latm_value(&mut br)?, 0xAB);

    let mut w = BitWriter::new();
    w.write(1, 2);
    w.write(0x0102, 16);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert_eq!(latm_value(&mut br)?, 0x0102);
    Ok(())
}

#[test]
fn mux_v1_latmgetvalue_parses_lc_48k() -> Result<(), Error> {
    // TASK-17 latm-mux-v1-latmgetvalue-2bit
    let bytes = [0x80, 0x08, 0x00, 0x01, 0x01, 0x18, 0x81, 0xfe, 0x00];
    let mut br = BitReader::new(&bytes);
    let cfg = MuxCfg::parse(&mut br)?;
    assert_eq!(cfg.asc.aot, 2);
    assert_eq!(cfg.asc.sample_rate, 48_000);
    assert_eq!(cfg.asc.channel_configuration, 1);
    assert_eq!(cfg.frame_length_type, 0);
    Ok(())
}

#[test]
fn mux_v1_asc_longer_than_asclen_is_error() {
    let mut w = BitWriter::new();
    w.write_bit(true); // audioMuxVersion
    w.write_bit(false); // versionA
    w.write(0, 2); // tara bytesForValue=0
    w.write(0, 8); // tara
    w.write_bit(true); // same time
    w.write(0, 6);
    w.write(0, 4);
    w.write(0, 3);
    w.write(0, 2); // ascLen bytesForValue=0
    w.write(8, 8); // ascLen=8 < 16-bit LC ASC
    w.write(2, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(matches!(
        MuxCfg::parse(&mut br),
        Err(Error::Format("LATM ASC longer than ascLen"))
    ));
}

#[test]
fn mux_cfg_stores_num_sub_frames() -> Result<(), Error> {
    let bytes = [0x41, 0x00, 0x23, 0x10, 0x3f, 0xc0];
    let mut br = BitReader::new(&bytes);
    let cfg = MuxCfg::parse(&mut br)?;
    assert_eq!(cfg.num_sub_frames, 1);
    assert_eq!(cfg.asc.sample_rate, 48_000);
    Ok(())
}

#[test]
fn second_subframe_truncated_is_unexpected_end() {
    // Rebuild with numSubFrames=1 and only one payload prefix.
    let mut w = BitWriter::new();
    w.write_bit(false); // version 0
    w.write_bit(true);
    w.write(1, 6); // two AUs
    w.write(0, 4);
    w.write(0, 3);
    w.write(2, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3);
    w.write(0, 3); // frameLengthType 0
    w.write(0xFF, 8);
    w.write_bit(false); // other
    w.write_bit(false); // crc
    w.write(1, 8); // one payload byte length
    w.write(0, 8); // one payload byte — missing second subframe
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let cfg = MuxCfg::parse(&mut br).unwrap();
    assert_eq!(cfg.num_sub_frames, 1);
    let _ = read_payload(&mut br, &cfg).unwrap();
    assert!(matches!(
        read_payload(&mut br, &cfg),
        Err(Error::UnexpectedEnd)
    ));
}

#[test]
fn mux_cfg_crc_absent_parses() -> Result<(), Error> {
    let mut b = mux_config_bits();
    b.push(false); // crcCheckPresent = 0
    let bytes = bits_to_bytes(&b);
    let mut br = BitReader::new(&bytes);
    let cfg = MuxCfg::parse(&mut br)?;
    assert_eq!(cfg.frame_length_type, 1);
    Ok(())
}

#[test]
fn mux_cfg_crc_present_valid_parses() -> Result<(), Error> {
    let mut b = mux_config_bits();
    let crc = u32::from(stream_mux_config_crc(&b));
    b.push(true); // crcCheckPresent
    push_bits(&mut b, crc, 8);
    let bytes = bits_to_bytes(&b);
    let mut br = BitReader::new(&bytes);
    let cfg = MuxCfg::parse(&mut br)?;
    assert_eq!(cfg.frame_length_type, 1);
    Ok(())
}

#[test]
fn mux_cfg_crc_present_corrupt_rejected() {
    let mut b = mux_config_bits();
    let crc = u32::from(stream_mux_config_crc(&b)) ^ 0xFF;
    b.push(true); // crcCheckPresent
    push_bits(&mut b, crc, 8);
    let bytes = bits_to_bytes(&b);
    let mut br = BitReader::new(&bytes);
    assert!(matches!(
        MuxCfg::parse(&mut br),
        Err(Error::LatmCrcMismatch)
    ));
}

#[test]
fn mux_cfg_ld_aot23_fdk_frame_parses_and_payload_extracts() -> Result<(), Error> {
    // First LOAS frame of the FDK v2.0.3 LD cell (lab/profiles/REPORT.md,
    // ld48.loas): AOT 23, 48 kHz mono, extensionFlag 1 + resilience 000 +
    // epConfig 0, frameLengthType 0, one 70-byte payload.
    let frame = unhex(
        "56e04e2000b989002788c1442dad0cdd2885a891324892e448ea913855d300a021e211ef9f42f30ecff0cf63e68d56d88bc120eec77bb2e31d4c8dcdd9d9e993ab299d9d9d9d9d9d859d1d058505200000",
    );
    let v = (u32::from(frame[0]) << 16) | (u32::from(frame[1]) << 8) | u32::from(frame[2]);
    assert_eq!(v >> 13, super::LOAS_SYNC);
    assert_eq!(frame.len(), 3 + (v & 0x1FFF) as usize);
    let mut br = BitReader::new(&frame[3..]);
    assert!(!br.read_bit()?); // useSameStreamMux
    let cfg = MuxCfg::parse(&mut br)?;
    assert_eq!(cfg.asc.aot, 23);
    assert_eq!(cfg.asc.sample_rate, 48_000);
    assert_eq!(cfg.asc.output_sample_rate, 48_000);
    assert_eq!(cfg.asc.channel_configuration, 1);
    assert!(!cfg.asc.sbr_present);
    assert_eq!(cfg.frame_length_type, 0);
    assert_eq!(cfg.num_sub_frames, 0);
    let payload = read_payload(&mut br, &cfg)?;
    assert_eq!(payload.len(), 70);
    Ok(())
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}
