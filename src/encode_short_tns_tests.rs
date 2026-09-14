//! TASK-72: opt-in short-window TNS. Default off = production bytes unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{DecodeOptions, EncodeOptions, encode, encode_with};

const RATE: u32 = 48_000;
const FRAME: usize = 1024;

fn click_pcm() -> Vec<Vec<f32>> {
    let n = 14 * FRAME;
    let mut v = vec![0.0f32; n];
    let at = 5 * FRAME + 700;
    let mut lcg = 0x1234_5678u32;
    for x in &mut v[at..at + 32] {
        lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let u = (lcg >> 9) as f32 / (1u32 << 23) as f32;
        *x = 0.9 * (2.0 * u - 1.0);
    }
    vec![v]
}

fn sine(n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / RATE as f32).sin())
        .collect()
}

fn err_around_click(decoded: &[f32], pcm: &[f32]) -> f64 {
    let click = 5 * FRAME + 700;
    let lo = click.saturating_sub(64);
    let hi = (click + 192).min(pcm.len());
    let dec_lo = lo + FRAME;
    let dec_hi = hi + FRAME;
    if dec_hi > decoded.len() {
        return f64::INFINITY;
    }
    pcm[lo..hi]
        .iter()
        .zip(decoded[dec_lo..dec_hi].iter())
        .map(|(&a, &b)| {
            let d = f64::from(a) - f64::from(b);
            d * d
        })
        .sum()
}

#[test]
fn default_matches_short_tns_off() {
    let pcm = vec![sine(4096)];
    let a = encode(&pcm, RATE).unwrap();
    let b = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_tns(false)).unwrap();
    assert_eq!(a, b);
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_tns(true)).unwrap();
    assert_eq!(a, on, "steady sine has no short windows");
}

#[test]
fn short_tns_is_deterministic() {
    let pcm = click_pcm();
    let opts = EncodeOptions::adts().with_short_tns(true);
    assert_eq!(
        encode_with(&pcm, RATE, &opts).unwrap(),
        encode_with(&pcm, RATE, &opts).unwrap()
    );
}

#[test]
fn short_tns_off_matches_default_on_transient() {
    let pcm = click_pcm();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let explicit = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_tns(false)).unwrap();
    assert_eq!(off, explicit);
}

#[test]
fn short_tns_may_change_transient_bytes() {
    let pcm = click_pcm();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_tns(true)).unwrap();
    let _ = (off, on);
}

#[test]
fn short_tns_click_error_vs_off() {
    let pcm = click_pcm();
    let off = encode_with(&pcm, RATE, &EncodeOptions::adts()).unwrap();
    let on = encode_with(&pcm, RATE, &EncodeOptions::adts().with_short_tns(true)).unwrap();
    let d_off = crate::decode_with(&off, &DecodeOptions::unbounded()).unwrap();
    let d_on = crate::decode_with(&on, &DecodeOptions::unbounded()).unwrap();
    let e_off = err_around_click(&d_off.channels[0], &pcm[0]);
    let e_on = err_around_click(&d_on.channels[0], &pcm[0]);
    let db = 10.0 * (e_off / e_on.max(1e-12)).log10();
    eprintln!(
        "TASK-72 short TNS click error: off {e_off:.4} on {e_on:.4} Δ {db:.1} dB; bytes {} vs {}",
        off.len(),
        on.len()
    );
    let _ = db;
}
