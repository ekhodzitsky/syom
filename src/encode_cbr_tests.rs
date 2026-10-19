//! Opt-in CBR bit reservoir (TASK-112.11). Default ABR is not exercised here
//! except to prove `with_cbr(false)` does not move bytes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::encode_cmp::adts_payload_bytes;
use crate::engine::adts::AdtsHeader;
use crate::engine::enc_frame::MAX_BITS_PER_CHANNEL;
use crate::{DecodeOptions, EncodeOptions, Encoder, Result, decode_with, encode_with};

const RATE: u32 = 48_000;
const N1: usize = 48_000;

fn sine(n: usize, hz: f64, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin() as f32)
        .collect()
}

fn noise(n: usize, amp: f32) -> Vec<f32> {
    let mut s = 0xC0FF_EE01u32;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn frames(adts: &[u8]) -> Vec<(u16, usize)> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < adts.len() {
        let (hdr, off) = AdtsHeader::parse(&adts[pos..]).expect("adts");
        let fl = usize::from(hdr.aac_frame_length);
        out.push((hdr.adts_buffer_fullness, fl - off));
        pos += fl;
    }
    out
}

fn cbr_opts(bps: u32) -> EncodeOptions {
    EncodeOptions::adts().with_bitrate_bps(bps).with_cbr(true)
}

#[test]
fn cbr_defaults_off_and_matches_abr_bytes() {
    assert!(!EncodeOptions::default().cbr);
    assert!(!EncodeOptions::adts().with_cbr(true).with_cbr(false).cbr);
    let pcm = vec![sine(N1, 440.0, 0.4)];
    let abr = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts().with_cbr(false)).unwrap();
    assert_eq!(abr, off);
}

#[test]
fn cbr_rejects_he_ld_quality_and_surround() {
    let he = EncodeOptions::adts()
        .with_he(true)
        .with_bitrate_bps(48_000)
        .with_cbr(true);
    let pcm = vec![vec![0.0f32; 2048]; 2];
    let err = encode_with(&pcm, RATE, &he).unwrap_err();
    assert!(err.to_string().contains("CBR"), "{err}");

    let ld = EncodeOptions::m4a().with_ld(true).with_cbr(true);
    let err = encode_with(&[vec![0.1f32; 1024]], RATE, &ld).unwrap_err();
    assert!(err.to_string().contains("CBR"), "{err}");

    let q = EncodeOptions::adts().with_quality(5).with_cbr(true);
    let err = encode_with(&[vec![0.1f32; 1024]], RATE, &q).unwrap_err();
    assert!(err.to_string().contains("CBR"), "{err}");

    let planes = vec![vec![0.1f32; 1024]; 3];
    let err = encode_with(&planes, RATE, &cbr_opts(128_000)).unwrap_err();
    assert!(err.to_string().contains("mono or stereo"), "{err}");
}

#[test]
fn cbr_fullness_is_not_vbr_and_frames_stay_inside_the_cap() -> Result<()> {
    let pcm = vec![sine(N1, 440.0, 0.5), sine(N1, 880.0, 0.35)];
    let adts = encode_with(&pcm, RATE, &cbr_opts(128_000))?;
    let body = crate::gapless::strip_id3(&adts);
    let fr = frames(body);
    assert!(fr.len() > 8, "{}", fr.len());
    let cap = MAX_BITS_PER_CHANNEL * 2;
    for &(full, pay) in &fr {
        assert_ne!(full, 0x7FF, "CBR must not use the VBR marker");
        assert!(full <= 0x7FE);
        assert!(pay * 8 <= cap, "{pay} bytes");
    }
    let dec = decode_with(&adts, &DecodeOptions::unbounded())?;
    assert_eq!(dec.channels.len(), 2);
    assert_eq!(dec.channels[0].len(), N1);
    assert!(
        dec.channels[0]
            .iter()
            .any(|s| s.is_finite() && s.abs() > 1e-4)
    );
    Ok(())
}

#[test]
fn cbr_reservoir_lets_a_hard_frame_exceed_the_average() -> Result<()> {
    let bps = 24_000u32;
    let quiet = sine(N1 * 2, 220.0, 0.02);
    let loud = noise(N1, 0.85);
    let mut pcm = quiet;
    pcm.extend_from_slice(&loud);
    let adts = encode_with(&[pcm], RATE, &cbr_opts(bps))?;
    let body = crate::gapless::strip_id3(&adts);
    let fr = frames(body);
    let budget = (u64::from(bps) * 1024 / u64::from(RATE)) as usize;
    let bits: Vec<usize> = fr.iter().map(|f| f.1 * 8).collect();
    let max = *bits.iter().max().unwrap();
    assert!(max > budget, "max {max} bits, budget {budget}");
    assert!(bits.iter().all(|&b| b <= MAX_BITS_PER_CHANNEL));
    assert!(
        fr.iter().any(|&(full, _)| full > 0),
        "reservoir never filled"
    );
    assert!(fr.iter().all(|&(full, _)| full != 0x7FF));
    Ok(())
}

#[test]
fn cbr_ten_second_sine_stays_within_three_percent() -> Result<()> {
    let secs = 10usize;
    let n = N1 * secs;
    let pcm = vec![sine(n, 440.0, 0.5), sine(n, 660.0, 0.35)];
    let bps = 128_000u32;
    let opts = cbr_opts(bps);
    let mut enc = Encoder::new(RATE, pcm.len(), &opts)?;
    let mut streamed = Vec::new();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |f| {
        streamed.extend_from_slice(f.au);
        Ok(())
    })?;
    let info = enc.finish(|f| {
        streamed.extend_from_slice(f.au);
        Ok(())
    })?;
    let oneshot = encode_with(&pcm, RATE, &opts)?;
    assert_eq!(streamed.as_slice(), crate::gapless::strip_id3(&oneshot));
    let payload = adts_payload_bytes(&streamed).expect("payload");
    let pay_valid = 8.0 * payload as f64 * f64::from(RATE) / info.samples as f64;
    let ratio = pay_valid / f64::from(bps);
    let pct = (ratio - 1.0) * 100.0;
    assert!(
        (0.97..=1.03).contains(&ratio),
        "cbr 10s stereo sine 128k pay/valid {pay_valid:.1} ({pct:+.3}%)"
    );
    let fr = frames(&streamed);
    let cap = MAX_BITS_PER_CHANNEL * 2;
    assert!(
        fr.iter()
            .all(|&(full, pay)| full != 0x7FF && pay * 8 <= cap)
    );
    eprintln!("cbr_10s_stereo_sine_128k {pct:+.3}%");
    Ok(())
}
