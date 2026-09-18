//! TASK-117: why syom and libavcodec differ by up to 5 LSB on a high-gain
//! TNS filter. Fixture `tns_gain.adts`: the last six frames of a stereo
//! 550 / 660 Hz encode cut abruptly at N = 20 000 (syom LC, 96 kbps); the
//! encoder answers the cut with a strong TNS filter on the right channel.
//! `tns_gain.s16` is ffmpeg 7.0.2's decode (`ffmpeg -i tns_gain.adts -f
//! s16le tns_gain.s16`).
//!
//! syom runs the all-pole synthesis in f64. Re-running it in f32 with the
//! f32 LPC step-up (what libavcodec does) reproduces libavcodec within
//! 1 LSB, so the gap is single-precision rounding amplified by the filter
//! gain, and the f64 result is the more exact one. Not changed.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::cell::Cell;

thread_local! {
    static F32: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn single_precision() -> bool {
    F32.with(Cell::get)
}

/// libavcodec-style synthesis: f32 coefficients, f32 state and sums.
pub(super) fn ar_filter_f32(spec: &mut [f32], start: usize, size: usize, inc: i32, lpc: &[f64]) {
    let order = lpc.len() - 1;
    let c: Vec<f32> = lpc.iter().map(|&v| v as f32).collect();
    let mut idx = start as isize;
    for m in 0..size {
        let mut y = spec[idx as usize];
        for i in 1..=order.min(m) {
            y -= spec[(idx - i as isize * inc as isize) as usize] * c[i];
        }
        spec[idx as usize] = y;
        idx += inc as isize;
    }
}

/// libavcodec's `compute_lpc_coefs` in f32: reflection → direct form,
/// symmetric in-place update.
pub(super) fn step_up_f32(refl: &[f64]) -> [f64; super::MAX_TNS_ORDER + 1] {
    let mut lpc = [0.0f32; super::MAX_TNS_ORDER];
    for (i, &k) in refl.iter().enumerate() {
        let r = k as f32;
        lpc[i] = r;
        for j in 0..(i + 1) >> 1 {
            let f = lpc[j];
            let b = lpc[i - 1 - j];
            lpc[j] = f + r * b;
            lpc[i - 1 - j] = b + r * f;
        }
    }
    let mut a = [0.0f64; super::MAX_TNS_ORDER + 1];
    a[0] = 1.0;
    for (dst, &c) in a[1..].iter_mut().zip(lpc.iter()) {
        *dst = f64::from(c);
    }
    a
}

/// Worst |syom − lavc| in LSB per channel over the fixture.
fn worst_lsb() -> [u32; 2] {
    let adts = include_bytes!("../goldens/tns_gain.adts");
    let lavc = include_bytes!("../goldens/tns_gain.s16");
    let dec = crate::decode_with(adts, &crate::DecodeOptions::unbounded()).unwrap();
    assert_eq!(dec.channels.len(), 2);
    assert_eq!(dec.channels[0].len() * 4, lavc.len());
    let mut worst = [0u32; 2];
    // Skip the first frame: a mid-stream cut has no valid overlap there.
    for i in 1024..dec.channels[0].len() {
        for (ch, w) in worst.iter_mut().enumerate() {
            let k = (i * 2 + ch) * 2;
            let lav = i16::from_le_bytes([lavc[k], lavc[k + 1]]);
            let ours = (f64::from(dec.channels[ch][i]) * 32768.0)
                .round()
                .clamp(-32768.0, 32767.0) as i16;
            *w = (*w).max((i32::from(lav) - i32::from(ours)).unsigned_abs());
        }
    }
    worst
}

#[test]
fn f64_synthesis_differs_from_lavc_only_by_single_precision_rounding() {
    let f64_gap = worst_lsb();
    assert!(
        f64_gap[0] <= 1,
        "left channel has no strong filter: {f64_gap:?}"
    );
    assert!(
        (2..=8).contains(&f64_gap[1]),
        "documented gap on the high-gain channel: {f64_gap:?}"
    );
    F32.with(|f| f.set(true));
    let f32_gap = worst_lsb();
    F32.with(|f| f.set(false));
    assert!(
        f32_gap.iter().all(|&g| g <= 1),
        "f32 synthesis reproduces libavcodec: {f32_gap:?}"
    );
}
