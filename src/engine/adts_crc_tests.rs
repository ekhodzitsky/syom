//! ADTS CRC: 11172-3 poly/init plus 13818-7 coverage, not a self-region guess.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::adts::AdtsHeader;
use super::super::bits::BitWriter;
use super::super::crc::{CrcPoly, crc_bits, mpeg1_crc16};
use super::super::decode::StreamDecoder;
use super::super::error::Error;
use super::{rdb_protected, verify_adts_crc};
use crate::options::DecodeOptions;
use crate::{Result as LibResult, decode_with};

fn hdr_bytes(h: &AdtsHeader) -> [u8; 7] {
    let mut w = BitWriter::new();
    w.write(0x0FFF, 12);
    w.write_bit(h.mpeg_version_mpeg2);
    w.write(0, 2);
    w.write_bit(h.protection_absent);
    w.write(u32::from(h.profile), 2);
    w.write(u32::from(h.sampling_frequency_index), 4);
    w.write_bit(false);
    w.write(u32::from(h.channel_configuration), 3);
    w.write(0, 4);
    w.write(u32::from(h.aac_frame_length), 13);
    w.write(u32::from(h.adts_buffer_fullness), 11);
    w.write(
        u32::from(h.number_of_raw_data_blocks_in_frame.saturating_sub(1)),
        2,
    );
    let b = w.finish();
    let mut o = [0u8; 7];
    o.copy_from_slice(&b);
    o
}

fn first_frame(adts: &[u8]) -> &[u8] {
    let (h, _) = AdtsHeader::parse(adts).unwrap();
    &adts[..usize::from(h.aac_frame_length)]
}

fn protect_single(frame: &[u8]) -> Vec<u8> {
    let (mut h, off) = AdtsHeader::parse(frame).unwrap();
    let payload = &frame[off..usize::from(h.aac_frame_length)];
    h.protection_absent = false;
    h.aac_frame_length = h.aac_frame_length.saturating_add(2);
    let hdr7 = hdr_bytes(&h);
    let (prot, _) =
        rdb_protected(payload, h.sampling_frequency_index, h.audio_object_type()).unwrap();
    let mut bits = Vec::new();
    for i in 0..56u64 {
        let byte = hdr7[(i / 8) as usize];
        bits.push(byte & (0x80 >> (i % 8)) != 0);
    }
    bits.extend(prot);
    let crc = mpeg1_crc16(&bits).to_be_bytes();
    let mut out = Vec::from(hdr7.as_slice());
    out.extend_from_slice(&crc);
    out.extend_from_slice(payload);
    out
}

#[test]
fn mpeg1_crc_is_not_mpeg4_ep_crc16() {
    let bits: Vec<bool> = (0..40).map(|i| i % 3 == 0).collect();
    assert_ne!(
        u64::from(mpeg1_crc16(&bits)),
        crc_bits(CrcPoly::Crc16, &bits)
    );
}

#[test]
fn crc_free_sine48_still_decodes() -> LibResult<()> {
    let adts = include_bytes!("../goldens/sine48.adts");
    let pcm = decode_with(adts, &DecodeOptions::unbounded())?;
    assert_eq!(pcm.sample_rate, 48_000);
    assert!(!pcm.channels[0].is_empty());
    let (h, _) = AdtsHeader::parse(adts).unwrap();
    assert!(h.protection_absent);
    Ok(())
}

#[test]
fn valid_protected_single_block_matches_crc_free_pcm() -> LibResult<()> {
    let adts = include_bytes!("../goldens/sine48.adts");
    let raw = first_frame(adts);
    let prot = protect_single(raw);
    verify_adts_crc(&prot, &AdtsHeader::parse(&prot).unwrap().0).unwrap();
    let a = decode_with(raw, &DecodeOptions::unbounded())?;
    let b = decode_with(&prot, &DecodeOptions::unbounded())?;
    assert_eq!(a.sample_rate, b.sample_rate);
    assert_eq!(a.channels.len(), b.channels.len());
    assert_eq!(a.channels[0].len(), b.channels[0].len());
    for (x, y) in a.channels[0].iter().zip(&b.channels[0]) {
        assert!((x - y).abs() < 1e-6);
    }
    Ok(())
}

