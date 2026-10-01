//! TASK-121 regression: ABR undershoot padding is EXT_FILL
//! `fill_element()`s before `ID_END` — never zero bytes after it, which
//! fdk-aac (the AOSP platform decoder codebase) rejects with
//! `AAC_DEC_UNKNOWN`, killing the stream.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::super::decode::StreamDecoder;
use super::super::super::error::Result;
use super::super::super::swb::LONG_WINDOW_LEN;
use super::super::LcEncoder;

fn sine_frame(t: usize, amp: f32) -> Vec<f32> {
    (0..LONG_WINDOW_LEN)
        .map(|i| {
            amp * (2.0 * std::f32::consts::PI * 440.0 * (t * LONG_WINDOW_LEN + i) as f32 / 48_000.0)
                .sin()
        })
        .collect()
}

/// Decode one AU; the decoder stops at `ID_END`, so every byte of the AU
/// must be consumed when padding is well-formed.
fn assert_consumed(dec: &mut StreamDecoder, enc: &LcEncoder, payload: &[u8], f: usize, ch: u8) {
    dec.decode_raw_data_block(2, enc.fs_index(), 48_000, ch, 1, payload)
        .unwrap_or_else(|e| panic!("frame {f}: decode failed: {e}"));
    assert_eq!(
        dec.last_rdb_bytes,
        payload.len(),
        "frame {f}: {} bytes trail ID_END",
        payload.len() - dec.last_rdb_bytes
    );
    assert_ne!(
        payload[payload.len() - 1],
        0,
        "frame {f}: zero stuffing after ID_END"
    );
}

#[test]
fn abr_undershoot_pads_with_fill_elements_not_trailing_zeros() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 1, 128_000)?;
    let mut dec = StreamDecoder::new();
    let budget = 128_000usize * 1024 / 48_000;
    let mut saw_pad = false;
    for f in 0..8 {
        let pcm = sine_frame(f, 0.5);
        let payload = enc.encode_frame(&[&pcm])?;
        saw_pad |= payload.len() * 8 >= budget * 97 / 100;
        assert_consumed(&mut dec, &enc, &payload, f, 1);
    }
    assert!(saw_pad, "sine at 128k must exercise ABR padding");
    Ok(())
}

#[test]
fn abr_padding_covers_short_frames() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 2, 128_000)?;
    let mut dec = StreamDecoder::new();
    let mut saw_short = false;
    for f in 0..12 {
        let mut l = sine_frame(f, 0.4);
        let r = sine_frame(f, 0.3);
        if f % 4 == 3 {
            l[100] = 0.95; // click forces EightShort
            saw_short = true;
        }
        let payload = enc.encode_frame(&[&l, &r])?;
        assert_consumed(&mut dec, &enc, &payload, f, 2);
    }
    assert!(saw_short);
    Ok(())
}

#[test]
fn abr_padding_splits_fill_over_269_byte_chunks() -> Result<()> {
    // A quiet signal at a high rate leaves > FILL_MAX_BYTES of leftover
    // budget: the padding needs more than one fill_element.
    let mut enc = LcEncoder::new(48_000, 2, 512_000)?;
    let mut dec = StreamDecoder::new();
    let budget = 512_000usize * 1024 / 48_000;
    let mut saw_wide_pad = false;
    for f in 0..8 {
        let pcm = sine_frame(f, 0.02);
        let payload = enc.encode_frame(&[&pcm, &pcm])?;
        let padded = payload.len() * 8 >= budget * 97 / 100;
        saw_wide_pad |= padded;
        assert_consumed(&mut dec, &enc, &payload, f, 2);
    }
    assert!(saw_wide_pad, "quiet stereo at 512k must pad over 269 bytes");
    Ok(())
}

