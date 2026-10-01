//! TASK-52: ASC + raw access-unit decode vs framed goldens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::engine::adts::AdtsHeader;
use crate::engine::asc::write_lc;
use crate::{
    AacError, DecodeOptions, Decoder, LifecycleState, Result, StreamInfo, UnsupportedFeature,
    decode_with,
};

const SINE48: &[u8] = include_bytes!("goldens/sine48.adts");
const HE48: &[u8] = include_bytes!("goldens/he48.adts");
const HE48_LATM: &[u8] = include_bytes!("goldens/he48.latm");
const HE48_M4A: &[u8] = include_bytes!("goldens/he48.m4a");
const PS48: &[u8] = include_bytes!("goldens/ps48.adts");
const PS48_M4A: &[u8] = include_bytes!("goldens/ps48.m4a");
/// TASK-127: 8 loud-window AUs' worth of lavf PCM plus all 94 raw
/// AUs of the GPAC-muxed Akamai BBB DASH HE-AAC segment
/// (`lab/fmp4/fixtures/thirdparty/bbb_seg1.m4a`, sha256 pinned in
/// `lab/fmp4/PIN.md`). 14 AUs carry a zero-count FIL element that
/// pristine syom rejected (lab/fmp4/REPORT.md §4.1); the first one is
/// AU 1. `.au` blob: u32-LE AU count, then per AU u32-LE length + AU
/// bytes. `.s16`: lavf decode (ffmpeg 7.0.2-static, oracle) of AUs
/// 60..68, stereo interleaved; the segment is quiet (peak ~376) so
/// parity is scored as max-LSB/rms, not the 70 dB SNR gate of
/// decode_tests.
const BBB_AU: &[u8] = include_bytes!("goldens/bbb_fil.au");
const BBB_S16: &[u8] = include_bytes!("goldens/bbb_fil.s16");

fn adts_payloads(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let Ok((hdr, off)) = AdtsHeader::parse(&bytes[pos..]) else {
            break;
        };
        let fl = usize::from(hdr.aac_frame_length);
        if fl < off || pos + fl > bytes.len() {
            break;
        }
        out.push(&bytes[pos + off..pos + fl]);
        pos += fl;
    }
    out
}

fn collect_framed(bytes: &[u8], opts: &DecodeOptions) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let info = crate::decode_streaming(bytes, opts, |f| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    })?;
    Ok((info, planes))
}

