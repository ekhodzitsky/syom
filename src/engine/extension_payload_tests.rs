//! FIL `extension_payload` parse/write (non-SBR types).

use super::bits::{BitReader, BitWriter};
use super::error::Error;
use super::extension_payload::{
    DrcBandRecord, DrcBands, DynamicRangeInfo, ExcludedChannels, ExtensionPayload,
    ExtensionPayloadOrSbr, FILL_DATA_BYTE, FILL_DATA_NIBBLE, PceTagFields, ProgRefLevelFields,
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
fn parse_and_write_drc_optional_fields() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0xb, 4);
    w.write_bit(true);
    w.write(1, 4);
    w.write(0, 4);
    w.write_bit(true);
    for _ in 0..7 {
        w.write_bit(false);
    }
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(true);
    w.write(3, 7);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 7);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let p = ExtensionPayload::parse(&mut br, 5)?;
    match &p {
        ExtensionPayload::DynamicRange(drc) => {
            assert!(drc.pce_tag.is_some());
            assert!(drc.excluded_channels.is_some());
            assert!(drc.prog_ref_level.is_some());
            assert_eq!(drc.byte_length(), 5);
        }
        other => panic!("{other:?}"),
    }
    let mut w2 = BitWriter::new();
    assert_eq!(p.write(&mut w2)?, 5);
    Ok(())
}

#[test]
fn write_drc_rejects_bad_fields() {
    let drc = DynamicRangeInfo {
        pce_tag: Some(PceTagFields {
            pce_instance_tag: 0xff,
            reserved: 0,
        }),
        excluded_channels: None,
        drc_bands: None,
        prog_ref_level: None,
        bands: vec![DrcBandRecord {
            dyn_rng_sgn: false,
            dyn_rng_ctl: 0,
        }],
    };
    let mut w = BitWriter::new();
    assert!(ExtensionPayload::DynamicRange(drc).write(&mut w).is_err());

    let drc = DynamicRangeInfo {
        pce_tag: None,
        excluded_channels: Some(ExcludedChannels {
            exclude_mask: vec![],
        }),
        drc_bands: Some(DrcBands {
            band_incr: 0,
            reserved: 0,
            band_top: vec![],
        }),
        prog_ref_level: Some(ProgRefLevelFields {
            level: 0xff,
            reserved: false,
        }),
        bands: vec![DrcBandRecord {
            dyn_rng_sgn: false,
            dyn_rng_ctl: 0xff,
        }],
    };
    let mut w = BitWriter::new();
    assert!(ExtensionPayload::DynamicRange(drc).write(&mut w).is_err());
}

#[test]
fn from_bits_errors_and_byte_length() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0xd, 4);
    w.write(0, 4);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(matches!(
        ExtensionPayload::parse(&mut br, 1),
        Err(Error::UnsupportedExtensionSbr(0xd))
    ));
    let mut w = BitWriter::new();
    w.write(2, 4);
    w.write(0, 4);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(matches!(
        ExtensionPayload::parse(&mut br, 1),
        Err(Error::UnsupportedExtensionType(2))
    ));
    assert!(
        ExtensionPayload::parse_with_sbr(&mut BitReader::new(&[0]), 0, IdSynEle::Sce, 48_000, None)
            .is_err()
    );

    let fill = ExtensionPayload::Fill {
        cnt: 1,
        other_bits: vec![0],
    };
    assert_eq!(fill.byte_length(), 1);
    let fd = ExtensionPayload::FillData { cnt: 2 };
    assert_eq!(fd.byte_length(), 2);
    Ok(())
}

#[test]
fn parse_with_sbr_fill_data_and_drc() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(1, 4);
    w.write(u32::from(FILL_DATA_NIBBLE), 4);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    match ExtensionPayload::parse_with_sbr(&mut br, 1, IdSynEle::Sce, 48_000, None)? {
        ExtensionPayloadOrSbr::Payload(ExtensionPayload::FillData { cnt: 1 }) => {}
        other => panic!("{other:?}"),
    }

    let mut w = BitWriter::new();
    w.write(0xb, 4);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    w.write_bit(false);
    w.write(0, 7);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    match ExtensionPayload::parse_with_sbr(&mut br, 2, IdSynEle::Sce, 48_000, None)? {
        ExtensionPayloadOrSbr::Payload(ExtensionPayload::DynamicRange(drc)) => {
            assert_eq!(drc.num_bands(), 1);
        }
        other => panic!("{other:?}"),
    }
    Ok(())
}

#[test]
fn write_fill_two_bytes_and_fill_data_bad_byte() -> Result<(), Error> {
    let payload = ExtensionPayload::Fill {
        cnt: 2,
        other_bits: vec![0xAB, 0xC0],
    };
    let mut w = BitWriter::new();
    assert_eq!(payload.write(&mut w)?, 2);
    let payload = ExtensionPayload::Fill {
        cnt: 2,
        other_bits: vec![0x00],
    };
    let mut w = BitWriter::new();
    assert!(payload.write(&mut w).is_err());

    let mut w = BitWriter::new();
    w.write(1, 4);
    w.write(u32::from(FILL_DATA_NIBBLE), 4);
    w.write(0, 8);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    assert!(ExtensionPayload::parse(&mut br, 2).is_err());
    Ok(())
}

#[test]
fn write_drc_bands_and_two_exclude_groups() -> Result<(), Error> {
    let drc = DynamicRangeInfo {
        pce_tag: None,
        excluded_channels: Some(ExcludedChannels {
            exclude_mask: vec![false; 14],
        }),
        drc_bands: Some(DrcBands {
            band_incr: 0,
            reserved: 0,
            band_top: vec![8],
        }),
        prog_ref_level: None,
        bands: vec![DrcBandRecord {
            dyn_rng_sgn: false,
            dyn_rng_ctl: 4,
        }],
    };
    let mut w = BitWriter::new();
    let n = ExtensionPayload::DynamicRange(drc).write(&mut w)?;
    assert!(n >= 1);
    let drc = DynamicRangeInfo {
        pce_tag: None,
        excluded_channels: None,
        drc_bands: Some(DrcBands {
            band_incr: 0,
            reserved: 0x10,
            band_top: vec![8],
        }),
        prog_ref_level: None,
        bands: vec![DrcBandRecord {
            dyn_rng_sgn: false,
            dyn_rng_ctl: 0,
        }],
    };
    let mut w = BitWriter::new();
    assert!(ExtensionPayload::DynamicRange(drc).write(&mut w).is_err());
    let drc = DynamicRangeInfo {
        pce_tag: None,
        excluded_channels: None,
        drc_bands: None,
        prog_ref_level: None,
        bands: vec![],
    };
    let mut w = BitWriter::new();
    assert!(ExtensionPayload::DynamicRange(drc).write(&mut w).is_err());
    Ok(())
}

#[test]
fn write_fill_rejects_zero_cnt() {
    let payload = ExtensionPayload::Fill {
        cnt: 0,
        other_bits: vec![],
    };
    let mut w = BitWriter::new();
    assert!(payload.write(&mut w).is_err());
    let payload = ExtensionPayload::FillData { cnt: 0 };
    let mut w = BitWriter::new();
    assert!(payload.write(&mut w).is_err());
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
