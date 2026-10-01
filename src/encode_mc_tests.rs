//! TASK-116: public surround encode — plane counts 3, 4, 5, 6, 8 through
//! `encode_with`, `Encoder`, `encode_write`, `encode_write_m4a`; ADTS, M4A
//! and LATM signalling; typed errors; libavcodec-decoded goldens.
//!
//! Offline mint: `MINT_GOLDENS=1 cargo test --lib mint_public_mc_goldens`, then
//!   ffmpeg -y -i src/goldens/enc_mc51.m4a -f s16le src/goldens/enc_mc51m.lavc.s16
//!   ffmpeg -y -i src/goldens/enc_mc71.latm -f s16le src/goldens/enc_mc71l.lavc.s16

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, DecodeOptions, EncodeContainer, EncodeOptions, Encoder, Layout, decode_with,
    encode_with, encode_write, encode_write_m4a, mpeg_channels, probe,
};
use std::io::Cursor;

const RATE: u32 = 48_000;
const TONES: [u32; 8] = [440, 880, 330, 100, 550, 660, 770, 220];
const N: usize = 20_000; // not a multiple of 1024: exercises the tail

fn tones_for(planes: usize) -> Vec<u32> {
    match planes {
        3 => vec![440, 880, 330],
        4 => vec![440, 880, 330, 550],
        5 => vec![440, 880, 330, 550, 660],
        6 => TONES[..6].to_vec(),
        _ => TONES.to_vec(),
    }
}

fn pcm(planes: usize) -> Vec<Vec<f32>> {
    tones_for(planes)
        .iter()
        .map(|&f| {
            (0..N)
                .map(|i| {
                    let cycles = (i as u64 * u64::from(f)) % u64::from(RATE);
                    let a = cycles as f32 / RATE as f32 * std::f32::consts::TAU;
                    0.16 * crate::engine::det_math::sincos(a).0
                })
                .collect()
        })
        .collect()
}

fn opts(planes: usize, container: EncodeContainer) -> EncodeOptions {
    EncodeOptions::adts()
        .with_bitrate_bps(48_000 * planes as u32)
        .with_container(container)
}

/// Strongest fixture tone in the middle 8192 samples of `plane`.
fn dominant(plane: &[f32], tones: &[u32]) -> u32 {
    let mid = &plane[plane.len() / 2 - 4096..plane.len() / 2 + 4096];
    let mag = |f: u32| {
        let w = 2.0 * std::f64::consts::PI * f64::from(f) / f64::from(RATE);
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in mid.iter().enumerate() {
            re += f64::from(v) * (w * i as f64).cos();
            im += f64::from(v) * (w * i as f64).sin();
        }
        re.hypot(im)
    };
    tones
        .iter()
        .copied()
        .max_by(|&a, &b| mag(a).total_cmp(&mag(b)))
        .unwrap()
}

#[test]
fn every_layout_and_container_round_trips_with_plane_identity() {
    for (planes, cfg) in [(3usize, 3u8), (4, 4), (5, 5), (6, 6), (8, 7)] {
        let src = pcm(planes);
        let tones = tones_for(planes);
        for container in [
            EncodeContainer::Adts,
            EncodeContainer::M4a,
            EncodeContainer::Latm,
        ] {
            let bytes = encode_with(&src, RATE, &opts(planes, container)).unwrap();
            let p = probe(&bytes).unwrap();
            assert_eq!(p.meta.layout, Layout::Mpeg(cfg), "{planes} {container:?}");
            assert_eq!(p.meta.labels().collect::<Vec<_>>(), mpeg_channels(cfg));
            let dec = decode_with(&bytes, &DecodeOptions::unbounded()).unwrap();
            assert_eq!(dec.layout, Layout::Mpeg(cfg));
            assert_eq!(dec.channels.len(), planes);
            let got: Vec<u32> = dec.channels.iter().map(|c| dominant(c, &tones)).collect();
            assert_eq!(got, tones, "{planes} planes {container:?}: plane identity");
            if container == EncodeContainer::Latm {
                assert_eq!(dec.channels[0].len(), (N.div_ceil(1024) + 1) * 1024);
            } else {
                assert_eq!(dec.channels[0].len(), N, "{container:?} presentation is N");
                assert_eq!(dec.priming, Some(1024));
            }
        }
    }
}

