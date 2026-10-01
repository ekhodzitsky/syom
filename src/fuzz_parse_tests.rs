//! TASK-48: independent BitReader / ASC / ADTS / LATM / PCE parse of the
//! smoke corpus. Panics are findings; errors are expected.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::engine::adts::AdtsHeader;
use crate::engine::asc::AudioSpecificConfig;
use crate::engine::bits::BitReader;
use crate::engine::channel_map::parse_pce;
use crate::engine::error::Error;
use crate::engine::latm::MuxCfg;

const SEEDS: &[&[u8]] = &[
    include_bytes!("goldens/sine48.adts"),
    include_bytes!("goldens/latm48.latm"),
    include_bytes!("goldens/sine441.m4a"),
    include_bytes!("goldens/he48.adts"),
    include_bytes!("goldens/fmp4_lc.mp4"),
    include_bytes!("../corpus/fuzz/empty.bin"),
    include_bytes!("../corpus/fuzz/adts-truncated.bin"),
    include_bytes!("../corpus/fuzz/adts-len-too-small.bin"),
    include_bytes!("../corpus/fuzz/adts-reserved-srate.bin"),
    include_bytes!("../corpus/fuzz/asc-main-aot.bin"),
    include_bytes!("../corpus/fuzz/latm-truncated.bin"),
    include_bytes!("../corpus/fuzz/m4a-truncated.bin"),
    include_bytes!("../corpus/fuzz/fmp4-truncated.bin"),
];

fn parsers(data: &[u8]) {
    let _ = AdtsHeader::parse(data);
    let _ = AudioSpecificConfig::parse(data);
    let mut br = BitReader::new(data);
    let n = u32::try_from(data.len().min(8)).unwrap_or(0);
    let _ = br.read(n.min(32));
    let _ = br.skip(n.saturating_mul(3));
    let _ = br.byte_align();
    let mut br = BitReader::new(data);
    let _ = MuxCfg::parse(&mut br);
    let mut br = BitReader::new(data);
    let _ = parse_pce(&mut br);
}

#[test]
fn independent_parsers_accept_arbitrary_smoke_bytes() {
    for (i, data) in SEEDS.iter().enumerate() {
        let panicked = catch_unwind(AssertUnwindSafe(|| parsers(data))).is_err();
        assert!(!panicked, "parser panic on seed {i} ({} B)", data.len());
    }
}

#[test]
fn known_invalid_adts_and_asc_reach_their_parsers() {
    let len = include_bytes!("../corpus/fuzz/adts-len-too-small.bin");
    assert_eq!(
        AdtsHeader::parse(len).unwrap_err(),
        Error::AdtsFrameLengthTooSmall
    );
    let srate = include_bytes!("../corpus/fuzz/adts-reserved-srate.bin");
    assert_eq!(
        AdtsHeader::parse(srate).unwrap_err(),
        Error::AdtsReservedSampleRateIndex
    );
    let trunc = include_bytes!("../corpus/fuzz/adts-truncated.bin");
    assert_eq!(AdtsHeader::parse(trunc).unwrap_err(), Error::UnexpectedEnd);
    let asc = include_bytes!("../corpus/fuzz/asc-main-aot.bin");
    match AudioSpecificConfig::parse(asc) {
        Err(Error::UnsupportedAot(1)) => {}
        other => panic!("ASC Main AOT: {other:?}"),
    }
}
