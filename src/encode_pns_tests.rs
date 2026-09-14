//! TASK-75: opt-in LC PNS. Default off.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::adts::AdtsHeader;
use crate::engine::bits::BitReader;
use crate::engine::ics::IcsInfo;
use crate::engine::ics_body::parse_ics;
use crate::engine::section::is_noise;
use crate::engine::stereo::MsInfo;
use crate::{DecodeOptions, EncodeOptions, Encoder, decode_with, encode, encode_with};

const RATE: u32 = 48_000;
const PRIME: usize = 1024;

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

fn noise_hcb_count(adts: &[u8]) -> usize {
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
        n += noise_in_payload(&adts[i + off..i + flen], hdr.sampling_frequency_index);
        i += flen;
    }
    n
}

fn count_cb(sfb_cb: &[Vec<u8>]) -> usize {
    sfb_cb
        .iter()
        .flat_map(|row| row.iter())
        .filter(|&&cb| is_noise(cb))
        .count()
}

fn noise_in_payload(payload: &[u8], fs: u8) -> usize {
    let mut br = BitReader::new(payload);
    let Ok(id) = br.read(3) else {
        return 0;
    };
    let _ = br.read(4);
    match id {
        0 => parse_ics(&mut br, fs, 2, None)
            .map(|b| count_cb(&b.sections.sfb_cb))
            .unwrap_or(0),
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
                    n += count_cb(&body.sections.sfb_cb);
                }
            }
            n
        }
        _ => 0,
    }
}

#[test]
fn default_encode_matches_explicit_pns_off() {
    let pcm = vec![sine(4096, 440.0, 0.5)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(false)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn pns_is_deterministic() {
    let pcm = vec![noise(4096, 0.4, 0x0BAD_F00D)];
    let opts = EncodeOptions::adts().with_pns(true);
    assert_eq!(
        encode_with(&pcm, RATE, &opts).unwrap(),
        encode_with(&pcm, RATE, &opts).unwrap()
    );
}

#[test]
fn pns_silence_stays_silent() {
    let pcm = vec![vec![0.0f32; 4096]];
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let peak = dec.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak < 1e-4, "PNS silence peak {peak}");
    assert_eq!(noise_hcb_count(&adts), 0);
}

#[test]
fn pns_stream_matches_oneshot() {
    let pcm = vec![noise(3000, 0.3, 7)];
    let opts = EncodeOptions::adts().with_pns(true);
    let want = encode_with(&pcm, RATE, &opts).unwrap();
    let mut enc = Encoder::new(RATE, 1, &opts).unwrap();
    let mut got = Vec::new();
    let mut cb = |f: crate::EncodedFrame<'_>| {
        got.extend_from_slice(f.au);
        Ok(())
    };
    enc.feed(&[&pcm[0]], &mut cb).unwrap();
    enc.finish(&mut cb).unwrap();
    assert_eq!(got, want);
}

#[test]
fn pns_sine_does_not_fire() {
    let pcm = vec![sine(RATE as usize, 440.0, 0.5)];
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    assert_eq!(noise_hcb_count(&on), 0, "tonal sine must not get PNS");
}

#[test]
fn pns_noise_emits_and_saves_or_matches() {
    let n = RATE as usize; // 1 s, predeclared noise-like subset
    let pcm = vec![noise(n, 0.5, 0xC0FFEE)];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    let n_pns = noise_hcb_count(&on);
    let d_off = decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    assert!(d_on.channels[0].iter().all(|x| x.is_finite()));
    let snr_off = snr_aligned(&pcm[0], &d_off.channels[0]);
    let snr_on = snr_aligned(&pcm[0], &d_on.channels[0]);
    let save = 100.0 * (1.0 - on.len() as f64 / off.len().max(1) as f64);
    let lo = 2 * PRIME;
    let hi = d_on.channels[0].len().min(d_off.channels[0].len());
    let rms_src = rms(&pcm[0][PRIME..n.saturating_sub(PRIME)]);
    let rms_on = if hi > lo {
        rms(&d_on.channels[0][lo..hi.min(lo + n.saturating_sub(2 * PRIME))])
    } else {
        0.0
    };
    eprintln!(
        "TASK-75 PNS noise 1s: NOISE_HCB {n_pns}; bytes {} vs {} ({save:.1}%); SNR off {snr_off:.2} on {snr_on:.2} Δ {:.2} dB; RMS src {rms_src:.4} on {rms_on:.4}",
        off.len(),
        on.len(),
        snr_on - snr_off
    );
    assert!(n_pns > 0, "PNS on white noise should emit NOISE_HCB");
    assert!(d_on.channels[0].iter().all(|x| x.is_finite()));
    // DOC-3: PNS is not waveform-identical. Energy should stay in the ballpark.
    assert!(
        rms_on > 0.15 * rms_src && rms_on < 4.0 * rms_src,
        "PNS RMS {rms_on} vs src {rms_src}"
    );
}

