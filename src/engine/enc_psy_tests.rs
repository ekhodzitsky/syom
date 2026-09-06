//! Psy model behavior: tones concentrate precision, noise spreads it,
//! silence codes nothing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::swb::LONG_WINDOW_LEN;
use super::super::swb::long_offsets;
use super::Psy;
use crate::engine::enc_quant::MAX_BANDS;

fn offsets_48k() -> (&'static [u16], usize) {
    let o = long_offsets(3).expect("48k offsets");
    (o, o.len() - 1)
}

/// MDCT-shaped spectrum with a single tone at `bin`.
fn tone_spec(bin: usize) -> [f32; LONG_WINDOW_LEN] {
    let mut spec = [0.0f32; LONG_WINDOW_LEN];
    spec[bin] = 1e6;
    spec[bin + 1] = 3e5;
    spec
}

#[test]
fn tone_concentrates_coding() {
    let (offsets, n_bands) = offsets_48k();
    let mut psy = Psy::new(offsets, 48_000);
    let spec = tone_spec(100);
    let mut coded = [false; MAX_BANDS];
    let mut tq = [0.0f32; MAX_BANDS];
    psy.analyze(&spec, offsets, 2048.0, &mut coded, &mut tq);
    let n_coded = coded[..n_bands].iter().filter(|&&c| c).count();
    assert!(n_coded <= 10, "one tone coded {n_coded} bands");
    // The tone's own band gets full precision.
    let tone_band = offsets.iter().position(|&o| o > 100).expect("band") - 1;
    assert!(coded[tone_band]);
    assert!(
        (tq[tone_band] - 2048.0).abs() < 1.0,
        "tone band target {}",
        tq[tone_band]
    );
}

#[test]
fn flat_noise_codes_everywhere() {
    let (offsets, n_bands) = offsets_48k();
    let mut psy = Psy::new(offsets, 48_000);
    let mut spec = [0.0f32; LONG_WINDOW_LEN];
    // Deterministic flat-ish spectrum.
    for (i, v) in spec.iter_mut().enumerate() {
        *v = if i % 2 == 0 { 1e4 } else { -1e4 };
    }
    let mut coded = [false; MAX_BANDS];
    let mut tq = [0.0f32; MAX_BANDS];
    psy.analyze(&spec, offsets, 2048.0, &mut coded, &mut tq);
    let n_coded = coded[..n_bands].iter().filter(|&&c| c).count();
    assert!(
        n_coded >= n_bands * 3 / 4,
        "noise coded only {n_coded}/{n_bands}"
    );
}

#[test]
fn silence_codes_nothing() {
    let (offsets, n_bands) = offsets_48k();
    let mut psy = Psy::new(offsets, 48_000);
    let spec = [0.0f32; LONG_WINDOW_LEN];
    let mut coded = [true; MAX_BANDS];
    let mut tq = [1.0f32; MAX_BANDS];
    psy.analyze(&spec, offsets, 2048.0, &mut coded, &mut tq);
    assert!(
        coded[..n_bands].iter().all(|&c| !c),
        "silence must code no bands"
    );
}
