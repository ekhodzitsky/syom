//! Extra ISOBMFF branches: 64-bit sizes, esds flags, co64, elst/mdhd v1, wave.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{
    AacTrack, BoxHdr, extract_asc, parse_elst, parse_elst_start, parse_esds, parse_mdhd_duration,
    parse_mdhd_timescale, parse_mvhd_timescale, parse_stco, parse_stsc, read_box, read_desc_len,
    sniff_is_m4a,
};
use crate::budgets::MemoryBudgets;
use crate::error::AacError;

fn mem() -> MemoryBudgets {
    MemoryBudgets::default()
}

fn ok<T>(r: Result<T, AacError>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("{e:?}"),
    }
}

fn esds_body(asc: &[u8]) -> Vec<u8> {
    let mut dsi = vec![0x05u8, asc.len() as u8];
    dsi.extend_from_slice(asc);
    let mut dcd = vec![0x04u8, (13 + dsi.len()) as u8];
    dcd.extend_from_slice(&[0x40, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    dcd.extend_from_slice(&dsi);
    let mut esd = vec![0x03u8, (3 + dcd.len()) as u8, 0x00, 0x01, 0x00];
    esd.extend_from_slice(&dcd);
    let mut body = vec![0u8; 4];
    body.extend_from_slice(&esd);
    body
}

#[test]
fn read_box_64bit_and_until_eof_and_undersize() {
    let mut data = vec![0, 0, 0, 1];
    data.extend_from_slice(b"mdat");
    data.extend_from_slice(&24u64.to_be_bytes());
    data.extend_from_slice(&[0u8; 8]);
    let (hdr, typ) = ok(read_box(&data, 0)).expect("64-bit box");
    assert_eq!(&typ, b"mdat");
    assert_eq!(hdr.content_start, 16);
    assert_eq!(hdr.content_end, 24);

    let mut eof = vec![0, 0, 0, 0];
    eof.extend_from_slice(b"mdatXXXX");
    let (hdr, typ) = ok(read_box(&eof, 0)).expect("until-eof box");
    assert_eq!(&typ, b"mdat");
    assert_eq!(hdr.content_end, eof.len());

    let small = {
        let mut v = 4u32.to_be_bytes().to_vec();
        v.extend_from_slice(b"mdat");
        v
    };
    assert!(read_box(&small, 0).is_err());
    assert!(read_box(&[0, 0, 0, 1, b'm', b'd', b'a', b't'], 0).is_err());
}

#[test]
fn read_box_largesize_past_the_buffer_is_an_error() {
    // 2^32 is a complete largesize. On a 32-bit target it does not fit in
    // `usize`; truncating before the bounds check would report a 0-byte box.
    let mut data = vec![0, 0, 0, 1];
    data.extend_from_slice(b"free");
    data.extend_from_slice(&(1u64 << 32).to_be_bytes());
    data.extend_from_slice(&[0u8; 8]);
    match read_box(&data, 0) {
        Err(_) => {}
        Ok(Some((hdr, _))) => panic!(
            "largesize 2^32 must not become a box ending at {}",
            hdr.content_end
        ),
        Ok(None) => panic!("header is complete"),
    }
}

#[test]
fn sniff_m4a_rejects_short_and_tiny_size() {
    assert!(!sniff_is_m4a(&[]));
    assert!(!sniff_is_m4a(b"1234"));
    let mut d = 4u32.to_be_bytes().to_vec();
    d.extend_from_slice(b"ftypM4A ");
    assert!(!sniff_is_m4a(&d));
    let mut d = 20u32.to_be_bytes().to_vec();
    d.extend_from_slice(b"ftyp");
    d.extend_from_slice(b"M4A ");
    d.extend_from_slice(&[0; 8]);
    assert!(sniff_is_m4a(&d));
}

#[test]
fn desc_len_rejects_four_continuations() {
    let data = [0x80, 0x80, 0x80, 0x80];
    assert!(read_desc_len(&data, 0).is_err());
}

#[test]
fn esds_errors_and_optional_flags() {
    assert!(parse_esds(&[0, 0, 0, 0, 0x04, 0]).is_err());

    let mut esds = vec![0u8; 4];
    esds.extend_from_slice(&[0x03, 8, 0x00, 0x01, 0x00, 0x04, 0x02, 0x40, 0x15]);
    assert!(parse_esds(&esds).is_err());

    let mut inner = esds_body(&[0x12, 0x10]);
    // ES flags at body[4+1+1+2] = skip version(4)+tag(1)+len(1)+ES_ID(2) → flags.
    let flags_at = 4 + 1 + 1 + 2;
    inner[flags_at] = 0xE0;
    inner.splice(
        flags_at + 1..flags_at + 1,
        [0x00, 0x02, 3, b'u', b'r', b'l', 0x00, 0x03],
    );
    inner[5] = (inner.len() - 6) as u8;
    assert_eq!(ok(parse_esds(&inner)), vec![0x12, 0x10]);

    let mut bad_ot = esds_body(&[0x12, 0x10]);
    let ot_at = 4 + 1 + 1 + 3 + 1 + 1;
    bad_ot[ot_at] = 0x6B;
    assert!(parse_esds(&bad_ot).is_err());

    let mut empty_asc = esds_body(&[]);
    empty_asc[5] = (empty_asc.len() - 6) as u8;
    assert!(parse_esds(&esds_body(&[])).is_err());

    let mut no_dsi = esds_body(&[0x12, 0x10]);
    // Drop the 0x05 descriptor: keep DecoderConfig 13-byte prefix only.
    let dcd_tag = no_dsi.iter().position(|&b| b == 0x04).unwrap_or(0);
    no_dsi.truncate(dcd_tag + 2 + 13);
    no_dsi[dcd_tag + 1] = 13;
    no_dsi[5] = (no_dsi.len() - 6) as u8;
    assert!(parse_esds(&no_dsi).is_err());
    let _ = empty_asc;
}

#[test]
fn extract_asc_from_wave_wrapper() {
    let esds = esds_body(&[0x12, 0x10]);
    let mut esds_box = ((esds.len() + 8) as u32).to_be_bytes().to_vec();
    esds_box.extend_from_slice(b"esds");
    esds_box.extend_from_slice(&esds);
    let mut wave = ((esds_box.len() + 8) as u32).to_be_bytes().to_vec();
    wave.extend_from_slice(b"wave");
    wave.extend_from_slice(&esds_box);
    let children = BoxHdr {
        content_start: 0,
        content_end: wave.len(),
    };
    assert_eq!(ok(extract_asc(&wave, children)), vec![0x12, 0x10]);
}

#[test]
fn stsc_empty_and_co64() {
    let mut stsc = vec![0u8; 4];
    stsc.extend_from_slice(&0u32.to_be_bytes());
    assert!(parse_stsc(&stsc, &mem()).is_err());

    let mut co64 = vec![0u8; 4];
    co64.extend_from_slice(&1u32.to_be_bytes());
    co64.extend_from_slice(&0u32.to_be_bytes());
    co64.extend_from_slice(&0x1000u32.to_be_bytes());
    let offs = ok(parse_stco(&co64, true, &mem()));
    assert_eq!(offs, vec![0x1000]);
}

#[test]
fn elst_v1_and_unsupported_and_negative_then_zero() {
    let mut v1 = vec![1u8, 0, 0, 0];
    v1.extend_from_slice(&1u32.to_be_bytes());
    v1.extend_from_slice(&0u64.to_be_bytes());
    v1.extend_from_slice(&1024u64.to_be_bytes());
    v1.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    assert_eq!(ok(parse_elst_start(&v1, &mem())), 1024);
    assert_eq!(ok(parse_elst(&v1, &mem())), Some((1024, 0)));

    let mut v0 = vec![0u8; 4];
    v0.extend_from_slice(&1u32.to_be_bytes());
    v0.extend_from_slice(&28800u32.to_be_bytes());
    v0.extend_from_slice(&1024u32.to_be_bytes());
    v0.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    assert_eq!(ok(parse_elst(&v0, &mem())), Some((1024, 28800)));

    let mut bad = vec![2u8, 0, 0, 0];
    bad.extend_from_slice(&1u32.to_be_bytes());
    bad.extend_from_slice(&[0u8; 12]);
    assert!(parse_elst_start(&bad, &mem()).is_err());

    let mut skip = vec![0u8; 4];
    skip.extend_from_slice(&2u32.to_be_bytes());
    skip.extend_from_slice(&10u32.to_be_bytes());
    skip.extend_from_slice(&(-1i32 as u32).to_be_bytes());
    skip.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    skip.extend_from_slice(&10u32.to_be_bytes());
    skip.extend_from_slice(&0u32.to_be_bytes());
    skip.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    let err = parse_elst(&skip, &mem()).unwrap_err();
    assert!(
        err.to_string().contains("multiple"),
        "empty-then-playable must not collapse to a prefix skip: {err}"
    );

    let mut empty = vec![0u8; 4];
    empty.extend_from_slice(&1u32.to_be_bytes());
    empty.extend_from_slice(&10u32.to_be_bytes());
    empty.extend_from_slice(&(-1i32 as u32).to_be_bytes());
    empty.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    let err = parse_elst(&empty, &mem()).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
}

#[test]
fn mdhd_v1_and_unsupported() {
    let mut v1 = vec![1u8, 0, 0, 0];
    v1.extend_from_slice(&[0u8; 16]);
    v1.extend_from_slice(&48_000u32.to_be_bytes());
    assert_eq!(ok(parse_mdhd_timescale(&v1)), 48_000);
    assert!(parse_mdhd_timescale(&[3u8, 0, 0, 0]).is_err());
}

#[test]
fn mdhd_duration_and_mvhd_timescale_are_checked() {
    let mut mdhd = vec![0u8; 4];
    mdhd.extend_from_slice(&[0u8; 8]);
    mdhd.extend_from_slice(&48_000u32.to_be_bytes());
    mdhd.extend_from_slice(&30720u32.to_be_bytes());
    assert_eq!(ok(parse_mdhd_duration(&mdhd)), 30720);

    let mut mdhd1 = vec![1u8, 0, 0, 0];
    mdhd1.extend_from_slice(&[0u8; 16]);
    mdhd1.extend_from_slice(&48_000u32.to_be_bytes());
    mdhd1.extend_from_slice(&30720u64.to_be_bytes());
    assert_eq!(ok(parse_mdhd_duration(&mdhd1)), 30720);
    mdhd[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
    mdhd1[24..32].copy_from_slice(&u64::MAX.to_be_bytes());
    assert_eq!(ok(parse_mdhd_duration(&mdhd)), 0, "v0 unknown duration");
    assert_eq!(ok(parse_mdhd_duration(&mdhd1)), 0, "v1 unknown duration");
    mdhd1[24..32].copy_from_slice(&u64::from(u32::MAX).to_be_bytes());
    assert_eq!(ok(parse_mdhd_duration(&mdhd1)), u64::from(u32::MAX));
    assert!(parse_mdhd_duration(&[3u8, 0, 0, 0]).is_err());

    let mut mvhd = vec![0u8; 4];
    mvhd.extend_from_slice(&[0u8; 8]);
    mvhd.extend_from_slice(&48_000u32.to_be_bytes());
    assert_eq!(ok(parse_mvhd_timescale(&mvhd)), 48_000);
    assert!(parse_mvhd_timescale(&[3u8, 0, 0, 0]).is_err());
}

#[test]
fn presentation_and_remainder_use_checked_arithmetic() {
    let huge = AacTrack {
        asc: Vec::new(),
        total_samples: 0,
        frames: Vec::new(),
        edit_start: 1024,
        edit_duration: u64::MAX,
        movie_timescale: 2,
        media_timescale: 3,
        media_duration: 0,
        has_elst: true,
        edit_open_end: false,
    };
    assert!(
        huge.presentation_samples().is_none(),
        "overflowing movie→media convert"
    );
    assert!(huge.edit_window(3).is_err(), "overflowing edit_window");
    let swapped = AacTrack {
        asc: Vec::new(),
        total_samples: 0,
        frames: Vec::new(),
        edit_start: 2048,
        edit_duration: 30720,
        movie_timescale: 48_000,
        media_timescale: 48_000,
        media_duration: 2048,
        has_elst: true,
        edit_open_end: false,
    };
    assert_eq!(swapped.presentation_samples(), Some(30720));
    assert!(
        swapped.remainder_samples().is_none(),
        "coded < priming+valid must not wrap"
    );
}

#[test]
fn reported_movie_tick_examples_have_consistent_windows() {
    for (duration, start, media_duration, expected) in [
        (1234, 1024, 55_443, 54_419),
        (257_208, 0, 11_342_848, 11_342_848),
    ] {
        let track = AacTrack {
            asc: Vec::new(),
            frames: Vec::new(),
            total_samples: media_duration,
            edit_start: start,
            edit_duration: duration,
            movie_timescale: 1000,
            media_timescale: 44_100,
            media_duration,
            has_elst: true,
            edit_open_end: false,
        };
        assert_eq!(
            track.edit_window(44_100).unwrap(),
            (start as usize, Some(expected as usize))
        );
        assert_eq!(track.presentation_samples(), Some(expected));
        assert_eq!(
            track.remainder_samples(),
            Some(media_duration - start - expected)
        );
        // HE output uses the same time window at twice the media rate.
        assert_eq!(
            track.edit_window(88_200).unwrap(),
            (
                start as usize * 2,
                Some(if start == 0 {
                    expected as usize * 2
                } else {
                    108_838
                })
            )
        );
    }
}

#[test]
fn movie_duration_rounding_checks_clocks_overflow_and_tolerance() {
    use super::timing::duration_samples;
    assert_eq!(duration_samples(1, 44_100, 1000, None).unwrap(), 44);
    assert_eq!(duration_samples(2, 44_100, 1000, None).unwrap(), 88);
    assert_eq!(duration_samples(3, 44_100, 1000, None).unwrap(), 132);
    assert_eq!(
        duration_samples(279, 44_100, 1000, Some(12_288)).unwrap(),
        12_288
    );
    // A full clock tick, a full millisecond, or a large mismatch is not rounding.
    assert_eq!(duration_samples(101, 1000, 1000, Some(100)).unwrap(), 101);
    assert_eq!(duration_samples(11, 1000, 100, Some(100)).unwrap(), 110);
    assert_eq!(
        duration_samples(1000, 44_100, 1000, Some(100)).unwrap(),
        44_100
    );
    assert!(duration_samples(0, 44_100, 0, None).is_err());
    assert!(duration_samples(0, 0, 1000, None).is_err());
    assert!(duration_samples(u64::MAX, 44_100, 1000, None).is_err());
}

#[test]
fn duration_conversion_keeps_valid_wide_products_and_exact_tail_bounds() {
    use super::timing::{available_samples, duration_samples, rounded_duration_tail};
    assert_eq!(
        duration_samples(u64::MAX, 44_100, 44_100, None).unwrap(),
        u64::MAX
    );
    assert_eq!(
        available_samples(u64::MAX, 1, 44_100, 44_100).unwrap(),
        Some(u64::MAX - 1)
    );
    assert_eq!(available_samples(0, 0, 44_100, 0).unwrap(), None);
    assert_eq!(available_samples(1, 2, 44_100, 44_100).unwrap(), None);
    assert!(!rounded_duration_tail(3, 44_100, 2000, 44)); // 22.15 > 22.05
    assert!(rounded_duration_tail(3, 44_100, 2000, 45)); // 21.15 < 22.05
    assert!(!rounded_duration_tail(u64::MAX, u32::MAX, u32::MAX, 0));

    // Independently check both strict rational inequalities around native
    // sample and clock boundaries, including clocks coarser than a millisecond.
    for rate in [22_050u32, 44_100, 48_000, 88_200] {
        for clock in [100u32, 1000, 2000, 44_100, 48_000] {
            for ticks in 1..20u64 {
                let product = u128::from(ticks) * u128::from(rate);
                let end = (product / u128::from(clock)) as u64;
                for delivered in end.saturating_sub(50)..=end + 1 {
                    let valid = product
                        .checked_sub(u128::from(delivered) * u128::from(clock))
                        .is_some_and(|delta| {
                            delta > 0
                                && delta < u128::from(rate)
                                && delta * 1000 < u128::from(rate) * u128::from(clock)
                        });
                    assert_eq!(
                        rounded_duration_tail(ticks, rate, clock, delivered),
                        valid,
                        "rate={rate} clock={clock} ticks={ticks} delivered={delivered}"
                    );
                }
            }
        }
    }
}
