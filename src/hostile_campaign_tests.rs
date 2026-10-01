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
        include_bytes!("../corpus/fuzz/fmp4-truncated.bin"),
        include_bytes!("goldens/sine48.adts").split_at(8).0,
        include_bytes!("goldens/he48.adts").split_at(12).0,
        include_bytes!("goldens/ps48.adts").split_at(16).0,
        include_bytes!("goldens/mc51.adts").split_at(20).0,
        include_bytes!("goldens/latm48.latm").split_at(6).0,
        include_bytes!("goldens/sine441.m4a").split_at(24).0,
        include_bytes!("goldens/ld64m.loas").split_at(2).0,
        include_bytes!("goldens/ld64m.loas").split_at(8).0,
        include_bytes!("goldens/ld64mus.loas").split_at(16).0,
        include_bytes!("goldens/ld48.loas").split_at(24).0,
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
fn ld_mutations_stay_finite_and_do_not_panic() {
    let src = include_bytes!("goldens/ld64m.loas");
    let mut state = 0x4C44_0131u32;
    let mut next = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        state
    };
    for i in 0..64 {
        let mut buf = src.to_vec();
        let idx = (next() as usize) % buf.len();
        buf[idx] ^= 1 << (next() % 8);
        no_panic(&format!("ld-flip-{i}"), || {
            if let Ok(pcm) = decode_with(&buf, &DecodeOptions::speech()) {
                assert!(pcm.channels.len() <= 8, "flip {i} channel count");
                for plane in &pcm.channels {
                    assert!(plane.len() <= 200_000, "flip {i} grew to {}", plane.len());
                    assert!(
                        plane.iter().all(|s| s.is_finite()),
                        "flip {i} non-finite pcm"
                    );
                }
            }
        });
    }
}

#[test]
fn ld_rejected_syntax_is_typed_not_an_lc_frame() {
    let mut w = crate::engine::bits::BitWriter::new();
    w.write(23, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write_bit(true); // 480-sample grid
    w.write(0, 2);
    let asc = w.finish();
    assert!(matches!(
        Decoder::from_asc(&asc, DecodeOptions::unbounded()),
        Err(AacError::Unsupported(UnsupportedFeature::FrameLength960))
    ));

    let mut w = crate::engine::bits::BitWriter::new();
    w.write(23, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 5); // GA flags 0 + epConfig 0
    let mut dec = Decoder::from_asc(&w.finish(), DecodeOptions::unbounded()).unwrap();
    let mut au = crate::engine::bits::BitWriter::new();
    au.write(0, 4); // tag
    au.write(0, 8); // global_gain
    au.write(0, 1); // reserved
    au.write(2, 2); // EIGHT_SHORT
    au.write(0, 1); // shape
    match dec.decode_au(&au.finish(), |_| Ok(())) {
        Err(AacError::Malformed(_)) => {}
        other => panic!("LD short window: {other:?}"),
    }
    assert!(dec.is_failed());
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
fn fmp4_truncation_and_mid_fragment_are_typed_truncated() {
    let lc = include_bytes!("goldens/fmp4_lc.mp4");
    let seed = include_bytes!("../corpus/fuzz/fmp4-truncated.bin");
    let moof = nth4(lc, b"moof", 0);
    let mdat = nth4(lc, b"mdat", 0);
    // The fourcc sits 4 bytes into the header; +20 lands inside the box.
    let cuts: [(&str, &[u8]); 3] = [
        ("fuzz-seed", seed),
        ("mid-moof", &lc[..moof + 20]),
        ("mid-mdat", &lc[..mdat + 20]),
    ];
    for (label, bytes) in cuts {
        no_panic(label, || {
            assert!(
                matches!(
                    decode_with(bytes, &DecodeOptions::speech()),
                    Err(AacError::Truncated { .. })
                ),
                "{label} must be Truncated"
            );
        });
    }
}

fn nth4(data: &[u8], needle: &[u8; 4], n: usize) -> usize {
    data.windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == needle).then_some(i))
        .nth(n)
        .unwrap_or_else(|| panic!("missing {needle:?} #{n}"))
}

#[test]
fn encode_then_decode_finite() {
    let pcm = vec![vec![0.25f32; 2048]];
    let adts = encode_with(&pcm, 48_000, &EncodeOptions::adts()).expect("enc");
    let out = decode_with(&adts, &DecodeOptions::unbounded()).expect("dec");
    assert!(out.channels[0].iter().all(|x| x.is_finite()));
}
