//! Deterministic-math accuracy tests: each replacement must sit within a few
//! ulp of the std libm call it stands in for (the std calls themselves are
//! only compared, never used, in the encoder).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{
    LOG2_10, acos_f64, atan, atan_f64, atan2_f64, exp2, exp2_f64, exp10_f64, log2,
    pow_three_quarter, sincos, sincos_f64, twiddle_table,
};

/// Max relative error of `got` vs `want` over a sweep, ignoring underflow.
fn max_rel_err(points: usize, f: impl Fn(f32) -> (f32, f32)) -> f32 {
    let mut worst = 0.0f32;
    for i in 0..points {
        let (want, got) = f(i as f32);
        if want.abs() < 1e-30 {
            continue;
        }
        worst = worst.max(((got - want) / want).abs());
    }
    worst
}

#[test]
fn exp2_tracks_std() {
    let err = max_rel_err(60_000, |i| {
        let x = -30.0 + i * 0.001; // the encoder's domain: [-30, 30)
        (x.exp2(), exp2(x))
    });
    assert!(err < 4e-7, "exp2 max rel err {err:e}");
    assert_eq!(exp2(0.0), 1.0);
    assert_eq!(exp2(4.0), 16.0);
    assert_eq!(exp2(-4.0), 0.0625);
}

#[test]
fn exp2_is_exact_for_scalefactor_gains() {
    // The quantizer's gain exponents are exact multiples of 1/16: the result
    // must be a clean table·powi product — and match the decoder's gain.
    for k in -100i32..=155 {
        let got = exp2(-0.1875 * k as f32);
        assert!(got.is_finite() && got > 0.0, "k {k}: gain {got}");
        let want = (-0.1875f32 * k as f32).exp2();
        let rel = ((got - want) / want).abs();
        assert!(rel <= 2.0 * f32::EPSILON, "k {k}: {got} vs libm {want}");
    }
}

#[test]
fn log2_tracks_std() {
    let err = max_rel_err(50_000, |i| {
        // magnitudes 1e-6..1e7 with a pseudo-random mantissa
        let m = 1.0 + ((i as u64 * 2_654_435_761) % 100_000) as f32 / 100_000.0;
        let x = m * 10f32.powi((i as i32 % 13) - 6);
        (x.log2(), log2(x))
    });
    assert!(err < 2e-6, "log2 max rel err {err:e}");
    assert_eq!(log2(1.0), 0.0);
    assert_eq!(log2(1024.0), 10.0);
}

#[test]
fn log2_handles_subnormals() {
    let tiny = 1e-42f32;
    let want = tiny.log2();
    let got = log2(tiny);
    assert!((got - want).abs() < 1e-5, "subnormal: {got} vs {want}");
}

#[test]
fn atan_tracks_std() {
    let mut worst = 0.0f32;
    for i in 0..60_000 {
        let x = -30.0 + i as f32 * 0.001; // bark() arguments live in [0, ~20]
        let d = (atan(x) - x.atan()).abs();
        worst = worst.max(d);
    }
    assert!(worst < 1e-6, "atan max abs err {worst:e}");
    assert_eq!(atan(0.0), 0.0);
}

#[test]
fn sincos_tracks_std() {
    let mut worst = 0.0f32;
    for i in 0..100_000 {
        let a = i as f32 * (std::f32::consts::TAU / 100_000.0);
        let (s, c) = sincos(a);
        worst = worst.max((s - a.sin()).abs()).max((c - a.cos()).abs());
    }
    assert!(worst < 4e-6, "sincos max abs err {worst:e}");
}

#[test]
fn twiddle_table_matches_layout() {
    let (re, im) = twiddle_table(8);
    // stages len 2, 4, 8: halves 1 + 2 + 4 = 7 entries, e^{i·2πk/len}
    assert_eq!(re.len(), 7);
    assert_eq!(re[0], 1.0); // len 2, k 0
    assert_eq!(im[0], 0.0);
    let (s, c) = sincos(std::f32::consts::FRAC_PI_2);
    assert_eq!(re[1 + 2 + 2], c); // stage len 8, k 2 → angle π/2
    assert_eq!(im[1 + 2 + 2], s);
}

#[test]
fn pow_three_quarter_tracks_powf() {
    let err = max_rel_err(50_000, |i| {
        let x = 1e-6 * (1.0 + i * 3.7);
        (x.powf(0.75), pow_three_quarter(x))
    });
    assert!(err < 4e-7, "pow3/4 max rel err {err:e}");
    assert_eq!(pow_three_quarter(0.0), 0.0);
    assert_eq!(pow_three_quarter(-16.0), 8.0);
}

