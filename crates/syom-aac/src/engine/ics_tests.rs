//! ICS grouping, ADTS, ASC, section_data.

use super::adts::AdtsHeader;
use super::asc::AudioSpecificConfig;
use super::bits::{BitReader, BitWriter};
use super::error::Error;
use super::ics::{IcsInfo, WindowSequence};
use super::section::SectionData;

#[test]
fn adts_lc_48k_mono_header() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write(0xFFF, 12);
    w.write_bit(false); // MPEG-4
    w.write(0, 2); // layer
    w.write_bit(true); // no CRC
    w.write(1, 2); // LC
    w.write(3, 4); // 48 kHz
    w.write_bit(false);
    w.write(1, 3); // mono
    w.write(0, 4); // original/home/copyright
    w.write(7, 13); // frame length = header
    w.write(0x7FF, 11);
    w.write(0, 2);
    let bytes = w.finish();
    let (hdr, off) = AdtsHeader::parse(&bytes)?;
    assert_eq!(hdr.audio_object_type(), 2);
    assert_eq!(hdr.sample_rate(), 48_000);
    assert_eq!(hdr.channel_configuration, 1);
    assert_eq!(off, 7);
    Ok(())
}

#[test]
fn asc_lc_is_aot_2() -> Result<(), Error> {
    // AOT=2, fs=3 (48k), chan=1, GA: frameLength=0, depends=0, ext=0
    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3);
    let bytes = w.finish();
    let (asc, _) = AudioSpecificConfig::parse(&bytes)?;
    assert_eq!(asc.aot, 2);
    assert_eq!(asc.sample_rate, 48_000);
    assert_eq!(asc.channel_configuration, 1);
    Ok(())
}

#[test]
fn asc_heaac_aot5_is_unsupported() {
    let mut w = BitWriter::new();
    w.write(5, 5); // SBR
    w.write(3, 4);
    w.write(1, 4);
    let bytes = w.finish();
    match AudioSpecificConfig::parse(&bytes) {
        Err(Error::UnsupportedAot(5)) => {}
        other => panic!("expected UnsupportedAot(5), got {other:?}"),
    }
}

#[test]
fn asc_lc_with_trailing_sbr_probe_still_lc() -> Result<(), Error> {
    // AOT 2 + GA zeros + syncExtensionType 0x2b7 / AOT 5 (implicit HE-AAC).
    let mut w = BitWriter::new();
    w.write(2, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3);
    w.write(0x2b7, 11);
    w.write(5, 5);
    w.write_bit(true);
    w.write(3, 4);
    let bytes = w.finish();
    let (asc, _) = AudioSpecificConfig::parse(&bytes)?;
    assert_eq!(asc.aot, 2);
    Ok(())
}

#[test]
fn eight_short_grouping_all_separate() -> Result<(), Error> {
    let mut w = BitWriter::new();
    w.write_bit(false);
    w.write(2, 2); // eight short
    w.write_bit(false); // sine
    w.write(1, 4); // max_sfb
    w.write(0, 7); // grouping: all new groups
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let ics = IcsInfo::parse(&mut br, 3, false)?;
    assert!(ics.window_sequence.is_eight_short());
    assert_eq!(ics.num_windows, 8);
    assert_eq!(ics.num_window_groups, 8);
    assert_eq!(&ics.window_group_length[..8], &[1, 1, 1, 1, 1, 1, 1, 1]);
    Ok(())
}

#[test]
fn section_single_zero_band() -> Result<(), Error> {
    let mut w = BitWriter::new();
    // ics only_long, max_sfb=1, predictor=0
    w.write_bit(false);
    w.write(0, 2);
    w.write_bit(false);
    w.write(1, 6);
    w.write_bit(false);
    // section: cb=0, len=1 (5 bits)
    w.write(0, 4);
    w.write(1, 5);
    let bytes = w.finish();
    let mut br = BitReader::new(&bytes);
    let ics = IcsInfo::parse(&mut br, 3, false)?;
    assert_eq!(ics.window_sequence, WindowSequence::OnlyLong);
    let sec = SectionData::parse(&mut br, &ics)?;
    assert_eq!(sec.sfb_cb[0][0], 0);
    Ok(())
}
