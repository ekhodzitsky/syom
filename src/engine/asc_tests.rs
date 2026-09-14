//! Independent ASC vectors (TASK-17) plus LC `write_lc` stability.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::bits::BitWriter;
use super::super::error::{Error, Result};
use super::{AudioSpecificConfig, write_lc};

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

#[test]
fn write_lc_roundtrip() -> Result<()> {
    for (fs_index, ch) in [(3u8, 1u8), (3, 2), (0, 2), (11, 1)] {
        let bytes = write_lc(fs_index, ch);
        assert_eq!(bytes.len(), 2, "bare LC ASC is 16 bits");
        let (asc, bits) = AudioSpecificConfig::parse(&bytes)?;
        assert_eq!(asc.aot, 2);
        assert_eq!(asc.sampling_frequency_index, fs_index);
        assert_eq!(asc.channel_configuration, ch);
        assert!(!asc.sbr_present);
        assert!(!asc.ps_present);
        assert_eq!(asc.sample_rate, asc.output_sample_rate);
        assert_eq!(bits, 16);
    }
    assert_eq!(write_lc(3, 1), unhex("1188"));
    Ok(())
}

#[test]
fn explicit_sbr_two_rate_24_48() -> Result<()> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex("2b098800"))?;
    assert_eq!(asc.aot, 2);
    assert_eq!(asc.sampling_frequency_index, 6);
    assert_eq!(asc.sample_rate, 24_000);
    assert_eq!(asc.output_sample_rate, 48_000);
    assert_eq!(asc.channel_configuration, 1);
    assert!(asc.sbr_present);
    assert!(!asc.ps_present);
    Ok(())
}

#[test]
fn explicit_ps_two_rate_24_48() -> Result<()> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex("eb098800"))?;
    assert_eq!(asc.aot, 2);
    assert_eq!(asc.sample_rate, 24_000);
    assert_eq!(asc.output_sample_rate, 48_000);
    assert!(asc.sbr_present);
    assert!(asc.ps_present);
    Ok(())
}

#[test]
fn implicit_sbr_24_48_and_downsampled_48_48() -> Result<()> {
    let (he, _) = AudioSpecificConfig::parse(&unhex("130856e598"))?;
    assert_eq!(he.sample_rate, 24_000);
    assert_eq!(he.output_sample_rate, 48_000);
    assert!(he.sbr_present);
    assert!(!he.ps_present);

    let (ds, _) = AudioSpecificConfig::parse(&unhex("118856e598"))?;
    assert_eq!(ds.sample_rate, 48_000);
    assert_eq!(ds.output_sample_rate, 48_000);
    assert!(ds.sbr_present);
    Ok(())
}

#[test]
fn implicit_ps_0x548() -> Result<()> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex("130856e59d4880"))?;
    assert_eq!(asc.sample_rate, 24_000);
    assert_eq!(asc.output_sample_rate, 48_000);
    assert!(asc.sbr_present);
    assert!(asc.ps_present);
    Ok(())
}

#[test]
fn one_rate_explicit_sbr_is_not_amd2() {
    // Local ics_tests layout: AOT5 + one rate index. Not Table 1.13.
    let err = AudioSpecificConfig::parse(&unhex("298880")).unwrap_err();
    assert!(matches!(err, Error::Format(_) | Error::UnsupportedAot(_)));
}

#[test]
fn truncated_implicit_sbr_is_unexpected_end_not_panic() {
    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3);
    w.write(0x2b7, 11);
    w.write(5, 5);
    // 32 bits exactly: flag bit is missing, no padding into a rate field.
    let err = AudioSpecificConfig::parse(&w.finish()).unwrap_err();
    assert!(matches!(err, Error::UnexpectedEnd));
}

#[test]
fn unsupported_960_and_main_are_explicit() {
    assert!(matches!(
        AudioSpecificConfig::parse(&unhex("118c")),
        Err(Error::UnsupportedFrameLength)
    ));
    assert!(matches!(
        AudioSpecificConfig::parse(&unhex("0988")),
        Err(Error::UnsupportedAot(1))
    ));
}

#[test]
fn explicit_24bit_core_rate() -> Result<()> {
    let (asc, _) = AudioSpecificConfig::parse(&unhex("17805dc008"))?;
    assert_eq!(asc.sample_rate, 48_000);
    assert!(!asc.sbr_present);
    Ok(())
}

#[test]
fn ch0_without_pce_is_truncated() {
    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(0, 4);
    w.write(0, 3);
    assert!(matches!(
        AudioSpecificConfig::parse(&w.finish()),
        Err(Error::UnexpectedEnd)
    ));
}

#[test]
fn embedded_pce_rejects_object_type_and_rate_and_dup_tag() {
    // object_type=0 (Main), otherwise the authored LC PCE.
    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(0, 4);
    w.write(0, 3);
    w.write(0, 4); // tag
    w.write(0, 2); // Main
    w.write(3, 4);
    w.write(1, 4); // one front
    w.write(0, 4);
    w.write(0, 4);
    w.write(0, 2);
    w.write(0, 3);
    w.write(0, 4);
    w.write(0, 3); // mixdowns
    w.write_bit(false);
    w.write(0, 4);
    w.write(0, 8); // align+comment may pad
    let err = AudioSpecificConfig::parse(&w.finish()).unwrap_err();
    assert!(
        matches!(err, Error::Format("PCE object_type is not LC")),
        "{err:?}"
    );

    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(0, 4);
    w.write(0, 3);
    w.write(0, 4);
    w.write(1, 2); // LC
    w.write(4, 4); // 44.1 kHz, ASC is 48 kHz
    w.write(1, 4);
    w.write(0, 4);
    w.write(0, 4);
    w.write(0, 2);
    w.write(0, 3);
    w.write(0, 4);
    w.write(0, 3);
    w.write_bit(false);
    w.write(0, 4);
    w.write(0, 8);
    let err = AudioSpecificConfig::parse(&w.finish()).unwrap_err();
    assert!(
        matches!(err, Error::Format("PCE sf_index does not match ASC")),
        "{err:?}"
    );

    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(0, 4);
    w.write(0, 3);
    w.write(0, 4);
    w.write(1, 2);
    w.write(3, 4);
    w.write(2, 4); // two front SCE, same tag
    w.write(0, 4);
    w.write(0, 4);
    w.write(0, 2);
    w.write(0, 3);
    w.write(0, 4);
    w.write(0, 3);
    w.write_bit(false);
    w.write(0, 4);
    w.write_bit(false);
    w.write(0, 4);
    w.write(0, 8);
    let err = AudioSpecificConfig::parse(&w.finish()).unwrap_err();
    assert!(
        matches!(err, Error::Format("PCE duplicate element tag")),
        "{err:?}"
    );
}

#[test]
fn sbr_present_flag_zero_stays_lc() -> Result<()> {
    // 0x2b7 + AOT5 + sbrPresentFlag=0 must not become HE (LC padding).
    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3);
    w.write(0x2b7, 11);
    w.write(5, 5);
    w.write_bit(false);
    let (asc, _) = AudioSpecificConfig::parse(&w.finish())?;
    assert!(!asc.sbr_present);
    assert_eq!(asc.sample_rate, 48_000);
    assert_eq!(asc.output_sample_rate, 48_000);
    Ok(())
}
