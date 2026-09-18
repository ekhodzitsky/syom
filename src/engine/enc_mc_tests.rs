//! TASK-114: multichannel element orchestration — round trip through the
//! syom decoder, plane identity / isolation by tone, budgets, stress, and
//! libavcodec-decoded goldens (`enc_mc51.adts`, `enc_mc71.adts`).
//!
//! Offline mint: `MINT_GOLDENS=1 cargo test --lib mint_mc_goldens`, then
//!   ffmpeg -y -i src/goldens/enc_mc51.adts -f s16le src/goldens/enc_mc51.lavc.s16
//!   ffmpeg -y -i src/goldens/enc_mc71.adts -f s16le src/goldens/enc_mc71.lavc.s16

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::McEncoder;
use crate::engine::adts::AdtsHeader;
use crate::engine::det_math;
use crate::{DecodeOptions, Layout, decode_with};

const RATE: u32 = 48_000;
/// FL FR FC LFE BL BR SL SR (the `mc71` decode fixture's tones).
const TONES: [u32; 8] = [440, 880, 330, 100, 550, 660, 770, 220];

/// Tone set for a plane count in the public order.
fn tones_for(planes: usize) -> Vec<u32> {
    match planes {
        3 => vec![440, 880, 330],
        4 => vec![440, 880, 330, 550],
        5 => vec![440, 880, 330, 550, 660],
        6 => TONES[..6].to_vec(),
        _ => TONES.to_vec(),
    }
}

/// Deterministic sine planes (det_math, f64-free phase accumulator).
fn tone_planes(tones: &[u32], frames: usize) -> Vec<Vec<f32>> {
    tones
        .iter()
        .map(|&f| {
            (0..frames * 1024)
                .map(|i| {
                    let cycles = (i as u64 * u64::from(f)) % u64::from(RATE);
                    let a = cycles as f32 / RATE as f32 * std::f32::consts::TAU;
                    0.16 * det_math::sincos(a).0
                })
                .collect()
        })
        .collect()
}

fn noise_planes(planes: usize, frames: usize) -> Vec<Vec<f32>> {
    (0..planes)
        .map(|p| {
            let mut s = 0x0BAD_F00Du32 ^ (p as u32).wrapping_mul(0x9E37_79B9);
            (0..frames * 1024)
                .map(|_| {
                    s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    0.5 * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
                })
                .collect()
        })
        .collect()
}

/// Encode whole frames into an ADTS stream; returns (adts, per-frame bytes).
fn encode_adts(pcm: &[Vec<f32>], bps: u32) -> (Vec<u8>, Vec<usize>) {
    let mut enc = McEncoder::new(RATE, pcm.len(), bps).unwrap();
    let mut adts = Vec::new();
    let mut sizes = Vec::new();
    let mut rdb = Vec::new();
    for f in 0..pcm[0].len() / 1024 {
        let frame: Vec<&[f32]> = pcm.iter().map(|p| &p[f * 1024..(f + 1) * 1024]).collect();
        enc.encode_into(&frame, &mut rdb).unwrap();
        let hdr = AdtsHeader {
            mpeg_version_mpeg2: false,
            protection_absent: true,
            profile: 1,
            sampling_frequency_index: enc.fs_index(),
            channel_configuration: enc.channel_configuration(),
            aac_frame_length: (7 + rdb.len()) as u16,
            adts_buffer_fullness: 0x7FF,
            number_of_raw_data_blocks_in_frame: 1,
        };
        adts.extend_from_slice(&hdr.write());
        adts.extend_from_slice(&rdb);
        sizes.push(rdb.len());
    }
    (adts, sizes)
}

/// Goertzel magnitude of `f` over the last 8192 samples (Hann).
fn mag(plane: &[f32], f: u32) -> f64 {
    let tail = &plane[plane.len() - 8192..];
    let w = 2.0 * std::f64::consts::PI * f64::from(f) / f64::from(RATE);
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, &v) in tail.iter().enumerate() {
        let h = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / 8192.0).cos();
        re += f64::from(v) * h * (w * i as f64).cos();
        im += f64::from(v) * h * (w * i as f64).sin();
    }
    re.hypot(im)
}

/// Own-tone level over the strongest foreign tone, dB, per plane.
fn isolation_db(planes: &[Vec<f32>], tones: &[u32]) -> Vec<f64> {
    planes
        .iter()
        .zip(tones.iter())
        .map(|(p, &own)| {
            let leak = tones
                .iter()
                .filter(|&&f| f != own)
                .map(|&f| mag(p, f))
                .fold(1e-9, f64::max);
            20.0 * (mag(p, own) / leak).log10()
        })
        .collect()
}

#[test]
fn every_layout_round_trips_with_plane_identity_and_isolation() {
    for (planes, cfg, bps) in [
        (3usize, 3u8, 160_000u32),
        (4, 4, 192_000),
        (5, 5, 256_000),
        (6, 6, 256_000),
        (8, 7, 320_000),
    ] {
        let tones = tones_for(planes);
        let pcm = tone_planes(&tones, 24);
        let (adts, _) = encode_adts(&pcm, bps);
        let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(dec.layout, Layout::Mpeg(cfg), "{planes} planes");
        assert_eq!(dec.channels.len(), planes);
        let iso = isolation_db(&dec.channels, &tones);
        assert!(
            iso.iter().all(|&d| d >= 40.0),
            "{planes} planes: isolation {iso:?}"
        );
    }
}

