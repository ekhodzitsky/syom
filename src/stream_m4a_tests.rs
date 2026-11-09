//! Streaming API: M4A/ISOBMFF slice decode — elst parity with one-shot.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::stream_tests::{assert_eq_collected, collect_slice};
use super::{DecodeOptions, Result};

fn m4a_cases() -> [(&'static [u8], &'static str); 5] {
    [
        (&include_bytes!("goldens/he48.m4a")[..], "he48"),
        (&include_bytes!("goldens/ps48.m4a")[..], "ps48"),
        (&include_bytes!("goldens/sine441.m4a")[..], "sine441"),
        (&include_bytes!("goldens/lecture.m4a")[..], "lecture"),
        (&include_bytes!("goldens/mc51.m4a")[..], "mc51"),
    ]
}

#[test]
fn m4a_slice_streaming_matches_oneshot() -> Result<()> {
    for (data, label) in m4a_cases() {
        for opts in [DecodeOptions::speech(), DecodeOptions::unbounded()] {
            let one = crate::decode_with(data, &opts)?;
            let s = collect_slice(data, &opts)?;
            // Bit-identical planes: the elst skip-left walk replaces the
            // one-shot post-hoc shift and must land on the same samples.
            assert_eq_collected(&one, &s, label);
        }
    }
    Ok(())
}

#[test]
fn m4a_stream_info_matches_elst_trimmed_length() -> Result<()> {
    for (data, label) in m4a_cases() {
        let opts = DecodeOptions::unbounded();
        let one = crate::decode_with(data, &opts)?;
        let s = collect_slice(data, &opts)?;
        assert_eq!(
            s.info.samples as usize,
            one.channels[0].len(),
            "{label} samples after elst skip"
        );
        assert!(s.info.aac_frames > 0, "{label} frames");
    }
    Ok(())
}

#[test]
fn m4a_caps_match_oneshot() {
    let data = &include_bytes!("goldens/sine441.m4a")[..];
    let opts = DecodeOptions::speech().with_max_sample_rate(8_000);
    let one = crate::decode_with(data, &opts);
    let mut tracks: Vec<Vec<f32>> = Vec::new();
    let stream = crate::decode_streaming(data, &opts, |f| {
        for (d, s) in tracks.iter_mut().zip(f.planar.iter()) {
            d.extend_from_slice(s);
        }
        Ok(())
    });
    assert!(matches!(
        one,
        Err(crate::AacError::UnsupportedSampleRate { .. })
    ));
    assert!(matches!(
        stream,
        Err(crate::AacError::UnsupportedSampleRate { .. })
    ));
}

fn patch_elst_field(m4a: &mut [u8], off: usize, v: u32) {
    let body = find_box_body(m4a, b"elst").expect("elst");
    let i = body + off;
    m4a[i..i + 4].copy_from_slice(&v.to_be_bytes());
}

fn find_box_body(data: &[u8], want: &[u8; 4]) -> Option<usize> {
    fn walk(data: &[u8], start: usize, end: usize, want: &[u8; 4]) -> Option<usize> {
        let mut pos = start;
        while pos + 8 <= end {
            let size = u32::from_be_bytes(data[pos..pos + 4].try_into().ok()?) as usize;
            if size < 8 || pos + size > end {
                return None;
            }
            if &data[pos + 4..pos + 8] == want {
                return Some(pos + 8);
            }
            if let Some(b) = walk(data, pos + 8, pos + size, want) {
                return Some(b);
            }
            pos += size;
        }
        None
    }
    walk(data, 0, data.len(), want)
}

fn encode_m4a_n(n: usize) -> Vec<u8> {
    crate::encode_with(&[vec![0.1f32; n]], 48_000, &crate::EncodeOptions::m4a()).unwrap()
}

#[test]
fn encoded_m4a_oneshot_and_stream_are_exact_valid_length() -> Result<()> {
    let opts = DecodeOptions::unbounded();
    for n in [1usize, 1023, 1024, 1025, 2049] {
        let m4a = encode_m4a_n(n);
        let one = crate::decode_with(&m4a, &opts)?;
        let s = collect_slice(&m4a, &opts)?;
        assert_eq!(one.channels[0].len(), n, "oneshot n={n}");
        assert_eq!(s.info.samples as usize, n, "stream n={n}");
        assert_eq_collected(&one, &s, "encode-m4a");
        let look = crate::encode_with(
            &[vec![0.1f32; n]],
            48_000,
            &crate::EncodeOptions::m4a().with_lookahead(true),
        )?;
        let dl = crate::decode_with(&look, &opts)?;
        assert_eq!(dl.channels[0].len(), n, "lookahead n={n}");
        if n >= 1024 {
            let last = crate::encode_with(
                &{
                    let mut v = vec![0.0f32; n];
                    v[n - 1] = 0.9;
                    vec![v]
                },
                48_000,
                &crate::EncodeOptions::m4a(),
            )?;
            let d = crate::decode_with(&last, &opts)?;
            assert_eq!(d.channels[0].len(), n);
            let (i, a) = d.channels[0]
                .iter()
                .enumerate()
                .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
                .unwrap();
            assert!(a.abs() > 0.15, "n={n} last impulse {a}");
            assert!((i as i32 - (n as i32 - 1)).abs() <= 8, "n={n} peak {i}");
        }
    }
    Ok(())
}

