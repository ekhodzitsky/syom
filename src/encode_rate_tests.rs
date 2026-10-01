//! TASK-65: live rate accounting on shipped `encode_with` / `Encoder`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::encode_cmp::adts_payload_bytes;
use crate::engine::adts::AdtsHeader;
use crate::engine::enc_frame::MAX_BITS_PER_CHANNEL;
use crate::{DecodeOptions, EncodeInfo, EncodeOptions, Encoder, Result, decode_with, encode_with};

const RATE: u32 = 48_000;
const N1: usize = 48_000; // 1 s, same as TASK-16

struct Sweep {
    name: &'static str,
    class: &'static str,
    pcm: Vec<Vec<f32>>,
    bps: u32,
}

struct Row {
    name: &'static str,
    class: &'static str,
    requested: u32,
    info: EncodeInfo,
    payload: usize,
    n_frames: usize,
    bits_min: usize,
    bits_max: usize,
    bits_mean: f64,
    budget: usize,
    payload_bps_valid: f64,
    payload_bps_coded: f64,
    adts_bps_valid: f64,
    fullness_vbr: bool,
}

fn sine(n: usize, hz: f64, amp: f32) -> Vec<f32> {
    (0..n)
        .map(|i| amp * (2.0 * std::f64::consts::PI * hz * i as f64 / f64::from(RATE)).sin() as f32)
        .collect()
}

