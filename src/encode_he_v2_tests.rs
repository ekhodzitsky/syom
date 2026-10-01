//! TASK-93: public HE-AAC v2 encode — containers, signalling, timeline,
//! spatial image, chunk invariance, errors, libavcodec-decoded goldens.
//!
//! Offline mint: `MINT_GOLDENS=1 cargo test --lib mint_he_v2_goldens`, then
//!   ffmpeg -y -i src/goldens/he2_48e.adts -f s16le src/goldens/he2_48e.lavc.s16
//!   ffmpeg -y -i src/goldens/he2_48em.m4a -f s16le src/goldens/he2_48em.lavc.s16

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, DecodeOptions, EncodeContainer, EncodeOptions, Encoder, Layout, ProbeProfile,
    decode_with, encode_with, encode_write, encode_write_m4a, probe,
};
use std::io::Cursor;

const RATE: u32 = 48_000;
const N: usize = 30 * 2048 + 777;
/// Source sample where the image jumps from left to right.
const FLIP: usize = 15 * 2048 + 700;

fn opts(container: EncodeContainer) -> EncodeOptions {
    EncodeOptions::adts()
        .with_he_v2(true)
        .with_bitrate_bps(32_000)
        .with_container(container)
}

/// Multi-tone programme, hard left then hard right (−26 dB on the far side).
fn pcm() -> Vec<Vec<f32>> {
    let mut s = vec![0.0f32; N];
    for f in [150u32, 420, 900, 1800, 3500, 7000, 12_000] {
        for (i, o) in s.iter_mut().enumerate() {
            let cycles = (i as u64 * u64::from(f)) % u64::from(RATE);
            let a = cycles as f32 / RATE as f32 * std::f32::consts::TAU;
            *o += 0.08 * crate::engine::det_math::sincos(a).0;
        }
    }
    let l = s
        .iter()
        .enumerate()
        .map(|(i, x)| if i < FLIP { *x } else { 0.05 * x })
        .collect();
    let r = s
        .iter()
        .enumerate()
        .map(|(i, x)| if i < FLIP { 0.05 * x } else { *x })
        .collect();
    vec![l, r]
}

fn ratio_db(l: &[f32], r: &[f32]) -> f64 {
    let e = |x: &[f32]| x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    10.0 * (e(l) / e(r).max(1e-12)).log10()
}

/// Image checks on presentation-aligned planes (`at` = source index 0).
fn assert_image(l: &[f32], r: &[f32], at: usize, label: &str) {
    let w = |from: usize, to: usize| ratio_db(&l[at + from..at + to], &r[at + from..at + to]);
    assert!(
        w(4 * 2048, FLIP - 4096) >= 15.0,
        "{label}: left image {:.1} dB",
        w(4 * 2048, FLIP - 4096)
    );
    assert!(
        w(FLIP + 4096, N - 4096) <= -15.0,
        "{label}: right image {:.1} dB",
        w(FLIP + 4096, N - 4096)
    );
    // The flip is followed within one access unit either side.
    assert!(w(FLIP - 4096, FLIP - 2048) > 6.0, "{label}: no early flip");
    assert!(
        w(FLIP + 2048, FLIP + 4096) < -6.0,
        "{label}: flip done after one unit"
    );
}