#[test]
fn push_chunking_and_sinks_match_one_shot() {
    let src = pcm(6);
    let want = encode_with(&src, RATE, &opts(6, EncodeContainer::Adts)).unwrap();
    for chunk in [1usize, 1000, 1024, 4097] {
        let mut enc = Encoder::new(RATE, 6, &opts(6, EncodeContainer::Adts)).unwrap();
        let mut got = Vec::new();
        let mut fed = 0usize;
        let mut at = 0usize;
        while at < N {
            let end = (at + chunk).min(N);
            let planes: Vec<&[f32]> = src.iter().map(|p| &p[at..end]).collect();
            enc.feed(&planes, |f| {
                fed += f.samples;
                got.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
            at = end;
            if chunk == 1 && at > 3000 {
                break; // byte-at-a-time is slow; the prefix is enough
            }
        }
        let raw = crate::gapless::strip_id3(&want);
        if at < N {
            assert_eq!(got.as_slice(), &raw[..got.len()], "chunk 1 prefix");
            continue;
        }
        let info = enc
            .finish(|f| {
                fed += f.samples;
                got.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
        assert_eq!(got.as_slice(), raw, "chunk {chunk}");
        assert_eq!(fed, N);
        assert_eq!(info.layout, Layout::Mpeg(6));
        assert_eq!(info.channels, 6);
        assert_eq!(info.aac_frames as usize, N.div_ceil(1024) + 1);
        assert_eq!(info.remainder as usize, 1024 - N % 1024);
    }
    let mut sink = Vec::new();
    encode_write(&mut sink, &src, RATE, &opts(6, EncodeContainer::Adts)).unwrap();
    assert_eq!(sink, want);
    let mut sink = Cursor::new(Vec::new());
    encode_write_m4a(&mut sink, &src, RATE, &opts(6, EncodeContainer::M4a)).unwrap();
    assert_eq!(
        sink.into_inner(),
        encode_with(&src, RATE, &opts(6, EncodeContainer::M4a)).unwrap()
    );
}

#[test]
fn quality_vbr_works_and_unsupported_shapes_fail_before_any_write() {
    let src = pcm(6);
    let q = encode_with(&src, RATE, &EncodeOptions::adts().with_quality(6)).unwrap();
    assert_eq!(
        decode_with(&q, &DecodeOptions::unbounded())
            .unwrap()
            .channels
            .len(),
        6
    );
    for planes in [7usize, 9] {
        let bad = vec![vec![0.1f32; 2048]; planes];
        let mut sink = Vec::new();
        let e = encode_write(&mut sink, &bad, RATE, &EncodeOptions::adts()).unwrap_err();
        assert!(matches!(e, AacError::Encode(_)), "{planes}: {e}");
        assert!(sink.is_empty());
    }
    for o in [
        EncodeOptions::low_rate(),
        EncodeOptions::adts().with_lookahead(true),
    ] {
        let mut sink = Vec::new();
        let e = encode_write(&mut sink, &src, RATE, &o).unwrap_err();
        assert!(matches!(e, AacError::Encode(_)), "{e}");
        assert!(sink.is_empty());
        assert!(Encoder::new(RATE, 6, &o).is_err());
    }
    // Whole-stream bitrate ceiling scales with the plane count.
    let over = EncodeOptions::adts().with_bitrate_bps(6 * 288_000 + 1);
    assert!(encode_with(&src, RATE, &over).is_err());
}

#[test]
fn mint_public_mc_goldens() {
    if std::env::var("MINT_GOLDENS").is_err() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    let m4a = encode_with(&pcm(6), RATE, &opts(6, EncodeContainer::M4a)).unwrap();
    std::fs::write(dir.join("enc_mc51.m4a"), m4a).unwrap();
    let latm = encode_with(&pcm(8), RATE, &opts(8, EncodeContainer::Latm)).unwrap();
    std::fs::write(dir.join("enc_mc71.latm"), latm).unwrap();
}

#[test]
fn libavcodec_decodes_our_surround_m4a_and_latm() {
    let m4a = &include_bytes!("goldens/enc_mc51.m4a")[..];
    let latm = &include_bytes!("goldens/enc_mc71.latm")[..];
    // Byte-exact tripwire (deterministic encoder); re-mint on intent.
    assert_eq!(
        encode_with(&pcm(6), RATE, &opts(6, EncodeContainer::M4a)).unwrap(),
        m4a,
        "enc_mc51.m4a drifted; re-mint"
    );
    assert_eq!(
        encode_with(&pcm(8), RATE, &opts(8, EncodeContainer::Latm)).unwrap(),
        latm,
        "enc_mc71.latm drifted; re-mint"
    );
    for (bytes, lavc, planes, label) in [
        (
            m4a,
            &include_bytes!("goldens/enc_mc51m.lavc.s16")[..],
            6usize,
            "enc-mc51-m4a",
        ),
        (
            latm,
            &include_bytes!("goldens/enc_mc71l.lavc.s16")[..],
            8,
            "enc-mc71-latm",
        ),
    ] {
        assert_close_to_lavc(bytes, lavc, planes, label);
        // libavcodec's own planes carry the source tones in the public order.
        let mut planes_pcm = vec![Vec::new(); planes];
        for (i, b) in lavc.chunks_exact(2).enumerate() {
            planes_pcm[i % planes].push(f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0);
        }
        let tones = tones_for(planes);
        let got: Vec<u32> = planes_pcm.iter().map(|c| dominant(c, &tones)).collect();
        assert_eq!(got, tones, "{label}: lavc plane identity");
    }
}

/// syom decode vs the libavcodec PCM over syom's presentation length:
/// SNR ≥ 70 dB per plane, ≤ 1 LSB except a short high-gain TNS stretch at
/// the abrupt end of the fixture (≤ 8 LSB on ≤ 100 samples — the same
/// divergence shows in plain stereo; tracked separately).
fn assert_close_to_lavc(bytes: &[u8], lavc: &[u8], planes: usize, label: &str) {
    let dec = decode_with(bytes, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels.len(), planes, "{label}");
    assert!(
        lavc.len() / 2 / planes >= dec.channels[0].len(),
        "{label} lavc length"
    );
    for (ch, got) in dec.channels.iter().enumerate() {
        let (mut worst, mut over, mut ps, mut pe) = (0u32, 0usize, 0.0f64, 0.0f64);
        for (i, &g) in got.iter().enumerate() {
            let k = (i * planes + ch) * 2;
            let lav = i16::from_le_bytes([lavc[k], lavc[k + 1]]);
            let ours = (f64::from(g) * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
            let d = (i32::from(lav) - i32::from(ours)).unsigned_abs();
            worst = worst.max(d);
            over += usize::from(d > 1);
            ps += f64::from(lav).powi(2);
            pe += f64::from(d).powi(2);
        }
        let snr = 10.0 * (ps / pe.max(1.0)).log10();
        assert!(
            worst <= 8 && over <= 100,
            "{label} ch{ch}: worst {worst} LSB, {over} over 1"
        );
        assert!(snr >= 70.0, "{label} ch{ch}: SNR {snr:.1} dB");
    }
}
