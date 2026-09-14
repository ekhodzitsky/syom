//! ASC-seeded SBR/PS rate: 1× vs 2× without waiting on FIL.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::adts::AdtsHeader;
use super::asc::AudioSpecificConfig;
use super::decode::StreamDecoder;
use super::error::{Error, Result};

fn hex_bytes(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn sine_payload() -> (&'static [u8], AdtsHeader, &'static [u8]) {
    let adts = include_bytes!("../goldens/sine48.adts");
    let (hdr, off) = AdtsHeader::parse(adts).unwrap();
    let fl = usize::from(hdr.aac_frame_length);
    (adts, hdr, &adts[off..fl])
}

#[test]
fn asc_implicit_he_seeds_decoder_mode() -> Result<()> {
    let (he, _) = AudioSpecificConfig::parse(&hex_bytes("130856e598"))?;
    assert!(he.sbr_present);
    assert_eq!(he.sample_rate, 24_000);
    assert_eq!(he.output_sample_rate, 48_000);
    let mut dec = StreamDecoder::new();
    dec.set_he_config(he.sbr_present, he.ps_present, he.output_sample_rate);
    assert_eq!(dec.he_config(), (true, false, Some(48_000)));
    Ok(())
}

#[test]
fn asc_downsampled_sbr_is_1x() -> Result<()> {
    let (ds, _) = AudioSpecificConfig::parse(&hex_bytes("118856e598"))?;
    assert!(ds.sbr_present);
    assert_eq!(ds.sample_rate, 48_000);
    assert_eq!(ds.output_sample_rate, 48_000);
    let mut dec = StreamDecoder::new();
    dec.set_he_config(ds.sbr_present, ds.ps_present, ds.output_sample_rate);
    assert_eq!(dec.he_config(), (true, false, Some(48_000)));
    Ok(())
}

#[test]
fn declared_2x_without_fil_upsamples() -> Result<()> {
    let (_, hdr, payload) = sine_payload();
    let mut dec = StreamDecoder::new();
    dec.set_he_config(true, false, 96_000);
    let frame = dec.decode_frame(&hdr, payload)?;
    assert_eq!(frame.sample_rate, 96_000);
    assert_eq!(frame.planar[0].len(), 2048);
    Ok(())
}

#[test]
fn declared_1x_keeps_core_rate_and_length() -> Result<()> {
    let (_, hdr, payload) = sine_payload();
    let mut dec = StreamDecoder::new();
    dec.set_he_config(true, false, 48_000);
    let frame = dec.decode_frame(&hdr, payload)?;
    assert_eq!(frame.sample_rate, 48_000);
    assert_eq!(frame.planar[0].len(), 1024);
    Ok(())
}

#[test]
fn declared_rate_not_1x_or_2x_is_format() {
    let (_, hdr, payload) = sine_payload();
    let mut dec = StreamDecoder::new();
    dec.set_he_config(true, false, 44_100);
    let err = dec.decode_frame(&hdr, payload).expect_err("ratio");
    assert!(matches!(
        err,
        Error::Format("SBR output rate must be 1x or 2x core")
    ));
}

#[test]
fn he48_adts_chunked_matches_oneshot() {
    let bytes = include_bytes!("../goldens/he48.adts");
    let opts = crate::DecodeOptions::unbounded();
    let one = crate::decode_with(bytes, &opts).unwrap();
    let mut dec = crate::Decoder::new(opts);
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut acc = |f: crate::Frame<'_>| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    };
    for chunk in bytes.chunks(13) {
        dec.feed(chunk, &mut acc).unwrap();
    }
    dec.finish(&mut acc).unwrap();
    assert_eq!(one.sample_rate, 48_000);
    assert_eq!(one.channels, planes);
}

#[test]
fn he48_adts_latm_m4a_agree_on_declared_rate() {
    let opts = crate::DecodeOptions::unbounded();
    let adts = crate::decode_with(include_bytes!("../goldens/he48.adts"), &opts).unwrap();
    let latm = crate::decode_with(include_bytes!("../goldens/he48.latm"), &opts).unwrap();
    let m4a = crate::decode_with(include_bytes!("../goldens/he48.m4a"), &opts).unwrap();
    assert_eq!(adts.sample_rate, 48_000);
    assert_eq!(latm.sample_rate, 48_000);
    assert_eq!(m4a.sample_rate, 48_000);
    assert_eq!(adts.channels.len(), 2);
    assert_eq!(latm.channels.len(), 2);
    assert_eq!(m4a.channels.len(), 2);
}

#[test]
fn implicit_ps_asc_seeds_ps_flag() -> Result<()> {
    let (asc, _) = AudioSpecificConfig::parse(&hex_bytes("130856e59d4880"))?;
    assert!(asc.sbr_present && asc.ps_present);
    let mut dec = StreamDecoder::new();
    dec.set_he_config(asc.sbr_present, asc.ps_present, asc.output_sample_rate);
    assert_eq!(dec.he_config(), (true, true, Some(48_000)));
    Ok(())
}
