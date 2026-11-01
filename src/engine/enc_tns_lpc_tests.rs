//! Independent scalar reference for the parallel-lag TNS autocorrelation.
use super::{MAX_ORDER, autocorrelate};

fn reference(x: &[f32]) -> [f64; MAX_ORDER + 1] {
    let mut out = [0.0f64; MAX_ORDER + 1];
    for (lag, sum) in out.iter_mut().enumerate().take(x.len()) {
        for i in 0..x.len() - lag {
            *sum += f64::from(x[i]) * f64::from(x[i + lag]);
        }
    }
    out
}

#[test]
fn parallel_lags_preserve_every_scalar_sum_bit() {
    let mut seed = 0xC0FF_EE42u32;
    let mut noise = Vec::with_capacity(1033);
    for _ in 0..1033 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        noise.push(seed as i32 as f32 * (1.0 / 2_147_483_648.0));
    }
    let mut lengths: Vec<_> = (0..35).collect();
    lengths.extend([
        63, 64, 127, 128, 255, 256, 479, 480, 511, 512, 959, 960, 1023, 1024, 1033,
    ]);
    for len in lengths {
        for input in [
            noise[..len].to_vec(),
            vec![0.0; len],
            vec![-0.0; len],
            (0..len)
                .map(|i| if i % 2 == 0 { 1e-30 } else { -1e30 })
                .collect(),
        ] {
            let got = autocorrelate(&input);
            let want = reference(&input);
            for (lag, (actual, expected)) in got.iter().zip(want).enumerate() {
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "length={len} lag={lag}"
                );
            }
        }
    }
}
