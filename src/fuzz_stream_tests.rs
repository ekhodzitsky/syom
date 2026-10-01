//! TASK-49: shipped Decoder/Encoder lifecycle smoke (no fuzzer spawn).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AacError, DecodeOptions, Decoder, EncodeOptions, Encoder, decode_with, encode_with};

const SINE48: &[u8] = include_bytes!("goldens/sine48.adts");
const HE48: &[u8] = include_bytes!("goldens/he48.adts");
const LATM48: &[u8] = include_bytes!("goldens/latm48.latm");
const SCRIPT: &str = include_str!("../corpus/fuzz/lifecycle.txt");

fn push_decode(
    data: &[u8],
    opts: &DecodeOptions,
    chunk: usize,
) -> Result<crate::DecodedAac, AacError> {
    let mut dec = Decoder::new(opts.clone());
    let mut tracks: Vec<Vec<f32>> = Vec::new();
    let mut cb = |f: crate::Frame<'_>| {
        if tracks.is_empty() {
            tracks.resize_with(f.planar.len(), Vec::new);
        }
        for (dst, src) in tracks.iter_mut().zip(f.planar.iter()) {
            assert!(src.iter().all(|x| x.is_finite()), "non-finite decode PCM");
            dst.extend_from_slice(src);
        }
        Ok(())
    };
    if chunk == 0 {
        dec.feed(data, &mut cb)?;
    } else {
        for piece in data.chunks(chunk) {
            dec.feed(piece, &mut cb)?;
        }
    }
    let info = dec.finish(&mut cb)?;
    assert!(!dec.is_failed());
    assert!(dec.is_finished());
    assert_eq!(info.samples, tracks.first().map_or(0, Vec::len) as u64);
    Ok(crate::DecodedAac::from_info(info, tracks))
}

fn push_encode(pcm: &[Vec<f32>], chunk: usize) -> Result<Vec<u8>, AacError> {
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
    assert!(enc.is_finished());
    Ok(out)
}

fn sine(n: usize) -> Vec<f32> {
    (0..n).map(|i| 0.25 * (i as f32 * 0.02).sin()).collect()
}

#[test]
fn chunked_decode_matches_oneshot_on_lc_he_latm() {
    let opts = DecodeOptions::unbounded();
    for (name, bytes) in [("sine48", SINE48), ("he48", HE48), ("latm48", LATM48)] {
        let one = decode_with(bytes, &opts).unwrap_or_else(|e| panic!("{name}: {e}"));
        for chunk in [1usize, 7, 13, 64, 0] {
            let got = push_decode(bytes, &opts, chunk).unwrap_or_else(|e| {
                panic!("{name} chunk {chunk}: {e}");
            });
            assert_eq!(got.sample_rate, one.sample_rate, "{name} {chunk} rate");
            assert_eq!(got.channels, one.channels, "{name} {chunk} pcm");
        }
    }
}

#[test]
fn chunked_encode_matches_oneshot_short_tail() {
    // 3000 is not a multiple of 1024: remainder + overlap drain.
    let pcm = vec![sine(3000)];
    let one = encode_with(&pcm, 48_000, &EncodeOptions::adts()).expect("oneshot");
    for chunk in [1usize, 77, 777, 2048] {
        let got = push_encode(&pcm, chunk).unwrap_or_else(|e| panic!("chunk {chunk}: {e}"));
        assert_eq!(
            got.as_slice(),
            crate::gapless::strip_id3(&one),
            "chunk {chunk} bytes"
        );
    }
    let dec = decode_with(&one, &DecodeOptions::unbounded()).expect("decode");
    assert!(dec.channels[0].iter().all(|x| x.is_finite()));
}

#[test]
fn callback_fail_is_failed_until_reset() {
    let mut dec = Decoder::new(DecodeOptions::speech());
    let err = dec
        .feed(SINE48, |_| Err(AacError::decode("boom")))
        .expect_err("cb fail");
    assert!(matches!(err, AacError::Decode(_)));
    assert!(dec.is_failed());
    let sticky = dec.feed(SINE48, |_| Ok(())).expect_err("sticky");
    assert!(sticky.to_string().contains("reset"), "{sticky}");
    assert!(dec.finish(|_| Ok(())).is_err());
    dec.reset();
    assert!(!dec.is_failed());
    let pcm = push_decode(SINE48, &DecodeOptions::speech(), 9).expect("after reset");
    assert!(pcm.channels[0].iter().all(|x| x.is_finite()));
}

