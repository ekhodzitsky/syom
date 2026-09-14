//! TASK-65: live rate accounting on shipped `encode_with` / `Encoder`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::encode_cmp::adts_payload_bytes;
use crate::engine::adts::AdtsHeader;
use crate::engine::enc_frame::MAX_BITS_PER_CHANNEL;
use crate::{DecodeOptions, EncodeInfo, EncodeOptions, Encoder, Result, decode_with, encode_with};

const RATE: u32 = 48_000;
const N1: usize = 48_000; // 1 s, same as TASK-16

struct Sweep {
    name: &'static str,
    class: &'static str,
    pcm: Vec<Vec<f32>>,
    bps: u32,
}

struct Row {
    name: &'static str,
    class: &'static str,
    requested: u32,
    info: EncodeInfo,
    payload: usize,
    n_frames: usize,
    bits_min: usize,
    bits_max: usize,
    bits_mean: f64,
    budget: usize,
    payload_bps_valid: f64,
    payload_bps_coded: f64,
    adts_bps_valid: f64,
    fullness_vbr: bool,
}

fn sine(n: usize, hz: f64, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin() as f32)
        .collect()
}

fn noise(n: usize, amp: f32) -> Vec<f32> {
    let mut s = 0x0BAD_F00Du32;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn clicks(n: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    let mut i = 0;
    while i < n {
        v[i] = 0.9;
        i += 2048;
    }
    v
}

fn encode_tracked(pcm: &[Vec<f32>], bps: u32) -> Result<(Vec<u8>, EncodeInfo)> {
    let opts = EncodeOptions::adts().with_bitrate_bps(bps);
    let mut enc = Encoder::new(RATE, pcm.len(), &opts)?;
    let mut out = Vec::new();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    let info = enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    Ok((out, info))
}

fn walk_frames(adts: &[u8]) -> Result<Vec<(u16, usize)>> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < adts.len() {
        let (hdr, off) = AdtsHeader::parse(&adts[pos..]).map_err(crate::AacError::from)?;
        let fl = usize::from(hdr.aac_frame_length);
        out.push((hdr.adts_buffer_fullness, fl - off));
        pos += fl;
    }
    Ok(out)
}

fn budget_bits(bps: u32, ch: usize) -> usize {
    let bits = u64::from(bps) * 1024 / u64::from(RATE);
    (bits as usize).min(MAX_BITS_PER_CHANNEL * ch)
}

fn row(s: &Sweep) -> Result<Row> {
    let (adts, info) = encode_tracked(&s.pcm, s.bps)?;
    let oneshot = encode_with(&s.pcm, RATE, &EncodeOptions::adts().with_bitrate_bps(s.bps))?;
    assert_eq!(adts, oneshot, "{} streaming != one-shot", s.name);
    assert_eq!(info.bytes as usize, adts.len());
    let frames = walk_frames(&adts)?;
    let payload = adts_payload_bytes(&adts).expect("payload");
    assert_eq!(payload, frames.iter().map(|f| f.1).sum::<usize>());
    assert_eq!(frames.len() as u64, info.aac_frames);
    let n = info.samples;
    let coded = info.coded_samples;
    let payload_bps_valid = 8.0 * payload as f64 * f64::from(RATE) / n as f64;
    let payload_bps_coded = 8.0 * payload as f64 * f64::from(RATE) / coded as f64;
    let adts_bps_valid = 8.0 * adts.len() as f64 * f64::from(RATE) / n as f64;
    let bits: Vec<usize> = frames.iter().map(|f| f.1 * 8).collect();
    let bits_min = *bits.iter().min().unwrap();
    let bits_max = *bits.iter().max().unwrap();
    let bits_mean = bits.iter().sum::<usize>() as f64 / bits.len() as f64;
    Ok(Row {
        name: s.name,
        class: s.class,
        requested: s.bps,
        info,
        payload,
        n_frames: frames.len(),
        bits_min,
        bits_max,
        bits_mean,
        budget: budget_bits(s.bps, s.pcm.len()),
        payload_bps_valid,
        payload_bps_coded,
        adts_bps_valid,
        fullness_vbr: frames.iter().all(|f| f.0 == 0x7FF),
    })
}

fn matrix() -> Result<Vec<Sweep>> {
    let lecture = decode_with(
        include_bytes!("goldens/lecture.m4a"),
        &DecodeOptions::audio(),
    )?;
    let n = lecture.channels[0].len().min(N1);
    let speech: Vec<Vec<f32>> = lecture.channels.iter().map(|p| p[..n].to_vec()).collect();
    let trem_l = sine(N1, 440.0, 0.4);
    let trem_r: Vec<f32> = trem_l.iter().map(|x| x * 0.8).collect();
    Ok(vec![
        Sweep {
            name: "silence",
            class: "silence",
            pcm: vec![vec![0.0f32; N1]],
            bps: 128_000,
        },
        Sweep {
            name: "sine440",
            class: "tonal",
            pcm: vec![sine(N1, 440.0, 0.5)],
            bps: 64_000,
        },
        Sweep {
            name: "sine440",
            class: "tonal",
            pcm: vec![sine(N1, 440.0, 0.5)],
            bps: 128_000,
        },
        Sweep {
            name: "noise",
            class: "noise",
            pcm: vec![noise(N1, 0.4)],
            bps: 64_000,
        },
        Sweep {
            name: "noise",
            class: "noise",
            pcm: vec![noise(N1, 0.4)],
            bps: 128_000,
        },
        Sweep {
            name: "click",
            class: "transient",
            pcm: vec![clicks(N1)],
            bps: 64_000,
        },
        Sweep {
            name: "click",
            class: "transient",
            pcm: vec![clicks(N1)],
            bps: 128_000,
        },
        Sweep {
            name: "tremolo",
            class: "stereo",
            pcm: vec![trem_l, trem_r],
            bps: 128_000,
        },
        Sweep {
            name: "lecture",
            class: "speech",
            pcm: speech,
            bps: 128_000,
        },
    ])
}