fn collect_au(
    asc: &[u8],
    aus: &[&[u8]],
    opts: DecodeOptions,
) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut dec = Decoder::from_asc(asc, opts)?;
    let mut planes: Vec<Vec<f32>> = Vec::new();
    for au in aus {
        dec.decode_au(au, |f| {
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
    Ok((info, planes))
}

fn m4a_asc(data: &[u8]) -> Vec<u8> {
    crate::isomp4::parse_aac_track(data).expect("m4a").asc
}

fn assert_same(a: &(StreamInfo, Vec<Vec<f32>>), b: &(StreamInfo, Vec<Vec<f32>>), tag: &str) {
    assert_eq!(a.0.sample_rate, b.0.sample_rate, "{tag} rate");
    assert_eq!(a.0.channels, b.0.channels, "{tag} ch");
    assert_eq!(a.0.samples, b.0.samples, "{tag} samples");
    assert_eq!(a.0.aac_frames, b.0.aac_frames, "{tag} frames");
    assert_eq!(a.1, b.1, "{tag} pcm");
}

#[test]
fn lc_raw_au_matches_adts() -> Result<()> {
    let opts = DecodeOptions::speech();
    let framed = collect_framed(SINE48, &opts)?;
    let aus = adts_payloads(SINE48);
    assert!(!aus.is_empty());
    let raw = collect_au(&write_lc(3, 1), &aus, opts)?;
    assert_same(&framed, &raw, "sine48");
    Ok(())
}

#[test]
fn he_raw_au_matches_adts_and_latm() -> Result<()> {
    let opts = DecodeOptions::audio();
    let adts = collect_framed(HE48, &opts)?;
    let latm = collect_framed(HE48_LATM, &opts)?;
    assert_same(&adts, &latm, "he48 adts/latm");
    let aus = adts_payloads(HE48);
    let raw = collect_au(&m4a_asc(HE48_M4A), &aus, opts)?;
    assert_same(&adts, &raw, "he48 au");
    Ok(())
}

#[test]
fn ps_raw_au_matches_adts() -> Result<()> {
    let opts = DecodeOptions::audio();
    let framed = collect_framed(PS48, &opts)?;
    let aus = adts_payloads(PS48);
    let raw = collect_au(&m4a_asc(PS48_M4A), &aus, opts)?;
    assert_same(&framed, &raw, "ps48");
    Ok(())
}

#[test]
fn m4a_samples_plus_asc_match_adts() -> Result<()> {
    let track = crate::isomp4::parse_aac_track(HE48_M4A)?;
    let aus: Vec<&[u8]> = track
        .frames
        .iter()
        .map(|&(off, len)| {
            let o = off as usize;
            &HE48_M4A[o..o + len as usize]
        })
        .collect();
    let opts = DecodeOptions::audio();
    let framed = collect_framed(HE48, &opts)?;
    let raw = collect_au(&track.asc, &aus, opts)?;
    assert_same(&framed, &raw, "he48 m4a samples");
    Ok(())
}

#[test]
fn reset_clears_then_he_is_a_new_session() -> Result<()> {
    let mut dec = Decoder::from_asc(&write_lc(3, 1), DecodeOptions::speech())?;
    let au = adts_payloads(SINE48)[0];
    dec.decode_au(au, |_| Ok(()))?;
    dec.reset();
    assert!(!dec.is_failed());
    assert!(!dec.is_finished());
    let he = m4a_asc(HE48_M4A);
    dec.set_asc(&he)?;
    let hu = adts_payloads(HE48)[0];
    dec.decode_au(hu, |_| Ok(()))?;
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.sample_rate, 48_000);
    Ok(())
}

#[test]
fn truncated_unsupported_change_callback_limit() {
    let opts = DecodeOptions::speech();
    match Decoder::from_asc(&[], opts.clone()) {
        Err(AacError::Truncated { .. }) => {}
        Err(e) => panic!("empty ASC: {e:?}"),
        Ok(_) => panic!("empty ASC must error"),
    }
    match Decoder::from_asc(&[0x09, 0x88], opts.clone()) {
        Err(AacError::Unsupported(UnsupportedFeature::AudioObjectType(1))) => {}
        Err(e) => panic!("Main AOT: {e:?}"),
        Ok(_) => panic!("Main AOT must error"),
    }
    let mut dec = Decoder::from_asc(&write_lc(3, 1), opts.clone()).unwrap();
    let au = adts_payloads(SINE48)[0];
    dec.decode_au(au, |_| Ok(())).unwrap();
    let he = m4a_asc(HE48_M4A);
    let e = dec.set_asc(&he).unwrap_err();
    assert!(matches!(
        e,
        AacError::Unsupported(UnsupportedFeature::AscChange)
    ));
    dec.set_asc(&write_lc(3, 1)).unwrap();
    let e = dec
        .decode_au(au, |_| Err(AacError::decode("sink")))
        .unwrap_err();
    assert!(matches!(e, AacError::Decode(_)));
    assert!(dec.is_failed());
    assert!(matches!(
        dec.decode_au(au, |_| Ok(())).unwrap_err(),
        AacError::Lifecycle {
            state: LifecycleState::Failed
        }
    ));
    let tiny = DecodeOptions::speech().with_memory(crate::MemoryBudgets {
        max_declared_au_bytes: 4,
        ..crate::MemoryBudgets::default()
    });
    let mut d2 = Decoder::from_asc(&write_lc(3, 1), tiny).unwrap();
    let e = d2.decode_au(au, |_| Ok(())).unwrap_err();
    assert!(matches!(e, AacError::Limit { .. }));
    let mut d3 = Decoder::from_asc(&write_lc(3, 1), opts).unwrap();
    assert!(matches!(
        d3.decode_au(&[], |_| Ok(())).unwrap_err(),
        AacError::Truncated { .. }
    ));
}

#[test]
fn feed_and_decode_au_do_not_mix() {
    let mut au = Decoder::from_asc(&write_lc(3, 1), DecodeOptions::speech()).unwrap();
    assert!(matches!(
        au.feed(SINE48, |_| Ok(())).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::RawAccessUnit)
    ));
    let mut framed = Decoder::new(DecodeOptions::speech());
    framed.feed(SINE48, |_| Ok(())).unwrap();
    let payload = adts_payloads(SINE48)[0];
    assert!(matches!(
        framed.decode_au(payload, |_| Ok(())).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::RawAccessUnit)
    ));
}

