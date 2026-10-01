//! AAC-LD (ER AAC LD, AOT 23) decode goldens — TASK-129.
//!
//! FDK v2.0.3 LOAS fixtures, FDK-decoded s16 oracles, and one libxaac
//! 0.1.13 cross-check. lavc 7.0.2 cannot decode these streams
//! (lab/profiles/REPORT.md). Fixtures regenerate offline:
//!
//! ```text
//! lab/profiles/build/run_cells.sh target/tmp/profiles/cells
//! cp target/tmp/profiles/cells/ld48.loas src/goldens/
//! cp target/tmp/profiles/cells/ld48.dec.s16le src/goldens/ld48.fdk.s16
//! ```
//!
//! Quant, TNS, the 512-line IMDCT and both LD windows match FDK within
//! the project gate (≤ 2 LSB and ≥ 55 dB) on every frame that does not
//! carry perceptual noise. ISO leaves the PNS generator non-normative;
//! this crate keeps the lavc LCG (`pns.rs`). FDK and libxaac draw the
//! same congruential sequence through a different fixed-point scale, so
//! a noise frame — and the next frame, which overlap-adds its right
//! half — is not a 2-LSB cell. `ld64m` / `ld64mus` stay inside the gate
//! on the whole file.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{AacError, DecodeOptions, Decoder};

const FRAME: usize = 512;

fn deinterleave(s16: &[u8], ch: usize) -> Vec<Vec<i16>> {
    let mut planes = vec![Vec::with_capacity(s16.len() / (2 * ch)); ch];
    for f in s16.chunks_exact(2 * ch) {
        for (p, b) in planes.iter_mut().zip(f.chunks_exact(2)) {
            p.push(i16::from_le_bytes([b[0], b[1]]));
        }
    }
    planes
}

/// Max |Δ| in s16 LSB and SNR dB of `ours` (float, ~[-1, 1]) vs `gold`.
fn score(ours: &[f32], gold: &[i16]) -> (u32, f64) {
    assert_eq!(ours.len(), gold.len(), "plane length");
    let mut max_lsb = 0u32;
    let mut ps = 0.0f64;
    let mut pe = 0.0f64;
    for (i, &gv) in gold.iter().enumerate() {
        let ov = (f64::from(ours[i]) * 32768.0)
            .round()
            .clamp(-32768.0, 32767.0) as i16;
        max_lsb = max_lsb.max((i32::from(gv) - i32::from(ov)).unsigned_abs());
        ps += f64::from(gv) * f64::from(gv);
        let e = f64::from(gv) - f64::from(ov);
        pe += e * e;
    }
    let snr = if pe == 0.0 {
        200.0
    } else {
        10.0 * (ps / pe).log10()
    };
    (max_lsb, snr)
}

fn assert_gate(label: &str, ours: &[f32], gold: &[i16]) {
    let (max_lsb, snr) = score(ours, gold);
    assert!(
        max_lsb <= 2 && snr >= 55.0,
        "{label}: max_lsb {max_lsb} snr {snr:.1}"
    );
}

fn decode_split(loas: &[u8]) -> Result<super::DecodedAac, AacError> {
    super::decode_with(loas, &DecodeOptions::unbounded())
}

/// Frames whose spectrum uses `NOISE_HCB` (mono SCE, 48 kHz).
fn noise_frames(loas: &[u8]) -> Vec<bool> {
    let mut pos = 0usize;
    let mut mux = None;
    let mut out = Vec::new();
    while pos + 3 <= loas.len() {
        let v = (u32::from(loas[pos]) << 16)
            | (u32::from(loas[pos + 1]) << 8)
            | u32::from(loas[pos + 2]);
        let ln = (v & 0x1FFF) as usize;
        let mut br = crate::engine::bits::BitReader::new(&loas[pos + 3..pos + 3 + ln]);
        let same = br.read_bit().unwrap();
        if !same {
            mux = Some(crate::engine::latm::MuxCfg::parse(&mut br).unwrap());
        }
        let payload = crate::engine::latm::read_payload(&mut br, mux.as_ref().unwrap()).unwrap();
        let mut br = crate::engine::bits::BitReader::new(&payload);
        let _tag = br.read(4).unwrap();
        let body = crate::engine::ics_body::parse_ics(&mut br, 3, 23, None).unwrap();
        let noise = body.sections.sfb_cb.iter().any(|row| row.contains(&13));
        out.push(noise);
        pos += 3 + ln;
    }
    out
}

/// FDK `outputDelay` 0: every 512-sample frame is present, and frames
/// the noise generator cannot affect match the FDK PCM.
#[test]
fn ld48_loas_decodes_full_length_at_48k() -> Result<(), AacError> {
    let loas = include_bytes!("goldens/ld48.loas");
    let pcm = decode_split(loas)?;
    assert_eq!(pcm.sample_rate, 48_000);
    assert_eq!(pcm.channels.len(), 1);
    let gold = deinterleave(include_bytes!("goldens/ld48.fdk.s16"), 1);
    assert_eq!(pcm.channels[0].len(), gold[0].len());
    assert_eq!(pcm.channels[0].len() % FRAME, 0);
    let noise = noise_frames(loas);
    assert_eq!(noise.len(), pcm.channels[0].len() / FRAME);
    let mut clean_ours = Vec::new();
    let mut clean_gold = Vec::new();
    for (f, &is_noise) in noise.iter().enumerate() {
        // A noise frame's right half is the next frame's overlap.
        let contaminated = is_noise || (f > 0 && noise[f - 1]);
        if contaminated {
            continue;
        }
        let s = f * FRAME;
        let (max_lsb, _) = score(&pcm.channels[0][s..s + FRAME], &gold[0][s..s + FRAME]);
        assert!(max_lsb <= 2, "ld48 frame {f}: max_lsb {max_lsb}");
        clean_ours.extend_from_slice(&pcm.channels[0][s..s + FRAME]);
        clean_gold.extend_from_slice(&gold[0][s..s + FRAME]);
    }
    assert!(
        clean_ours.len() >= 32 * FRAME,
        "expected a run of PNS-free frames"
    );
    // A near-silent frame can sit under 55 dB from a 1 LSB difference.
    // The aggregate of the clean frames is the SNR gate.
    assert_gate("ld48 clean", &clean_ours, &clean_gold);
    Ok(())
}