#[test]
fn unsupported_elst_is_format_not_prefix_skip() {
    let mut m4a = encode_m4a_n(1025);
    patch_elst_field(&mut m4a, 4, 2); // entry_count = 2
    let err = crate::decode_with(&m4a, &DecodeOptions::unbounded()).unwrap_err();
    assert!(
        err.to_string().contains("multiple"),
        "multiple edits: {err}"
    );
    let s = crate::decode_streaming(&m4a, &DecodeOptions::unbounded(), |_| Ok(()));
    assert!(s.unwrap_err().to_string().contains("multiple"));

    let mut empty = encode_m4a_n(1025);
    patch_elst_field(&mut empty, 12, (-1i32) as u32); // media_time = -1
    let err = crate::decode_with(&empty, &DecodeOptions::unbounded()).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");

    let mut rate = encode_m4a_n(1025);
    patch_elst_field(&mut rate, 16, 0x0002_0000); // media_rate = 2
    let err = crate::decode_with(&rate, &DecodeOptions::unbounded()).unwrap_err();
    assert!(err.to_string().contains("media_rate"), "{err}");
}

#[test]
fn fractional_edit_duration_is_rounded_down() -> Result<()> {
    let mut m4a = encode_m4a_n(1025);
    let body = find_box_body(&m4a, b"mvhd").expect("mvhd");
    m4a[body + 12..body + 16].copy_from_slice(&44_100u32.to_be_bytes());
    let opts = DecodeOptions::unbounded();
    let one = crate::decode_with(&m4a, &opts)?;
    assert_eq!(one.channels[0].len(), 1115); // floor(1025 * 48000 / 44100)
    let streamed = collect_slice(&m4a, &opts)?;
    assert_eq_collected(&one, &streamed, "fractional edit");
    Ok(())
}

#[test]
fn fractional_duration_over_a_full_tick_is_incomplete() -> Result<()> {
    use std::io::Cursor;
    let mut m4a = crate::encode_with(&[vec![0.1f32; 1]], 44_100, &crate::EncodeOptions::m4a())?;
    let body = find_box_body(&m4a, b"mvhd").unwrap();
    m4a[body + 12..body + 16].copy_from_slice(&2000u32.to_be_bytes());
    patch_elst_field(&mut m4a, 8, 3); // 66.15 samples
    patch_elst_field(&mut m4a, 12, 2004); // 44 samples available after skip
    let opts = DecodeOptions::audio();
    // 22.15 missing samples exceeds the 22.05-sample movie tick.
    for err in [
        crate::decode_with(&m4a, &opts).unwrap_err(),
        crate::decode_seek_streaming(Cursor::new(&m4a), &opts, |_| Ok(())).unwrap_err(),
    ] {
        assert!(
            err.to_string().contains("incomplete M4A presentation"),
            "{err}"
        );
    }
    let mut seek = crate::M4aSeek::open(Cursor::new(&m4a), opts)?;
    assert!(seek.decode(|_| Ok(())).is_err());
    assert!(!seek.presentation_complete());
    Ok(())
}

#[test]
fn fractional_mdhd_duration_does_not_reject_a_valid_edit() -> Result<()> {
    let mut m4a = crate::encode_with(&[vec![0.1f32; 1024]], 44_100, &crate::EncodeOptions::m4a())?;
    let body = find_box_body(&m4a, b"mdhd").unwrap();
    m4a[body + 12..body + 16].copy_from_slice(&1000u32.to_be_bytes());
    m4a[body + 16..body + 20].copy_from_slice(&47u32.to_be_bytes()); // 2072.7 samples
    patch_elst_field(&mut m4a, 12, 0);
    let opts = DecodeOptions::audio();
    assert_eq!(crate::decode_with(&m4a, &opts)?.channels[0].len(), 1024);
    assert_eq!(
        crate::probe(&m4a)?.trim,
        crate::ProbeTrim::Exact {
            priming: 0,
            remainder: 1048,
        }
    );
    Ok(())
}

