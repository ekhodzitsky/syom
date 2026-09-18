//! TASK-55: one-shot encode over borrowed planes — no copy to satisfy the
//! input type, same bytes and errors as owned planes and the push path.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AacError, EncodeOptions, Encoder, PcmReject, encode, encode_with};

fn tone(n: usize, hz: f32) -> Vec<f32> {
    (0..n)
        .map(|i| 0.3 * (2.0 * std::f32::consts::PI * hz * i as f32 / 48_000.0).sin())
        .collect()
}

#[test]
fn borrowed_slices_arrays_and_owned_planes_encode_identically() {
    let l = tone(5000, 440.0);
    let r = tone(5000, 660.0);
    let owned = vec![l.clone(), r.clone()];
    let a = encode(&owned, 48_000).unwrap();
    let b = encode(&[&l[..], &r[..]], 48_000).unwrap();
    let c = encode(&[l.as_slice(), r.as_slice()], 48_000).unwrap();
    let boxed: Vec<Box<[f32]>> = vec![l.clone().into_boxed_slice(), r.clone().into_boxed_slice()];
    let d = encode(&boxed, 48_000).unwrap();
    assert!(a == b && b == c && c == d);
    // A sub-slice of a longer buffer: no allocation to select the window.
    let long = tone(20_000, 440.0);
    let window = encode(&[&long[3_000..8_000]], 48_000).unwrap();
    assert_eq!(
        window,
        encode(&[long[3_000..8_000].to_vec()], 48_000).unwrap()
    );
    // Lookahead / HE / quality flow through unchanged.
    for opts in [
        EncodeOptions::adts().with_lookahead(true),
        EncodeOptions::low_rate(),
        EncodeOptions::adts().with_quality(6),
    ] {
        assert_eq!(
            encode_with(&[&l[..], &r[..]], 48_000, &opts).unwrap(),
            encode_with(&owned, 48_000, &opts).unwrap()
        );
    }
}

#[test]
fn borrowed_input_errors_match_the_streaming_contract() {
    let l = tone(3000, 440.0);
    let short = tone(2000, 440.0);
    let e = encode(&[&l[..], &short[..]], 48_000).unwrap_err();
    assert!(
        matches!(e, AacError::InvalidPcm(PcmReject::PlaneLength)),
        "{e}"
    );
    let e = encode(&[&l[..0]], 48_000).unwrap_err();
    assert!(matches!(e, AacError::InvalidPcm(PcmReject::Empty)), "{e}");
    let loud = [1.5f32; 100];
    let e = encode(&[&loud[..]], 48_000).unwrap_err();
    assert!(
        matches!(e, AacError::InvalidPcm(PcmReject::Amplitude)),
        "{e}"
    );
    let e = encode(&[&l[..]; 7], 48_000).unwrap_err(); // 7 planes: no layout
    assert!(matches!(e, AacError::Encode(_)), "{e}");
    let mut push = Encoder::new(48_000, 2, &EncodeOptions::adts()).unwrap();
    let e = push.feed(&[&l[..], &short[..]], |_| Ok(())).unwrap_err();
    assert!(
        matches!(e, AacError::InvalidPcm(PcmReject::PlaneLength)),
        "push: {e}"
    );
}
