//! TASK-67: quality VBR — monotonic levels, hard cap, content-following
//! bytes, one-shot / push parity, mode checks.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{AacError, DecodeOptions, EncodeOptions, Encoder, decode_with, encode_with};

const SR: u32 = 48_000;

fn lcg(s: &mut u32) -> f32 {
    *s ^= *s << 13;
    *s ^= *s >> 17;
    *s ^= *s << 5;
    (*s as f32 / u32::MAX as f32) - 0.5
}

fn sine(n: usize, hz: f64, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * ((2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(SR)).sin() as f32))
        .collect()
}

fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
    let mut s = 0x9e37_79b9u32.wrapping_mul(seed);
    (0..n).map(|_| 2.0 * amp * lcg(&mut s)).collect()
}

fn mix(n: usize) -> Vec<f32> {
    sine(n, 440.0, 0.3)
        .iter()
        .zip(noise(n, 0.05, 3))
        .map(|(a, b)| a + b)
        .collect()
}

fn q(level: u8) -> EncodeOptions {
    EncodeOptions::adts().with_quality(level)
}

/// (payload bytes per ADTS frame) — headers are 7 bytes, no CRC.
fn frame_payloads(adts: &[u8]) -> Vec<usize> {
    let adts = crate::gapless::strip_id3(adts);
    let mut out = Vec::new();
    let mut at = 0;
    while at + 7 <= adts.len() {
        let len = ((usize::from(adts[at + 3]) & 3) << 11)
            | (usize::from(adts[at + 4]) << 3)
            | (usize::from(adts[at + 5]) >> 5);
        if len < 7 || at + len > adts.len() {
            break;
        }
        out.push(len - 7);
        at += len;
    }
    out
}

fn snr(want: &[f32], got: &[f32]) -> f64 {
    let g = if got.len() == want.len() {
        got
    } else {
        &got[1024..1024 + want.len()]
    };
    let (mut ps, mut pe) = (0.0f64, 0.0f64);
    for (a, b) in want.iter().zip(g) {
        ps += f64::from(*a).powi(2);
        pe += (f64::from(*a) - f64::from(*b)).powi(2);
    }
    10.0 * (ps / pe.max(1e-30)).log10()
}

#[test]
fn levels_are_monotonic_in_bytes_and_snr_on_aggregate() {
    let n = SR as usize;
    let clips: Vec<Vec<Vec<f32>>> = vec![
        vec![sine(n, 440.0, 0.4)],
        vec![noise(n, 0.3, 1)],
        vec![mix(n)],
        vec![mix(n), noise(n, 0.2, 5)],
    ];
    let mut prev_bytes = 0usize;
    let mut prev_snr = f64::NEG_INFINITY;
    for level in 0..=10u8 {
        let (mut bytes, mut snr_sum) = (0usize, 0.0f64);
        for pcm in &clips {
            let st = encode_with(pcm, SR, &q(level)).unwrap();
            let dec = decode_with(&st, &DecodeOptions::unbounded()).unwrap();
            bytes += st.len();
            snr_sum += snr(&pcm[0], &dec.channels[0]);
        }
        println!(
            "quality {level}: {bytes} B, aggregate SNR {:.1} dB",
            snr_sum / clips.len() as f64
        );
        assert!(
            bytes >= prev_bytes,
            "level {level}: {bytes} B < previous {prev_bytes}"
        );
        assert!(
            snr_sum >= prev_snr - 0.5,
            "level {level}: aggregate SNR fell"
        );
        prev_bytes = bytes;
        prev_snr = snr_sum;
    }
}

#[test]
fn hard_cap_holds_and_bytes_follow_the_content() {
    let n = SR as usize;
    let loud = vec![noise(n, 0.45, 7), noise(n, 0.45, 8)];
    let st = encode_with(&loud, SR, &q(10)).unwrap();
    let frames = frame_payloads(&st);
    assert!(
        frames.iter().all(|&b| b * 8 <= 2 * 6144),
        "cap: {:?}",
        frames.iter().max()
    );
    assert!(
        st.len() * 8 > 200_000,
        "level 10 white noise spends bits: {} B/s",
        st.len()
    );
    let tone = encode_with(&[sine(n, 440.0, 0.4)], SR, &q(5)).unwrap();
    let nse = encode_with(&[noise(n, 0.3, 1)], SR, &q(5)).unwrap();
    let silence = encode_with(&[vec![0.0f32; n]], SR, &q(10)).unwrap();
    assert!(
        tone.len() * 3 < nse.len(),
        "tone {} B vs noise {} B at level 5",
        tone.len(),
        nse.len()
    );
    assert!(
        silence.len() < 1_500,
        "silence at level 10 is {} B",
        silence.len()
    );
    // No ABR padding: a tone at level 5 stays far under the 128 kbps default.
    let abr = encode_with(&[sine(n, 440.0, 0.4)], SR, &EncodeOptions::adts()).unwrap();
    assert!(
        tone.len() * 2 < abr.len(),
        "quality {} B vs ABR {} B",
        tone.len(),
        abr.len()
    );
}

#[test]
fn push_matches_one_shot_and_lookahead_composes() {
    let n = SR as usize / 2 + 333;
    let pcm = vec![mix(n), noise(n, 0.2, 9)];
    for opts in [q(3), q(7).with_lookahead(true)] {
        let one = encode_with(&pcm, SR, &opts).unwrap();
        let mut enc = Encoder::new(SR, 2, &opts).unwrap();
        let mut got = Vec::new();
        for at in (0..n).step_by(777) {
            let end = (at + 777).min(n);
            let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[at..end]).collect();
            enc.feed(&planes, |f| {
                got.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
        }
        let info = enc
            .finish(|f| {
                got.extend_from_slice(f.au);
                Ok(())
            })
            .unwrap();
        assert_eq!(got.as_slice(), crate::gapless::strip_id3(&one));
        assert_eq!(info.bytes as usize, got.len());
        assert_eq!(info.priming, 1024);
        assert_eq!(encode_with(&pcm, SR, &opts).unwrap(), one, "deterministic");
    }
}

#[test]
fn mode_checks_are_typed_and_shared() {
    let pcm = vec![vec![0.1f32; 4096]];
    assert!(matches!(
        encode_with(&pcm, SR, &q(11)).unwrap_err(),
        AacError::Encode(_)
    ));
    assert!(matches!(
        encode_with(&pcm, SR, &q(5).with_he(true)).unwrap_err(),
        AacError::Encode(_)
    ));
    assert!(matches!(
        Encoder::new(SR, 1, &q(11)).err(),
        Some(AacError::Encode(_))
    ));
    assert!(matches!(
        Encoder::new(SR, 1, &q(5).with_he(true)).err(),
        Some(AacError::Encode(_))
    ));
    assert!(EncodeOptions::default().quality.is_none());
    assert_eq!(q(4).quality, Some(4));
}