#[test]
fn abr_padding_stacks_behind_an_attached_fill() -> Result<()> {
    // HE frames carry an extension_payload in one FIL (`set_fill`); ABR
    // padding adds EXT_FILL FILs behind it, still before ID_END.
    let mut enc = LcEncoder::new(48_000, 2, 128_000)?;
    enc.set_fill(&[0x00, 0x00, 0x00])?; // EXT_FILL nibble + body
    let mut dec = StreamDecoder::new();
    for f in 0..8 {
        let pcm = sine_frame(f, 0.5);
        let payload = enc.encode_frame(&[&pcm, &pcm])?;
        assert_consumed(&mut dec, &enc, &payload, f, 2);
    }
    Ok(())
}

/// `fits` is a step at `answer`. The warm start must return that answer
/// for every guess, including guesses outside the neighbour walk.
#[test]
fn monotonic_search_matches_a_linear_scan() {
    for answer in super::OFFSET_LO..=super::OFFSET_HI {
        for guess in [super::OFFSET_LO, -1, 0, 40, answer, super::OFFSET_HI] {
            let got =
                super::search_monotonic(super::OFFSET_LO, super::OFFSET_HI, guess, |x| x >= answer);
            assert_eq!(got, answer, "guess {guess}");
        }
    }
    assert_eq!(
        super::search_monotonic(super::OFFSET_LO, super::OFFSET_HI, 10, |_| false),
        super::OFFSET_HI
    );
    assert_eq!(
        super::search_monotonic(super::OFFSET_LO, super::OFFSET_HI, 10, |_| true),
        super::OFFSET_LO
    );
}

fn noise_frame(t: usize, seed: u32, amp: f32) -> Vec<f32> {
    let mut s = seed.wrapping_add(t as u32).wrapping_mul(0x9E37_79B9);
    (0..LONG_WINDOW_LEN)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (s >> 8) as f32 / (u32::MAX as f32) * 2.0 - amp
        })
        .collect()
}

fn tremolo_frame(t: usize) -> Vec<f32> {
    (0..LONG_WINDOW_LEN)
        .map(|i| {
            let n = (t * LONG_WINDOW_LEN + i) as f32 / 48_000.0;
            let env = 0.5 * (1.0 + 0.8 * (2.0 * std::f32::consts::PI * 8.0 * n).sin());
            env * (2.0 * std::f32::consts::PI * 440.0 * n).sin()
        })
        .collect()
}

fn click_frame(t: usize) -> Vec<f32> {
    let mut pcm = sine_frame(t, 0.05);
    pcm[80] = 0.95;
    pcm[81] = -0.95;
    pcm
}

/// Running guess (previous frame's offset) must track a cold bisection
/// across low-rate noise, where the bit curve dips by a few bits.
#[test]
fn warm_trajectory_matches_cold_bisection_at_low_rate() -> Result<()> {
    for seed in [1u32, 2, 3, 5, 8, 13, 21, 34] {
        for bps in [16_000u32, 24_000, 32_000, 48_000] {
            let warm = rate_payloads(seed, bps, false)?;
            let cold = rate_payloads(seed, bps, true)?;
            assert_eq!(warm, cold, "seed {seed} bps {bps}");
        }
    }
    Ok(())
}

fn rate_payloads(seed: u32, bps: u32, cold: bool) -> Result<Vec<Vec<u8>>> {
    let mut enc = LcEncoder::new(48_000, 2, bps)?;
    let mut out = Vec::new();
    for f in 0..6 {
        if cold {
            enc.pin_rate_guess(super::OFFSET_LO);
        }
        let l = noise_frame(f, seed, 0.3);
        let r = noise_frame(f, seed.wrapping_add(99), 0.25);
        out.push(enc.encode_frame(&[&l, &r])?);
    }
    Ok(out)
}

