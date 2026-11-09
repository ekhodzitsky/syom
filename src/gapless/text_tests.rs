//! Format boundaries are independent of the encoder's own tag generator.
#![allow(clippy::unwrap_used)]
use super::{Delay, itunsmpb_value, parse_text};

#[test]
fn text_fields_have_exact_widths_and_do_not_resynchronise() {
    let expected = Delay {
        priming: 2112,
        remainder: 416,
        source: 352800,
    };
    for text in [
        " 00000000 00000840 000001A0 0000000000056220 00000000",
        "\t00000000  00000840\t000001a0 0000000000056220\0",
    ] {
        assert_eq!(parse_text(text.as_bytes()), Some(expected));
    }
    for text in [
        "junk 00000000 00000840 000001A0 0000000000056220",
        "000000000 00000840 000001A0 0000000000056220",
        "00000000 000008400 000001A0 0000000000056220",
        "00000000 00000840 000001A0 00000000000562200",
        "00000000 00000840 000001A0 0000000000056220!",
        "00000000 0000084 000001A0 0000000000056220",
        "00000000 00000840 000001A0",
        "00000000 00000840 000001A0 -000000000056220",
        "00000840 000001A0 00000000 0000000000056220",
        "\0 00000000 00000840 000001A0 0000000000056220",
    ] {
        assert_eq!(parse_text(text.as_bytes()), None, "{text:?}");
    }
}

#[test]
fn full_width_source_and_checked_totals_never_wrap_or_saturate() {
    let d = parse_text(b"00000000 00000001 00000001 FFFFFFFFFFFFFFFF").unwrap();
    assert_eq!(d.source, u64::MAX);
    assert_eq!(d.coded_samples(), None);
    assert!(!d.matches(u64::MAX));
    assert!(!d.matches_frames(u64::MAX, 1024));
    let d = Delay {
        priming: 1,
        remainder: 1,
        source: 10,
    };
    assert!(d.matches_frames(3, 4));
    assert!(!d.matches_frames(3, 0));
    assert!(!d.matches_frames(0, 4));
}

#[test]
fn writer_emits_the_entire_standard_twelve_field_value() {
    let d = Delay {
        priming: 2112,
        remainder: 416,
        source: 352800,
    };
    assert_eq!(
        itunsmpb_value(&d),
        " 00000000 00000840 000001A0 0000000000056220 00000000 00000000 00000000 00000000 00000000 00000000 00000000 00000000",
    );
}

#[test]
fn packet_grid_tracks_short_cores_and_actual_output_rate() {
    use crate::engine::asc::AudioSpecificConfig;
    use crate::engine::bits::BitWriter;
    let mut w = BitWriter::new();
    w.write(23, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write_bit(true);
    w.write(0, 4);
    let (mut asc, _) = AudioSpecificConfig::parse(&w.finish()).unwrap();
    assert_eq!(super::output_frame_samples(&asc), 480);
    asc.frame_samples = 512;
    assert_eq!(super::output_frame_samples(&asc), 512);
    asc.aot = 2;
    asc.frame_samples = 960;
    assert_eq!(super::output_frame_samples(&asc), 960);
    asc.frame_samples = 1024;
    asc.sbr_present = true;
    // A signalled SBR flag alone must not double a downsampled output.
    assert_eq!(super::output_frame_samples(&asc), 1024);
    asc.output_sample_rate *= 2;
    assert_eq!(super::output_frame_samples(&asc), 2048);
}
