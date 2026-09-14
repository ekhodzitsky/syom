//! TASK-76: opt-in intensity stereo. Default off.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::adts::AdtsHeader;
use crate::engine::bits::BitReader;
use crate::engine::ics::IcsInfo;
use crate::engine::ics_body::parse_ics;
use crate::engine::section::is_intensity;
use crate::engine::stereo::MsInfo;
use crate::{DecodeOptions, EncodeOptions, Encoder, decode_with, encode, encode_with};

const RATE: u32 = 48_000;
const PRIME: usize = 1024;
const LOW: u32 = 64_000;

fn sine(n: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin())
        .collect()
}

fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn rms(x: &[f32]) -> f64 {
    if x.is_empty() {
        return 0.0;
    }
    let s: f64 = x.iter().map(|&v| f64::from(v) * f64::from(v)).sum();
    (s / x.len() as f64).sqrt()
}

fn corr(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    let mut sa = 0.0f64;
    let mut sb = 0.0f64;
    let mut sab = 0.0f64;
    for i in 0..n {
        let x = f64::from(a[i]);
        let y = f64::from(b[i]);
        sa += x * x;
        sb += y * y;
        sab += x * y;
    }
    if sa == 0.0 || sb == 0.0 {
        0.0
    } else {
        sab / (sa.sqrt() * sb.sqrt())
    }
}

fn snr_aligned(want: &[f32], got: &[f32]) -> f64 {
    let n = want.len();
    if got.len() < n + PRIME {
        return 0.0;
    }
    let w = &want[PRIME..n.saturating_sub(PRIME)];
    let g = &got[2 * PRIME..2 * PRIME + w.len()];
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for i in 0..w.len().min(g.len()) {
        let s = f64::from(w[i]);
        let e = s - f64::from(g[i]);
        ps += s * s;
        pe += e * e;
    }
    if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    }
}

fn valid(dec: &[f32], n: usize) -> &[f32] {
    let lo = 2 * PRIME;
    let hi = (lo + n.saturating_sub(2 * PRIME)).min(dec.len().saturating_sub(PRIME));
    if hi > lo { &dec[lo..hi] } else { &[] }
}

fn is_hcb_count(adts: &[u8]) -> usize {
    let mut i = 0usize;
    let mut n = 0usize;
    while i + 7 <= adts.len() {
        let Ok((hdr, off)) = AdtsHeader::parse(&adts[i..]) else {
            break;
        };
        let flen = hdr.aac_frame_length as usize;
        if flen < off || i + flen > adts.len() {
            break;
        }
        n += intensity_in_payload(&adts[i + off..i + flen], hdr.sampling_frequency_index);
        i += flen;
    }
    n
}

fn intensity_in_payload(payload: &[u8], fs: u8) -> usize {
    let mut br = BitReader::new(payload);
    let Ok(id) = br.read(3) else {
        return 0;
    };
    let _ = br.read(4);
    match id {
        1 => {
            if !br.read_bit().unwrap_or(false) {
                return 0;
            }
            let Ok(ics) = IcsInfo::parse(&mut br, fs, true) else {
                return 0;
            };
            let _ = MsInfo::parse(&mut br, &ics);
            let mut n = 0usize;
            for _ in 0..2 {
                if let Ok(body) = parse_ics(&mut br, fs, 2, Some(&ics)) {
                    n += body
                        .sections
                        .sfb_cb
                        .iter()
                        .flatten()
                        .filter(|&&cb| is_intensity(cb))
                        .count();
                }
            }
            n
        }
        _ => 0,
    }
}

#[test]
fn default_encode_matches_explicit_intensity_off() {
    let pcm = vec![sine(4096, 440.0, 0.5), sine(4096, 440.0, 0.5)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_intensity(false)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn intensity_is_deterministic() {
    let n = 4096;
    let pcm = vec![sine(n, 8000.0, 0.4), sine(n, 8000.0, 0.2)];
    let opts = EncodeOptions::adts()
        .with_bitrate_bps(LOW)
        .with_intensity(true);
    assert_eq!(
        encode_with(&pcm, RATE, &opts).unwrap(),
        encode_with(&pcm, RATE, &opts).unwrap()
    );
}

#[test]
fn intensity_silence_stays_silent() {
    let pcm = vec![vec![0.0f32; 4096], vec![0.0f32; 4096]];
    let adts = encode_with(
        &pcm,
        RATE,
        &EncodeOptions::adts()
            .with_bitrate_bps(LOW)
            .with_intensity(true),
    )
    .unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let peak = dec
        .channels
        .iter()
        .flat_map(|c| c.iter())
        .fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak < 1e-4, "IS silence peak {peak}");
    assert_eq!(is_hcb_count(&adts), 0);
}

#[test]
fn intensity_stream_matches_oneshot() {
    let n = 3000;
    let pcm = vec![sine(n, 8000.0, 0.4), sine(n, 8000.0, 0.2)];
    let opts = EncodeOptions::adts()
        .with_bitrate_bps(LOW)
        .with_intensity(true);
    let want = encode_with(&pcm, RATE, &opts).unwrap();
    let mut enc = Encoder::new(RATE, 2, &opts).unwrap();
    let mut got = Vec::new();
    let mut cb = |f: crate::EncodedFrame<'_>| {
        got.extend_from_slice(f.au);
        Ok(())
    };
    enc.feed(&[&pcm[0], &pcm[1]], &mut cb).unwrap();
    enc.finish(&mut cb).unwrap();
    assert_eq!(got, want);
}