#[test]
fn fractional_edit_trim_is_reported_in_output_samples() -> Result<()> {
    use std::io::Cursor;
    let mut m4a = crate::encode_with(&[vec![0.1f32; 1025]], 44_100, &crate::EncodeOptions::m4a())?;
    let body = find_box_body(&m4a, b"mvhd").unwrap();
    m4a[body + 12..body + 16].copy_from_slice(&1000u32.to_be_bytes());
    let body = find_box_body(&m4a, b"mdhd").unwrap();
    m4a[body + 12..body + 16].copy_from_slice(&22_050u32.to_be_bytes());
    m4a[body + 16..body + 20].copy_from_slice(&1536u32.to_be_bytes());
    let body = find_box_body(&m4a, b"stts").unwrap();
    m4a[body + 12..body + 16].copy_from_slice(&512u32.to_be_bytes());
    patch_elst_field(&mut m4a, 8, 11); // 485.1 output samples, 242.55 media ticks
    patch_elst_field(&mut m4a, 12, 512);
    let opts = DecodeOptions::audio();
    let one = crate::decode_with(&m4a, &opts)?;
    assert_eq!(one.channels[0].len(), 485);
    assert_eq!(one.priming, Some(1024));
    assert_eq!(one.remainder, Some(1563)); // 3072 - 1024 - 485
    assert_eq!(
        crate::probe(&m4a)?.trim,
        crate::ProbeTrim::Exact {
            priming: 1024,
            remainder: 1563,
        }
    );
    let streamed = crate::decode_seek_streaming(Cursor::new(&m4a), &opts, |_| Ok(()))?;
    assert_eq!(streamed.remainder, one.remainder);
    let mut seek = crate::M4aSeek::open(Cursor::new(&m4a), opts)?;
    seek.seek_exact(480, u64::MAX)?;
    assert_eq!(seek.decode(|_| Ok(()))?.remainder, one.remainder);
    Ok(())
}

#[test]
fn unknown_mdhd_duration_does_not_bound_a_fractional_edit() -> Result<()> {
    use std::io::Cursor;
    for version in [1u8, 0] {
        let mut m4a =
            crate::encode_with(&[vec![0.1f32; 1024]], 44_100, &crate::EncodeOptions::m4a())?;
        // The encoder writes moov after mdat; growing mdhd preserves all AU offsets.
        let body = find_box_body(&m4a, b"mdhd").unwrap();
        if version == 1 {
            let headers = [b"moov", b"trak", b"mdia", b"mdhd"]
                .map(|kind| find_box_body(&m4a, kind).unwrap() - 8);
            for at in headers {
                let size = u32::from_be_bytes(m4a[at..at + 4].try_into().unwrap());
                m4a[at..at + 4].copy_from_slice(&(size + 12).to_be_bytes());
            }
            let mut header = vec![1u8, 0, 0, 0];
            header.extend_from_slice(&[0; 16]);
            header.extend_from_slice(&22_050u32.to_be_bytes());
            header.extend_from_slice(&u64::MAX.to_be_bytes());
            m4a.splice(body..body + 20, header);
        } else {
            m4a[body + 12..body + 16].copy_from_slice(&22_050u32.to_be_bytes());
            m4a[body + 16..body + 20].copy_from_slice(&u32::MAX.to_be_bytes());
        }
        let body = find_box_body(&m4a, b"stts").unwrap();
        m4a[body + 12..body + 16].copy_from_slice(&512u32.to_be_bytes());
        let body = find_box_body(&m4a, b"mvhd").unwrap();
        m4a[body + 12..body + 16].copy_from_slice(&1000u32.to_be_bytes());
        patch_elst_field(&mut m4a, 8, 23); // floor(23 ms * 44100) = 1014
        patch_elst_field(&mut m4a, 12, 0);
        let opts = DecodeOptions::audio();
        let one = crate::decode_with(&m4a, &opts)?;
        assert_eq!(one.channels[0].len(), 1014);
        assert_eq!(one.remainder, None, "unknown mdhd v{version} tail");
        let p = crate::probe(&m4a)?;
        assert_eq!(p.duration, crate::ProbeDuration::Exact { samples: 1014 });
        assert_eq_collected(&one, &collect_slice(&m4a, &opts)?, "unknown duration");
        let info = crate::decode_seek_streaming(Cursor::new(&m4a), &opts, |_| Ok(()))?;
        assert_eq!(info.samples, 1014);
        assert_eq!(info.remainder, None);
        let mut seek = crate::M4aSeek::open(Cursor::new(&m4a), opts)?;
        assert_eq!(seek.presentation_len(), 1014);
        seek.seek_exact(1000, u64::MAX)?;
        assert_eq!(seek.decode(|_| Ok(()))?.samples, 14);
        assert!(seek.presentation_complete());
    }
    Ok(())
}