#[test]
fn unsupported_plane_counts_and_frame_shapes_are_errors() {
    for planes in [0usize, 1, 2, 7, 9] {
        assert!(McEncoder::new(RATE, planes, 256_000).is_err(), "{planes}");
    }
    let mut enc = McEncoder::new(RATE, 6, 256_000).unwrap();
    let short = vec![vec![0.0f32; 1000]; 6];
    let refs: Vec<&[f32]> = short.iter().map(Vec::as_slice).collect();
    assert!(enc.encode_into(&refs, &mut Vec::new()).is_err());
    let five = vec![vec![0.0f32; 1024]; 5];
    let refs: Vec<&[f32]> = five.iter().map(Vec::as_slice).collect();
    assert!(enc.encode_into(&refs, &mut Vec::new()).is_err());
}

#[test]
fn budgets_hold_on_noise_and_transient_stress() {
    for (planes, bps) in [(6usize, 256_000u32), (8, 320_000), (6, 96_000)] {
        let mut pcm = noise_planes(planes, 40);
        for p in &mut pcm {
            for k in (0..p.len()).step_by(5000) {
                p[k] = 0.95; // clicks force block switching per element
            }
        }
        let (adts, sizes) = encode_adts(&pcm, bps);
        let budget = (u64::from(bps) * 1024 / u64::from(RATE) / 8) as usize;
        // One frame of credit per element, never above the LC cap.
        let cap = planes * 6144 / 8;
        assert!(sizes.iter().all(|&s| s <= cap), "LC cap");
        assert!(
            sizes.iter().all(|&s| s <= budget * 3 / 2 + 16),
            "frame over 1.5x budget: max {} vs {budget}",
            sizes.iter().max().unwrap()
        );
        let mean = sizes.iter().sum::<usize>() as f64 / sizes.len() as f64;
        assert!(
            mean <= budget as f64 * 1.03 && mean >= budget as f64 * 0.9,
            "{planes} planes {bps}: mean {mean:.0} vs budget {budget}"
        );
        let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(dec.channels.len(), planes);
    }
}

#[test]
fn lfe_element_is_band_limited_and_long_only() {
    // A click in the LFE plane must not reach above ~200 Hz.
    let mut pcm = tone_planes(&tones_for(6), 24);
    for k in (0..pcm[3].len()).step_by(3000) {
        pcm[3][k] = 0.9;
    }
    let (adts, _) = encode_adts(&pcm, 256_000);
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let lfe = &dec.channels[3];
    let low = mag(lfe, 100);
    for f in [400u32, 1000, 4000, 12_000] {
        assert!(mag(lfe, f) < low * 0.01, "LFE energy at {f} Hz");
    }
}

fn golden_pcm(planes: usize) -> Vec<Vec<f32>> {
    tone_planes(&tones_for(planes), 16)
}

#[test]
fn mint_mc_goldens() {
    if std::env::var("MINT_GOLDENS").is_err() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    std::fs::write(
        dir.join("enc_mc51.adts"),
        encode_adts(&golden_pcm(6), 256_000).0,
    )
    .unwrap();
    std::fs::write(
        dir.join("enc_mc71.adts"),
        encode_adts(&golden_pcm(8), 320_000).0,
    )
    .unwrap();
}

/// Interleaved s16 → f32 planes.
fn deinterleave(s16: &[u8], planes: usize) -> Vec<Vec<f32>> {
    let mut out = vec![Vec::new(); planes];
    for (i, b) in s16.chunks_exact(2).enumerate() {
        out[i % planes].push(f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0);
    }
    out
}

#[test]
fn libavcodec_decodes_our_surround_streams_with_isolated_planes() {
    for (planes, bps, adts, lavc, label) in [
        (
            6usize,
            256_000u32,
            &include_bytes!("../goldens/enc_mc51.adts")[..],
            &include_bytes!("../goldens/enc_mc51.lavc.s16")[..],
            "enc-mc51",
        ),
        (
            8,
            320_000,
            &include_bytes!("../goldens/enc_mc71.adts")[..],
            &include_bytes!("../goldens/enc_mc71.lavc.s16")[..],
            "enc-mc71",
        ),
    ] {
        // Layer 1: byte-exact tripwire (deterministic encoder).
        let fresh = encode_adts(&golden_pcm(planes), bps).0;
        assert_eq!(fresh.as_slice(), adts, "{label} drifted; re-mint");
        // Layer 2: syom decode equals the libavcodec decode.
        crate::decode_tests::assert_native_matches_lavc_with(
            adts,
            lavc,
            RATE,
            label,
            &DecodeOptions::unbounded(),
        )
        .unwrap();
        // Layer 3: libavcodec's planes carry the right tones, isolated.
        let iso = isolation_db(&deinterleave(lavc, planes), &tones_for(planes));
        assert!(
            iso.iter().all(|&d| d >= 40.0),
            "{label} lavc isolation {iso:?}"
        );
    }
}
