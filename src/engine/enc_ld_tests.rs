//! TASK-132 stage 2: one LD access unit decodes in syom.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::LdEncoder;
use crate::{DecodeOptions, Decoder, decode_with};

fn sine(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.4 * (i as f32 * 440.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin())
        .collect()
}

fn snr(got: &[f32], want: &[f32]) -> f64 {
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for (g, w) in got.iter().zip(want.iter()) {
        ps += f64::from(*w) * f64::from(*w);
        let e = f64::from(*g) - f64::from(*w);
        pe += e * e;
    }
    if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    }
}

#[test]
fn mono_au_decodes_with_512_sample_delay() {
    let src = sine(512 * 6);
    let mut enc = LdEncoder::new(48_000).unwrap();
    let mut aus = Vec::new();
    for frame in src.chunks_exact(512) {
        aus.push(enc.push_mono(frame).unwrap());
    }
    let mut dec = Decoder::from_asc(&enc.asc(), DecodeOptions::unbounded()).unwrap();
    let mut pcm = Vec::new();
    for au in &aus {
        dec.decode_au(au, |f| {
            assert_eq!(f.samples, 512);
            assert_eq!(f.sample_rate, 48_000);
            pcm.extend_from_slice(f.planar[0]);
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(pcm.len(), src.len());
    let score = snr(&pcm[512..512 + 2048], &src[..2048]);
    assert!(score >= 20.0, "delayed sine snr {score:.1}");
}

#[test]
fn loas_and_m4a_match_the_raw_au_delay() {
    let src = sine(512 * 6);
    let mut raw_enc = LdEncoder::new(48_000).unwrap();
    let mut loas_enc = LdEncoder::new(48_000).unwrap();
    let mut aus = Vec::new();
    let mut loas = Vec::new();
    for frame in src.chunks_exact(512) {
        aus.push(raw_enc.push_mono(frame).unwrap());
        loas.extend(loas_enc.push_loas(frame).unwrap());
    }
    let loas_pcm = decode_with(&loas, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(loas_pcm.sample_rate, 48_000);
    assert_eq!(loas_pcm.channels[0].len(), src.len());
    let score = snr(&loas_pcm.channels[0][512..512 + 2048], &src[..2048]);
    assert!(score >= 20.0, "loas snr {score:.1}");

    let m4a = crate::m4a_write::mux_aac(&aus, &raw_enc.asc(), 1, 48_000, 2048, 512, 512).unwrap();
    let m4a_pcm = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(m4a_pcm.channels[0].len(), 2048);
    assert_eq!(m4a_pcm.priming, Some(512));
    let score = snr(&m4a_pcm.channels[0], &src[..2048]);
    assert!(score >= 20.0, "m4a trimmed snr {score:.1}");
}

#[test]
fn stereo_loas_keeps_each_plane() {
    let n = 512 * 6;
    let left = sine(n);
    let right: Vec<f32> = (0..n)
        .map(|i| 0.3 * (i as f32 * 660.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin())
        .collect();
    let mut enc = LdEncoder::stereo(48_000).unwrap();
    let mut bytes = Vec::new();
    for (l, r) in left.chunks_exact(512).zip(right.chunks_exact(512)) {
        bytes.extend(enc.push_stereo_loas(l, r).unwrap());
    }
    let pcm = decode_with(&bytes, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(pcm.channels.len(), 2);
    assert!(snr(&pcm.channels[0][512..512 + 2048], &left[..2048]) >= 20.0);
    assert!(snr(&pcm.channels[1][512..512 + 2048], &right[..2048]) >= 20.0);
}

#[test]
fn bitrate_cap_limits_the_frame() {
    let mut s = 1u32;
    let pcm: Vec<f32> = (0..512 * 4)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 16) as i16 as f32 / 32768.0
        })
        .collect();
    let mut enc = LdEncoder::new(48_000).unwrap();
    enc.set_bitrate(24_000);
    let budget = 24_000u64 * 512 / 48_000;
    for frame in pcm.chunks_exact(512) {
        let au = enc.push_mono(frame).unwrap();
        assert!(
            (au.len() as u64) * 8 <= budget + 7,
            "frame {} bytes over {budget} bits",
            au.len()
        );
    }
}

#[test]
fn click_selects_the_low_overlap_window() {
    let mut enc = LdEncoder::new(48_000).unwrap();
    let quiet = enc.push_mono(&[0.0; 512]).unwrap();
    assert!(!shape_bit(&quiet), "silence stays on the sine window");
    let mut click = [0.0f32; 512];
    click[10] = 0.9;
    let au = enc.push_mono(&click).unwrap();
    assert!(shape_bit(&au), "a click from silence uses low-overlap");
}

#[test]
fn stereo_m4a_trims_the_512_sample_delay() {
    let n = 512 * 6;
    let left = sine(n);
    let right: Vec<f32> = (0..n)
        .map(|i| 0.3 * (i as f32 * 660.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin())
        .collect();
    let mut enc = LdEncoder::stereo(48_000).unwrap();
    let mut aus = Vec::new();
    for (l, r) in left.chunks_exact(512).zip(right.chunks_exact(512)) {
        aus.push(enc.push(&[l, r]).unwrap());
    }
    let m4a = crate::m4a_write::mux_aac(&aus, &enc.asc(), 2, 48_000, 2048, 512, 512).unwrap();
    let pcm = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(pcm.channels.len(), 2);
    assert_eq!(pcm.channels[0].len(), 2048);
    assert_eq!(pcm.priming, Some(512));
    assert!(snr(&pcm.channels[0], &left[..2048]) >= 20.0);
    assert!(snr(&pcm.channels[1], &right[..2048]) >= 20.0);
}

#[test]
fn loud_noise_stays_inside_6144_bits_per_channel() {
    let mut s = 1u32;
    let pcm: Vec<f32> = (0..512 * 4)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            if s & 1 == 0 { 1.0 } else { -1.0 }
        })
        .collect();
    let mut mono = LdEncoder::new(48_000).unwrap();
    mono.set_bitrate(u32::MAX);
    for frame in pcm.chunks_exact(512) {
        let au = mono.push_mono(frame).unwrap();
        assert!(au.len() * 8 <= 6144, "mono {} bytes", au.len());
    }
    let mut stereo = LdEncoder::stereo(48_000).unwrap();
    for (l, r) in pcm.chunks_exact(512).zip(pcm.chunks_exact(512)) {
        let au = stereo.push(&[l, r]).unwrap();
        assert!(au.len() * 8 <= 6144 * 2, "stereo {} bytes", au.len());
    }
}

#[test]
fn rejects_a_short_frame() {
    let mut enc = LdEncoder::new(48_000).unwrap();
    assert!(enc.push_mono(&[0.0; 511]).is_err());
}

/// Lab dump: `DUMP_LD_ORACLE=/tmp/syom-ld-enc cargo test --lib dump_ld_oracle_streams`.
/// Writes LOAS plus syom's own interleaved s16. Ordinary tests do not spawn FDK.
#[test]
fn dump_ld_oracle_streams() {
    let Ok(dir) = std::env::var("DUMP_LD_ORACLE") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    write_mono(&dir, "sine64.loas", 64_000, &sine(512 * 32));
    write_mono(&dir, "noise32.loas", 32_000, &noise(512 * 16));
    let mut click = vec![0.0f32; 512 * 8];
    click[512 * 2 + 10] = 0.9;
    write_mono(&dir, "click64.loas", 64_000, &click);
    let mut impulse = vec![0.0f32; 512 * 8];
    impulse[0] = 1.0;
    write_mono(&dir, "impulse64.loas", 64_000, &impulse);
    let n = 512 * 32;
    let right: Vec<f32> = (0..n)
        .map(|i| 0.3 * (i as f32 * 660.0 * 2.0 * std::f32::consts::PI / 48_000.0).sin())
        .collect();
    write_stereo(&dir, "stereo128.loas", 128_000, &sine(n), &right);
}

fn write_mono(dir: &std::path::Path, name: &str, bps: u32, pcm: &[f32]) {
    let mut enc = LdEncoder::new(48_000).unwrap();
    enc.set_bitrate(bps);
    let mut bytes = Vec::new();
    for frame in pcm.chunks_exact(512) {
        bytes.extend(enc.push_loas(frame).unwrap());
    }
    std::fs::write(dir.join(name), &bytes).unwrap();
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).unwrap();
    std::fs::write(
        dir.join(name.replace(".loas", ".syom.s16")),
        s16_interleaved(&dec.channels),
    )
    .unwrap();
}

fn write_stereo(dir: &std::path::Path, name: &str, bps: u32, left: &[f32], right: &[f32]) {
    let mut enc = LdEncoder::stereo(48_000).unwrap();
    enc.set_bitrate(bps);
    let mut bytes = Vec::new();
    for (l, r) in left.chunks_exact(512).zip(right.chunks_exact(512)) {
        bytes.extend(enc.push_stereo_loas(l, r).unwrap());
    }
    std::fs::write(dir.join(name), &bytes).unwrap();
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).unwrap();
    std::fs::write(
        dir.join(name.replace(".loas", ".syom.s16")),
        s16_interleaved(&dec.channels),
    )
    .unwrap();
}

