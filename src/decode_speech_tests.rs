//! TASK-122 regression: LC stereo speech mono is the documented mean of the
//! decoded planes, not the left plane — ADTS one-shot, push streaming, M4A.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{DecodeOptions, Decoder, EncodeOptions};
use crate::AacError;

/// Distinct tones per channel so left-only and the mean are far apart.
fn stereo_fixture() -> [Vec<f32>; 2] {
    let l = (0..48_000)
        .map(|i| 0.3 * (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / 48_000.0).sin())
        .collect();
    let r = (0..48_000)
        .map(|i| 0.2 * (i as f32 * 2.0 * std::f32::consts::PI * 660.0 / 48_000.0 + 0.5).sin())
        .collect();
    [l, r]
}

fn assert_speech_is_mean(speech: &[f32], split: &[Vec<f32>], label: &str) {
    assert_eq!(split.len(), 2, "{label} split not stereo");
    let (l, r) = (&split[0], &split[1]);
    assert_eq!(speech.len(), l.len(), "{label} speech length");
    assert_eq!(l.len(), r.len(), "{label} split length");
    // The fixture must separate left-only from the mean, or the check is blind.
    let lr = l
        .iter()
        .zip(r)
        .fold(0.0f32, |m, (&a, &b)| m.max((a - b).abs()));
    assert!(lr > 0.1, "{label} degenerate stereo fixture");
    let mut mean_err = 0.0f32;
    let mut left_err = 0.0f32;
    for i in 0..speech.len() {
        mean_err = mean_err.max((speech[i] - 0.5 * (l[i] + r[i])).abs());
        left_err = left_err.max((speech[i] - l[i]).abs());
    }
    assert!(
        mean_err <= 2e-5,
        "{label}: speech mono is not the mean of the planes (max err {mean_err})"
    );
    assert!(
        left_err > 1e-3,
        "{label}: speech pinned to the mean must differ from the left plane"
    );
}

#[test]
fn lc_stereo_speech_is_mean_adts_one_shot() -> Result<(), AacError> {
    let [l, r] = stereo_fixture();
    let adts = crate::encode(&[&l[..], &r[..]], 48_000)?;
    let speech = crate::decode(&adts)?;
    assert_eq!(speech.channels.len(), 1, "speech not mono");
    let split = crate::decode_with(&adts, &DecodeOptions::audio())?;
    assert_speech_is_mean(&speech.channels[0], &split.channels, "adts");
    Ok(())
}

#[test]
fn lc_stereo_speech_is_mean_m4a() -> Result<(), AacError> {
    let [l, r] = stereo_fixture();
    let m4a = crate::encode_with(&[&l[..], &r[..]], 48_000, &EncodeOptions::m4a())?;
    let speech = crate::decode_with(&m4a, &DecodeOptions::speech())?;
    assert_eq!(speech.channels.len(), 1, "speech not mono");
    let split = crate::decode_with(&m4a, &DecodeOptions::audio())?;
    assert_speech_is_mean(&speech.channels[0], &split.channels, "m4a");
    Ok(())
}

#[test]
fn lc_stereo_speech_is_mean_push_streaming() -> Result<(), AacError> {
    let [l, r] = stereo_fixture();
    let adts = crate::encode(&[&l[..], &r[..]], 48_000)?;

    let mut speech = Vec::new();
    let mut dec = Decoder::new(DecodeOptions::speech());
    for chunk in adts.chunks(1_319) {
        dec.feed(chunk, |f| {
            speech.extend_from_slice(f.planar[0]);
            Ok(())
        })?;
    }
    dec.finish(|f| {
        speech.extend_from_slice(f.planar[0]);
        Ok(())
    })?;

    let (mut sl, mut sr) = (Vec::new(), Vec::new());
    let mut dec = Decoder::new(DecodeOptions::audio());
    for chunk in adts.chunks(1_319) {
        dec.feed(chunk, |f| {
            sl.extend_from_slice(f.planar[0]);
            sr.extend_from_slice(f.planar[1]);
            Ok(())
        })?;
    }
    dec.finish(|f| {
        sl.extend_from_slice(f.planar[0]);
        sr.extend_from_slice(f.planar[1]);
        Ok(())
    })?;

    assert_speech_is_mean(&speech, &[sl, sr], "push");
    Ok(())
}
