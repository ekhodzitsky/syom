//! Encode PCM domain: finite samples in [-1, 1], matching one-shot and push.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AacError, EncodeOptions, Encoder, decode_with, encode};

fn oneshot(pcm: &[Vec<f32>]) -> Result<Vec<u8>, AacError> {
    encode(pcm, 48_000)
}

fn push(pcm: &[Vec<f32>], chunk: usize) -> Result<Vec<u8>, AacError> {
    let mut enc = Encoder::new(48_000, pcm.len(), &EncodeOptions::adts())?;
    let mut out = Vec::new();
    let n = pcm[0].len();
    let mut i = 0;
    while i < n {
        let end = (i + chunk).min(n);
        let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[i..end]).collect();
        enc.feed(&planes, |f| {
            out.extend_from_slice(f.au);
            Ok(())
        })?;
        i = end;
    }
    enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    Ok(out)
}

fn err_msg(e: AacError) -> String {
    assert!(
        matches!(e, AacError::InvalidPcm(_)),
        "expected InvalidPcm, got {e:?}"
    );
    e.to_string()
}

#[test]
fn nan_and_infinity_are_encode_errors_on_both_apis() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let pcm = vec![vec![bad; 1024]];
        let a = err_msg(oneshot(&pcm).unwrap_err());
        let b = err_msg(push(&pcm, 256).unwrap_err());
        assert_eq!(a, "encode: non-finite sample");
        assert_eq!(a, b, "one-shot vs push: {bad:?}");
    }
}

#[test]
fn amplitude_above_one_is_encode_error_on_both_apis() {
    for bad in [1.0 + f32::EPSILON, 2.0, f32::MAX, -1.0 - f32::EPSILON, -2.0] {
        let pcm = vec![vec![0.0; 1023].into_iter().chain([bad]).collect()];
        let a = err_msg(oneshot(&pcm).unwrap_err());
        let b = err_msg(push(&pcm, 64).unwrap_err());
        assert_eq!(a, "encode: sample amplitude exceeds ±1");
        assert_eq!(a, b, "one-shot vs push: {bad}");
    }
}

#[test]
fn signed_zero_subnormal_and_full_scale_encode() {
    let n = 2048;
    let cases = [
        vec![0.0f32; n],
        vec![-0.0f32; n],
        vec![f32::from_bits(1); n],
        vec![1.0f32; n],
        vec![-1.0f32; n],
    ];
    for pcm0 in cases {
        let pcm = vec![pcm0];
        let a = oneshot(&pcm).expect("oneshot");
        let b = push(&pcm, 100).expect("push");
        assert_eq!(
            crate::gapless::strip_id3(&a),
            b.as_slice(),
            "chunked push must match one-shot"
        );
        let again = oneshot(&pcm).expect("repeat");
        assert_eq!(a, again, "deterministic");
        let dec = decode_with(&a, &crate::DecodeOptions::unbounded()).expect("decode");
        assert!(dec.channels[0].iter().all(|x| x.is_finite()));
    }
}

#[test]
fn huge_finite_does_not_panic() {
    let pcm = vec![vec![f32::MAX; 1024]];
    let e = oneshot(&pcm).expect_err("must reject before MDCT");
    assert!(matches!(
        e,
        AacError::InvalidPcm(crate::PcmReject::Amplitude)
    ));
}