fn payloads(kind: &str, cold: bool) -> Result<Vec<Vec<u8>>> {
    let (ch, bps, quality) = match kind {
        "q5" => (2, 128_000, Some(5u8)),
        "mono64" => (1, 64_000, None),
        "low32" => (2, 32_000, None),
        "low16" => (2, 16_000, None),
        "low24m" => (1, 24_000, None),
        _ => (2, 128_000, None),
    };
    let mut enc = LcEncoder::new(48_000, ch, bps)?.with_quality(quality);
    let mut out = Vec::new();
    for f in 0..12 {
        if cold {
            enc.pin_rate_guess(super::OFFSET_LO);
        }
        let l = match kind {
            "noise" => noise_frame(f, 11, 0.25),
            "click" => click_frame(f),
            "tremolo" => tremolo_frame(f),
            "silence" => vec![0.0; LONG_WINDOW_LEN],
            _ => sine_frame(f, 0.5),
        };
        let r = sine_frame(f + 3, 0.35);
        let payload = if ch == 1 {
            enc.encode_frame(&[&l])?
        } else {
            enc.encode_frame(&[&l, &r])?
        };
        out.push(payload);
    }
    Ok(out)
}

/// Warm start must pick the same access units as a cold bisection.
#[test]
fn warm_rate_search_matches_a_cold_search() -> Result<()> {
    for kind in [
        "sine", "noise", "click", "tremolo", "silence", "mono64", "q5", "low32", "low16", "low24m",
    ] {
        assert_eq!(
            payloads(kind, false)?,
            payloads(kind, true)?,
            "{kind}: warm and cold searches diverged"
        );
    }
    Ok(())
}

/// After the opening frame, a steady tone stays near its offset.
/// A cold bisection quantizes about 12 times per frame (two bounds, the
/// steps, and the emit build). The goal is at most 4 after warmup.
#[test]
fn steady_tone_quantizes_at_most_four_times_per_frame() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 2, 128_000)?;
    let _ = super::take_builds();
    let l = sine_frame(0, 0.5);
    let r = sine_frame(1, 0.4);
    enc.encode_frame(&[&l, &r])?;
    let _ = super::take_builds();
    const FRAMES: u32 = 47;
    for f in 1..FRAMES as usize + 1 {
        let l = sine_frame(f, 0.5);
        let r = sine_frame(f + 1, 0.4);
        enc.encode_frame(&[&l, &r])?;
    }
    let builds = super::take_builds();
    assert!(
        builds <= FRAMES * 4,
        "steady tone used {builds} quantizations over {FRAMES} frames (goal ≤ {})",
        FRAMES * 4
    );
    assert!(
        enc.payload.capacity() <= super::super::MAX_PAYLOAD_BYTES,
        "payload scratch grew past the ADTS ceiling"
    );
    Ok(())
}

/// Busy noise may walk a few offsets as credit moves, and must still
/// stay well under a cold bisection (~12 quantizations per frame).
#[test]
fn noise_quantizes_under_a_cold_bisection() -> Result<()> {
    let mut enc = LcEncoder::new(48_000, 2, 64_000)?;
    let _ = super::take_builds();
    const FRAMES: u32 = 24;
    for f in 0..FRAMES as usize {
        let l = noise_frame(f, 3, 0.3);
        let r = noise_frame(f, 9, 0.3);
        enc.encode_frame(&[&l, &r])?;
    }
    let builds = super::take_builds();
    assert!(
        builds <= FRAMES * 8,
        "noise used {builds} quantizations over {FRAMES} frames (goal ≤ {})",
        FRAMES * 8
    );
    Ok(())
}

/// 5.1 shares one noise offset. The warm start must not move it.
#[test]
fn surround_warm_search_matches_a_cold_search() -> Result<()> {
    use crate::engine::enc_mc::McEncoder;
    let run = |cold: bool| -> Result<Vec<u8>> {
        let mut enc = McEncoder::new(48_000, 6, 256_000)?;
        let mut all = Vec::new();
        for f in 0..8 {
            if cold {
                enc.pin_rate_guess(super::OFFSET_LO);
            }
            let planes: Vec<Vec<f32>> = (0..6)
                .map(|ch| sine_frame(f + ch, 0.15 + 0.05 * ch as f32))
                .collect();
            let refs: Vec<&[f32]> = planes.iter().map(|p| p.as_slice()).collect();
            let mut out = Vec::new();
            enc.encode_into(&refs, &mut out)?;
            all.extend_from_slice(&out);
        }
        Ok(all)
    };
    assert_eq!(run(false)?, run(true)?);
    Ok(())
}