#[test]
fn log2_10_literal_is_correct() {
    assert_eq!(LOG2_10, 10f32.log2());
}

#[test]
fn f64_trig_tracks_std_inside_a_ulp() {
    let mut sincos_worst = 0.0f64;
    for i in 0..20_000 {
        let a = -80.0 + i as f64 * 160.0 / 20_000.0;
        let (s, c) = sincos_f64(a);
        sincos_worst = sincos_worst
            .max((s - a.sin()).abs())
            .max((c - a.cos()).abs());
    }
    // Negative QMF twiddles and one large finite angle.
    for a in [-2.0 * std::f64::consts::PI * 63.0 / 128.0, 1.0e8, -1.0e16] {
        let (s, c) = sincos_f64(a);
        sincos_worst = sincos_worst
            .max((s - a.sin()).abs())
            .max((c - a.cos()).abs());
    }
    assert!(sincos_worst < 5e-16, "sincos_f64 {sincos_worst:e}");
    let mut atan_worst = 0.0f64;
    for i in 0..20_000 {
        let x = -30.0 + i as f64 * 60.0 / 20_000.0;
        atan_worst = atan_worst.max((atan_f64(x) - x.atan()).abs());
    }
    assert!(atan_worst < 1e-15, "atan_f64 {atan_worst:e}");
    let mut acos_worst = 0.0f64;
    for i in 0..10_000 {
        let x = -1.0 + i as f64 * 2.0 / 10_000.0;
        acos_worst = acos_worst.max((acos_f64(x) - x.acos()).abs());
    }
    assert!(acos_worst < 1e-15, "acos_f64 {acos_worst:e}");
    let mut atan2_worst = 0.0f64;
    for i in 0..400 {
        let y = -2.0 + (i % 20) as f64 * 0.2;
        let x = -2.0 + (i / 20) as f64 * 0.2;
        atan2_worst = atan2_worst.max((atan2_f64(y, x) - y.atan2(x)).abs());
    }
    assert!(atan2_worst < 1e-15, "atan2_f64 {atan2_worst:e}");
    assert_eq!(sincos_f64(0.0), (0.0, 1.0));
    assert_eq!(atan_f64(0.0), 0.0);
    assert_eq!(acos_f64(1.0), 0.0);
}

/// Degree-16 Taylor for `e^r`, copied so it is not the library call.
fn exp_series(r: f64) -> f64 {
    let mut p = 1.0;
    let mut k = 16i32;
    while k >= 1 {
        p = 1.0 + r * p / (k as f64);
        k -= 1;
    }
    p
}

/// `2^n` by repeated doubling. Exact for the IID exponent range.
fn pow2_series(n: i32) -> f64 {
    let mut v = 1.0;
    let mut i = n;
    if i > 0 {
        while i > 0 {
            v *= 2.0;
            i -= 1;
        }
    } else {
        while i < 0 {
            v *= 0.5;
            i += 1;
        }
    }
    v
}

#[test]
fn exp2_f64_matches_series_and_stays_near_libm() {
    let ln2 = f64::from_bits(0x3FE6_2E42_FEFA_39EF);
    let mut worst = 0.0f64;
    for i in 0..20_000 {
        let x = -8.5 + i as f64 * 17.0 / 20_000.0;
        let n = x.floor() as i32;
        let f = x - (n as f64);
        let series = pow2_series(n) * exp_series(f * ln2);
        assert_eq!(exp2_f64(x).to_bits(), series.to_bits(), "series {x}");
        let want = x.exp2();
        worst = worst.max(((exp2_f64(x) - want) / want).abs());
    }
    assert!(worst < 1e-12, "exp2_f64 max rel err {worst:e}");
    assert_eq!(exp2_f64(0.0), 1.0);
    assert_eq!(exp2_f64(1.0), 2.0);
    assert_eq!(exp2_f64(-1.0), 0.5);
    assert_eq!(exp2_f64(10.0), 1024.0);
    assert_eq!(exp10_f64(0.0), 1.0);
    assert!(((exp10_f64(1.0) - 10.0) / 10.0).abs() < 1e-12);
    assert!(((exp10_f64(2.0) - 100.0) / 100.0).abs() < 1e-12);
    assert!(((exp10_f64(-2.0) - 0.01) / 0.01).abs() < 1e-12);
    let sqrt10 = 10f64.sqrt();
    assert!(((exp10_f64(0.5) - sqrt10) / sqrt10).abs() < 1e-12);
    assert!(exp2_f64(f64::NAN).is_nan());
    assert_eq!(exp2_f64(f64::INFINITY), f64::INFINITY);
    assert_eq!(exp2_f64(f64::NEG_INFINITY), 0.0);
}