#[test]
fn movie_tick_rounding_agrees_across_flat_m4a_apis() -> Result<()> {
    use crate::{M4aSeek, ProbeDuration, ProbeTrim};
    use std::io::Cursor;

    for (n, ticks, skip, expected) in [(54_419, 1234, 1024, 54_419), (11_264, 279, 0, 12_288)] {
        let mut m4a = crate::encode_with(&[vec![0.1f32; n]], 44_100, &crate::EncodeOptions::m4a())?;
        let body = find_box_body(&m4a, b"mvhd").unwrap();
        m4a[body + 12..body + 16].copy_from_slice(&1000u32.to_be_bytes());
        patch_elst_field(&mut m4a, 8, ticks);
        patch_elst_field(&mut m4a, 12, skip);
        let opts = DecodeOptions::audio();
        let one = crate::decode_with(&m4a, &opts)?;
        assert_eq!(one.channels[0].len(), expected);
        assert_eq_collected(&one, &collect_slice(&m4a, &opts)?, "movie tick");
        let mut pcm = Vec::new();
        let info = crate::decode_seek_streaming(Cursor::new(&m4a), &opts, |f| {
            pcm.extend_from_slice(f.planar[0]);
            Ok(())
        })?;
        assert_eq!(pcm, one.channels[0]);
        assert_eq!(info.samples, expected as u64);
        let p = crate::probe(&m4a)?;
        assert_eq!(
            p.duration,
            ProbeDuration::Exact {
                samples: expected as u64
            }
        );
        let track = crate::isomp4::parse_aac_track_with(&m4a, &opts.memory)?;
        let remainder = track.media_duration - u64::from(skip) - expected as u64;
        assert_eq!(
            p.trim,
            ProbeTrim::Exact {
                priming: u64::from(skip),
                remainder
            }
        );
        assert_eq!(track.presentation_samples(), Some(expected as u64));
        assert_eq!(track.remainder_samples(), Some(remainder));
        let mut seek = M4aSeek::open(Cursor::new(&m4a), opts)?;
        assert_eq!(seek.presentation_len(), expected as u64);
        assert_eq!(
            seek.seek_exact(expected as i64 - 17, u64::MAX)?,
            expected as u64 - 17
        );
        let mut tail = Vec::new();
        seek.decode(|f| {
            tail.extend_from_slice(f.planar[0]);
            Ok(())
        })?;
        assert_eq!(tail, one.channels[0][expected - 17..]);
        assert!(seek.presentation_complete());
    }
    Ok(())
}

#[test]
fn fractional_media_time_stays_an_error() {
    let mut m4a = encode_m4a_n(1025);
    let body = find_box_body(&m4a, b"mdhd").unwrap();
    m4a[body + 12..body + 16].copy_from_slice(&44_100u32.to_be_bytes());
    let err = crate::decode_with(&m4a, &DecodeOptions::audio()).unwrap_err();
    assert!(
        err.to_string()
            .contains("elst media_time is not an integer")
    );
    assert!(
        crate::probe(&m4a)
            .unwrap_err()
            .to_string()
            .contains("elst media_time")
    );
}

#[test]
fn fractional_fragment_duration_matches_push_probe_and_indexed_decode() -> Result<()> {
    let mut bytes = include_bytes!("goldens/fmp4_dash_init.m4s").to_vec();
    bytes.extend_from_slice(include_bytes!("goldens/fmp4_dash_seg1.m4s"));
    let body = find_box_body(&bytes, b"mvhd").unwrap();
    bytes[body + 12..body + 16].copy_from_slice(&44_100u32.to_be_bytes());
    patch_elst_field(&mut bytes, 8, 1025);
    let opts = DecodeOptions::audio();
    let one = crate::decode_with(&bytes, &opts)?;
    assert_eq!(one.channels[0].len(), 1115);
    assert_eq!(
        crate::probe(&bytes)?.duration,
        crate::ProbeDuration::Exact { samples: 1115 }
    );
    let mut decoder = crate::Decoder::new(opts);
    let mut pcm = Vec::new();
    for chunk in bytes.chunks(137) {
        decoder.feed(chunk, |f| {
            pcm.extend_from_slice(f.planar[0]);
            Ok(())
        })?;
    }
    decoder.finish(|f| {
        pcm.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert_eq!(pcm, one.channels[0]);
    Ok(())
}
