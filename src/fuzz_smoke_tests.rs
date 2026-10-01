//! TASK-48: replay the committed parser-fuzz smoke corpus on shipped APIs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::engine::adts::AdtsHeader;
use crate::engine::error::Error;
use crate::{
    AacError, DecodeOptions, decode_with, sniff_aac, sniff_is_adts, sniff_is_isobmff, sniff_is_latm,
};

const SINE48: &[u8] = include_bytes!("goldens/sine48.adts");
const LATM48: &[u8] = include_bytes!("goldens/latm48.latm");
const SINE441: &[u8] = include_bytes!("goldens/sine441.m4a");
const HE48: &[u8] = include_bytes!("goldens/he48.adts");
const FMP4_LC: &[u8] = include_bytes!("goldens/fmp4_lc.mp4");
const EMPTY: &[u8] = include_bytes!("../corpus/fuzz/empty.bin");
const ADTS_TRUNC: &[u8] = include_bytes!("../corpus/fuzz/adts-truncated.bin");
const ADTS_LEN: &[u8] = include_bytes!("../corpus/fuzz/adts-len-too-small.bin");
const ADTS_SRATE: &[u8] = include_bytes!("../corpus/fuzz/adts-reserved-srate.bin");
const ASC_MAIN: &[u8] = include_bytes!("../corpus/fuzz/asc-main-aot.bin");
const LATM_TRUNC: &[u8] = include_bytes!("../corpus/fuzz/latm-truncated.bin");
const M4A_TRUNC: &[u8] = include_bytes!("../corpus/fuzz/m4a-truncated.bin");
const FMP4_TRUNC: &[u8] = include_bytes!("../corpus/fuzz/fmp4-truncated.bin");

fn harvest(data: &[u8]) {
    let panicked = catch_unwind(AssertUnwindSafe(|| {
        let _ = sniff_aac(data);
        let _ = sniff_is_adts(data);
        let _ = sniff_is_latm(data);
        let _ = sniff_is_isobmff(data);
        let _ = decode_with(data, &DecodeOptions::speech());
    }))
    .is_err();
    assert!(!panicked, "parser panic on {} bytes", data.len());
}

#[test]
fn valid_goldens_still_decode() {
    for (name, bytes) in [
        ("sine48", SINE48),
        ("latm48", LATM48),
        ("sine441", SINE441),
        ("he48", HE48),
        ("fmp4_lc", FMP4_LC),
    ] {
        harvest(bytes);
        let pcm = decode_with(bytes, &DecodeOptions::speech()).unwrap_or_else(|e| {
            panic!("{name} decode: {e}");
        });
        assert!(!pcm.channels.is_empty(), "{name}");
        assert!(pcm.channels[0].iter().any(|x| x.abs() > 1e-8), "{name}");
    }
}

#[test]
fn malformed_seeds_never_panic_and_do_not_succeed() {
    for (name, bytes) in [
        ("empty", EMPTY),
        ("adts-truncated", ADTS_TRUNC),
        ("adts-len-too-small", ADTS_LEN),
        ("adts-reserved-srate", ADTS_SRATE),
        ("asc-main-aot", ASC_MAIN),
        ("latm-truncated", LATM_TRUNC),
        ("m4a-truncated", M4A_TRUNC),
        ("fmp4-truncated", FMP4_TRUNC),
    ] {
        harvest(bytes);
        assert!(
            decode_with(bytes, &DecodeOptions::speech()).is_err(),
            "{name} must not decode as AAC"
        );
    }
}

#[test]
fn fmp4_truncated_reaches_the_fragment_walk() {
    harvest(FMP4_TRUNC);
    assert!(
        matches!(
            decode_with(FMP4_TRUNC, &DecodeOptions::speech()),
            Err(AacError::Truncated { .. })
        ),
        "init + partial moof must be typed Truncated (TASK-126)"
    );
}

#[test]
fn adts_len_too_small_reaches_adts_parser() {
    harvest(ADTS_LEN);
    assert_eq!(
        AdtsHeader::parse(ADTS_LEN).unwrap_err(),
        Error::AdtsFrameLengthTooSmall
    );
    assert!(decode_with(ADTS_LEN, &DecodeOptions::speech()).is_err());
}

#[test]
fn adts_reserved_srate_reaches_adts_parser() {
    harvest(ADTS_SRATE);
    assert_eq!(
        AdtsHeader::parse(ADTS_SRATE).unwrap_err(),
        Error::AdtsReservedSampleRateIndex
    );
}

#[test]
fn provenance_lists_smoke_corpus() {
    let raw = include_str!("../corpus/fuzz/SEEDS.md");
    assert!(raw.contains("TASK-48"));
    assert!(raw.contains("adts-len-too-small"));
    assert!(raw.contains("sine48.adts"));
    assert!(raw.contains("fmp4_lc"));
    assert!(raw.contains("fmp4-truncated.bin"));
    assert!(raw.contains("lab/fuzz"));
}
