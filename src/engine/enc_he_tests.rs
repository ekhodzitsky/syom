//! TASK-89: HE v1 access units — decode rate/length, chunk/reset
//! identity, rate accounting, the HE-vs-LC HF energy gate, delay and
//! tail accounting.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{HE_PRIMING_OUT, HeEncoder, HeInfo};
use crate::engine::enc_frame::MAX_BITS_PER_CHANNEL;
use crate::engine::enc_sbr_prep::SbrPrep;
use crate::engine::enc_sbr_qmf::{BANDS, EncAnalysisQmf, EncSlot, core_cutoff_hz};
use crate::{DecodeOptions, EncodeOptions, ProbeProfile, decode_with, encode_with, probe};

const SR: u32 = 48_000;

fn lcg(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed as f32 / u32::MAX as f32) - 0.5
}

/// Harmonic "voice" (150 Hz comb with vibrato to 6 kHz) over a wideband floor plus
/// noise 6–16 kHz that ducks every 0.4 s: LF the core can code, HF only
/// SBR carries, and level changes for the grid.
fn material(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed;
    let mut out = vec![0.0f32; n];
    for h in 1..=40 {
        let f = 150.0 * h as f32;
        for (i, v) in out.iter_mut().enumerate() {
            let vib = 1.0 + 0.01 * (2.0 * std::f32::consts::PI * 5.0 * i as f32 / SR as f32).sin();
            let ph = (f64::from(f * vib) * i as f64 / f64::from(SR)).fract();
            *v += 0.012 * (2.0 * std::f64::consts::PI * ph).sin() as f32;
        }
    }
    // HF noise through a crude high-pass (≈ 6 kHz knee) that ducks. The
    // floor keeps every patch-source band under the crossover non-empty.
    let mut y1 = 0.0f32;
    let mut y2 = 0.0f32;
    for (i, v) in out.iter_mut().enumerate() {
        let x = lcg(&mut s);
        y1 = 0.55 * (y1 + x - y2);
        y2 = x;
        let duck = if (i / 19_200) % 2 == 0 { 1.0 } else { 0.25 };
        *v += 0.18 * duck * y1 + 0.01 * x; // plus a −26 dB wideband floor
    }
    out
}

fn stereo(n: usize) -> Vec<Vec<f32>> {
    let l = material(n, 0x1234_5678);
    let mut r = material(n, 0x8765_4321);
    for (a, b) in r.iter_mut().zip(&l) {
        *a = 0.7 * *a + 0.3 * *b;
    }
    vec![l, r]
}

fn encode_he(pcm: &[Vec<f32>], bps: u32, lookahead: bool, chunk: usize) -> (Vec<Vec<u8>>, HeInfo) {
    let mut enc = HeEncoder::new(SR, pcm.len(), bps, lookahead).unwrap();
    let mut aus = Vec::new();
    let n = pcm[0].len();
    let mut at = 0;
    while at < n {
        let end = (at + chunk).min(n);
        let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[at..end]).collect();
        enc.push(&planes, |au| {
            aus.push(au.to_vec());
            Ok(())
        })
        .unwrap();
        at = end;
    }
    let info = enc
        .finish(|au| {
            aus.push(au.to_vec());
            Ok(())
        })
        .unwrap();
    (aus, info)
}

fn adts(aus: &[Vec<u8>], channels: usize) -> Vec<u8> {
    let fs = HeEncoder::new(SR, channels, 48_000, false)
        .unwrap()
        .fs_index();
    let mut out = Vec::new();
    for au in aus {
        crate::encode::adts_frame_into(au, fs, channels, &mut out);
    }
    out
}

fn he_adts(pcm: &[Vec<f32>], bps: u32) -> Vec<u8> {
    adts(&encode_he(pcm, bps, false, 4096).0, pcm.len())
}

/// Energy in the v1 SBR range at 48 kHz (bands 18..41 = 6.75–15.4 kHz;
/// the crossover moves down with the rate, k2 stays) and below 6.75 kHz.
/// Content above k2 is not coded by v1 and is not counted.
fn hf_lf(x: &[f32]) -> (f64, f64) {
    let mut q = EncAnalysisQmf::new();
    let (mut hf, mut lf) = (0.0f64, 0.0f64);
    for c in x.chunks_exact(64) {
        let s = q.push_slot(c).unwrap();
        hf += f64::from(s.band_energy(18, 41));
        lf += f64::from(s.band_energy(0, 18));
    }
    assert_eq!(BANDS, 64);
    (hf, lf)
}