#[test]
fn burst_peaks_one_frame_later() {
    let mut pcm = vec![0.0f32; 512 * 6];
    for sample in pcm.iter_mut().take(32) {
        *sample = 0.8;
    }
    let mut enc = LdEncoder::new(48_000).unwrap();
    enc.set_bitrate(64_000);
    let mut bytes = Vec::new();
    for frame in pcm.chunks_exact(512) {
        bytes.extend(enc.push_loas(frame).unwrap());
    }
    let dec = decode_with(&bytes, &DecodeOptions::unbounded()).unwrap();
    let plane = &dec.channels[0];
    let (idx, val) = plane
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .unwrap();
    assert!(
        (idx as i32 - 512).abs() <= 64 && val.abs() > 0.05,
        "peak idx {idx} val {val}"
    );
}

#[test]
fn asc_matches_the_fdk_ld_layout() {
    let enc = LdEncoder::new(48_000).unwrap();
    let (asc, bits) = super::super::asc::AudioSpecificConfig::parse(&enc.asc()).unwrap();
    assert_eq!(asc.aot, 23);
    assert_eq!(asc.channel_configuration, 1);
    assert_eq!(bits, 22);
    let enc = LdEncoder::stereo(48_000).unwrap();
    let (asc, bits) = super::super::asc::AudioSpecificConfig::parse(&enc.asc()).unwrap();
    assert_eq!((asc.channel_configuration, bits), (2, 22));
}