#[test]
fn intensity_lf_panned_does_not_fire() {
    // 440 Hz is below the 6 kHz IS floor — Huffman must keep the image.
    let n = RATE as usize / 2;
    let pcm = vec![sine(n, 440.0, 0.5), sine(n, 440.0, 0.15)];
    let on = encode_with(
        &pcm,
        RATE,
        &EncodeOptions::adts()
            .with_bitrate_bps(LOW)
            .with_intensity(true),
    )
    .unwrap();
    assert_eq!(is_hcb_count(&on), 0, "LF panned tone must not get IS");
}

#[test]
fn intensity_panned_hf_ild_and_rate() {
    let n = RATE as usize; // 1 s, predeclared low-rate panned-HF cell
    let pcm = vec![sine(n, 8000.0, 0.5), sine(n, 8000.0, 0.15)];
    let opts_off = EncodeOptions::adts().with_bitrate_bps(LOW);
    let opts_on = opts_off.clone().with_intensity(true);
    let off = encode_with(&pcm, RATE, &opts_off).unwrap();
    let on = encode_with(&pcm, RATE, &opts_on).unwrap();
    let n_is = is_hcb_count(&on);
    let d_off = decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let src_ild = rms(&pcm[1][PRIME..n - PRIME]) / rms(&pcm[0][PRIME..n - PRIME]).max(1e-9);
    let on_ild = rms(valid(&d_on.channels[1], n)) / rms(valid(&d_on.channels[0], n)).max(1e-9);
    let save = 100.0 * (1.0 - on.len() as f64 / off.len().max(1) as f64);
    let snr_off = snr_aligned(&pcm[0], &d_off.channels[0]);
    let snr_on = snr_aligned(&pcm[0], &d_on.channels[0]);
    eprintln!(
        "TASK-76 IS panned 8 kHz 64k: HCB {n_is}; bytes {} vs {} ({save:.1}%); ILD src {src_ild:.3} on {on_ild:.3}; L SNR off {snr_off:.2} on {snr_on:.2} Δ {:.2} dB",
        off.len(),
        on.len(),
        snr_on - snr_off
    );
    assert!(n_is > 0, "panned 8 kHz should emit intensity");
    assert!(
        (on_ild - src_ild).abs() / src_ild < 0.5,
        "ILD collapsed {on_ild} vs {src_ild}"
    );
}

#[test]
fn intensity_antiphase_stays_negative() {
    let n = RATE as usize / 2;
    let l = sine(n, 8000.0, 0.4);
    let r: Vec<f32> = l.iter().map(|&x| -x).collect();
    let pcm = vec![l, r];
    let on = encode_with(
        &pcm,
        RATE,
        &EncodeOptions::adts()
            .with_bitrate_bps(LOW)
            .with_intensity(true),
    )
    .unwrap();
    let dec = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let c = corr(valid(&dec.channels[0], n), valid(&dec.channels[1], n));
    eprintln!(
        "TASK-76 IS anti-phase 8 kHz corr {c:.3}; HCB {}",
        is_hcb_count(&on)
    );
    assert!(c < -0.5, "anti-phase became in-phase corr {c:.3}");
}

#[test]
fn intensity_ambience_does_not_collapse() {
    let n = RATE as usize / 2;
    let pcm = vec![noise(n, 0.4, 1), noise(n, 0.4, 99)];
    let on = encode_with(
        &pcm,
        RATE,
        &EncodeOptions::adts()
            .with_bitrate_bps(LOW)
            .with_intensity(true),
    )
    .unwrap();
    let dec = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let c = corr(valid(&dec.channels[0], n), valid(&dec.channels[1], n));
    eprintln!(
        "TASK-76 IS uncorrelated noise corr {c:.3}; HCB {}",
        is_hcb_count(&on)
    );
    assert!(c < 0.5, "ambience collapsed to mid corr {c:.3}");
}

#[test]
fn intensity_mono_is_noop() {
    let pcm = vec![sine(4096, 8000.0, 0.4)];
    let a = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_intensity(true)).unwrap();
    assert_eq!(a, b);
}