fn db(a: f64, b: f64) -> f64 {
    10.0 * (a.max(1e-30) / b.max(1e-30)).log10()
}

/// Mean |level error| (dB) per QMF band over the v1 SBR range — the
/// per-band twin of `hf_lf`'s aggregate (TASK-133: the improved LC holds
/// the aggregate HF *level*, so the HE gate moved to per-band fidelity).
fn hf_band_err(x: &[f32], src: &[f32]) -> f64 {
    let mut qx = EncAnalysisQmf::new();
    let mut qs = EncAnalysisQmf::new();
    let mut ex = [0.0f64; BANDS];
    let mut es = [0.0f64; BANDS];
    for (cx, cs) in x.chunks_exact(64).zip(src.chunks_exact(64)) {
        let sx = qx.push_slot(cx).unwrap();
        let ss = qs.push_slot(cs).unwrap();
        for b in 18..41 {
            ex[b] += f64::from(sx.band_energy(b, b + 1));
            es[b] += f64::from(ss.band_energy(b, b + 1));
        }
    }
    let (mut err, mut n) = (0.0f64, 0usize);
    for b in 18..41 {
        if es[b] > 0.0 {
            err += db(ex[b], es[b]).abs();
            n += 1;
        }
    }
    err / n as f64
}

#[test]
fn decodes_at_twice_the_core_rate_with_one_au_per_2048_samples() {
    let pcm = stereo(SR as usize);
    let (aus, info) = encode_he(&pcm, 48_000, false, 4096);
    let stream = adts(&aus, 2);
    assert_eq!(
        probe(&stream).unwrap().profile,
        ProbeProfile::Lc,
        "ADTS header stays LC (implicit SBR)"
    );
    let dec = decode_with(&stream, &DecodeOptions::audio()).unwrap();
    assert_eq!(
        (dec.sample_rate, dec.core_rate, dec.channels.len()),
        (48_000, 24_000, 2)
    );
    assert_eq!(dec.channels[0].len() as u64, info.aus * 2048);
    assert_eq!(info.source, SR as u64);
    // content core = ceil((N + 8) / 2) → frames + one drain.
    assert_eq!(info.core_content, (SR as u64 + 8).div_ceil(2));
    assert_eq!(info.aus, info.core_content.div_ceil(1024) + 1);
    assert_eq!(info.priming_out, HE_PRIMING_OUT);
    assert_eq!(
        info.remainder_out,
        info.aus * 2048 - HE_PRIMING_OUT - info.source
    );
    let speech = decode_with(&stream, &DecodeOptions::speech()).unwrap();
    assert_eq!(speech.sample_rate, 48_000);
}

#[test]
fn every_au_carries_sbr_data_not_only_a_header() {
    let silent = [vec![0.0f32; 2048 * 3]];
    let mut enc = HeEncoder::new(SR, 1, 24_000, false).unwrap();
    let mut lens = Vec::new();
    enc.push(&[&silent[0]], |_| Ok(())).unwrap();
    lens.push(enc.last_fill_len());
    enc.finish(|_| Ok(())).unwrap();
    lens.push(enc.last_fill_len());
    // nibble 4 + flag 1 + header 16 + grid/dtdf/invf/rows ≥ 42 → ≥ 8 bytes on silence.
    assert!(lens.iter().all(|&l| l >= 8), "{lens:?}");
    assert!(enc.push(&[&silent[0]], |_| Ok(())).is_err(), "finished");
    assert!(HeEncoder::new(SR, 3, 24_000, false).is_err());
    assert!(HeEncoder::new(96_000, 1, 24_000, false).is_err());
    let mut empty = HeEncoder::new(SR, 1, 24_000, false).unwrap();
    assert!(empty.finish(|_| Ok(())).is_err(), "no input");
}

