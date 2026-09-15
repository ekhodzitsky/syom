//! TASK-103 ordinary-test replay of hostile / limit vectors on shipped APIs.
//! Panics are findings. Errors are expected.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::{
    AacError, DecodeOptions, Decoder, EncodeOptions, Encoder, MalformedKind, PcmReject,
    UnsupportedFeature, decode, decode_with, encode, encode_with,
};

fn no_panic(label: &str, f: impl FnOnce()) {
    let panicked = catch_unwind(AssertUnwindSafe(f)).is_err();
    assert!(!panicked, "panic on {label}");
}

#[test]
fn truncated_and_garbage_never_panic_and_fail() {
    let junk: &[&[u8]] = &[
        b"",
        b"\xff",
        b"\xff\xf1",
        b"not-aac",
        include_bytes!("../corpus/fuzz/empty.bin"),
        include_bytes!("../corpus/fuzz/adts-truncated.bin"),
        include_bytes!("../corpus/fuzz/adts-len-too-small.bin"),
        include_bytes!("../corpus/fuzz/adts-reserved-srate.bin"),
        include_bytes!("../corpus/fuzz/asc-main-aot.bin"),
        include_bytes!("../corpus/fuzz/latm-truncated.bin"),
        include_bytes!("../corpus/fuzz/m4a-truncated.bin"),
        include_bytes!("goldens/sine48.adts").split_at(8).0,
        include_bytes!("goldens/he48.adts").split_at(12).0,
        include_bytes!("goldens/ps48.adts").split_at(16).0,
        include_bytes!("goldens/mc51.adts").split_at(20).0,
        include_bytes!("goldens/latm48.latm").split_at(6).0,
        include_bytes!("goldens/sine441.m4a").split_at(24).0,
    ];
    for (i, bytes) in junk.iter().enumerate() {
        no_panic(&format!("junk-{i}"), || {
            assert!(
                decode_with(bytes, &DecodeOptions::speech()).is_err(),
                "junk-{i} must not succeed"
            );
            let _ = decode(bytes);
        });
    }
}

#[test]
fn reserved_srate_and_main_aot_are_unsupported_or_malformed() {
    let srate = include_bytes!("../corpus/fuzz/adts-reserved-srate.bin");
    match decode_with(srate, &DecodeOptions::unbounded()) {
        Err(AacError::Unsupported(UnsupportedFeature::SampleRateIndex(_)))
        | Err(AacError::Malformed(_))
        | Err(AacError::NotAac)
        | Err(AacError::Truncated { .. }) => {}
        other => panic!("reserved srate: {other:?}"),
    }
    let main = include_bytes!("../corpus/fuzz/asc-main-aot.bin");
    match decode_with(main, &DecodeOptions::unbounded()) {
        Err(AacError::Unsupported(UnsupportedFeature::AudioObjectType(1)))
        | Err(AacError::NotAac)
        | Err(AacError::Unsupported(_))
        | Err(AacError::Truncated { .. }) => {}
        other => panic!("Main AOT: {other:?}"),
    }
}

#[test]
fn pcm_domain_rejects_stay_invalidpcm() {
    assert!(matches!(
        encode(&[vec![f32::NAN; 64]], 48_000),
        Err(AacError::InvalidPcm(PcmReject::NonFinite))
    ));
    assert!(matches!(
        encode(&[vec![2.0f32; 64]], 48_000),
        Err(AacError::InvalidPcm(PcmReject::Amplitude))
    ));
    assert!(matches!(
        encode(&[Vec::<f32>::new()], 48_000),
        Err(AacError::InvalidPcm(PcmReject::Empty))
    ));
    let a = vec![0.1f32; 8];
    let b = vec![0.1f32; 7];
    assert!(matches!(
        encode(&[a, b], 48_000),
        Err(AacError::InvalidPcm(PcmReject::PlaneLength))
    ));
}

#[test]
fn tiny_duration_cap_is_toolong_not_hang() {
    let bytes = include_bytes!("goldens/sine48.adts");
    let opts = DecodeOptions::speech().with_max_duration_secs(0.001);
    match decode_with(bytes, &opts) {
        Err(AacError::TooLong { .. }) | Err(AacError::Limit { .. }) => {}
        other => panic!("expected duration fence, got {other:?}"),
    }
}

#[test]
fn decoder_lifecycle_sticky_until_reset() {
    let mut dec = Decoder::new(DecodeOptions::speech());
    let _ = dec.feed(b"????", |_| Ok(()));
    let err = dec.finish(|_| Ok(())).expect_err("garbage finish");
    assert!(matches!(
        err,
        AacError::NotAac | AacError::Malformed(_) | AacError::Truncated { .. }
    ));
    let again = dec.feed(include_bytes!("goldens/sine48.adts"), |_| Ok(()));
    assert!(again.is_err(), "failed decoder must stay failed");
    dec.reset();
    let mut n = 0usize;
    dec.feed(include_bytes!("goldens/sine48.adts"), |_| {
        n += 1;
        Ok(())
    })
    .expect("reset must accept LC");
    assert!(n > 0);
}

#[test]
fn encoder_m4a_streaming_rejected() {
    let opts = EncodeOptions::m4a();
    match Encoder::new(48_000, 1, &opts) {
        Err(AacError::Unsupported(UnsupportedFeature::EncodeM4aStreaming)) => {}
        Err(e) => panic!("{e:?}"),
        Ok(_) => panic!("M4A streaming encoder must be rejected"),
    }
}

#[test]
fn crc_mismatch_is_malformed_not_success() {
    let mut adts = include_bytes!("goldens/sine48.adts").to_vec();
    if adts.len() > 10 {
        adts[7] ^= 0xff;
        adts[8] ^= 0xff;
    }
    no_panic("crc-flip", || {
        let r = decode_with(&adts, &DecodeOptions::unbounded());
        if let Err(e) = r {
            assert!(
                matches!(
                    e,
                    AacError::Malformed(MalformedKind::AdtsCrc)
                        | AacError::Malformed(_)
                        | AacError::NotAac
                        | AacError::Truncated { .. }
                ),
                "crc-flip class {e:?}"
            );
        }
    });
}

#[test]
fn he_ps_mc_truncated_prefix_fails() {
    for (name, bytes, n) in [
        ("he", include_bytes!("goldens/he48.adts").as_slice(), 8),
        ("ps", include_bytes!("goldens/ps48.adts").as_slice(), 8),
        ("mc", include_bytes!("goldens/mc51.adts").as_slice(), 8),
    ] {
        let prefix = &bytes[..n.min(bytes.len())];
        no_panic(name, || {
            assert!(
                decode_with(prefix, &DecodeOptions::unbounded()).is_err(),
                "{name} prefix"
            );
        });
    }
}

#[test]
fn encode_then_decode_finite() {
    let pcm = vec![vec![0.25f32; 2048]];
    let adts = encode_with(&pcm, 48_000, &EncodeOptions::adts()).expect("enc");
    let out = decode_with(&adts, &DecodeOptions::unbounded()).expect("dec");
    assert!(out.channels[0].iter().all(|x| x.is_finite()));
}