#[test]
fn corrupt_midstream_fails_without_nan() {
    let mut bad = SINE48.to_vec();
    let (h, off) = crate::engine::adts::AdtsHeader::parse(&bad).expect("hdr");
    let fl = usize::from(h.aac_frame_length);
    let poke = (off + fl + 10).min(bad.len() - 1);
    bad[poke] ^= 0xFF;
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut finite = true;
    let r = dec.feed(&bad, |f| {
        finite &= f.planar.iter().all(|p| p.iter().all(|x| x.is_finite()));
        Ok(())
    });
    let r = r.and_then(|_| dec.finish(|_| Ok(())));
    assert!(r.is_err(), "corrupt stream must error");
    assert!(dec.is_failed());
    assert!(finite, "emitted PCM must stay finite");
    assert!(dec.feed(SINE48, |_| Ok(())).is_err());
    dec.reset();
    assert!(!dec.is_failed());
}

#[test]
fn reset_then_he_is_a_new_session() {
    let mut dec = Decoder::new(DecodeOptions::unbounded());
    dec.feed(SINE48, |_| Ok(())).expect("lc");
    dec.finish(|_| Ok(())).expect("lc finish");
    assert!(dec.is_finished());
    dec.reset();
    dec.feed(HE48, |_| Ok(())).expect("he after reset");
    let info = dec.finish(|_| Ok(())).expect("he finish");
    assert_eq!(info.sample_rate, 48_000);
    assert!(info.samples > 0);
}

#[test]
fn encoder_invalid_pcm_is_encode_error_not_roundtrip() {
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts()).expect("enc");
    for bad in [f32::NAN, f32::INFINITY, 1.0 + f32::EPSILON, -2.0] {
        let plane = [bad];
        let e = enc.feed(&[&plane], |_| Ok(())).expect_err("invalid");
        assert!(
            matches!(e, AacError::InvalidPcm(_)),
            "oracle is InvalidPcm, got {e:?} for {bad:?}"
        );
        assert!(enc.is_failed());
        assert!(enc.feed(&[&[0.0][..]], |_| Ok(())).is_err());
        enc.reset().expect("reset");
    }
    let pcm = vec![sine(512)];
    let bytes = push_encode(&pcm, 64).expect("valid after reset");
    assert!(!bytes.is_empty());
}

#[test]
fn encoder_callback_fail_is_failed_until_reset() {
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts()).expect("enc");
    let plane = sine(2048);
    let e = enc
        .feed(&[&plane], |_| Err(AacError::encode("sink")))
        .expect_err("cb");
    assert!(matches!(e, AacError::Encode(_)));
    assert!(enc.is_failed());
    enc.reset().expect("reset");
    assert!(!enc.is_failed());
}

#[test]
fn replay_committed_lifecycle_script() {
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut enc: Option<Encoder> = None;
    let mut src = SINE48;
    for raw in SCRIPT.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut it = line.split_whitespace();
        let op = it.next().expect("op");
        match op {
            "DNEW" => dec = Decoder::new(DecodeOptions::speech()),
            "DRESET" => dec.reset(),
            "DHE" => src = HE48,
            "DFEED" => {
                let n: usize = it.next().unwrap_or("0").parse().unwrap();
                let chunk = if n == 0 { src.len().max(1) } else { n };
                for piece in src.chunks(chunk) {
                    let _ = dec.feed(piece, |_| Ok(()));
                    if dec.is_failed() {
                        break;
                    }
                }
            }
            "DFINISH" => {
                let _ = dec.finish(|_| Ok(()));
            }
            "DCBFAIL" => {
                let _ = dec.feed(src, |_| Err(AacError::decode("script")));
                assert!(dec.is_failed());
            }
            "ENEW" => {
                let rate: u32 = it.next().unwrap().parse().unwrap();
                let ch: usize = it.next().unwrap().parse().unwrap();
                enc = Some(Encoder::new(rate, ch, &EncodeOptions::adts()).expect("ENEW"));
            }
            "ERESET" => {
                enc.as_mut().expect("enc").reset().expect("ERESET");
            }
            "EFEED" => {
                let n: usize = it.next().unwrap().parse().unwrap();
                let p = sine(n);
                enc.as_mut()
                    .expect("enc")
                    .feed(&[&p], |_| Ok(()))
                    .expect("EFEED");
            }
            "EFINISH" => {
                enc.as_mut()
                    .expect("enc")
                    .finish(|_| Ok(()))
                    .expect("EFINISH");
            }
            "EFEEDNAN" => {
                let p = [f32::NAN];
                assert!(enc.as_mut().expect("enc").feed(&[&p], |_| Ok(())).is_err());
            }
            "EFEEDOVER" => {
                let p = [2.0f32];
                assert!(enc.as_mut().expect("enc").feed(&[&p], |_| Ok(())).is_err());
            }
            other => panic!("unknown op {other}"),
        }
    }
}