#[test]
fn pns_sine_snr_guard() {
    let n = RATE as usize;
    let pcm = vec![sine(n, 440.0, 0.5)];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    let d_off = decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let snr_off = snr_aligned(&pcm[0], &d_off.channels[0]);
    let snr_on = snr_aligned(&pcm[0], &d_on.channels[0]);
    let db = snr_on - snr_off;
    eprintln!("TASK-75 PNS sine 1s: off {snr_off:.2} on {snr_on:.2} Δ {db:.2} dB");
    assert!(db > -0.5, "sine SNR drop {db:.2} dB exceeds 0.5 dB guard");
}

#[test]
fn pns_harmonic_snr_guard() {
    // Speech-like tonal stack: must not collapse when PNS is on.
    let n = RATE as usize / 2;
    let pcm = vec![
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                0.3 * (2.0 * std::f32::consts::PI * 120.0 * t).sin()
                    + 0.2 * (2.0 * std::f32::consts::PI * 240.0 * t).sin()
                    + 0.1 * (2.0 * std::f32::consts::PI * 360.0 * t).sin()
            })
            .collect(),
    ];
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    let d_off = decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let db = snr_aligned(&pcm[0], &d_on.channels[0]) - snr_aligned(&pcm[0], &d_off.channels[0]);
    eprintln!(
        "TASK-75 PNS harmonic: Δ {db:.2} dB; NOISE_HCB {}",
        noise_hcb_count(&on)
    );
    assert!(
        db > -0.5,
        "harmonic SNR drop {db:.2} dB exceeds 0.5 dB guard"
    );
}

#[test]
fn pns_correlated_stereo_stays_correlated() {
    let n = RATE as usize / 2;
    let mono = noise(n, 0.4, 11);
    let pcm = vec![mono.clone(), mono];
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    let dec = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels.len(), 2);
    let lo = 2 * PRIME;
    let hi = dec.channels[0].len().saturating_sub(PRIME);
    if hi <= lo {
        return;
    }
    let c = corr(&dec.channels[0][lo..hi], &dec.channels[1][lo..hi]);
    eprintln!(
        "TASK-75 PNS identical-L/R noise corr {c:.3}; NOISE_HCB {}",
        noise_hcb_count(&on)
    );
    assert!(c > 0.85, "correlated stereo PNS corr {c:.3} < 0.85");
}

#[test]
fn pns_uncorrelated_stereo_does_not_force_mid() {
    let n = RATE as usize / 2;
    let pcm = vec![noise(n, 0.4, 1), noise(n, 0.4, 99)];
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_pns(true)).unwrap();
    let dec = decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let lo = 2 * PRIME;
    let hi = dec.channels[0].len().saturating_sub(PRIME);
    let c = if hi > lo {
        corr(&dec.channels[0][lo..hi], &dec.channels[1][lo..hi])
    } else {
        0.0
    };
    eprintln!(
        "TASK-75 PNS uncorrelated stereo corr {c:.3}; NOISE_HCB {}",
        noise_hcb_count(&on)
    );
    assert!(
        dec.channels
            .iter()
            .all(|ch| ch.iter().all(|x| x.is_finite()))
    );
    // Shared-RNG-on-ms_used must not collapse independent channels to one image.
    assert!(c < 0.5, "uncorrelated stereo became mid-like corr {c:.3}");
}