#[test]
fn empty_au_session_is_not_aac_at_finish() {
    let mut dec = Decoder::from_asc(&write_lc(3, 1), DecodeOptions::speech()).unwrap();
    assert!(matches!(
        dec.finish(|_| Ok(())).unwrap_err(),
        AacError::NotAac
    ));
}

#[test]
fn one_shot_still_speech_mono() -> Result<()> {
    let pcm = decode_with(SINE48, &DecodeOptions::speech())?;
    assert_eq!(pcm.channels.len(), 1);
    Ok(())
}

fn au_blob(bytes: &[u8]) -> Vec<&[u8]> {
    let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let mut pos = 4usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        out.push(&bytes[pos..pos + len]);
        pos += len;
    }
    assert_eq!(pos, bytes.len(), "au blob trailing bytes");
    out
}

/// Walk one BBB raw AU (LC core, 24 kHz = fs_index 6, stereo CPE) and
/// count FIL elements whose count resolves to 0. Fixture-drift guard
/// for `bbb_fil.au`, in the spirit of
/// `engine::extension_payload_tests::ps48_adts_frames_signal_extension_id_ps`.
fn zero_count_fils(au: &[u8]) -> Result<u32> {
    use crate::engine::bits::BitReader;
    use crate::engine::ics::IcsInfo;
    use crate::engine::ics_body::parse_ics;
    use crate::engine::raw_data_block::IdSynEle;
    use crate::engine::skip::fill_count;
    use crate::engine::stereo::MsInfo;
    let mut br = BitReader::new(au);
    let mut zeros = 0u32;
    loop {
        if br.bits_remaining() < 3 {
            break;
        }
        match IdSynEle::from_bits(br.read(3)? as u8) {
            IdSynEle::End => break,
            IdSynEle::Sce | IdSynEle::Lfe => {
                let _tag = br.read(4)?;
                parse_ics(&mut br, 6, 2, None)?;
            }
            IdSynEle::Cpe => {
                let _tag = br.read(4)?;
                let common = br.read_bit()?;
                let ics = if common {
                    let i = IcsInfo::parse(&mut br, 6, true)?;
                    let _ms = MsInfo::parse(&mut br, &i)?;
                    Some(i)
                } else {
                    None
                };
                parse_ics(&mut br, 6, 2, ics.as_ref())?;
                parse_ics(&mut br, 6, 2, ics.as_ref())?;
            }
            IdSynEle::Fil => {
                let cnt = fill_count(&mut br)?;
                if cnt == 0 {
                    zeros += 1;
                } else {
                    br.skip(cnt * 8)?;
                }
            }
            _ => break,
        }
    }
    Ok(zeros)
}

#[test]
fn bbb_aus_carry_zero_count_fils() -> Result<()> {
    let aus = au_blob(BBB_AU);
    assert_eq!(aus.len(), 94, "bbb_fil.au AU count");
    let zeros: Vec<u32> = aus
        .iter()
        .map(|au| zero_count_fils(au).map_err(|e| AacError::decode(format!("{e:?}"))))
        .collect::<Result<_>>()?;
    assert_eq!(zeros[0], 0, "AU 0 carries the SBR header FIL only");
    let with_zero: Vec<usize> = zeros
        .iter()
        .enumerate()
        .filter_map(|(i, &z)| (z > 0).then_some(i))
        .collect();
    assert_eq!(
        with_zero,
        [1, 7, 12, 15, 16, 30, 37, 38, 40, 46, 61, 85, 87, 89],
        "zero-count FIL positions drifted"
    );
    assert!(zeros.iter().all(|&z| z <= 1), "unexpected FILs: {zeros:?}");
    Ok(())
}

