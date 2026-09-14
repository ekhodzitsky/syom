//! TASK-73: production section planner stays greedy (DP is off-path).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, EncodeOptions, decode_with, encode, encode_with};

const RATE: u32 = 48_000;

#[test]
fn greedy_sine_encode_still_decodes() {
    let pcm = vec![
        (0..4096)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / RATE as f32).sin())
            .collect::<Vec<f32>>(),
    ];
    let adts = encode(&pcm, RATE).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    assert!(dec.channels[0].iter().all(|x| x.is_finite()));
    assert_eq!(adts[0], 0xff);
}

#[test]
fn zero_holes_and_click_stay_deterministic() {
    let mut v = vec![0.0f32; 8 * 1024];
    v[3 * 1024 + 100] = 0.8;
    let pcm = vec![v];
    let a = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    assert_eq!(a, b);
    let d = decode_with(&a, &DecodeOptions::unbounded()).unwrap();
    assert!(d.channels[0].iter().all(|x| x.is_finite()));
}