fn ratio(r: &Row) -> f64 {
    r.payload_bps_valid / f64::from(r.requested)
}

#[test]
fn live_rate_matrix_payload_vs_transport_vs_duration() -> Result<()> {
    let rows: Vec<Row> = matrix()?.iter().map(|s| row(s).unwrap()).collect();
    eprintln!(
        "clip class req payload adts frames valid coded pay/valid pay/coded adts/valid bits[min,mean,max] budget ratio"
    );
    for r in &rows {
        eprintln!(
            "{} {} {} {} {} {} {} {} {:.0} {:.0} {:.0} [{}, {:.0}, {}] {} {:.3}",
            r.name,
            r.class,
            r.requested,
            r.payload,
            r.info.bytes,
            r.n_frames,
            r.info.samples,
            r.info.coded_samples,
            r.payload_bps_valid,
            r.payload_bps_coded,
            r.adts_bps_valid,
            r.bits_min,
            r.bits_mean,
            r.bits_max,
            r.budget,
            ratio(r)
        );
    }
    for r in &rows {
        assert!(r.fullness_vbr, "{}: ADTS fullness is not VBR 0x7FF", r.name);
        assert_eq!(
            r.info.bytes as usize,
            r.payload + 7 * r.n_frames,
            "{}: ADTS bytes != payload + 7*frames",
            r.name
        );
        assert_eq!(r.info.priming, 1024);
        assert_eq!(r.info.coded_samples, r.info.aac_frames * 1024);
        assert!(
            r.info.coded_samples > r.info.samples,
            "{}: drain/remainder must lengthen coded vs valid N",
            r.name
        );
        let cap = MAX_BITS_PER_CHANNEL * r.info.channels;
        assert!(
            r.bits_max <= cap,
            "{}: frame {} bits > LC cap {cap}",
            r.name,
            r.bits_max
        );
        assert!(
            r.payload_bps_coded < r.payload_bps_valid,
            "{}: payload/coded must be below payload/valid",
            r.name
        );
    }
    let find = |name, bps| {
        rows.iter()
            .find(|r| r.name == name && r.requested == bps)
            .unwrap()
    };
    let silence = find("silence", 128_000);
    let sine64 = find("sine440", 64_000);
    let sine128 = find("sine440", 128_000);
    let noise64 = find("noise", 64_000);
    let noise128 = find("noise", 128_000);
    let trem = find("tremolo", 128_000);
    let speech = find("lecture", 128_000);
    // Current contract (TASK-65): bitrate_bps is a per-frame ceiling.
    // Easy content undershoots; noise spends the budget.
    assert!(
        ratio(silence) < 0.15,
        "silence should not spend 128k: {:.0}",
        silence.payload_bps_valid
    );
    assert!(
        ratio(sine64) < 0.50 && ratio(sine128) < 0.40,
        "tonal undershoot: 64k {:.0} 128k {:.0}",
        sine64.payload_bps_valid,
        sine128.payload_bps_valid
    );
    assert!(
        (0.80..=1.20).contains(&ratio(noise64)) && (0.80..=1.20).contains(&ratio(noise128)),
        "noise should spend the ceiling: 64k {:.0} 128k {:.0}",
        noise64.payload_bps_valid,
        noise128.payload_bps_valid
    );
    assert!(
        ratio(trem) < 0.50,
        "correlated stereo undershoots 128k: {:.0}",
        trem.payload_bps_valid
    );
    assert!(
        ratio(speech) < 0.85,
        "lecture speech does not hit 128k today: {:.0}",
        speech.payload_bps_valid
    );
    Ok(())
}

#[test]
fn four_second_sine_still_undershoots_so_drain_is_not_the_cause() -> Result<()> {
    let pcm = vec![sine(4 * N1, 440.0, 0.5)];
    let r = row(&Sweep {
        name: "sine4s",
        class: "tonal",
        pcm,
        bps: 128_000,
    })?;
    assert!(
        ratio(&r) < 0.40,
        "4 s sine payload/valid still {:.0} bps vs 128k",
        r.payload_bps_valid
    );
    assert!(
        r.bits_mean < r.budget as f64 * 0.30,
        "mean frame bits {:.0} vs budget {}",
        r.bits_mean,
        r.budget
    );
    Ok(())
}

#[test]
fn rate_report_names_measurement_and_task66_gates() {
    let raw = include_str!("../corpus/rate/REPORT.md");
    assert!(raw.contains("pay/valid"));
    assert!(raw.contains("0x7FF"));
    assert!(raw.contains("±3%"));
    assert!(raw.contains("TASK-66"));
    assert!(raw.contains("credit"));
}