#[test]
fn chunking_reset_and_lookahead_are_deterministic() {
    let pcm = stereo(SR as usize / 2 + 777);
    let (a, ia) = encode_he(&pcm, 32_000, false, 1);
    let (b, ib) = encode_he(&pcm, 32_000, false, 333);
    let (c, _) = encode_he(&pcm, 32_000, false, 2048);
    let (d, _) = encode_he(&pcm, 32_000, false, 100_000);
    assert!(
        a == b && b == c && c == d,
        "chunk partition changed the AUs"
    );
    assert_eq!(ia, ib);
    let (la, li) = encode_he(&pcm, 32_000, true, 1000);
    let (lb, _) = encode_he(&pcm, 32_000, true, 5000);
    assert_eq!(la, lb, "lookahead chunking");
    assert_eq!(li.aus, ia.aus, "lookahead adds no AU");
    let mut enc = HeEncoder::new(SR, 2, 32_000, false).unwrap();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.push(&planes[..], |_| Ok(())).unwrap();
    enc.reset();
    let mut again = Vec::new();
    enc.push(&planes[..], |au| {
        again.push(au.to_vec());
        Ok(())
    })
    .unwrap();
    enc.finish(|au| {
        again.push(au.to_vec());
        Ok(())
    })
    .unwrap();
    assert_eq!(again, a, "reset restarts the stream");
}

#[test]
fn rate_accounting_holds_the_whole_stream_ceiling() {
    let pcm = stereo(SR as usize * 10);
    for bps in [24_000u32, 48_000] {
        let (aus, info) = encode_he(&pcm, bps, false, 4096);
        let bytes: usize = aus.iter().map(Vec::len).sum();
        let secs = info.aus as f64 * 2048.0 / SR as f64;
        let achieved = bytes as f64 * 8.0 / secs;
        let max_bits = MAX_BITS_PER_CHANNEL * 2;
        let biggest = aus.iter().map(|a| a.len() * 8).max().unwrap();
        println!(
            "{bps} bps: achieved {achieved:.0} bps over {secs:.2} s, largest AU {biggest} bits"
        );
        assert!(biggest <= max_bits, "AU {biggest} bits > 6144/channel");
        assert!(
            achieved <= f64::from(bps) * 1.03,
            "{bps}: achieved {achieved}"
        );
        assert!(
            achieved >= f64::from(bps) * 0.90,
            "{bps}: achieved {achieved}"
        );
    }
}

#[test]
fn he_keeps_more_hf_energy_than_lc_at_the_same_rate() {
    // HE_ENC.md screening gate: decoded HF (above k0) vs the original.
    // TASK-133: the retuned LC core holds the aggregate HF *level* on the
    // voice clip (coarse quantization of noise-like bands preserves energy),
    // so the HE ≥ LC gate runs on per-band HF fidelity of a white-noise
    // probe, where a bit-starved LC cannot hold the spectrum.
    let pcm = stereo(SR as usize * 2);
    let (hf_src, lf_src) = hf_lf(&pcm[0]);
    let mut s = 0x2468_ACE0u32;
    let noise: Vec<Vec<f32>> = (0..2)
        .map(|_| (0..SR as usize * 2).map(|_| 0.6 * lcg(&mut s)).collect())
        .collect();
    for bps in [24_000u32, 32_000, 48_000] {
        let he = decode_with(&he_adts(&pcm, bps), &DecodeOptions::audio()).unwrap();
        let lc_stream =
            encode_with(&pcm, SR, &EncodeOptions::adts().with_bitrate_bps(bps)).unwrap();
        let lc = decode_with(&lc_stream, &DecodeOptions::audio()).unwrap();
        let (hf_he, lf_he) = hf_lf(&he.channels[0]);
        let (hf_lc, lf_lc) = hf_lf(&lc.channels[0]);
        let (g_he, g_lc) = (db(hf_he, hf_src), db(hf_lc, hf_src));
        let he_n = decode_with(&he_adts(&noise, bps), &DecodeOptions::audio()).unwrap();
        let lc_n = encode_with(&noise, SR, &EncodeOptions::adts().with_bitrate_bps(bps)).unwrap();
        let lc_n = decode_with(&lc_n, &DecodeOptions::audio()).unwrap();
        let err_he = hf_band_err(&he_n.channels[0], &noise[0]);
        let err_lc = hf_band_err(&lc_n.channels[0], &noise[0]);
        println!(
            "{bps} bps: HF vs source HE {g_he:+.1} dB, LC {g_lc:+.1} dB; LF HE {:+.1} dB, LC {:+.1} dB; noise HF band err HE {err_he:.1} dB, LC {err_lc:.1} dB",
            db(lf_he, lf_src),
            db(lf_lc, lf_src)
        );
        assert!(
            err_he + 4.0 <= err_lc,
            "{bps}: HE noise HF band err {err_he:.1} dB vs LC {err_lc:.1} dB"
        );
        assert!(
            g_he.abs() <= 3.0,
            "{bps}: HE HF level {g_he} dB off the source"
        );
        // Coarse quantization of noise-like bands adds energy (LC at 128 kbps
        // shows the same +2–3 dB on this material).
        assert!(db(lf_he, lf_src).abs() <= 4.0, "{bps}: HE LF level off");
    }
}