/// `det_math` tone so a re-encode matches the committed LOAS on every platform.
fn tone(n: usize, freq: f32, amp: f32) -> Vec<f32> {
    let mut phase = 0.0f32;
    let step = 2.0 * std::f32::consts::PI * freq / 48_000.0;
    (0..n)
        .map(|_| {
            let v = amp * crate::engine::det_math::sincos(phase).0;
            phase += step;
            if phase >= std::f32::consts::TAU {
                phase -= std::f32::consts::TAU;
            }
            v
        })
        .collect()
}

fn loas_mono(bps: u32, pcm: &[f32]) -> Vec<u8> {
    let mut enc = LdEncoder::new(48_000).unwrap();
    enc.set_bitrate(bps);
    let mut bytes = Vec::new();
    for frame in pcm.chunks_exact(512) {
        bytes.extend(enc.push_loas(frame).unwrap());
    }
    bytes
}

fn loas_stereo(bps: u32, left: &[f32], right: &[f32]) -> Vec<u8> {
    let mut enc = LdEncoder::stereo(48_000).unwrap();
    enc.set_bitrate(bps);
    let mut bytes = Vec::new();
    for (l, r) in left.chunks_exact(512).zip(right.chunks_exact(512)) {
        bytes.extend(enc.push_stereo_loas(l, r).unwrap());
    }
    bytes
}

/// Offline mint: `MINT_LD_GOLDENS=1 cargo test --lib mint_ld_oracle_loas`,
/// then decode with FDK and libxaac into the sibling `.fdk.s16` / `.xaac.s16`.
/// Tests never spawn those decoders.
#[test]
fn mint_ld_oracle_loas() {
    if std::env::var("MINT_LD_GOLDENS").is_err() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    let n = 512 * 32;
    std::fs::write(
        dir.join("ldenc64.loas"),
        loas_mono(64_000, &tone(n, 440.0, 0.4)),
    )
    .unwrap();
    std::fs::write(
        dir.join("ldenc128s.loas"),
        loas_stereo(128_000, &tone(n, 440.0, 0.4), &tone(n, 660.0, 0.3)),
    )
    .unwrap();
}

