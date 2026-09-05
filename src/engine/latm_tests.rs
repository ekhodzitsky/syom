//! LATM mux config / payload helper branches.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{LOAS_SYNC, MuxCfg, latm_value, read_frame_len, skip_mux_tail};
use crate::engine::bits::{BitReader, BitWriter};
use crate::engine::crc::stream_mux_config_crc;
use crate::engine::error::Error;
use crate::engine::latm::decode_loas_planar;

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

fn to_bits(data: &[u8]) -> Vec<bool> {
    let mut v = Vec::with_capacity(data.len() * 8);
    for &b in data {
        push_bits(&mut v, u32::from(b), 8);
    }
    v
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

/// Wrap `element_bits` (an `AudioMuxElement`) in a LOAS sync header.
fn loas_frame(element_bits: &[bool]) -> Vec<u8> {
    let payload = bits_to_bytes(element_bits);
    let hdr = (LOAS_SYNC << 13) | payload.len() as u32;
    let mut out = vec![(hdr >> 16) as u8, (hdr >> 8) as u8, hdr as u8];
    out.extend_from_slice(&payload);
    out
}

/// he48.latm carries `crcCheckPresent == 0`; splice a `crcCheckSum`
/// into its first `StreamMuxConfig()` (corrupt on demand) and return
/// the rebuilt LOAS stream.
fn splice_he48_crc(corrupt: bool) -> Result<Vec<u8>, Error> {
    let data = include_bytes!("../goldens/he48.latm");
    let hdr = (u32::from(data[0]) << 16) | (u32::from(data[1]) << 8) | u32::from(data[2]);
    let mux_len = (hdr & 0x1FFF) as usize;
    let frame = &data[3..3 + mux_len];
    // Locate the protected region with the real parser.
    let mut br = BitReader::new(frame);
    let _use_same = br.read_bit()?;
    let cfg_start = br.bit_position();
    let _cfg = MuxCfg::parse(&mut br)?; // crcCheckPresent == 0 today
    let crc_end = br.bit_position() - 1;
    let payload_start = br.bit_position();

    let frame_bits = to_bits(frame);
    let mut crc = u32::from(stream_mux_config_crc(
        &frame_bits[cfg_start as usize..crc_end as usize],
    ));
    if corrupt {
        crc ^= 0xFF;
    }
    let mut spliced = frame_bits[..payload_start as usize].to_vec();
    spliced[payload_start as usize - 1] = true; // crcCheckPresent = 1
    push_bits(&mut spliced, crc, 8);
    spliced.extend_from_slice(&frame_bits[payload_start as usize..]);

    let element = bits_to_bytes(&spliced);
    let hdr = (LOAS_SYNC << 13) | element.len() as u32;
    let mut out = vec![(hdr >> 16) as u8, (hdr >> 8) as u8, hdr as u8];
    out.extend_from_slice(&element);
    out.extend_from_slice(&data[3 + mux_len..]);
    Ok(out)
}

#[test]
fn decode_loas_resync_and_empty_is_error() {
    let mut data = vec![0u8, 0, 0];
    data.extend_from_slice(&[0x2B, 0xE0, 0x00]);
    assert!(decode_loas_planar(&[0, 1, 2], false, usize::MAX).is_err());
    assert!(decode_loas_planar(&[], false, usize::MAX).is_err());
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
    skip_mux_tail(&mut br, 0)?;

    // otherDataPresent (byte-escaped form), crcCheckPresent = 0.
    let mut b = Vec::new();
    push_bits(&mut b, 1, 1);
    push_bits(&mut b, 1, 1); // escaped
    b.push(false); // more = 0
    push_bits(&mut b, 1, 8);
    b.push(false); // crcCheckPresent = 0
    let bytes = bits_to_bytes(&b);
    let mut br = BitReader::new(&bytes);
    skip_mux_tail(&mut br, 0)?;

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
fn loas_corrupt_crc_is_error() {
    let cfg = mux_config_bits();
    let mut b = vec![false]; // useSameStreamMux
    b.extend_from_slice(&cfg);
    let crc = u32::from(stream_mux_config_crc(&cfg)) ^ 0xFF;
    b.push(true); // crcCheckPresent
    push_bits(&mut b, crc, 8);
    push_bits(&mut b, 0, 32); // payload (never reached)
    let loas = loas_frame(&b);
    assert!(matches!(
        decode_loas_planar(&loas, false, usize::MAX),
        Err(Error::LatmCrcMismatch)
    ));
}

#[test]
fn he48_mux_config_with_valid_crc_decodes() -> Result<(), Error> {
    let spliced = splice_he48_crc(false)?;
    let (rate, tracks) = decode_loas_planar(&spliced, false, usize::MAX)?;
    let (rate0, tracks0) =
        decode_loas_planar(include_bytes!("../goldens/he48.latm"), false, usize::MAX)?;
    assert_eq!(rate, rate0);
    assert_eq!(tracks.len(), tracks0.len());
    for (a, b) in tracks.iter().zip(tracks0.iter()) {
        assert_eq!(a.len(), b.len());
    }
    Ok(())
}

#[test]
fn he48_mux_config_with_corrupt_crc_rejected() -> Result<(), Error> {
    let spliced = splice_he48_crc(true)?;
    assert!(matches!(
        decode_loas_planar(&spliced, false, usize::MAX),
        Err(Error::LatmCrcMismatch)
    ));
    Ok(())
}