fn noise(n: usize, amp: f32) -> Vec<f32> {
    let mut s = 0x0BAD_F00Du32;
    (0..n)
        .map(|_| {
            s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            amp * (((s >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect()
}

fn clicks(n: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    let mut i = 0;
    while i < n {
        v[i] = 0.9;
        i += 2048;
    }
    v
}

fn encode_tracked(pcm: &[Vec<f32>], bps: u32) -> Result<(Vec<u8>, EncodeInfo)> {
    let opts = EncodeOptions::adts().with_bitrate_bps(bps);
    let mut enc = Encoder::new(RATE, pcm.len(), &opts)?;
    let mut out = Vec::new();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    let info = enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    Ok((out, info))
}

fn walk_frames(adts: &[u8]) -> Result<Vec<(u16, usize)>> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos < adts.len() {
        let (hdr, off) = AdtsHeader::parse(&adts[pos..]).map_err(crate::AacError::from)?;
        let fl = usize::from(hdr.aac_frame_length);
        out.push((hdr.adts_buffer_fullness, fl - off));
        pos += fl;
    }
    Ok(out)
}

fn budget_bits(bps: u32, ch: usize) -> usize {
    let bits = u64::from(bps) * 1024 / u64::from(RATE);
    (bits as usize).min(MAX_BITS_PER_CHANNEL * ch)
}

fn row(s: &Sweep) -> Result<Row> {
    let (adts, info) = encode_tracked(&s.pcm, s.bps)?;
    let oneshot = encode_with(&s.pcm, RATE, &EncodeOptions::adts().with_bitrate_bps(s.bps))?;
    assert_eq!(
        adts.as_slice(),
        crate::gapless::strip_id3(&oneshot),
        "{} streaming != one-shot",
        s.name
    );
    assert_eq!(info.bytes as usize, adts.len());
    let frames = walk_frames(&adts)?;
    let payload = adts_payload_bytes(&adts).expect("payload");
    assert_eq!(payload, frames.iter().map(|f| f.1).sum::<usize>());
    assert_eq!(frames.len() as u64, info.aac_frames);
    let n = info.samples;
    let coded = info.coded_samples;
    let payload_bps_valid = 8.0 * payload as f64 * f64::from(RATE) / n as f64;
    let payload_bps_coded = 8.0 * payload as f64 * f64::from(RATE) / coded as f64;
    let adts_bps_valid = 8.0 * adts.len() as f64 * f64::from(RATE) / n as f64;
    let bits: Vec<usize> = frames.iter().map(|f| f.1 * 8).collect();
    let bits_min = *bits.iter().min().unwrap();
    let bits_max = *bits.iter().max().unwrap();
    let bits_mean = bits.iter().sum::<usize>() as f64 / bits.len() as f64;
    Ok(Row {
        name: s.name,
        class: s.class,
        requested: s.bps,
        info,
        payload,
        n_frames: frames.len(),
        bits_min,
        bits_max,
        bits_mean,
        budget: budget_bits(s.bps, s.pcm.len()),
        payload_bps_valid,
        payload_bps_coded,
        adts_bps_valid,
        fullness_vbr: frames.iter().all(|f| f.0 == 0x7FF),
    })
}

fn matrix() -> Result<Vec<Sweep>> {
    let lecture = decode_with(
        include_bytes!("goldens/lecture.m4a"),
        &DecodeOptions::audio(),
    )?;
    let n = lecture.channels[0].len().min(N1);
    let speech: Vec<Vec<f32>> = lecture.channels.iter().map(|p| p[..n].to_vec()).collect();
    let trem_l = sine(N1, 440.0, 0.4);
    let trem_r: Vec<f32> = trem_l.iter().map(|x| x * 0.8).collect();
    Ok(vec![
        Sweep {
            name: "silence",
            class: "silence",
            pcm: vec![vec![0.0f32; N1]],
            bps: 128_000,
        },
        Sweep {
            name: "sine440",
            class: "tonal",
            pcm: vec![sine(N1, 440.0, 0.5)],
            bps: 64_000,
        },
        Sweep {
            name: "sine440",
            class: "tonal",
            pcm: vec![sine(N1, 440.0, 0.5)],
            bps: 128_000,
        },
        Sweep {
            name: "noise",
            class: "noise",
            pcm: vec![noise(N1, 0.4)],
            bps: 64_000,
        },
        Sweep {
            name: "noise",
            class: "noise",
            pcm: vec![noise(N1, 0.4)],
            bps: 128_000,
        },
        Sweep {
            name: "click",
            class: "transient",
            pcm: vec![clicks(N1)],
            bps: 64_000,
        },
        Sweep {
            name: "click",
            class: "transient",
            pcm: vec![clicks(N1)],
            bps: 128_000,
        },
        Sweep {
            name: "tremolo",
            class: "stereo",
            pcm: vec![trem_l, trem_r],
            bps: 128_000,
        },
        Sweep {
            name: "lecture",
            class: "speech",
            pcm: speech,
            bps: 128_000,
        },
    ])
}

fn ratio(r: &Row) -> f64 {
    r.payload_bps_valid / f64::from(r.requested)
}

#[test]
fn live_rate_matrix_payload_vs_transport_vs_duration() -> Result<()> {
    let rows: Vec<Row> = matrix()?.iter().map(|s| row(s).unwrap()).collect();
    eprintln!(
        "clip class req payload adts frames valid coded pay/valid pay/coded adts/valid bits[min,mean,max] budget ratio"
    );
    for r in &rows {
        eprintln!(
            "{} {} {} {} {} {} {} {} {:.0} {:.0} {:.0} [{}, {:.0}, {}] {} {:.3}",
            r.name,
            r.class,
            r.requested,
            r.payload,
            r.info.bytes,
            r.n_frames,
            r.info.samples,
            r.info.coded_samples,
            r.payload_bps_valid,
            r.payload_bps_coded,
            r.adts_bps_valid,
            r.bits_min,
            r.bits_mean,
            r.bits_max,
            r.budget,
            ratio(r)
        );
    }
    for r in &rows {
        assert!(r.fullness_vbr, "{}: ADTS fullness is not VBR 0x7FF", r.name);
        assert_eq!(
            r.info.bytes as usize,
            r.payload + 7 * r.n_frames,
            "{}: ADTS bytes != payload + 7*frames",
            r.name
        );
        assert_eq!(r.info.priming, 1024);
        assert_eq!(r.info.coded_samples, r.info.aac_frames * 1024);
        assert!(
            r.info.coded_samples > r.info.samples,
            "{}: drain/remainder must lengthen coded vs valid N",
            r.name
        );
        let cap = MAX_BITS_PER_CHANNEL * r.info.channels;
        assert!(
            r.bits_max <= cap,
            "{}: frame {} bits > LC cap {cap}",
            r.name,
            r.bits_max
        );
        assert!(
            r.payload_bps_coded < r.payload_bps_valid,
            "{}: payload/coded must be below payload/valid",
            r.name
        );
    }
    let find = |name, bps| {
        rows.iter()
            .find(|r| r.name == name && r.requested == bps)
            .unwrap()
    };
    let silence = find("silence", 128_000);
    assert!(
        ratio(silence) < 0.15,
        "silence must not be stuffed to 128k: {:.0}",
        silence.payload_bps_valid
    );
    Ok(())
}

const N10: usize = 10 * N1;

fn tile(planes: &[Vec<f32>], n: usize) -> Vec<Vec<f32>> {
    planes
        .iter()
        .map(|p| (0..n).map(|i| p[i % p.len()]).collect())
        .collect()
}

fn assert_abr_pm3(r: &Row) {
    let rel = (ratio(r) - 1.0).abs();
    assert!(
        rel <= 0.03,
        "{} {}k: payload/valid {:.0} bps (ratio {:.3}, need ±3%) bits[{}, {:.0}, {}] budget {}",
        r.name,
        r.requested / 1000,
        r.payload_bps_valid,
        ratio(r),
        r.bits_min,
        r.bits_mean,
        r.bits_max,
        r.budget
    );
    let cap = MAX_BITS_PER_CHANNEL * r.info.channels;
    assert!(r.bits_max <= cap, "{} over LC cap", r.name);
    assert!(r.fullness_vbr);
}

#[test]
fn ten_second_non_silent_tracks_meet_abr_pm3() -> Result<()> {
    let lecture = decode_with(
        include_bytes!("goldens/lecture.m4a"),
        &DecodeOptions::audio(),
    )?;
    let trem_l = sine(N10, 440.0, 0.4);
    let trem_r: Vec<f32> = trem_l.iter().map(|x| x * 0.8).collect();
    let clips = [
        Sweep {
            name: "sine10s",
            class: "tonal",
            pcm: vec![sine(N10, 440.0, 0.5)],
            bps: 128_000,
        },
        Sweep {
            name: "noise10s",
            class: "noise",
            pcm: vec![noise(N10, 0.4)],
            bps: 64_000,
        },
        Sweep {
            name: "noise10s",
            class: "noise",
            pcm: vec![noise(N10, 0.4)],
            bps: 128_000,
        },
        Sweep {
            name: "tremolo10s",
            class: "stereo",
            pcm: vec![trem_l, trem_r],
            bps: 128_000,
        },
        Sweep {
            name: "lecture10s",
            class: "speech",
            pcm: tile(&lecture.channels, N10),
            bps: 128_000,
        },
    ];
    for s in &clips {
        let r = row(s)?;
        eprintln!(
            "ABR {} req {} pay/valid {:.0} ratio {:.3} bits mean {:.0}/{}",
            r.name,
            r.requested,
            r.payload_bps_valid,
            ratio(&r),
            r.bits_mean,
            r.budget
        );
        assert_abr_pm3(&r);
    }
    Ok(())
}

#[test]
fn short_and_impossible_rates_stay_exceptions() -> Result<()> {
    let pcm = vec![sine(1000, 440.0, 0.5)];
    let r = row(&Sweep {
        name: "short",
        class: "tonal",
        pcm,
        bps: 128_000,
    })?;
    assert!(r.info.samples < 2048);
    assert!(r.bits_max <= MAX_BITS_PER_CHANNEL);
    assert!(Encoder::new(RATE, 1, &EncodeOptions::adts().with_bitrate_bps(0)).is_err());
    assert!(Encoder::new(RATE, 1, &EncodeOptions::adts().with_bitrate_bps(1_000_000)).is_err());
    Ok(())
}

#[test]
fn abr_fill_keeps_sine_audible_after_priming() -> Result<()> {
    let src = sine(N1, 440.0, 0.5);
    let r = row(&Sweep {
        name: "sine_snr",
        class: "tonal",
        pcm: vec![src.clone()],
        bps: 128_000,
    })?;
    // 1 s pay/valid includes drain; ±3% is the ≥10 s gate (`ten_second_*`).
    assert!(r.fullness_vbr);
    assert!(r.bits_max <= MAX_BITS_PER_CHANNEL);
    assert!(
        ratio(&r) < 1.10,
        "1 s sine pay/valid {:.3} (drain may exceed ±3%)",
        ratio(&r)
    );
    let dec = decode_with(
        &encode_with(
            std::slice::from_ref(&src),
            RATE,
            &EncodeOptions::adts().with_bitrate_bps(128_000),
        )?,
        &DecodeOptions::unbounded(),
    )?;
    let y = if dec.channels[0].len() == N1 {
        dec.channels[0].as_slice()
    } else {
        &dec.channels[0][1024..1024 + N1]
    };
    let mut sig = 0.0f64;
    let mut err = 0.0f64;
    for (a, b) in src.iter().zip(y.iter()) {
        let a = f64::from(*a);
        let d = a - f64::from(*b);
        sig += a * a;
        err += d * d;
    }
    let snr = 10.0 * (sig / err.max(1e-20)).log10();
    assert!(
        snr > 25.0 && y.iter().all(|x| x.is_finite()),
        "sine SNR after ABR fill {snr:.1} dB"
    );
    Ok(())
}

#[test]
fn abr_undershoot_pads_fill_elements_before_end() -> Result<()> {
    let src = sine(N1, 440.0, 0.5);
    let (adts, _) = encode_tracked(&[src], 128_000)?;
    let frames = walk_frames(&adts)?;
    let mut pos = 0usize;
    let mut saw_pad = false;
    for &(fullness, pay) in &frames {
        assert_eq!(fullness, 0x7FF);
        let (hdr, off) = AdtsHeader::parse(&adts[pos..]).map_err(crate::AacError::from)?;
        let fl = usize::from(hdr.aac_frame_length);
        let payload = &adts[pos + off..pos + fl];
        if pay * 8 >= budget_bits(128_000, 1) * 97 / 100 && payload.len() >= 16 {
            // TASK-121: leftover budget is EXT_FILL fill_element()s before
            // ID_END; the frame ends with END + <8 align bits, so the last
            // byte is never zero (fdk-aac rejects trailing zero stuffing).
            saw_pad = true;
            assert_ne!(payload[payload.len() - 1], 0, "zero stuffing after ID_END");
        }
        pos += fl;
    }
    assert!(saw_pad, "leftover budget must pad to the ABR ceiling");
    Ok(())
}

#[test]
fn abr_fill_padding_is_decode_neutral() -> Result<()> {
    let l = sine(N1 / 4, 440.0, 0.5);
    let r = sine(N1 / 4, 2_997.0, 0.4);
    let stuffed = encode_with(
        &[l, r],
        RATE,
        &EncodeOptions::adts().with_bitrate_bps(128_000),
    )?;
    // Every AU is consumed through ID_END: padding is in-band EXT_FILL
    // (ignored by the decoder), never trailing bytes (TASK-121).
    let au = crate::gapless::strip_id3(&stuffed);
    let mut pos = 0usize;
    let mut dec = crate::engine::decode::StreamDecoder::new();
    while pos < au.len() {
        let (hdr, off) = AdtsHeader::parse(&au[pos..]).map_err(crate::AacError::from)?;
        let fl = usize::from(hdr.aac_frame_length);
        let payload = &au[pos + off..pos + fl];
        dec.decode_raw_data_block(
            hdr.audio_object_type(),
            hdr.sampling_frequency_index,
            RATE,
            hdr.channel_configuration,
            1,
            payload,
        )
        .map_err(crate::AacError::from)?;
        assert_eq!(
            dec.last_rdb_bytes,
            payload.len(),
            "padding bytes must sit inside the AU, not trail ID_END"
        );
        pos += fl;
    }
    let pcm = decode_with(&stuffed, &DecodeOptions::unbounded())?;
    assert_eq!(pcm.channels.len(), 2);
    assert_eq!(pcm.sample_rate, RATE);
    Ok(())
}

#[test]
fn rate_report_names_measurement_and_task66_gates() {
    let raw = include_str!("../corpus/rate/REPORT.md");
    assert!(raw.contains("pay/valid"));
    assert!(raw.contains("0x7FF"));
    assert!(raw.contains("±3%"));
    assert!(raw.contains("TASK-66"));
    assert!(raw.contains("credit"));
}
