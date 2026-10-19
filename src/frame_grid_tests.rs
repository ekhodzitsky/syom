//! 960-sample LC and 480-sample AAC-LD grids.
//!
//! Vectors under `lab/frame960/vectors` are the TASK-63 byte strings.
//! This file does not spawn an external decoder.

use crate::engine::asc::AudioSpecificConfig;
use crate::engine::bits::BitWriter;
use crate::engine::error::Error as EngineError;
use crate::{AacError, ChannelMode, DecodeOptions, Decoder, UnsupportedFeature, decode_with};

fn opts() -> DecodeOptions {
    DecodeOptions::speech().with_channel_mode(ChannelMode::Split)
}

fn peak(plane: &[f32]) -> f32 {
    plane.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

fn assert_lc960(planes: &[Vec<f32>], channels: usize) {
    assert_eq!(planes.len(), channels);
    for plane in planes {
        assert_eq!(plane.len(), 7680);
        assert!(plane.iter().all(|s| s.is_finite()));
        let p = peak(plane);
        assert!(p > 0.05 && p < 1.0, "peak {p}");
    }
}

fn decode_aus(asc: &[u8], packed: &[u8], sizes: &[usize]) -> Result<Vec<Vec<f32>>, AacError> {
    let mut dec = Decoder::from_asc(asc, opts())?;
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut pos = 0usize;
    for &n in sizes {
        let end = pos + n;
        let au = packed
            .get(pos..end)
            .ok_or_else(|| AacError::decode("au short"))?;
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
        pos = end;
    }
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(pos, packed.len());
    Ok(planes)
}

#[test]
fn frame_length_960_display_stays_locked() {
    assert_eq!(
        UnsupportedFeature::FrameLength960.to_string(),
        "aac: 960-line frames are Media"
    );
    let err = AacError::from(EngineError::UnsupportedHeFrame960);
    assert!(matches!(
        err,
        AacError::Unsupported(UnsupportedFeature::HeFrame960)
    ));
    assert_ne!(err.to_string(), "aac: 960-line frames are Media");
}

#[test]
fn he_over_960_is_not_the_lc_success_path() -> Result<(), EngineError> {
    let mut explicit = BitWriter::new();
    explicit.write(5, 5);
    explicit.write(3, 4);
    explicit.write(1, 4);
    explicit.write(3, 4);
    explicit.write(2, 5);
    explicit.write_bit(true);
    explicit.write(0, 2);
    match AudioSpecificConfig::parse(&explicit.finish()) {
        Err(EngineError::UnsupportedHeFrame960) => {}
        Err(e) => return Err(e),
        Ok(_) => return Err(EngineError::Format("HE 960 took the LC path")),
    }

    let mut implicit = BitWriter::new();
    implicit.write(2, 5);
    implicit.write(3, 4);
    implicit.write(1, 4);
    implicit.write_bit(true);
    implicit.write(0, 2);
    implicit.write(0x2b7, 11);
    implicit.write(5, 5);
    implicit.write_bit(true);
    implicit.write(3, 4);
    match AudioSpecificConfig::parse(&implicit.finish()) {
        Err(EngineError::UnsupportedHeFrame960) => {}
        Err(e) => return Err(e),
        Ok(_) => return Err(EngineError::Format("implicit HE 960 took the LC path")),
    }

    let (lc, _) = AudioSpecificConfig::parse(&[0x11, 0x88])?;
    assert_eq!(lc.frame_samples, 1024);
    Ok(())
}

#[test]
fn lc960_loas_m4a_and_raw_match_the_lab_geometry() -> Result<(), AacError> {
    let mono = decode_with(
        include_bytes!("../lab/frame960/vectors/lc960-m48.loas"),
        &opts(),
    )?;
    assert_eq!(mono.sample_rate, 48_000);
    assert_lc960(&mono.channels, 1);

    let stereo = decode_with(
        include_bytes!("../lab/frame960/vectors/lc960-s48.loas"),
        &opts(),
    )?;
    assert_eq!(stereo.sample_rate, 48_000);
    assert_lc960(&stereo.channels, 2);

    let m4a = decode_with(
        include_bytes!("../lab/frame960/vectors/lc960-s48.m4a"),
        &opts(),
    )?;
    assert_eq!(m4a.sample_rate, 48_000);
    assert_lc960(&m4a.channels, 2);

    let mono_au = decode_aus(
        include_bytes!("../lab/frame960/vectors/lc960-m48.asc"),
        include_bytes!("../lab/frame960/vectors/lc960-m48.au"),
        &[29, 29, 29, 29, 29, 29, 29, 29],
    )?;
    assert_lc960(&mono_au, 1);

    let stereo_au = decode_aus(
        include_bytes!("../lab/frame960/vectors/lc960-s48.asc"),
        include_bytes!("../lab/frame960/vectors/lc960-s48.au"),
        &[54, 54, 54, 25, 54, 54, 54, 54],
    )?;
    assert_lc960(&stereo_au, 2);
    Ok(())
}

fn ld480_tone_au() -> Vec<u8> {
    let mut au = BitWriter::new();
    au.write(0, 4);
    au.write(100, 8);
    au.write(0, 1);
    au.write(0, 2);
    au.write(0, 1);
    au.write(1, 6);
    au.write(0, 1);
    au.write(1, 4);
    au.write(1, 5);
    au.write(0, 1);
    au.write(0, 1);
    au.write(0, 1);
    au.write(0, 1);
    au.write(0b10000, 5);
    au.finish()
}

#[test]
fn aac_ld_480_decodes_one_frame_and_is_not_a_960_error() -> Result<(), AacError> {
    let mut asc = BitWriter::new();
    asc.write(23, 5);
    asc.write(3, 4);
    asc.write(1, 4);
    asc.write_bit(true);
    asc.write(0, 4);
    let mut dec = Decoder::from_asc(&asc.finish(), DecodeOptions::speech())?;
    let mut n = 0usize;
    let mut energy = 0.0f32;
    dec.decode_au(&ld480_tone_au(), |f| {
        n = f.samples;
        assert_eq!(f.sample_rate, 48_000);
        assert_eq!(f.planar.len(), 1);
        for s in f.planar[0] {
            assert!(s.is_finite());
            energy += s.abs();
        }
        Ok(())
    })?;
    assert_eq!(n, 480);
    assert!(energy > 0.0, "480-sample IMDCT produced silence");
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.samples, 480);
    Ok(())
}
