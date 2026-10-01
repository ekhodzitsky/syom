//! TASK-70: named attack-position pre-echo sweep on shipped encode_with.
//! Failing subset: causal clicks in the first 448 samples of a frame.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, EncodeOptions, decode_with, encode, encode_with};

const RATE: u32 = 48_000;
const FRAME: usize = 1024;
const PRE: usize = 480;

fn click_pcm(n_frames: usize, frame: usize, pos: usize, amp: f32) -> Vec<Vec<f32>> {
    let n = n_frames * FRAME;
    let mut v = vec![0.0f32; n];
    let at = frame * FRAME + pos;
    let mut lcg = 0x1234_5678u32;
    for x in &mut v[at..at + 32] {
        lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let u = (lcg >> 9) as f32 / (1u32 << 23) as f32;
        *x = amp * (2.0 * u - 1.0);
    }
    vec![v]
}

fn sine(n: usize, hz: f32, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f32::consts::PI * hz * i as f32 / RATE as f32).sin())
        .collect()
}

fn pre_onset_energy(decoded: &[f32], click_abs: usize) -> f64 {
    // Tagged decode is presentation-aligned: the click stays at its source index.
    let click_dec = click_abs;
    let lo = click_dec.saturating_sub(PRE);
    decoded[lo..click_dec]
        .iter()
        .map(|&x| f64::from(x) * f64::from(x))
        .sum()
}

fn encode_click(pos: usize, lookahead: bool) -> (Vec<f32>, usize, usize) {
    let pcm = click_pcm(14, 5, pos, 0.9);
    let opts = EncodeOptions::adts().with_lookahead(lookahead);
    let adts = encode_with(&pcm, RATE, &opts).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    (dec.channels[0].clone(), 5 * FRAME + pos, adts.len())
}

#[test]
fn causal_early_click_is_the_failing_pre_echo_subset() {
    // Predeclared failing subset: causal, in-frame pos < 448 (LongStart flat).
    for pos in [50usize, 100, 200, 400] {
        let (causal, abs, _) = encode_click(pos, false);
        let (look, _, _) = encode_click(pos, true);
        let e_c = pre_onset_energy(&causal, abs);
        let e_l = pre_onset_energy(&look, abs);
        let db = 10.0 * (e_c / e_l.max(1e-12)).log10();
        eprintln!("pos {pos}: lookahead vs causal {db:.1} dB (c {e_c:.4} l {e_l:.4})");
        assert!(
            e_l * 2.0 < e_c,
            "pos {pos}: lookahead vs causal {db:.1} dB (c {e_c:.4} l {e_l:.4})"
        );
    }
}

#[test]
fn lookahead_keeps_one_frame_latency() {
    let pcm = click_pcm(6, 2, 100, 0.9);
    let off = decode_with(
        &encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap(),
        &DecodeOptions::unbounded(),
    )
    .unwrap();
    let on = decode_with(
        &encode_with(&pcm, RATE, &EncodeOptions::adts().with_lookahead(true)).unwrap(),
        &DecodeOptions::unbounded(),
    )
    .unwrap();
    // Lookahead drain still one extra MDCT; decoded length matches causal.
    assert_eq!(off.channels[0].len(), on.channels[0].len());
}

#[test]
fn causal_silence_and_sine_unchanged_by_this_task() {
    let silence = vec![vec![0.0f32; 4096]];
    let s = encode(&silence, RATE).unwrap();
    let dec = decode_with(&s, &DecodeOptions::unbounded()).unwrap();
    let peak = dec.channels[0].iter().fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak < 1e-4, "silence peak {peak}");
    let pcm = vec![sine(4096, 440.0, 0.5)];
    assert_eq!(
        encode(&pcm, RATE).unwrap(),
        encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap()
    );
}

#[test]
fn late_click_tail_still_present() {
    let pcm = click_pcm(14, 5, 700, 0.9);
    let adts = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let dec = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let click_dec = 5 * FRAME + 700;
    let peak = dec.channels[0][click_dec..click_dec + 64]
        .iter()
        .fold(0.0f32, |m, x| m.max(x.abs()));
    assert!(peak > 0.05, "late click lost, peak {peak}");
}
