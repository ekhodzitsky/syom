//! TASK-134: coarse measurement bytes match `with_he_v2`; fine IID decodes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::encode_he_v2_adts;
use crate::{DecodeOptions, EncodeOptions, decode_with, encode_with};

fn pan() -> (Vec<f32>, Vec<f32>) {
    let n = 8192usize;
    let l: Vec<f32> = (0..n).map(|i| 0.25 * (i as f32 * 0.02).sin()).collect();
    let r: Vec<f32> = l.iter().map(|x| 0.3 * x).collect();
    (l, r)
}

#[test]
fn coarse_measure_path_matches_with_he_v2() {
    let (l, r) = pan();
    let opts = EncodeOptions::adts()
        .with_bitrate_bps(32_000)
        .with_he_v2(true);
    let a = encode_with(&[l.clone(), r.clone()], 48_000, &opts).unwrap();
    let (b, ps, ext) = encode_he_v2_adts(&[l, r], 48_000, 32_000, false).unwrap();
    assert_eq!(a, b, "mode 1 measurement path drifted from with_he_v2");
    assert!(ps > 0 && ext >= ps, "ps {ps} ext {ext}");
}

#[test]
fn fine_iid_stream_decodes_to_stereo_and_is_not_mode_1() {
    let (l, r) = pan();
    let (fine, ps, ext) = encode_he_v2_adts(&[l.clone(), r.clone()], 48_000, 32_000, true).unwrap();
    let (coarse, _, _) = encode_he_v2_adts(&[l, r], 48_000, 32_000, false).unwrap();
    assert_ne!(fine, coarse, "fine grid must change the payload");
    assert!(ps > 0 && ext >= ps, "ps {ps} ext {ext}");
    let dec = decode_with(&fine, &DecodeOptions::unbounded()).unwrap();
    assert_eq!((dec.channels.len(), dec.sample_rate), (2, 48_000));
    assert_eq!(dec.channels[0].len(), 8192);
    assert_eq!(dec.priming, Some(3018));
}
