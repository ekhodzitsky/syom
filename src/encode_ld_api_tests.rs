//! Public AAC-LD encode: opt-in, not the default, no ADTS.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    DecodeOptions, EncodeContainer, EncodeOptions, Encoder, ProbeProfile, decode_with, encode,
    encode_with, probe,
};

fn opts(container: EncodeContainer) -> EncodeOptions {
    EncodeOptions::default()
        .with_container(container)
        .with_ld(true)
        .with_bitrate_bps(64_000)
}

fn tone(n: usize, freq: f32) -> Vec<f32> {
    (0..n)
        .map(|i| 0.4 * (i as f32 * freq * 2.0 * std::f32::consts::PI / 48_000.0).sin())
        .collect()
}

fn snr(got: &[f32], want: &[f32]) -> f64 {
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for (g, w) in got.iter().zip(want.iter()) {
        ps += f64::from(*w) * f64::from(*w);
        let e = f64::from(*g) - f64::from(*w);
        pe += e * e;
    }
    if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    }
}

#[test]
fn default_encode_stays_lc_adts() {
    let pcm = vec![0.2f32; 2048];
    let adts = encode(&[&pcm[..]], 48_000).unwrap();
    assert_eq!(crate::gapless::strip_id3(&adts)[0], 0xff);
    assert_eq!(probe(&adts).unwrap().profile, ProbeProfile::Lc);
}

#[test]
fn loas_roundtrip_delays_one_frame_and_keeps_the_tone() {
    let src = tone(2048, 440.0);
    let loas = encode_with(&[&src], 48_000, &opts(EncodeContainer::Latm)).unwrap();
    let info = probe(&loas).unwrap();
    assert_eq!(info.profile, ProbeProfile::Ld);
    assert_eq!(info.meta.core_rate, 48_000);
    let pcm = decode_with(&loas, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(pcm.channels[0].len(), 2560);
    // The last frame is the overlap flush. At 64 kbps its tail is coarse;
    // the frames before it are the delay check.
    let score = snr(&pcm.channels[0][512..2048], &src[..1536]);
    assert!(score >= 20.0, "snr {score:.1}");
    let fine = opts(EncodeContainer::Latm).with_bitrate_bps(256_000);
    let pcm = decode_with(
        &encode_with(&[&src], 48_000, &fine).unwrap(),
        &DecodeOptions::unbounded(),
    )
    .unwrap();
    let score = snr(&pcm.channels[0][512..512 + 2048], &src);
    assert!(score >= 20.0, "256k full-window snr {score:.1}");
}

#[test]
fn m4a_trims_priming_to_the_source_length() {
    let src = tone(600, 440.0);
    let m4a = encode_with(&[&src], 48_000, &opts(EncodeContainer::M4a)).unwrap();
    let pcm = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(pcm.channels[0].len(), 600);
    assert_eq!(pcm.priming, Some(512));
    assert_eq!(probe(&m4a).unwrap().profile, ProbeProfile::Ld);
}

#[test]
fn push_matches_one_shot_at_any_chunking() {
    let src = tone(1500, 440.0);
    let opt = opts(EncodeContainer::Latm);
    let one = encode_with(&[&src], 48_000, &opt).unwrap();
    let mut enc = Encoder::new(48_000, 1, &opt).unwrap();
    let mut chunked = Vec::new();
    for chunk in src.chunks(100) {
        enc.feed(&[chunk], |f| {
            chunked.extend_from_slice(f.au);
            Ok(())
        })
        .unwrap();
    }
    let info = enc
        .finish(|f| {
            chunked.extend_from_slice(f.au);
            Ok(())
        })
        .unwrap();
    assert_eq!(chunked, one);
    assert_eq!(info.priming, 512);
    assert_eq!(info.samples, 1500);
    assert_eq!(info.remainder, 512 - (1500 % 512));
}

#[test]
fn stereo_loas_keeps_both_planes() {
    let n = 2048;
    let left = tone(n, 440.0);
    let right = tone(n, 660.0);
    let opt = opts(EncodeContainer::Latm).with_bitrate_bps(128_000);
    let loas = encode_with(&[&left, &right], 48_000, &opt).unwrap();
    let pcm = decode_with(&loas, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(pcm.channels.len(), 2);
    assert!(snr(&pcm.channels[0][512..2048], &left[..1536]) >= 20.0);
    assert!(snr(&pcm.channels[1][512..2048], &right[..1536]) >= 20.0);
}

#[test]
fn adts_he_lookahead_and_surround_are_rejected() {
    let pcm = vec![0.2f32; 512];
    let adts = EncodeOptions::adts().with_ld(true);
    assert!(encode_with(&[&pcm[..]], 48_000, &adts).is_err());
    let he = opts(EncodeContainer::Latm).with_he(true);
    assert!(encode_with(&[&pcm[..]], 48_000, &he).is_err());
    let look = opts(EncodeContainer::Latm).with_lookahead(true);
    assert!(encode_with(&[&pcm[..]], 48_000, &look).is_err());
    let q = opts(EncodeContainer::Latm).with_quality(5);
    assert!(encode_with(&[&pcm[..]], 48_000, &q).is_err());
    let planes = [pcm.clone(), pcm.clone(), pcm];
    let refs: Vec<&[f32]> = planes.iter().map(Vec::as_slice).collect();
    assert!(encode_with(&refs, 48_000, &opts(EncodeContainer::Latm)).is_err());
}