#[test]
fn flipped_crc_byte_is_crc_mismatch_not_truncation() {
    let adts = include_bytes!("../goldens/sine48.adts");
    let mut prot = protect_single(first_frame(adts));
    prot[8] ^= 0x01;
    let (h, _) = AdtsHeader::parse(&prot).unwrap();
    assert!(matches!(
        verify_adts_crc(&prot, &h),
        Err(Error::AdtsCrcMismatch)
    ));
    let err = decode_with(&prot, &DecodeOptions::unbounded()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("CRC"), "{msg}");
    assert!(!msg.contains("unexpected end"), "{msg}");
}

#[test]
fn flipped_protected_header_bit_is_crc_mismatch() {
    let adts = include_bytes!("../goldens/sine48.adts");
    let mut prot = protect_single(first_frame(adts));
    // original/copy lives in the 56-bit header, not the CRC field.
    prot[2] ^= 0x02; // private_bit: in the 56-bit CRC, ignored by parse
    let (h, _) = AdtsHeader::parse(&prot).unwrap();
    assert!(!h.protection_absent);
    assert!(matches!(
        verify_adts_crc(&prot, &h),
        Err(Error::AdtsCrcMismatch)
    ));
}

#[test]
fn multi_block_header_and_per_block_crc() {
    let adts = include_bytes!("../goldens/sine48.adts");
    let (h0, off) = AdtsHeader::parse(adts).unwrap();
    let raw = &adts[off..usize::from(h0.aac_frame_length)];
    let mut probe = StreamDecoder::new();
    probe
        .decode_raw_data_block(2, h0.sampling_frequency_index, 48_000, 1, 1, raw)
        .unwrap();
    let tight = &raw[..probe.last_rdb_bytes];
    let (prot_rdb, used) = rdb_protected(tight, h0.sampling_frequency_index, 2).unwrap();
    assert_eq!(used, tight.len());
    let blk_crc = mpeg1_crc16(&prot_rdb).to_be_bytes();

    let mut h = h0;
    h.protection_absent = false;
    h.number_of_raw_data_blocks_in_frame = 2;
    let overhead = 7 + 2 + 2 + 2 + 2; // hdr + pos + hdr CRC + 2 block CRCs
    h.aac_frame_length = (overhead + tight.len() * 2) as u16;
    let hdr7 = hdr_bytes(&h);
    let pos = u16::try_from(tight.len()).unwrap().to_be_bytes();
    let mut hdr_bits = Vec::new();
    for i in 0..56u64 {
        let byte = hdr7[(i / 8) as usize];
        hdr_bits.push(byte & (0x80 >> (i % 8)) != 0);
    }
    for i in 0..16u64 {
        hdr_bits.push(pos[(i / 8) as usize] & (0x80 >> (i % 8)) != 0);
    }
    let hdr_crc = mpeg1_crc16(&hdr_bits).to_be_bytes();

    let mut frame = Vec::from(hdr7.as_slice());
    frame.extend_from_slice(&pos);
    frame.extend_from_slice(&hdr_crc);
    frame.extend_from_slice(tight);
    frame.extend_from_slice(&blk_crc);
    frame.extend_from_slice(tight);
    frame.extend_from_slice(&blk_crc);

    let (hp, _) = AdtsHeader::parse(&frame).unwrap();
    verify_adts_crc(&frame, &hp).unwrap();
    frame[crc_index_header(&hp)] ^= 0x01;
    assert!(matches!(
        verify_adts_crc(&frame, &hp),
        Err(Error::AdtsCrcMismatch)
    ));
}

fn crc_index_header(h: &AdtsHeader) -> usize {
    7 + 2 * usize::from(h.number_of_raw_data_blocks_in_frame - 1)
}

#[test]
fn truncated_crc_frame_is_unexpected_end() {
    let adts = include_bytes!("../goldens/sine48.adts");
    let prot = protect_single(first_frame(adts));
    let (h, _) = AdtsHeader::parse(&prot).unwrap();
    let short = &prot[..prot.len() - 1];
    assert!(matches!(
        verify_adts_crc(short, &h),
        Err(Error::UnexpectedEnd)
    ));
}
