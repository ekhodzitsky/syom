//! TASK-52: ASC + raw access-unit decode vs framed goldens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::adts::AdtsHeader;
use crate::engine::asc::write_lc;
use crate::{
    AacError, DecodeOptions, Decoder, LifecycleState, Result, StreamInfo, UnsupportedFeature,
    decode_with,
};

const SINE48: &[u8] = include_bytes!("goldens/sine48.adts");
const HE48: &[u8] = include_bytes!("goldens/he48.adts");
const HE48_LATM: &[u8] = include_bytes!("goldens/he48.latm");
const HE48_M4A: &[u8] = include_bytes!("goldens/he48.m4a");
const PS48: &[u8] = include_bytes!("goldens/ps48.adts");
const PS48_M4A: &[u8] = include_bytes!("goldens/ps48.m4a");

fn adts_payloads(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let Ok((hdr, off)) = AdtsHeader::parse(&bytes[pos..]) else {
            break;
        };
        let fl = usize::from(hdr.aac_frame_length);
        if fl < off || pos + fl > bytes.len() {
            break;
        }
        out.push(&bytes[pos + off..pos + fl]);
        pos += fl;
    }
    out
}

fn collect_framed(bytes: &[u8], opts: &DecodeOptions) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let info = crate::decode_streaming(bytes, opts, |f| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    })?;
    Ok((info, planes))
}

fn collect_au(
    asc: &[u8],
    aus: &[&[u8]],
    opts: DecodeOptions,
) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut dec = Decoder::from_asc(asc, opts)?;
    let mut planes: Vec<Vec<f32>> = Vec::new();
    for au in aus {
        dec.decode_au(au, |f| {
            if planes.is_empty() {
                planes = f.planar.iter().map(|p| p.to_vec()).collect();
            } else {
                for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                    dst.extend_from_slice(src);
                }
            }
            Ok(())
        })?;
    }
    let info = dec.finish(|_| Ok(()))?;
    Ok((info, planes))
}

fn m4a_asc(data: &[u8]) -> Vec<u8> {
    crate::isomp4::parse_aac_track(data).expect("m4a").asc
}

fn assert_same(a: &(StreamInfo, Vec<Vec<f32>>), b: &(StreamInfo, Vec<Vec<f32>>), tag: &str) {
    assert_eq!(a.0.sample_rate, b.0.sample_rate, "{tag} rate");
    assert_eq!(a.0.channels, b.0.channels, "{tag} ch");
    assert_eq!(a.0.samples, b.0.samples, "{tag} samples");
    assert_eq!(a.0.aac_frames, b.0.aac_frames, "{tag} frames");
    assert_eq!(a.1, b.1, "{tag} pcm");
}

#[test]
fn lc_raw_au_matches_adts() -> Result<()> {
    let opts = DecodeOptions::speech();
    let framed = collect_framed(SINE48, &opts)?;
    let aus = adts_payloads(SINE48);
    assert!(!aus.is_empty());
    let raw = collect_au(&write_lc(3, 1), &aus, opts)?;
    assert_same(&framed, &raw, "sine48");
    Ok(())
}

#[test]
fn he_raw_au_matches_adts_and_latm() -> Result<()> {
    let opts = DecodeOptions::audio();
    let adts = collect_framed(HE48, &opts)?;
    let latm = collect_framed(HE48_LATM, &opts)?;
    assert_same(&adts, &latm, "he48 adts/latm");
    let aus = adts_payloads(HE48);
    let raw = collect_au(&m4a_asc(HE48_M4A), &aus, opts)?;
    assert_same(&adts, &raw, "he48 au");
    Ok(())
}

#[test]
fn ps_raw_au_matches_adts() -> Result<()> {
    let opts = DecodeOptions::audio();
    let framed = collect_framed(PS48, &opts)?;
    let aus = adts_payloads(PS48);
    let raw = collect_au(&m4a_asc(PS48_M4A), &aus, opts)?;
    assert_same(&framed, &raw, "ps48");
    Ok(())
}

#[test]
fn m4a_samples_plus_asc_match_adts() -> Result<()> {
    let track = crate::isomp4::parse_aac_track(HE48_M4A)?;
    let aus: Vec<&[u8]> = track
        .frames
        .iter()
        .map(|&(off, len)| {
            let o = off as usize;
            &HE48_M4A[o..o + len as usize]
        })
        .collect();
    let opts = DecodeOptions::audio();
    let framed = collect_framed(HE48, &opts)?;
    let raw = collect_au(&track.asc, &aus, opts)?;
    assert_same(&framed, &raw, "he48 m4a samples");
    Ok(())
}