/// Integer lag of `out` against `src` maximising the cross-correlation
/// over `range` (a Hann-windowed LF burst has one unambiguous peak).
fn lag_of(src: &[f32], out: &[f32], range: std::ops::Range<usize>) -> usize {
    let c = |l: usize| -> f64 {
        src.iter()
            .zip(&out[l..])
            .map(|(&s, &o)| f64::from(s) * f64::from(o))
            .sum()
    };
    range
        .max_by(|&a, &b| c(a).partial_cmp(&c(b)).unwrap())
        .unwrap()
}

#[test]
fn burst_delay_and_final_samples_are_accounted() {
    let n = SR as usize;
    let mut pcm = vec![vec![0.0f32; n]];
    // 300 Hz burst under a Hann window (LF: the core carries it, the
    // QMF chain only delays it), then a noise burst in the final 200
    // samples that only survives if the drain frame carries the tail.
    for i in 0..2_000 {
        let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / 2_000.0).cos();
        pcm[0][5_000 + i] =
            0.8 * w * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / SR as f32).sin();
    }
    let mut s = 7u32;
    for v in &mut pcm[0][n - 200..] {
        *v = 0.5 * lcg(&mut s);
    }
    let (aus, info) = encode_he(&pcm, 48_000, false, 4096);
    let dec = decode_with(&adts(&aus, 1), &DecodeOptions::audio()).unwrap();
    let out = &dec.channels[0];
    let lag = lag_of(&pcm[0][..8_000], out, 2_000..4_500);
    println!(
        "burst lag {lag} output samples (priming {})",
        info.priming_out
    );
    assert_eq!(
        lag as u64, info.priming_out,
        "priming_out must pin the decoded lag"
    );
    let start = info.priming_out as usize + n - 200;
    assert!(start + 200 <= out.len(), "decoded length covers the tail");
    let tail: f64 = out[start..start + 200]
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum();
    let src: f64 = pcm[0][n - 200..]
        .iter()
        .map(|&v| f64::from(v) * f64::from(v))
        .sum();
    assert!(
        tail > 0.1 * src,
        "final 200 samples reconstruct: {tail} vs {src}"
    );
    assert!(info.remainder_out < 2048 * 3);
}

/// Lengths whose coded tail already covers priming + source stay at one
/// drain frame. 3127 samples need 6145 decoded samples and three AUs
/// only provide 6144, so finish adds exactly one silent AU.
#[test]
fn sbr_delay_deficit_gains_exactly_one_silent_au() {
    for lookahead in [false, true] {
        for n in [2048usize, 4096] {
            let pcm = [vec![0.0f32; n]];
            let (_, info) = encode_he(&pcm, 24_000, lookahead, n);
            assert_eq!(
                info.aus,
                info.core_content.div_ceil(1024) + 1,
                "n={n} lookahead={lookahead} already covers the tail"
            );
            assert_eq!(
                info.remainder_out,
                info.aus * 2048 - HE_PRIMING_OUT - n as u64
            );
        }
        let n = 3127usize;
        let pcm = [vec![0.0f32; n]];
        let (_, info) = encode_he(&pcm, 24_000, lookahead, 1000);
        let need = HE_PRIMING_OUT + n as u64;
        assert_eq!(
            info.aus,
            info.core_content.div_ceil(1024) + 2,
            "lookahead={lookahead}"
        );
        assert_eq!(info.remainder_out, info.aus * 2048 - need);
        assert!(info.remainder_out < 2048);
    }
}

/// Offline mint: `MINT_GOLDENS=1 cargo test --lib mint_lavc_he_golden`, then
/// `ffmpeg -y -i src/goldens/he48e.adts -f s16le src/goldens/he48e.lavc.s16`.
/// Tests never spawn ffmpeg; the committed s16 is the oracle.
#[test]
fn mint_lavc_he_golden() {
    if std::env::var("MINT_GOLDENS").is_err() {
        return;
    }
    let pcm = crate::encode_tests::lavc_fixture();
    let stream = he_adts(&pcm, 48_000);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    std::fs::write(dir.join("he48e.adts"), stream).expect("write golden");
}

fn slot_band(k: usize, amp: f32) -> EncSlot {
    let mut s = EncSlot {
        re: [0.0; BANDS],
        im: [0.0; BANDS],
    };
    s.re[k] = amp;
    s
}

