//! Dense storage must preserve every coefficient and scalar threshold sum.
#![allow(clippy::expect_used)]
use super::*;

fn check(offsets: &[u16], rate: u32, transform: usize) {
    let psy = Psy::with_transform(offsets, rate, transform);
    let n = offsets.len() - 1;
    assert_eq!(psy.spread.len(), n * n);
    let mut bark_centers = [0.0; MAX_BANDS];
    let mut original = [[0.0f32; MAX_BANDS]; MAX_BANDS];
    for (b, center_bark) in bark_centers.iter_mut().enumerate().take(n) {
        let center = (f32::from(offsets[b]) + f32::from(offsets[b + 1]))
            * 0.5
            * (rate as f32 / transform as f32);
        *center_bark = bark(center.max(20.0));
        assert_eq!(
            psy.ath_energy[b].to_bits(),
            ath_band_energy(center.max(20.0)).to_bits()
        );
    }
    for (i, row) in original.iter_mut().enumerate().take(n) {
        for (j, cell) in row.iter_mut().enumerate().take(n) {
            let dz = bark_centers[i] - bark_centers[j];
            if dz.abs() <= SPREAD_RANGE_BARK {
                let attenuation = if dz >= 0.0 {
                    SPREAD_UP_DB * dz
                } else {
                    SPREAD_DOWN_DB * -dz
                };
                *cell = det_math::exp2(-attenuation / 10.0 * det_math::LOG2_10);
            }
            assert_eq!(psy.spread[i * n + j].to_bits(), cell.to_bits());
        }
    }
    let mut seed = 413u32;
    for frame in 0..8 {
        let mut energy = [0.0; MAX_BANDS];
        for value in &mut energy[..n] {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *value = if frame == 0 {
                -0.0
            } else if frame == 1 {
                f32::from_bits(1)
            } else {
                (seed % 100_000) as f32 * 0.003
            };
        }
        let mut got = [7.0; MAX_BANDS];
        psy.masked_thresholds(&energy, &mut got);
        for (i, row) in original.iter().enumerate().take(n) {
            let mut sum = 0.0f32;
            for (j, &value) in energy.iter().enumerate().take(n) {
                sum += value * row[j];
            }
            assert_eq!(
                got[i].to_bits(),
                (sum / SMR).to_bits(),
                "row={i} frame={frame}"
            );
        }
        assert!(got[n..].iter().all(|&value| value == 7.0));
    }
    eprintln!(
        "psy_matrix rate={rate} transform={transform} bands={n} bytes={} old_bytes={}",
        psy.spread.len() * std::mem::size_of::<f32>(),
        std::mem::size_of::<[[f32; MAX_BANDS]; MAX_BANDS]>()
    );
}

#[test]
fn dense_matrix_matches_original_storage_for_rates_grids_and_custom_offsets() {
    use crate::engine::swb::{long_offsets, long_offsets_ld, short_offsets};
    for (index, rate) in [(0, 96000), (3, 48000), (4, 44100), (8, 16000), (11, 8000)] {
        check(long_offsets(index).expect("long offsets"), rate, 2048);
        check(short_offsets(index).expect("short offsets"), rate, 256);
        check(long_offsets_ld(index).expect("LD offsets"), rate, 1024);
    }
    check(&[0], 48000, 2048);
    check(&[0, 1, 4, 20, 512], 44100, 1024);
    check(&[0, 4, 8, 12, 16, 32, 64, 128, 256, 512], 48000, 1024);
    eprintln!("psy_struct_bytes={}", std::mem::size_of::<Psy>());
    eprintln!(
        "psy_box_pointer_delta={}",
        std::mem::size_of::<Box<[f32]>>()
            - std::mem::size_of::<Box<[[f32; MAX_BANDS]; MAX_BANDS]>>()
    );
}