#[test]
fn reset_clears_then_he_is_a_new_session() -> Result<()> {
    let mut dec = Decoder::from_asc(&write_lc(3, 1), DecodeOptions::speech())?;
    let au = adts_payloads(SINE48)[0];
    dec.decode_au(au, |_| Ok(()))?;
    dec.reset();
    assert!(!dec.is_failed());
    assert!(!dec.is_finished());
    let he = m4a_asc(HE48_M4A);
    dec.set_asc(&he)?;
    let hu = adts_payloads(HE48)[0];
    dec.decode_au(hu, |_| Ok(()))?;
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.sample_rate, 48_000);
    Ok(())
}

#[test]
fn truncated_unsupported_change_callback_limit() {
    let opts = DecodeOptions::speech();
    match Decoder::from_asc(&[], opts.clone()) {
        Err(AacError::Truncated { .. }) => {}
        Err(e) => panic!("empty ASC: {e:?}"),
        Ok(_) => panic!("empty ASC must error"),
    }
    match Decoder::from_asc(&[0x09, 0x88], opts.clone()) {
        Err(AacError::Unsupported(UnsupportedFeature::AudioObjectType(1))) => {}
        Err(e) => panic!("Main AOT: {e:?}"),
        Ok(_) => panic!("Main AOT must error"),
    }
    let mut dec = Decoder::from_asc(&write_lc(3, 1), opts.clone()).unwrap();
    let au = adts_payloads(SINE48)[0];
    dec.decode_au(au, |_| Ok(())).unwrap();
    let he = m4a_asc(HE48_M4A);
    let e = dec.set_asc(&he).unwrap_err();
    assert!(matches!(
        e,
        AacError::Unsupported(UnsupportedFeature::AscChange)
    ));
    dec.set_asc(&write_lc(3, 1)).unwrap();
    let e = dec
        .decode_au(au, |_| Err(AacError::decode("sink")))
        .unwrap_err();
    assert!(matches!(e, AacError::Decode(_)));
    assert!(dec.is_failed());
    assert!(matches!(
        dec.decode_au(au, |_| Ok(())).unwrap_err(),
        AacError::Lifecycle {
            state: LifecycleState::Failed
        }
    ));
    let tiny = DecodeOptions::speech().with_memory(crate::MemoryBudgets {
        max_declared_au_bytes: 4,
        ..crate::MemoryBudgets::default()
    });
    let mut d2 = Decoder::from_asc(&write_lc(3, 1), tiny).unwrap();
    let e = d2.decode_au(au, |_| Ok(())).unwrap_err();
    assert!(matches!(e, AacError::Limit { .. }));
    let mut d3 = Decoder::from_asc(&write_lc(3, 1), opts).unwrap();
    assert!(matches!(
        d3.decode_au(&[], |_| Ok(())).unwrap_err(),
        AacError::Truncated { .. }
    ));
}

#[test]
fn feed_and_decode_au_do_not_mix() {
    let mut au = Decoder::from_asc(&write_lc(3, 1), DecodeOptions::speech()).unwrap();
    assert!(matches!(
        au.feed(SINE48, |_| Ok(())).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::RawAccessUnit)
    ));
    let mut framed = Decoder::new(DecodeOptions::speech());
    framed.feed(SINE48, |_| Ok(())).unwrap();
    let payload = adts_payloads(SINE48)[0];
    assert!(matches!(
        framed.decode_au(payload, |_| Ok(())).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::RawAccessUnit)
    ));
}

#[test]
fn empty_au_session_is_not_aac_at_finish() {
    let mut dec = Decoder::from_asc(&write_lc(3, 1), DecodeOptions::speech()).unwrap();
    assert!(matches!(
        dec.finish(|_| Ok(())).unwrap_err(),
        AacError::NotAac
    ));
}

#[test]
fn one_shot_still_speech_mono() -> Result<()> {
    let pcm = decode_with(SINE48, &DecodeOptions::speech())?;
    assert_eq!(pcm.channels.len(), 1);
    Ok(())
}