/// 20 dB skirt rule: a formant keeps its own band, a second channel's
/// peak raises the cutoff, flat energy and silence stay at k0.
#[test]
fn formant_cutoff_drops_skirts_and_flat_noise_keeps_k0() -> Result<(), &'static str> {
    let rate = 48_000u32;
    let kx = 12usize;
    let peaked = vec![slot_band(1, 1.0); 32];
    // Band 2 at −40 dB stays under the 20 dB threshold (750 Hz is band 1's top).
    let mut skirt = slot_band(1, 1.0);
    skirt.re[2] = 0.01;
    let skirt_slots = vec![skirt; 32];
    assert_eq!(core_cutoff_hz(&[&peaked], kx, rate), 750);
    assert_eq!(core_cutoff_hz(&[&skirt_slots], kx, rate), 750);
    // −14 dB (amplitude 0.2) stays inside the 20 dB window → band 2.
    let mut near = slot_band(1, 1.0);
    near.re[2] = 0.2;
    let near_slots = vec![near; 32];
    assert_eq!(core_cutoff_hz(&[&near_slots], kx, rate), 1_125);
    let mut flat = EncSlot {
        re: [0.0; BANDS],
        im: [0.0; BANDS],
    };
    for k in 0..kx {
        flat.re[k] = 1.0;
    }
    let flat_slots = vec![flat; 32];
    assert_eq!(core_cutoff_hz(&[&flat_slots], kx, rate), 4_500);
    let silent = vec![
        EncSlot {
            re: [0.0; BANDS],
            im: [0.0; BANDS],
        };
        32
    ];
    assert_eq!(core_cutoff_hz(&[&silent], kx, rate), 4_500);
    let hi = vec![slot_band(8, 1.0); 32];
    assert_eq!(core_cutoff_hz(&[&peaked, &hi], kx, rate), 3_375);

    let mut pcm = vec![0.0f32; 2048];
    for (i, s) in pcm.iter_mut().enumerate() {
        let ph = 2.0 * std::f32::consts::PI * 500.0 * i as f32 / rate as f32;
        *s = 0.2 * ph.sin();
    }
    let mut prep = SbrPrep::new();
    let mut core = Vec::new();
    let mut slots = Vec::new();
    prep.push(&pcm, &mut core, |s| slots.push(*s))
        .map_err(|_| "prep")?;
    let hz = core_cutoff_hz(&[&slots], kx, rate);
    assert!(hz <= 1_125, "500 Hz tone cutoff {hz} Hz (QMF leakage)");
    Ok(())
}

/// A tone then noise must not take its cutoff from the tail of the push.
#[test]
fn shaped_cutoff_is_per_frame_not_per_push() {
    let n = SR as usize / 2;
    let mut l = vec![0.0f32; n];
    for (i, s) in l.iter_mut().enumerate().take(n / 2) {
        let ph = 2.0 * std::f32::consts::PI * 400.0 * i as f32 / SR as f32;
        *s = 0.3 * ph.sin();
    }
    let mut seed = 0xA5A5_5A5Au32;
    for s in l.iter_mut().skip(n / 2) {
        *s = 0.2 * lcg(&mut seed);
    }
    let pcm = vec![l.clone(), l];
    let (one, _) = encode_he(&pcm, 24_000, false, n);
    let (fine, _) = encode_he(&pcm, 24_000, false, 64);
    assert_eq!(one, fine, "push size changed the shaped core");
}

/// Two decoder lineages on our HE bits: the committed stream is
/// byte-exact with a fresh encode (determinism tripwire), and our decode
/// of it matches libavcodec's within the LC golden tolerance.
#[test]
fn lavc_matches_our_decode_of_our_he_adts() {
    let golden = include_bytes!("../goldens/he48e.adts");
    let lavc = include_bytes!("../goldens/he48e.lavc.s16");
    let fresh = he_adts(&crate::encode_tests::lavc_fixture(), 48_000);
    assert_eq!(
        fresh.as_slice(),
        &golden[..],
        "HE encoder drifted; re-mint with MINT_GOLDENS=1"
    );
    let dec = decode_with(&fresh, &DecodeOptions::unbounded()).unwrap();
    assert_eq!((dec.sample_rate, dec.channels.len()), (48_000, 2));
    crate::encode_tests::assert_decode_matches_lavc_pub(&fresh, lavc);
}