#[test]
fn bbb_zero_count_fil_aus_decode_and_match_lavc() -> Result<()> {
    let asc = [0x2b, 0x11, 0x88, 0x00]; // AOT 5, 24 kHz core / 48 kHz out, stereo
    let aus = au_blob(BBB_AU);
    let (info, planes) = collect_au(&asc, &aus, DecodeOptions::unbounded())?;
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(info.channels, 2);
    assert_eq!(info.aac_frames, 94);
    assert_eq!(planes.len(), 2);
    assert_eq!(planes[0].len(), 94 * 2048);
    // lavf reference covers AUs 60..68 (8 * 2048 samples, stereo s16).
    let start = 60 * 2048;
    let n = 8 * 2048;
    let gold: Vec<i16> = BBB_S16
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(gold.len(), n * 2, "bbb_fil.s16 length");
    for ch in 0..2 {
        let mut max_lsb = 0u32;
        let mut se = 0.0f64;
        for i in 0..n {
            let gv = i32::from(gold[i * 2 + ch]);
            let ov = (f64::from(planes[ch][start + i]) * 32768.0)
                .round()
                .clamp(-32768.0, 32767.0) as i32;
            max_lsb = max_lsb.max((gv - ov).unsigned_abs());
            let d = f64::from(gv - ov);
            se += d * d;
        }
        let rms = (se / n as f64).sqrt();
        assert!(max_lsb <= 1, "bbb ch{ch} max lsb {max_lsb}");
        assert!(rms <= 1.0, "bbb ch{ch} rms lsb {rms}");
    }
    Ok(())
}

/// AAC-LD ASC: AOT 23, 48 kHz, mono, GA flags 0, `epConfig` 0 (TASK-128).
fn write_ld_asc() -> Vec<u8> {
    let mut w = crate::engine::bits::BitWriter::new();
    w.write(23, 5);
    w.write(3, 4);
    w.write(1, 4);
    w.write(0, 3); // GASpecificConfig flags
    w.write(0, 2); // epConfig
    w.finish()
}

fn first_ld_au() -> Vec<u8> {
    let loas = include_bytes!("goldens/ld48.loas");
    let v = (u32::from(loas[0]) << 16) | (u32::from(loas[1]) << 8) | u32::from(loas[2]);
    let ln = (v & 0x1FFF) as usize;
    let mut br = crate::engine::bits::BitReader::new(&loas[3..3 + ln]);
    assert!(!br.read_bit().unwrap());
    let cfg = crate::engine::latm::MuxCfg::parse(&mut br).unwrap();
    crate::engine::latm::read_payload(&mut br, &cfg).unwrap()
}

#[test]
fn ld_asc_decodes_one_au_and_garbage_fails_without_lc_fallback() -> Result<()> {
    let mut dec = Decoder::from_asc(&write_ld_asc(), DecodeOptions::unbounded())?;
    let mut n = 0usize;
    dec.decode_au(&first_ld_au(), |f| {
        n += f.samples;
        assert_eq!(f.sample_rate, 48_000);
        Ok(())
    })?;
    assert_eq!(n, 512);
    // A short garbage AU is a syntax error, not a successful LC frame
    // and not `Unsupported(23)` now that the signal core accepts AOT 23.
    match dec.decode_au(&[0x12, 0x34], |_| Ok(())) {
        Err(AacError::Malformed(_)) => {}
        other => panic!("garbage LD AU must be Malformed, got {other:?}"),
    }
    assert!(dec.is_failed());
    dec.reset();
    dec.set_asc(&write_lc(3, 1))?;
    let au = adts_payloads(SINE48)[0];
    dec.decode_au(au, |_| Ok(()))?;
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.sample_rate, 48_000);
    Ok(())
}
