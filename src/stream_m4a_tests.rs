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
fn inexact_elst_timescale_is_format() {
    let mut m4a = encode_m4a_n(1025);
    // duration * 48000 / 44100 is not an integer.
    let body = find_box_body(&m4a, b"mvhd").expect("mvhd");
    m4a[body + 12..body + 16].copy_from_slice(&44_100u32.to_be_bytes());
    let err = crate::decode_with(&m4a, &DecodeOptions::unbounded()).unwrap_err();
    assert!(
        err.to_string().contains("integer"),
        "inexact convert: {err}"
    );
}
