//! TASK-53: raw AUs + ASC rewrap to ADTS/M4A and decode_au.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, DecodeOptions, Decoder, EncodeOptions, Encoder, UnsupportedFeature,
    encode_with, mux_raw_lc_m4a, wrap_adts_au,
};

fn fixture(n: usize) -> Vec<Vec<f32>> {
    let l: Vec<f32> = (0..n)
        .map(|i| 0.4 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin())
        .collect();
    vec![l]
}

fn collect_raw(
    pcm: &[Vec<f32>],
    opts: &EncodeOptions,
) -> (Vec<Vec<u8>>, Vec<u8>, crate::EncodeInfo) {
    let mut enc = Encoder::new(48_000, pcm.len(), opts).unwrap();
    let mut aus = Vec::new();
    let mut cb = |f: crate::EncodedFrame<'_>| {
        assert_eq!(f.au, f.payload, "raw encoder au is the payload");
        assert!(!f.payload.is_empty());
        aus.push(f.payload.to_vec());
        Ok(())
    };
    let n = pcm[0].len();
    let mut off = 0usize;
    while off < n {
        let end = (off + 777).min(n);
        let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[off..end]).collect();
        enc.feed(&planes, &mut cb).unwrap();
        off = end;
    }
    let info = enc.finish(&mut cb).unwrap();
    let asc = enc.asc().to_vec();
    (aus, asc, info)
}

#[test]
fn raw_rewrap_adts_matches_one_shot_causal_and_lookahead() {
    let pcm = fixture(3000);
    for look in [false, true] {
        let opts = EncodeOptions::raw().with_lookahead(look);
        let (aus, asc, info) = collect_raw(&pcm, &opts);
        assert_eq!(asc, crate::engine::asc::write_lc(3, 1));
        assert_eq!(info.samples, 3000);
        assert_eq!(info.priming, 1024);
        let mut adts = Vec::new();
        for au in &aus {
            adts.extend_from_slice(&wrap_adts_au(au, 48_000, 1).unwrap());
        }
        let want = encode_with(&pcm, 48_000, &EncodeOptions::adts().with_lookahead(look)).unwrap();
        assert_eq!(adts, want, "lookahead={look} rewrap != one-shot ADTS");
        let refs: Vec<&[u8]> = aus.iter().map(Vec::as_slice).collect();
        let m4a = mux_raw_lc_m4a(&refs, 48_000, 1, 3000).unwrap();
        let want_m4a =
            encode_with(&pcm, 48_000, &EncodeOptions::m4a().with_lookahead(look)).unwrap();
        assert_eq!(m4a, want_m4a, "lookahead={look} mux != one-shot M4A");
    }
}

#[test]
fn raw_asc_decode_au_is_finite_and_matches_adts_len() {
    let pcm = fixture(2048);
    let (aus, asc, info) = collect_raw(&pcm, &EncodeOptions::raw());
    let mut dec = Decoder::from_asc(&asc, DecodeOptions::unbounded()).unwrap();
    let mut n = 0usize;
    for au in &aus {
        dec.decode_au(au, |f| {
            assert!(
                f.planar
                    .iter()
                    .flat_map(|p| p.iter())
                    .all(|x| x.is_finite())
            );
            n += f.samples;
            Ok(())
        })
        .unwrap();
    }
    dec.finish(|_| Ok(())).unwrap();
    assert_eq!(n as u64, info.coded_samples);
}

#[test]
fn adts_encoder_payload_is_header_stripped_au() {
    let pcm = fixture(1024);
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts()).unwrap();
    enc.feed(&[&pcm[0]], |f| {
        assert!(f.au.len() >= 7);
        assert_eq!(f.au[0], 0xff);
        assert_eq!(&f.au[7..], f.payload);
        Ok(())
    })
    .unwrap();
    let _ = enc.finish(|f| {
        assert_eq!(&f.au[7..], f.payload);
        Ok(())
    });
}

#[test]
fn raw_one_shot_and_lifecycle() {
    let pcm = fixture(512);
    assert!(matches!(
        encode_with(&pcm, 48_000, &EncodeOptions::raw()),
        Err(AacError::Unsupported(UnsupportedFeature::EncodeRawOneShot))
    ));
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::raw()).unwrap();
    enc.feed(&[&pcm[0]], |_| Ok(())).unwrap();
    enc.finish(|_| Ok(())).unwrap();
    assert!(enc.finish(|_| Ok(())).is_err());
    assert!(enc.feed(&[&pcm[0]], |_| Ok(())).is_err());
    enc.reset().unwrap();
    enc.feed(&[&[f32::NAN]], |_| Ok(())).unwrap_err();
    assert!(enc.is_failed());
}

#[test]
fn encoder_asc_stable_across_reset() {
    let mut enc = Encoder::new(48_000, 2, &EncodeOptions::raw()).unwrap();
    let a = enc.asc().to_vec();
    enc.reset().unwrap();
    assert_eq!(enc.asc(), a.as_slice());
    assert_eq!(a, crate::engine::asc::write_lc(3, 2));
}

#[test]
fn m4a_push_still_rejected() {
    assert!(matches!(
        Encoder::new(48_000, 1, &EncodeOptions::m4a()),
        Err(AacError::Unsupported(
            UnsupportedFeature::EncodeM4aStreaming
        ))
    ));
}