#[test]
fn fdk_and_xaac_match_our_decode_of_our_ld() {
    let n = 512 * 32;
    let mono = loas_mono(64_000, &tone(n, 440.0, 0.4));
    assert_eq!(
        mono.as_slice(),
        &include_bytes!("../goldens/ldenc64.loas")[..]
    );
    let pcm = decode_with(&mono, &DecodeOptions::unbounded()).unwrap();
    assert_gate(
        "ldenc64 fdk",
        &pcm.channels[0],
        &deinterleave(include_bytes!("../goldens/ldenc64.fdk.s16"), 1)[0],
    );
    assert_gate(
        "ldenc64 xaac",
        &pcm.channels[0],
        &deinterleave(include_bytes!("../goldens/ldenc64.xaac.s16"), 1)[0],
    );
    let left = tone(n, 440.0, 0.4);
    let right = tone(n, 660.0, 0.3);
    let stereo = loas_stereo(128_000, &left, &right);
    assert_eq!(
        stereo.as_slice(),
        &include_bytes!("../goldens/ldenc128s.loas")[..]
    );
    let pcm = decode_with(&stereo, &DecodeOptions::unbounded()).unwrap();
    for (label, bytes) in [
        ("fdk", &include_bytes!("../goldens/ldenc128s.fdk.s16")[..]),
        ("xaac", &include_bytes!("../goldens/ldenc128s.xaac.s16")[..]),
    ] {
        let planes = deinterleave(bytes, 2);
        assert_gate(
            &format!("ldenc128s {label} L"),
            &pcm.channels[0],
            &planes[0],
        );
        assert_gate(
            &format!("ldenc128s {label} R"),
            &pcm.channels[1],
            &planes[1],
        );
    }
}

fn deinterleave(s16: &[u8], ch: usize) -> Vec<Vec<i16>> {
    let mut planes = vec![Vec::with_capacity(s16.len() / (2 * ch)); ch];
    for frame in s16.chunks_exact(2 * ch) {
        for (plane, bytes) in planes.iter_mut().zip(frame.chunks_exact(2)) {
            plane.push(i16::from_le_bytes([bytes[0], bytes[1]]));
        }
    }
    planes
}

fn assert_gate(label: &str, ours: &[f32], gold: &[i16]) {
    assert_eq!(ours.len(), gold.len(), "{label} length");
    let mut max_lsb = 0u32;
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for (i, &gv) in gold.iter().enumerate() {
        let ov = (f64::from(ours[i]) * 32768.0)
            .round()
            .clamp(-32768.0, 32767.0) as i16;
        max_lsb = max_lsb.max((i32::from(gv) - i32::from(ov)).unsigned_abs());
        ps += f64::from(gv) * f64::from(gv);
        let e = f64::from(gv) - f64::from(ov);
        pe += e * e;
    }
    let snr = if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    };
    assert!(
        max_lsb <= 2 && snr >= 55.0,
        "{label}: max_lsb {max_lsb} snr {snr:.1}"
    );
}

fn noise(n: usize) -> Vec<f32> {
    let mut s = 1u32;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (s >> 16) as i16 as f32 / 32768.0
        })
        .collect()
}

fn s16_interleaved(planes: &[Vec<f32>]) -> Vec<u8> {
    let n = planes[0].len();
    let mut out = Vec::with_capacity(n * planes.len() * 2);
    for i in 0..n {
        for plane in planes {
            let v = (f64::from(plane[i]) * 32768.0)
                .round()
                .clamp(-32768.0, 32767.0) as i16;
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

fn shape_bit(au: &[u8]) -> bool {
    let mut br = crate::engine::bits::BitReader::new(au);
    let _ = br.read(4).unwrap();
    let _ = br.read(8).unwrap();
    let _ = br.read(1).unwrap();
    let _ = br.read(2).unwrap();
    br.read_bit().unwrap()
}
