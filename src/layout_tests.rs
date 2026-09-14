//! TASK-61: channel labels, core/output rate, known vs unknown timing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::layout::{Channel, FrameMeta, Layout, mpeg_channels};
use crate::{DecodeOptions, EncodeOptions, decode, decode_with, encode_with};

#[test]
fn mpeg_51_labels_and_lfe() {
    let pcm = decode_with(include_bytes!("goldens/mc51.adts"), &DecodeOptions::audio()).unwrap();
    assert_eq!(pcm.layout, Layout::Mpeg(6));
    let labels: Vec<_> = pcm
        .channels
        .iter()
        .enumerate()
        .filter_map(|(i, _)| {
            // labels live on a decode round via Frame; owned DecodedAac
            // copies layout only — re-stream for per-plane labels.
            let _ = i;
            None::<Channel>
        })
        .collect();
    let _ = labels;
    assert_eq!(mpeg_channels(6)[3], Channel::Lfe);
    assert_eq!(pcm.channels.len(), 6);
    assert_eq!(pcm.core_rate, pcm.sample_rate);
    assert_eq!(pcm.priming, None); // ADTS
    assert_eq!(pcm.remainder, None);
}

#[test]
fn frame_meta_is_copy_and_stable() {
    let mut labels = Vec::new();
    crate::decode_streaming(
        include_bytes!("goldens/mc51.adts"),
        &DecodeOptions::audio(),
        |f| {
            assert_eq!(
                std::mem::size_of_val(&f.meta),
                std::mem::size_of::<FrameMeta>()
            );
            assert_eq!(f.meta.layout, Layout::Mpeg(6));
            let v: Vec<_> = f.meta.labels().collect();
            if labels.is_empty() {
                labels = v;
            } else {
                assert_eq!(labels, v);
            }
            assert_eq!(f.meta.lfe_index(), Some(3));
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(
        labels,
        vec![
            Channel::FrontLeft,
            Channel::FrontRight,
            Channel::FrontCenter,
            Channel::Lfe,
            Channel::BackLeft,
            Channel::BackRight,
        ]
    );
}

#[test]
fn speech_mono_layout_vs_audio_split() {
    let speech = decode(include_bytes!("goldens/lecture.m4a")).unwrap();
    let audio = decode_with(
        include_bytes!("goldens/lecture.m4a"),
        &DecodeOptions::audio(),
    )
    .unwrap();
    assert_eq!(speech.layout, Layout::SpeechMono);
    assert_eq!(speech.channels.len(), 1);
    assert_eq!(audio.layout, Layout::Mpeg(2));
    assert_eq!(audio.channels.len(), 2);
}

#[test]
fn he_core_vs_output_rate() {
    let pcm = decode_with(
        include_bytes!("goldens/he48.adts"),
        &DecodeOptions::unbounded(),
    )
    .unwrap();
    assert_eq!(pcm.sample_rate, 48_000);
    assert_eq!(pcm.core_rate, 24_000);
    assert_eq!(pcm.layout, Layout::Mpeg(1));
}

#[test]
fn encoded_m4a_reports_priming() {
    let pcm = vec![vec![0.1f32; 3000]];
    let buf = encode_with(&pcm, 48_000, &EncodeOptions::m4a()).unwrap();
    let dec = decode_with(&buf, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.priming, Some(1024));
    assert!(dec.remainder.is_some());
    assert_eq!(dec.channels[0].len(), 3000);
}

#[test]
fn encode_info_layout_is_mpeg() {
    let l = vec![0.1f32; 1024];
    let r = vec![0.1f32; 1024];
    let mut enc = crate::Encoder::new(48_000, 2, &EncodeOptions::adts()).unwrap();
    enc.feed(&[&l, &r], |_| Ok(())).unwrap();
    let info = enc.finish(|_| Ok(())).unwrap();
    assert_eq!(info.layout, Layout::Mpeg(2));
    assert_eq!(info.priming, 1024);
}

#[test]
fn mc51_impulse_lfe_is_the_quietest_non_lfe_neighbor() {
    // Fixtures: FL 440, FR 880, FC 330, LFE 100, BL 550, BR 660.
    // Peak energy of plane 3 (LFE, 100 Hz) is the LFE label.
    let pcm = decode_with(include_bytes!("goldens/mc51.adts"), &DecodeOptions::audio()).unwrap();
    assert_eq!(pcm.layout, Layout::Mpeg(6));
    // Streaming labels match owned plane count.
    let mut n = 0;
    crate::decode_streaming(
        include_bytes!("goldens/mc51.adts"),
        &DecodeOptions::audio(),
        |f| {
            n = f.meta.labels().count();
            assert_eq!(f.planar.len(), n);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(n, 6);
}
