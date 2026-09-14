//! TASK-50: 0.x API contract — speech default, `audio()` helper, signatures.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, ChannelMode, DecodeOptions, DecodedAac, EncodeContainer, EncodeInfo, EncodeOptions,
    EncodedFrame, Encoder, Frame, MemoryBudgets, Result, StreamInfo, decode, decode_streaming,
    decode_with, encode_with,
};

const LECTURE: &[u8] = include_bytes!("goldens/lecture.m4a");
const SINE: &[u8] = include_bytes!("goldens/sine48.adts");
const MC51: &[u8] = include_bytes!("goldens/mc51.adts");

fn bytes_to_pcm(bytes: &[u8]) -> Result<DecodedAac> {
    decode(bytes)
}

fn stereo_kept(bytes: &[u8]) -> Result<DecodedAac> {
    decode_with(bytes, &DecodeOptions::audio())
}

fn archival(bytes: &[u8]) -> Result<DecodedAac> {
    decode_with(bytes, &DecodeOptions::unbounded())
}

fn stream_adts(bytes: &[u8]) -> Result<StreamInfo> {
    let mut samples = 0u64;
    let info = decode_streaming(bytes, &DecodeOptions::audio(), |f: Frame<'_>| {
        samples += f.samples as u64;
        let _: &[&[f32]] = f.planar;
        Ok(())
    })?;
    assert_eq!(samples, info.samples);
    Ok(info)
}

fn encode_sink(pcm: &[Vec<f32>]) -> Result<(Vec<u8>, EncodeInfo)> {
    let opts = EncodeOptions::adts();
    let one = encode_with(pcm, 48_000, &opts)?;
    let mut enc = Encoder::new(48_000, pcm.len(), &opts)?;
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    let mut push = Vec::new();
    enc.feed(&planes, |f: EncodedFrame<'_>| {
        push.extend_from_slice(f.au);
        Ok(())
    })?;
    let info = enc.finish(|f| {
        push.extend_from_slice(f.au);
        Ok(())
    })?;
    assert_eq!(push, one);
    Ok((one, info))
}

#[test]
fn one_call_decode_stays_speech_mono() -> Result<()> {
    let speech = bytes_to_pcm(LECTURE)?;
    assert_eq!(speech.channels.len(), 1);
    assert_eq!(DecodeOptions::default().channel_mode, ChannelMode::Mono);
    assert_eq!(DecodeOptions::speech().channel_mode, ChannelMode::Mono);
    assert_eq!(ChannelMode::default(), ChannelMode::Mono);
    Ok(())
}

#[test]
fn audio_keeps_coded_layout_under_lecture_caps() -> Result<()> {
    let mixed = bytes_to_pcm(LECTURE)?;
    let split = stereo_kept(LECTURE)?;
    let full = archival(LECTURE)?;
    assert_eq!(mixed.channels.len(), 1);
    assert_eq!(split.channels.len(), 2);
    assert_eq!(full.channels.len(), 2);
    assert_eq!(split.sample_rate, mixed.sample_rate);
    assert_eq!(split.channels[0].len(), mixed.channels[0].len());
    assert_eq!(split.channels[0].len(), split.channels[1].len());
    assert!(split.channels[0].iter().any(|s| s.abs() > 1e-4));
    assert!(split.channels[1].iter().any(|s| s.abs() > 1e-4));
    assert_eq!(split.channels, full.channels, "lecture fits audio() caps");

    let mc_speech = decode(MC51)?;
    let mc_audio = decode_with(MC51, &DecodeOptions::audio())?;
    assert_eq!(mc_speech.channels.len(), 1);
    assert_eq!(mc_audio.channels.len(), 6);
    Ok(())
}

#[test]
fn audio_caps_match_speech_not_unbounded() {
    let s = DecodeOptions::speech();
    let a = DecodeOptions::audio();
    let u = DecodeOptions::unbounded();
    assert_eq!(a.channel_mode, ChannelMode::Split);
    assert_eq!(a.max_duration_secs, s.max_duration_secs);
    assert_eq!(a.max_sample_rate, s.max_sample_rate);
    assert_eq!(a.max_decode_sample_rate, s.max_decode_sample_rate);
    assert_eq!(a.memory, MemoryBudgets::default());
    assert!(u.max_duration_secs.is_infinite());
    assert_ne!(a.memory, u.memory);
}

#[test]
fn stream_and_encode_signatures_hold() -> Result<()> {
    let info = stream_adts(SINE)?;
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(info.channels, 1);
    let pcm = vec![vec![0.0f32; 2048]];
    let (adts, enc) = encode_sink(&pcm)?;
    assert_eq!(adts[0], 0xff);
    assert_eq!(enc.samples, 2048);
    assert_eq!(enc.priming, 1024);
    assert_eq!(enc.remainder, 0);
    assert_eq!(EncodeContainer::default(), EncodeContainer::Adts);
    assert!(!EncodeOptions::default().lookahead);
    assert_eq!(EncodeOptions::default().bitrate_bps, 128_000);
    Ok(())
}

#[test]
fn errors_are_matchable_without_thiserror() {
    let e = decode(&[]).unwrap_err();
    assert!(matches!(e, AacError::NotAac));
    assert!(e.is_format_class());
    let e = decode_with(SINE, &DecodeOptions::speech().with_max_sample_rate(8_000)).unwrap_err();
    assert!(matches!(
        e,
        AacError::UnsupportedSampleRate {
            rate: 48_000,
            max: 8_000
        }
    ));
}

#[test]
fn versioned_default_flip_to_split_is_no_go() {
    // TASK-18 / D3 / TASK-50: do not change decode() to Split in 0.x.
    assert_eq!(decode(LECTURE).unwrap().channels.len(), 1);
    assert_eq!(
        decode_with(LECTURE, &DecodeOptions::audio())
            .unwrap()
            .channels
            .len(),
        2
    );
}