#[test]
fn every_container_decodes_to_stereo_on_the_he_timeline_with_the_image() {
    let src = pcm();
    for container in [
        EncodeContainer::Adts,
        EncodeContainer::M4a,
        EncodeContainer::Latm,
    ] {
        let bytes = encode_with(&src, RATE, &opts(container)).unwrap();
        let p = probe(&bytes).unwrap();
        assert_eq!(p.meta.core_rate, RATE / 2, "{container:?}");
        if container != EncodeContainer::Adts {
            // ADTS signals SBR / PS implicitly: a header probe sees the core.
            assert_eq!(p.meta.output_rate, RATE);
            assert_eq!(p.profile, ProbeProfile::HeAacV2, "explicit AOT 29");
            assert_eq!(p.meta.layout, Layout::Mpeg(2));
        }
        let dec = decode_with(&bytes, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(
            (dec.channels.len(), dec.sample_rate),
            (2, RATE),
            "{container:?}"
        );
        let at = if container == EncodeContainer::Latm {
            3018
        } else {
            assert_eq!(dec.channels[0].len(), N, "{container:?} presentation is N");
            assert_eq!(dec.priming, Some(3018));
            0
        };
        assert_image(
            &dec.channels[0],
            &dec.channels[1],
            at,
            &format!("{container:?}"),
        );
        // Whole-stream rate: 32 kbps ABR, short clip, generous band.
        let secs = N as f64 / f64::from(RATE);
        let kbps = bytes.len() as f64 * 8.0 / secs / 1000.0;
        assert!(
            (20.0..=40.0).contains(&kbps),
            "{container:?}: {kbps:.1} kbps"
        );
    }
}

#[test]
fn push_chunking_and_sinks_match_one_shot() {
    let src = pcm();
    let want = encode_with(&src, RATE, &opts(EncodeContainer::Adts)).unwrap();
    for chunk in [777usize, 2048, 5000] {
        let mut enc = Encoder::new(RATE, 2, &opts(EncodeContainer::Adts)).unwrap();
        assert_eq!(enc.asc()[0] >> 3, 29, "explicit AOT 29 config");
        let (mut got, mut fed) = (Vec::new(), 0usize);
        for at in (0..N).step_by(chunk) {
            let end = (at + chunk).min(N);
            enc.feed(&[&src[0][at..end], &src[1][at..end]], |f| {
                fed += f.samples;
                got.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
        }
        let info = enc
            .finish(|f| {
                fed += f.samples;
                got.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            got.as_slice(),
            crate::gapless::strip_id3(&want),
            "chunk {chunk}"
        );
        assert_eq!((fed, info.channels, info.layout), (N, 2, Layout::Mpeg(2)));
        assert_eq!(info.priming, 3018);
        enc.reset().unwrap();
    }
    let mut sink = Vec::new();
    encode_write(&mut sink, &src, RATE, &opts(EncodeContainer::Adts)).unwrap();
    assert_eq!(sink, want);
    let mut sink = Cursor::new(Vec::new());
    encode_write_m4a(&mut sink, &src, RATE, &opts(EncodeContainer::M4a)).unwrap();
    assert_eq!(
        sink.into_inner(),
        encode_with(&src, RATE, &opts(EncodeContainer::M4a)).unwrap()
    );
}

#[test]
fn reset_repeats_the_stream_and_bad_shapes_are_typed_errors() {
    let src = pcm();
    let mut enc = Encoder::new(RATE, 2, &opts(EncodeContainer::Adts)).unwrap();
    let run = |enc: &mut Encoder| {
        let mut out = Vec::new();
        enc.feed(&[&src[0][..], &src[1][..]], |f| {
            out.extend_from_slice(f.au);
            Ok(())
        })
        .unwrap();
        enc.finish(|f| {
            out.extend_from_slice(f.au);
            Ok(())
        })
        .unwrap();
        out
    };
    let first = run(&mut enc);
    enc.reset().unwrap();
    assert_eq!(run(&mut enc), first);
    let mono = vec![src[0].clone()];
    let e = encode_with(&mono, RATE, &opts(EncodeContainer::Adts)).unwrap_err();
    assert!(matches!(e, AacError::Encode(_)), "{e}");
    assert!(Encoder::new(RATE, 1, &opts(EncodeContainer::Adts)).is_err());
    let mut ps_only = EncodeOptions::adts();
    ps_only.ps = true;
    assert!(matches!(
        encode_with(&src, RATE, &ps_only).unwrap_err(),
        AacError::Encode(_)
    ));
    let q = opts(EncodeContainer::Adts).with_quality(5);
    assert!(
        encode_with(&src, RATE, &q).is_err(),
        "quality VBR is LC only"
    );
    assert!(
        encode_with(&src, 96_000, &opts(EncodeContainer::Adts)).is_err(),
        "HE rate rule"
    );
    assert!(!EncodeOptions::adts().with_he_v2(true).with_he_v2(false).he);
}

#[test]
fn mint_he_v2_goldens() {
    if std::env::var("MINT_GOLDENS").is_err() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    let src = pcm();
    let adts = encode_with(&src, RATE, &opts(EncodeContainer::Adts)).unwrap();
    std::fs::write(dir.join("he2_48e.adts"), crate::gapless::strip_id3(&adts)).unwrap();
    std::fs::write(
        dir.join("he2_48em.m4a"),
        encode_with(&src, RATE, &opts(EncodeContainer::M4a)).unwrap(),
    )
    .unwrap();
}

fn lavc_planes(s16: &[u8]) -> [Vec<f32>; 2] {
    let mut out = [Vec::new(), Vec::new()];
    for (i, b) in s16.chunks_exact(2).enumerate() {
        out[i % 2].push(f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0);
    }
    out
}

#[test]
fn libavcodec_decodes_our_he_v2_streams_as_stereo_with_the_same_image() {
    let src = pcm();
    for (container, bytes, lavc, at, label) in [
        (
            EncodeContainer::Adts,
            &include_bytes!("goldens/he2_48e.adts")[..],
            &include_bytes!("goldens/he2_48e.lavc.s16")[..],
            3018usize,
            "he2-adts",
        ),
        (
            EncodeContainer::M4a,
            &include_bytes!("goldens/he2_48em.m4a")[..],
            &include_bytes!("goldens/he2_48em.lavc.s16")[..],
            0,
            "he2-m4a",
        ),
    ] {
        let fresh = encode_with(&src, RATE, &opts(container)).unwrap();
        let fresh = if container == EncodeContainer::Adts {
            crate::gapless::strip_id3(&fresh).to_vec()
        } else {
            fresh
        };
        assert_eq!(fresh.as_slice(), bytes, "{label} drifted; re-mint");
        let [l, r] = lavc_planes(lavc);
        assert_image(&l, &r, at, &format!("{label} lavc"));
        // syom's own decode agrees with libavcodec's.
        let dec = decode_with(bytes, &DecodeOptions::unbounded()).unwrap();
        let n = dec.channels[0].len().min(l.len());
        for (ch, lav) in [&l, &r].into_iter().enumerate() {
            let (mut ps, mut pe) = (0.0f64, 0.0f64);
            for (&x, &y) in lav.iter().zip(dec.channels[ch].iter()).take(n) {
                ps += f64::from(x).powi(2);
                pe += (f64::from(x) - f64::from(y)).powi(2);
            }
            let snr = 10.0 * (ps / pe.max(1e-12)).log10();
            assert!(snr >= 60.0, "{label} ch{ch}: syom vs lavc {snr:.1} dB");
        }
    }
}

/// Lab print: `cargo test --release --lib -- --ignored he_v2_report --nocapture`.
#[test]
#[ignore = "prints the HE v2 vs v1 table for lab/quality/HE_V2.md"]
fn he_v2_report() {
    let n = 96 * 2048;
    let mut s = vec![0.0f32; n];
    for f in [150u32, 420, 900, 1800, 3500, 7000] {
        for (i, o) in s.iter_mut().enumerate() {
            let cycles = (i as u64 * u64::from(f)) % u64::from(RATE);
            *o += 0.07
                * crate::engine::det_math::sincos(
                    cycles as f32 / RATE as f32 * std::f32::consts::TAU,
                )
                .0;
        }
    }
    let mut seed = 0x0BAD_F00Du32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((seed >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0
    };
    // Panned 6 dB tones + independent ambience in each channel.
    let l: Vec<f32> = s.iter().map(|x| x + 0.03 * noise()).collect();
    let r: Vec<f32> = s.iter().map(|x| 0.5 * x + 0.03 * noise()).collect();
    let src = vec![l, r];
    let corr = |a: &[f32], b: &[f32]| {
        let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
        for (x, y) in a.iter().zip(b.iter()) {
            ab += f64::from(*x) * f64::from(*y);
            aa += f64::from(*x).powi(2);
            bb += f64::from(*y).powi(2);
        }
        ab / (aa * bb).sqrt()
    };
    println!(
        "source: ILD {:.2} dB, L/R correlation {:.3}",
        ratio_db(&src[0], &src[1]),
        corr(&src[0], &src[1])
    );
    for bps in [24_000u32, 32_000, 48_000] {
        for v2 in [false, true] {
            let o = EncodeOptions::adts().with_he(true).with_bitrate_bps(bps);
            let o = if v2 { o.with_he_v2(true) } else { o };
            let t = std::time::Instant::now();
            let adts = encode_with(&src, RATE, &o).unwrap();
            let ms = t.elapsed().as_secs_f64() * 1e3;
            let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
            let (dl, dr) = (
                &dec.channels[0][3018..3018 + n],
                &dec.channels[1][3018..3018 + n],
            );
            let (mut ps, mut pe) = (0.0f64, 0.0f64);
            for i in 8192..n - 8192 {
                let m = 0.5 * f64::from(src[0][i] + src[1][i]);
                let d = 0.5 * f64::from(dl[i] + dr[i]);
                ps += m * m;
                pe += (m - d).powi(2);
            }
            println!(
                "{} {:2} kbps: {:6} B ({:.1} kbps) {:6.1} ms  mid SNR {:5.1} dB  ILD {:5.2} dB  corr {:.3}",
                if v2 { "v2" } else { "v1" },
                bps / 1000,
                adts.len(),
                adts.len() as f64 * 8.0 / (n as f64 / f64::from(RATE)) / 1000.0,
                ms,
                10.0 * (ps / pe).log10(),
                ratio_db(&dl[8192..n - 8192], &dr[8192..n - 8192]),
                corr(&dl[8192..n - 8192], &dr[8192..n - 8192]),
            );
        }
    }
}
