//! FIL `extension_payload` parse/write (non-SBR types).

use super::bits::{BitReader, BitWriter};
use super::error::Error;
use super::extension_payload::{
    ExtensionPayload, ExtensionPayloadOrSbr, FILL_DATA_BYTE, FILL_DATA_NIBBLE,
};
use super::raw_data_block::IdSynEle;

#[test]
fn id_syn_ele_from_bits_roundtrip() {
    assert_eq!(IdSynEle::from_bits(0), IdSynEle::Sce);
    assert_eq!(IdSynEle::from_bits(1), IdSynEle::Cpe);
    assert_eq!(IdSynEle::from_bits(2), IdSynEle::Cce);
    assert_eq!(IdSynEle::from_bits(3), IdSynEle::Lfe);
    assert_eq!(IdSynEle::from_bits(4), IdSynEle::Dse);
    assert_eq!(IdSynEle::from_bits(5), IdSynEle::Pce);
    assert_eq!(IdSynEle::from_bits(6), IdSynEle::Fil);
    assert_eq!(IdSynEle::from_bits(7), IdSynEle::End);
}

#[test]
fn parse_fill_cnt1() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0, 4); // EXT_FILL
    w.write(0, 4); // other_bits
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let p = ExtensionPayload::parse(&mut br, 1)?;
    assert!(matches!(p, ExtensionPayload::Fill { cnt: 1, .. }));
    Ok(())
}

#[test]
fn parse_fill_data_two_bytes() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(1, 4); // EXT_FILL_DATA
    w.write(u32::from(FILL_DATA_NIBBLE), 4);
    w.write(u32::from(FILL_DATA_BYTE), 8);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let p = ExtensionPayload::parse(&mut br, 2)?;
    assert!(matches!(p, ExtensionPayload::FillData { cnt: 2 }));
    Ok(())
}

#[test]
fn write_fill_data_roundtrip() -> Result<(), Error> {
    let payload = ExtensionPayload::FillData { cnt: 2 };
    let mut w = BitWriter::new();
    let n = payload.write(&mut w)?;
    assert_eq!(n, 2);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let back = ExtensionPayload::parse(&mut br, 2)?;
    assert_eq!(payload, back);
    Ok(())
}

#[test]
fn parse_cnt0_is_invalid() {
    let mut br = BitReader::new(&[0]);
    assert!(matches!(
        ExtensionPayload::parse(&mut br, 0),
        Err(Error::ExtensionPayloadInvalid)
    ));
}

#[test]
fn parse_with_sbr_fill_is_payload() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0, 4);
    w.write(0, 4);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    match ExtensionPayload::parse_with_sbr(&mut br, 1, IdSynEle::Sce, 48_000, None)? {
        ExtensionPayloadOrSbr::Payload(ExtensionPayload::Fill { cnt: 1, .. }) => {}
        other => panic!("{other:?}"),
    }
    Ok(())
}

#[test]
fn parse_drc_minimal() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0xb, 4); // EXT_DYNAMIC_RANGE
    w.write_bit(false); // pce
    w.write_bit(false); // excluded
    w.write_bit(false); // bands
    w.write_bit(false); // prog_ref
    w.write_bit(false); // sgn
    w.write(0, 7); // ctl
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    match ExtensionPayload::parse(&mut br, 2)? {
        ExtensionPayload::DynamicRange(drc) => {
            assert!(drc.pce_tag.is_none());
            assert_eq!(drc.bands.len(), 1);
        }
        other => panic!("{other:?}"),
    }
    Ok(())
}

#[test]
fn write_fill_roundtrip() -> Result<(), Error> {
    let payload = ExtensionPayload::Fill {
        cnt: 1,
        other_bits: vec![0],
    };
    let mut w = BitWriter::new();
    let n = payload.write(&mut w)?;
    assert!(n >= 1);
    Ok(())
}

#[test]
fn fill_data_bad_nibble_is_invalid() {
    let mut w = BitWriter::new();
    w.write(1, 4);
    w.write(1, 4); // not 0
    w.write(u32::from(FILL_DATA_BYTE), 8);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(ExtensionPayload::parse(&mut br, 2).is_err());
}