#[test]
fn ld64mus_loas_stereo_matches_fdk() -> Result<(), AacError> {
    let pcm = decode_split(include_bytes!("goldens/ld64mus.loas"))?;
    assert_eq!(pcm.sample_rate, 48_000);
    assert_eq!(pcm.channels.len(), 2);
    let gold = deinterleave(include_bytes!("goldens/ld64mus.fdk.s16"), 2);
    for (ch, (got, exp)) in pcm.channels.iter().zip(gold.iter()).enumerate() {
        assert_eq!(got.len(), exp.len(), "ld64mus ch{ch}");
        assert_gate("ld64mus", got, exp);
    }
    Ok(())
}

#[test]
fn ld64m_loas_matches_fdk_and_xaac() -> Result<(), AacError> {
    let pcm = decode_split(include_bytes!("goldens/ld64m.loas"))?;
    assert_eq!(pcm.channels.len(), 1);
    let fdk = deinterleave(include_bytes!("goldens/ld64m.fdk.s16"), 1);
    let xaac = deinterleave(include_bytes!("goldens/ld64m.xaac.s16"), 1);
    assert_eq!(pcm.channels[0].len(), fdk[0].len());
    assert_eq!(pcm.channels[0].len(), xaac[0].len());
    assert_gate("ld64m fdk", &pcm.channels[0], &fdk[0]);
    // libxaac is fixed-point. This cell has no PNS residue above the FDK
    // gate; the remaining peak is rounding, and SNR stays on the gate.
    let (max_lsb, snr) = score(&pcm.channels[0], &xaac[0]);
    assert!(
        snr >= 55.0 && max_lsb <= 128,
        "ld64m xaac: max_lsb {max_lsb} snr {snr:.1}"
    );
    Ok(())
}

/// FDK reports encoder `nDelay` 512 and decoder `outputDelay` 0
/// (lab/profiles/REPORT.md). Matching the FDK PCM length, and the first
/// frame of a deterministic cell, means syom adds no decoder delay.
#[test]
fn ld_decoder_delay_matches_fdk_output_delay_zero() -> Result<(), AacError> {
    let cells: &[(&str, &[u8], &[u8], usize)] = &[
        (
            "ld64m",
            include_bytes!("goldens/ld64m.loas"),
            include_bytes!("goldens/ld64m.fdk.s16"),
            1,
        ),
        (
            "ld64mus",
            include_bytes!("goldens/ld64mus.loas"),
            include_bytes!("goldens/ld64mus.fdk.s16"),
            2,
        ),
        (
            "ld48",
            include_bytes!("goldens/ld48.loas"),
            include_bytes!("goldens/ld48.fdk.s16"),
            1,
        ),
    ];
    for (name, loas, pcm_bytes, ch) in cells {
        let pcm = decode_split(loas)?;
        let gold = deinterleave(pcm_bytes, *ch);
        assert_eq!(pcm.channels.len(), gold.len(), "{name}");
        for (plane, (got, exp)) in pcm.channels.iter().zip(gold.iter()).enumerate() {
            assert_eq!(
                got.len(),
                exp.len(),
                "{name} ch{plane} length (FDK outputDelay 0)"
            );
            assert_eq!(got.len() % FRAME, 0, "{name}");
        }
    }
    let pcm = decode_split(include_bytes!("goldens/ld64m.loas"))?;
    let gold = deinterleave(include_bytes!("goldens/ld64m.fdk.s16"), 1);
    assert_gate(
        "ld64m first frame",
        &pcm.channels[0][..FRAME],
        &gold[0][..FRAME],
    );
    let (slipped, _) = score(&pcm.channels[0][FRAME..], &gold[0][..gold[0].len() - FRAME]);
    assert!(
        slipped > 2,
        "a one-frame slip must not still match FDK (max_lsb {slipped})"
    );
    Ok(())
}

/// Push `feed` of the same LOAS bytes, any chunking, matches one-shot.
#[test]
fn ld_loas_chunked_feed_matches_one_shot() -> Result<(), AacError> {
    let loas = include_bytes!("goldens/ld64mus.loas");
    let one = decode_split(loas)?;
    let mut dec = Decoder::new(DecodeOptions::unbounded());
    let mut planes: Vec<Vec<f32>> = Vec::new();
    for chunk in loas.chunks(3) {
        dec.feed(chunk, |f| {
            assert_eq!(f.samples, FRAME);
            assert_eq!(f.sample_rate, 48_000);
            if planes.is_empty() {
                planes = f.planar.iter().map(|p| p.to_vec()).collect();
            } else {
                for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                    dst.extend_from_slice(src);
                }
            }
            Ok(())
        })?;
    }
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(planes.len(), one.channels.len());
    for (got, exp) in planes.iter().zip(one.channels.iter()) {
        assert_eq!(got, exp);
    }
    assert_eq!(info.samples, one.channels[0].len() as u64);
    Ok(())
}
