//! ADTS header write ↔ parse roundtrip and multi-block decode.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::decode::StreamDecoder;
use super::super::error::{Error, Result};
use super::{ADTS_HEADER_BYTES_NO_CRC, AdtsHeader, payload_offset};

fn hdr(frame_len: u16, fs_index: u8, ch: u8) -> AdtsHeader {
    AdtsHeader {
        mpeg_version_mpeg2: false,
        protection_absent: true,
        profile: 1, // LC
        sampling_frequency_index: fs_index,
        channel_configuration: ch,
        aac_frame_length: frame_len,
        adts_buffer_fullness: 0x7FF,
        number_of_raw_data_blocks_in_frame: 1,
    }
}

#[test]
fn write_parse_roundtrip() -> Result<()> {
    for (fs_index, ch) in [(0u8, 1u8), (3, 2), (4, 1), (7, 2), (11, 1)] {
        let want = hdr(7 + 100, fs_index, ch);
        let bytes = want.write();
        assert_eq!(bytes.len(), ADTS_HEADER_BYTES_NO_CRC);
        let (got, off) = AdtsHeader::parse(&bytes)?;
        assert_eq!(got, want, "fs_index {fs_index} ch {ch}");
        assert_eq!(off, ADTS_HEADER_BYTES_NO_CRC);
    }
    Ok(())
}

#[test]
fn write_field_extremes() -> Result<()> {
    let want = hdr(0x1FFF, 12, 7);
    let (got, _) = AdtsHeader::parse(&want.write())?;
    assert_eq!(got, want);
    Ok(())
}

#[test]
fn crc_payload_offset_is_7_plus_2n() {
    assert_eq!(payload_offset(true, 1), 7);
    assert_eq!(payload_offset(true, 4), 7);
    assert_eq!(payload_offset(false, 1), 9);
    assert_eq!(payload_offset(false, 2), 11);
    assert_eq!(payload_offset(false, 4), 15);
}

#[test]
fn crc_two_block_header_offset_is_11() -> Result<()> {
    let mut w = super::super::bits::BitWriter::new();
    w.write(0xFFF, 12);
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false); // CRC present
    w.write(1, 2);
    w.write(3, 4);
    w.write_bit(false);
    w.write(1, 3);
    w.write(0, 4);
    w.write(11, 13); // header + 4 CRC/position bytes
    w.write(0x7FF, 11);
    w.write(1, 2); // N-1 = 1 → two blocks
    w.write(0, 32); // dummy positions+crc
    let bytes = w.finish();
    let (h, off) = AdtsHeader::parse(&bytes)?;
    assert!(!h.protection_absent);
    assert_eq!(h.number_of_raw_data_blocks_in_frame, 2);
    assert_eq!(off, 11);
    Ok(())
}

#[test]
fn two_rdb_sine_delivers_two_core_frames() -> Result<()> {
    let adts = include_bytes!("../goldens/sine48.adts");
    let (h, off) = AdtsHeader::parse(adts)?;
    let fl = usize::from(h.aac_frame_length);
    let rdb = &adts[off..fl];
    let mut probe = StreamDecoder::new();
    let one = probe.decode_raw_data_block(2, h.sampling_frequency_index, 48_000, 1, 1, rdb)?;
    let tight = &rdb[..probe.last_rdb_bytes];
    let mut body = Vec::from(tight);
    body.extend_from_slice(tight);
    let mut b = StreamDecoder::new();
    let two = b.decode_raw_data_block(2, h.sampling_frequency_index, 48_000, 1, 2, &body)?;
    assert_eq!(two.planar[0].len(), one.planar[0].len() * 2);
    let mut four_body = Vec::new();
    for _ in 0..4 {
        four_body.extend_from_slice(tight);
    }
    let mut d4 = StreamDecoder::new();
    let four = d4.decode_raw_data_block(2, h.sampling_frequency_index, 48_000, 1, 4, &four_body)?;
    assert_eq!(four.planar[0].len(), one.planar[0].len() * 4);
    Ok(())
}

#[test]
fn two_rdb_crc_skips_16bit_between_blocks() -> Result<()> {
    let adts = include_bytes!("../goldens/sine48.adts");
    let (h, off) = AdtsHeader::parse(adts)?;
    let fl = usize::from(h.aac_frame_length);
    let rdb = &adts[off..fl];
    let mut probe = StreamDecoder::new();
    let _ = probe.decode_raw_data_block(2, h.sampling_frequency_index, 48_000, 1, 1, rdb)?;
    let tight = &rdb[..probe.last_rdb_bytes];
    let mut body = Vec::from(tight);
    body.extend_from_slice(&[0, 0]); // dummy CRC
    body.extend_from_slice(tight);
    body.extend_from_slice(&[0, 0]); // trailing per-block CRC
    let mut dec = StreamDecoder::new();
    let two = dec.decode_adts_blocks(2, h.sampling_frequency_index, 48_000, 1, 2, false, &body)?;
    assert_eq!(two, 48_000);
    assert_eq!(dec.frame_ch[0].len() % 1024, 0);
    assert!(dec.frame_ch[0].len() >= 2048);
    Ok(())
}

#[test]
fn two_rdb_truncated_is_unexpected_end() {
    let err = StreamDecoder::new()
        .decode_raw_data_block(2, 3, 48_000, 1, 2, &[])
        .unwrap_err();
    assert!(matches!(err, Error::UnexpectedEnd));
}
