//! TASK-40: encoder priming, padded length, and omitted tail (no repair).
//!
//! Decoded ADTS length is `ceil(N/1024)*1024` on syom and oxideav; that is
//! **not** valid duration. A last-sample impulse on a 1024-aligned input is
//! absent after decode (omitted overlap), not merely window ringing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use oxideav_aac::decode::StreamDecoder;

use crate::enc_stream::Encoder;
use crate::{DecodeOptions, EncodeInfo, EncodeOptions, decode_with, encode, encode_with};

fn impulse(n: usize, at: usize) -> Vec<Vec<f32>> {
    let mut v = vec![0.0f32; n];
    if at < n {
        v[at] = 0.9;
    }
    vec![v]
}

fn peak(x: &[f32]) -> (usize, f32) {
    x.iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, a)| (i, a.abs()))
        .unwrap_or((0, 0.0))
}

fn adts_len(n: usize) -> usize {
    (n.div_ceil(1024) + 1) * 1024
}

fn oxideav_samples(adts: &[u8]) -> usize {
    let mut d = StreamDecoder::new();
    let frames = d.decode_all(adts).expect("oxideav");
    let ch = frames[0].channels.max(1);
    frames.iter().map(|f| f.pcm.len() / ch).sum()
}

fn syom_adts(n: usize, at: usize) -> (Vec<u8>, Vec<f32>) {
    let pcm = impulse(n, at);
    let adts = encode(&pcm, 48_000).expect("encode");
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).expect("syom");
    (adts, dec.channels[0].clone())
}

#[test]
fn empty_input_is_encode_error() {
    let err = encode(&[vec![]], 48_000).unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");
}

#[test]
fn adts_decoded_length_is_padded_not_valid_duration() {
    for n in [1usize, 1023, 1024, 1025, 2048, 2049, 3072] {
        let (adts, got) = syom_adts(n, 0);
        assert_eq!(got.len(), adts_len(n), "n={n}");
        assert_eq!(oxideav_samples(&adts), got.len(), "oxideav n={n}");
        if n % 1024 != 0 {
            assert_ne!(
                got.len(),
                n,
                "padded length must not be read as source length n={n}"
            );
        }
    }
}

#[test]
fn first_impulse_lands_after_1024_once_a_second_frame_exists() {
    let (_, short) = syom_adts(1024, 0);
    let (i, a) = peak(&short);
    assert_eq!(i, 1024, "drain gives the start a second window i={i}");
    assert!(a > 0.4, "first impulse amplitude {a}");

    let (_, long) = syom_adts(2048, 0);
    let (i, a) = peak(&long);
    assert_eq!(i, 1024, "decoded[i] ≈ input[i-1024]");
    assert!(a > 0.4, "first impulse amplitude {a}");
}

#[test]
fn last_impulse_on_aligned_input_is_reconstructed() {
    for n in [1024usize, 2048, 3072, 4096] {
        let (_, got) = syom_adts(n, n - 1);
        let (i, a) = peak(&got);
        let expect = n + 1024 - 1;
        assert!(
            a > 0.2,
            "N={n} last impulse must survive drain (peak {a} at {i})"
        );
        assert!(
            (i as i32 - expect as i32).abs() <= 8,
            "N={n} peak {i} want ~{expect}"
        );
    }
}

#[test]
fn last_impulse_just_past_a_frame_is_reconstructed() {
    let (_, got) = syom_adts(1025, 1024);
    let (i, a) = peak(&got);
    assert_eq!(got.len(), 3 * 1024);
    assert!(a > 0.2, "N=1025 last impulse a={a}");
    assert!((i as i32 - 2048).abs() <= 8, "peak {i}");
}

#[test]
fn lookahead_drain_preserves_the_last_impulse() {
    let pcm = impulse(2048, 2047);
    let opts = EncodeOptions::adts().with_lookahead(true);
    let adts = encode_with(&pcm, 48_000, &opts).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels[0].len(), 3 * 1024);
    let (i, a) = peak(&dec.channels[0]);
    assert!(a > 0.2, "lookahead last impulse {a}");
    assert!((i as i32 - 3071).abs() <= 8, "peak {i}");
}

#[test]
fn encode_info_counts_input_not_decoded_valid_samples() {
    let pcm = impulse(1124, 0);
    let opts = EncodeOptions::adts();
    let mut enc = Encoder::new(48_000, 1, &opts).unwrap();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |_| Ok(())).unwrap();
    let info: EncodeInfo = enc.finish(|_| Ok(())).unwrap();
    assert_eq!(info.samples, 1124, "input count");
    assert_eq!(info.aac_frames, 3, "2 content + drain");
    assert_eq!(info.priming, 1024);
    assert_eq!(info.remainder, 924);
    assert_eq!(info.coded_samples, 3 * 1024);
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(info.channels, 1);
    let one = encode_with(&pcm, 48_000, &opts).unwrap();
    let dec = decode_with(&one, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels[0].len(), 3 * 1024);
    assert_ne!(
        info.samples as usize,
        dec.channels[0].len(),
        "do not infer validity from padded decode length"
    );
}

#[test]
fn m4a_elst_skips_1024_and_cannot_invent_the_omitted_tail() {
    let pcm = impulse(2048, 2047);
    let m4a = encode_with(&pcm, 48_000, &EncodeOptions::m4a()).unwrap();
    let dec = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(
        dec.channels[0].len(),
        2048,
        "elst drops priming; drain keeps the tail"
    );
    let (i, a) = peak(&dec.channels[0]);
    assert!(a > 0.2, "M4A last impulse {a}");
    assert!((i as i32 - 2047).abs() <= 8, "peak {i}");

    let start = impulse(2048, 0);
    let m4a = encode_with(&start, 48_000, &EncodeOptions::m4a()).unwrap();
    let dec = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels[0].len(), 2048);
    let (i, a) = peak(&dec.channels[0]);
    assert_eq!(i, 0, "after elst the first impulse is at presentation 0");
    assert!(a > 0.4, "first impulse {a}");
}

#[test]
fn one_content_frame_m4a_keeps_the_last_sample_after_elst() {
    let pcm = impulse(1024, 1023);
    let m4a = encode_with(&pcm, 48_000, &EncodeOptions::m4a()).unwrap();
    let dec = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels[0].len(), 1024);
    let (i, a) = peak(&dec.channels[0]);
    assert!(a > 0.2, "last sample of a 1024-input M4A {a}");
    assert!((i as i32 - 1023).abs() <= 8, "peak {i}");
}

#[test]
fn push_matches_oneshot_omission() {
    let pcm = impulse(2048, 2047);
    let opts = EncodeOptions::adts();
    let one = encode_with(&pcm, 48_000, &opts).unwrap();
    let mut enc = Encoder::new(48_000, 1, &opts).unwrap();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    let mut push = Vec::new();
    enc.feed(&planes, |f| {
        push.extend_from_slice(f.au);
        Ok(())
    })
    .unwrap();
    enc.finish(|f| {
        push.extend_from_slice(f.au);
        Ok(())
    })
    .unwrap();
    assert_eq!(push, one);
}
