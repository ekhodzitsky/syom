//! DecodeOptions defaults and invalid-limit rejection.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{AacError, Decoder, decode_with};

const SINE: &[u8] = include_bytes!("goldens/sine48.adts");

fn speech_with_duration(d: f64) -> DecodeOptions {
    DecodeOptions {
        max_duration_secs: d,
        ..DecodeOptions::speech()
    }
}

#[test]
fn speech_defaults() {
    let o = DecodeOptions::speech();
    assert_eq!(o.channel_mode, ChannelMode::Mono);
    assert_eq!(o.max_frames(16_000), 16_000 * 7200);
    o.validate().unwrap();
    assert_eq!(o.with_max_duration_secs(1.0).max_frames(16_000), 16_000);
    DecodeOptions::unbounded().validate().unwrap();
}

#[test]
fn audio_is_split_with_speech_caps() {
    let a = DecodeOptions::audio();
    assert_eq!(a.channel_mode, ChannelMode::Split);
    assert_eq!(
        a.max_duration_secs,
        DecodeOptions::speech().max_duration_secs
    );
    a.validate().unwrap();
}

#[test]
fn nan_and_negative_duration_no_longer_mean_unlimited() {
    // Prior bug: !is_finite() (NaN and -inf) returned usize::MAX.
    let nan = speech_with_duration(f64::NAN);
    assert_eq!(nan.max_frames(48_000), 0);
    assert!(nan.validate().is_err());
    let ninf = speech_with_duration(f64::NEG_INFINITY);
    assert_eq!(ninf.max_frames(48_000), 0);
    assert!(ninf.validate().is_err());
    let neg = speech_with_duration(-1.0);
    assert_eq!(neg.max_frames(48_000), 0);
    assert!(neg.validate().is_err());
}

#[test]
fn plus_infinity_is_the_unbounded_contract() {
    let o = DecodeOptions::unbounded();
    assert!(o.max_duration_secs.is_infinite() && o.max_duration_secs > 0.0);
    assert_eq!(o.max_frames(48_000), usize::MAX);
    o.validate().unwrap();
    let pcm = decode_with(SINE, &o).unwrap();
    assert!(!pcm.channels[0].is_empty());
}

#[test]
fn invalid_limits_fail_before_decode_output() {
    for d in [f64::NAN, f64::NEG_INFINITY, -0.5] {
        let err = decode_with(SINE, &speech_with_duration(d)).unwrap_err();
        assert!(
            matches!(err, AacError::InvalidLimits(_)),
            "duration {d:?} -> {err}"
        );
        assert!(err.to_string().contains("invalid decode limits"));
    }
    let mut z = DecodeOptions::speech();
    z.max_sample_rate = 0;
    assert!(matches!(
        decode_with(SINE, &z).unwrap_err(),
        AacError::InvalidLimits(_)
    ));
    let mut z = DecodeOptions::speech();
    z.max_decode_sample_rate = 0;
    assert!(matches!(
        decode_with(SINE, &z).unwrap_err(),
        AacError::InvalidLimits(_)
    ));
}

#[test]
fn zero_and_subframe_duration_are_finite_caps() {
    let zero = speech_with_duration(0.0);
    zero.validate().unwrap();
    assert_eq!(zero.max_frames(48_000), 0);
    // First frame exceeds a 0-sample budget.
    assert!(matches!(
        decode_with(SINE, &zero).unwrap_err(),
        AacError::TooLong { .. }
    ));
    let sub = speech_with_duration(0.001); // 48 samples at 48 kHz
    sub.validate().unwrap();
    assert_eq!(sub.max_frames(48_000), 48);
    assert!(matches!(
        decode_with(SINE, &sub).unwrap_err(),
        AacError::TooLong { .. }
    ));
}

#[test]
fn overflow_sized_duration_saturates_without_panic() {
    let huge = speech_with_duration(1.0e300);
    huge.validate().unwrap();
    assert_eq!(huge.max_frames(48_000), usize::MAX);
    let pcm = decode_with(SINE, &huge).unwrap();
    assert_eq!(pcm.channels[0].len(), 13_312);
}

#[test]
fn push_decoder_rejects_nan_before_buffering() {
    let mut d = Decoder::new(speech_with_duration(f64::NAN));
    let err = d.feed(SINE, |_| Ok(())).unwrap_err();
    assert!(matches!(err, AacError::InvalidLimits(_)));
}

#[test]
fn speech_and_unbounded_still_decode_sine() {
    let a = decode_with(SINE, &DecodeOptions::speech()).unwrap();
    let b = decode_with(SINE, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(a.sample_rate, 48_000);
    assert_eq!(b.sample_rate, 48_000);
    assert_eq!(a.channels.len(), 1);
    assert_eq!(b.channels.len(), 1);
    assert_eq!(a.channels[0].len(), b.channels[0].len());
}
