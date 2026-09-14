//! enc_frame raw_data_block structure, decoded by the shipped decoder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::decode::StreamDecoder;
use super::super::error::{Error, Result};
use super::super::swb::LONG_WINDOW_LEN;
use super::{LcEncoder, MAX_BITS_PER_CHANNEL, MAX_PAYLOAD_BYTES, max_bitrate_bps};

fn sine_frame(t: usize, amp: f32) -> Vec<f32> {
    (0..LONG_WINDOW_LEN)
        .map(|i| {
            amp * (2.0 * std::f32::consts::PI * 440.0 * (t * LONG_WINDOW_LEN + i) as f32 / 48_000.0)
                .sin()
        })
        .collect()
}

#[test]
fn mono_frames_decode_and_fit() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 1, 128_000)?;
    let mut dec = StreamDecoder::new();
    for f in 0..4 {
        let pcm = sine_frame(f, 0.5);
        let payload = enc.encode_frame(&[&pcm])?;
        assert!(payload.len() <= MAX_PAYLOAD_BYTES);
        let frame = dec.decode_raw_data_block(2, enc.fs_index(), 48_000, 1, 1, &payload)?;
        assert_eq!(frame.planar.len(), 1);
        assert_eq!(frame.planar[0].len(), LONG_WINDOW_LEN);
        if f >= 1 {
            let energy: f64 = frame.planar[0]
                .iter()
                .map(|&x| f64::from(x) * f64::from(x))
                .sum();
            assert!(energy > 1e6, "frame {f} lost the sine (energy {energy})");
        }
    }
    Ok(())
}

#[test]
fn stereo_frames_decode_as_cpe() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 2, 128_000)?;
    let mut dec = StreamDecoder::new();
    let l = sine_frame(0, 0.5);
    let r = sine_frame(0, 0.25);
    let payload = enc.encode_frame(&[&l, &r])?;
    assert!(payload.len() <= MAX_PAYLOAD_BYTES);
    let frame = dec.decode_raw_data_block(2, enc.fs_index(), 48_000, 2, 1, &payload)?;
    assert_eq!(frame.planar.len(), 2, "CPE must yield two planes");
    assert_eq!(frame.planar[0].len(), LONG_WINDOW_LEN);
    assert_eq!(frame.planar[1].len(), LONG_WINDOW_LEN);
    Ok(())
}

#[test]
fn silence_encodes_small() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 1, 128_000)?;
    let pcm = [0.0f32; LONG_WINDOW_LEN];
    let payload = enc.encode_frame(&[&pcm])?;
    assert!(
        payload.len() < 64,
        "silent frame is {} bytes",
        payload.len()
    );
    let mut dec = StreamDecoder::new();
    let frame = dec.decode_raw_data_block(2, enc.fs_index(), 48_000, 1, 1, &payload)?;
    let peak = frame.planar[0]
        .iter()
        .map(|x| x.abs())
        .fold(0.0f32, f32::max);
    assert!(peak < 1.0, "silent frame decoded with peak {peak}");
    Ok(())
}

#[test]
fn rejects_bad_shape() {
    let mut enc = LcEncoder::new(48_000, 1, 128_000).expect("encoder");
    let pcm = [0.0f32; 512];
    assert!(enc.encode_frame(&[&pcm]).is_err());
    assert!(LcEncoder::new(47_000, 1, 128_000).is_err());
    assert!(LcEncoder::new(48_000, 3, 128_000).is_err());
    assert!(LcEncoder::new(48_000, 1, 0).is_err());
}

fn lcg_noise(seed: u32) -> Vec<f32> {
    let mut s = seed;
    (0..LONG_WINDOW_LEN)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let u = (s >> 9) as f32 / (1u32 << 23) as f32;
            0.5 * (2.0 * u - 1.0)
        })
        .collect()
}

fn click_noise(seed: u32) -> Vec<f32> {
    let mut pcm = lcg_noise(seed);
    // Burst in the second half so the causal switcher goes short.
    for v in &mut pcm[600..632] {
        *v = if *v >= 0.0 { 0.95 } else { -0.95 };
    }
    pcm
}

#[test]
fn lc_cap_is_independent_of_adts_byte_ceiling() {
    assert_eq!(MAX_BITS_PER_CHANNEL, 6144);
    assert_eq!(MAX_PAYLOAD_BYTES, 8184);
    assert_eq!(max_bitrate_bps(48_000, 1), 288_000);
    assert_eq!(max_bitrate_bps(48_000, 2), 576_000);
    let adts_bps = (MAX_PAYLOAD_BYTES as u64) * 8 * 48_000 / 1024;
    assert!(
        u64::from(max_bitrate_bps(48_000, 2)) < adts_bps,
        "LC 6144*ch must bind before ADTS {adts_bps} bps"
    );
}

#[test]
fn bitrate_above_6144_per_channel_is_format() {
    let max = max_bitrate_bps(48_000, 1);
    assert!(LcEncoder::new(48_000, 1, max).is_ok());
    assert!(matches!(
        LcEncoder::new(48_000, 1, max + 1),
        Err(Error::Format(
            "LC encoder: bitrate exceeds 6144 bits/channel"
        ))
    ));
    assert!(LcEncoder::new(48_000, 2, 576_001).is_err());
    assert!(LcEncoder::new(8_000, 1, 128_000).is_err());
}

#[test]
fn noise_at_lc_cap_stays_within_6144_and_decodes() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 1, max_bitrate_bps(48_000, 1))?;
    let mut dec = StreamDecoder::new();
    let mut max_bits = 0usize;
    for f in 0..8 {
        let pcm = lcg_noise(0x1000 + f as u32);
        let payload = enc.encode_frame(&[&pcm])?;
        max_bits = max_bits.max(payload.len() * 8);
        assert!(payload.len() * 8 <= MAX_BITS_PER_CHANNEL);
        let frame = dec.decode_raw_data_block(2, enc.fs_index(), 48_000, 1, 1, &payload)?;
        assert_eq!(frame.planar[0].len(), LONG_WINDOW_LEN);
        assert!(frame.planar[0].iter().any(|&x| x != 0.0));
    }
    assert!(max_bits > 0);
    Ok(())
}

#[test]
fn stereo_and_short_windows_stay_within_6144() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 2, max_bitrate_bps(48_000, 2))?;
    let mut dec = StreamDecoder::new();
    for f in 0..12 {
        let l = click_noise(0x2000 + f as u32);
        let r = lcg_noise(0x3000 + f as u32);
        let payload = enc.encode_frame(&[&l, &r])?;
        assert!(
            payload.len() * 8 <= MAX_BITS_PER_CHANNEL * 2,
            "frame {f} {} bits",
            payload.len() * 8
        );
        let frame = dec.decode_raw_data_block(2, enc.fs_index(), 48_000, 2, 1, &payload)?;
        assert_eq!(frame.planar.len(), 2);
    }
    Ok(())
}
