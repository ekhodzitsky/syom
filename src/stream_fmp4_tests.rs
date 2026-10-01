//! TASK-124: bounded fragmented MP4 (init `moov` + `moof`/`trun`) on the
//! slice and seekable paths. Goldens minted offline by
//! `lab/fmp4/gen_goldens.sh` (ffmpeg 7.0.2 mov/dash muxers — the same
//! structures as the lab/fmp4 fixtures, 2.5 s); lavf parity of the
//! resolved AUs is measured in lab/fmp4/REPORT.md (≤ 2 LSB). Here the
//! fMP4 decodes are pinned against the established flat-M4A/ADTS paths
//! (exact same-AU PCM) and against each other across addressing modes.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::Cursor;

use crate::{
    AacError, DecodeOptions, M4aSeek, MalformedKind, MemoryBudgets, ProbeDuration, ProbeTrim,
    Result, StreamInfo, UnsupportedFeature, decode_seek_streaming, decode_streaming, decode_with,
    probe,
};

/// tfhd carries `base-data-offset` (absolute addressing).
const LC: &[u8] = include_bytes!("goldens/fmp4_lc.mp4");
/// tfhd `default_base_is_moof`: trun `data_offset` is moof-relative.
const LC_DBMOOF: &[u8] = include_bytes!("goldens/fmp4_lc_dbmoof.mp4");
/// No tfhd offset at all: implicit base = end of the previous fragment.
const LC_OMIT: &[u8] = include_bytes!("goldens/fmp4_lc_omit.mp4");
/// trun version 1 (negative_cts_offsets muxer flag; no cto for audio).
const LC_V1: &[u8] = include_bytes!("goldens/fmp4_lc_v1.mp4");
/// CMAF shape: tfhd 0x2003a (sample_description_index + defaults + dbmoof).
const LC_CMAF: &[u8] = include_bytes!("goldens/fmp4_lc_cmaf.mp4");
/// Flat control: same source/settings, unfragmented (elst priming trim).
const LC_FLAT: &[u8] = include_bytes!("goldens/fmp4_lc_flat.m4a");
/// ffmpeg 7.0.2 `global_sidx` inconsistency: declared tfhd base resolves
/// inside the moof, not the mdat (lab/fmp4/REPORT.md §4.2) → Malformed.
const LC_SIDX: &[u8] = include_bytes!("goldens/fmp4_lc_sidx.mp4");
/// HE v1 / v2: ffmpeg remux of the syom goldens' AUs (explicit ASC).
const HE: &[u8] = include_bytes!("goldens/fmp4_he.mp4");
const HE2: &[u8] = include_bytes!("goldens/fmp4_he2.mp4");
const HE_FLAT: &[u8] = include_bytes!("goldens/he48em.m4a");
const HE2_FLAT: &[u8] = include_bytes!("goldens/he2_48em.m4a");
/// DASH: separate init (elst media_time 1024, open segment duration) + segs.
const DASH_INIT: &[u8] = include_bytes!("goldens/fmp4_dash_init.m4s");
const DASH_SEG1: &[u8] = include_bytes!("goldens/fmp4_dash_seg1.m4s");
const DASH_SEG2: &[u8] = include_bytes!("goldens/fmp4_dash_seg2.m4s");
/// CENC-encrypted init + first fragment (cut at the first mdat boundary).
const CENC: &[u8] = include_bytes!("goldens/fmp4_cenc.mp4");
/// Two AAC tracks, one moof with two trafs (cut at the first mdat boundary).
const TWO_TRACK: &[u8] = include_bytes!("goldens/fmp4_2track.mp4");

fn collect(data: &[u8], opts: &DecodeOptions) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let info = decode_streaming(data, opts, |f| {
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

fn dash_concat() -> Vec<u8> {
    [DASH_INIT, DASH_SEG1, DASH_SEG2].concat()
}

#[test]
fn lc_addressing_modes_decode_identically() -> Result<()> {
    let opts = DecodeOptions::audio();
    let (info, base) = collect(LC, &opts)?;
    assert_eq!((info.sample_rate, info.channels), (48_000, 2));
    assert_eq!(info.aac_frames, 119);
    assert_eq!(info.samples, 119 * 1024);
    assert_eq!(info.priming, None, "mov-muxer fMP4 presents priming");
    for (tag, data) in [
        ("dbmoof", LC_DBMOOF),
        ("omit", LC_OMIT),
        ("v1", LC_V1),
        ("cmaf", LC_CMAF),
    ] {
        let (i, pcm) = collect(data, &opts)?;
        assert_eq!(i.aac_frames, info.aac_frames, "{tag} frame count");
        assert_eq!(i.samples, info.samples, "{tag} samples");
        assert_eq!(pcm, base, "{tag} pcm");
    }
    Ok(())
}

#[test]
fn lc_fmp4_matches_flat_after_priming() -> Result<()> {
    let opts = DecodeOptions::audio();
    let (finfo, fpcm) = collect(LC_FLAT, &opts)?;
    let (info, pcm) = collect(LC, &opts)?;
    let priming = finfo.priming.expect("flat m4a elst priming") as usize;
    assert_eq!(priming, 1024);
    // mov-muxer fMP4 has no elst: priming + flat presentation + coded tail.
    assert_eq!(info.samples, 119 * 1024);
    assert!(info.samples as usize >= priming + finfo.samples as usize);
    let n = finfo.samples as usize;
    for ch in 0..2 {
        assert_eq!(
            &pcm[ch][priming..priming + n],
            &fpcm[ch][..],
            "ch{ch} pcm window"
        );
    }
    Ok(())
}

#[test]
fn he_fmp4_matches_flat_goldens() -> Result<()> {
    let opts = DecodeOptions::audio();
    for (tag, frag, flat, coded) in [
        ("he", HE, HE_FLAT, 32768u64),
        ("he2", HE2, HE2_FLAT, 65536u64),
    ] {
        let (finfo, fpcm) = collect(flat, &opts)?;
        let (info, pcm) = collect(frag, &opts)?;
        let priming = finfo.priming.expect("flat HE elst priming") as usize;
        assert_eq!(priming, 3018, "{tag} HE_PRIMING_OUT");
        // Same AUs as the flat golden, presented without the elst trim.
        assert_eq!(info.samples, coded, "{tag} coded output");
        assert!(info.samples as usize >= priming + finfo.samples as usize);
        let n = finfo.samples as usize;
        for ch in 0..fpcm.len() {
            assert_eq!(
                &pcm[ch][priming..priming + n],
                &fpcm[ch][..],
                "{tag} ch{ch} pcm window"
            );
        }
    }
    Ok(())
}

#[test]
fn dash_init_plus_segments_trims_priming() -> Result<()> {
    let opts = DecodeOptions::audio();
    let dash = dash_concat();
    let (info, pcm) = collect(&dash, &opts)?;
    assert_eq!((info.sample_rate, info.channels), (48_000, 2));
    assert_eq!(info.aac_frames, 94);
    assert_eq!(info.samples, 94 * 1024 - 1024);
    assert_eq!(info.priming, Some(1024));
    // Same encode as fmp4_lc (deterministic lavfi source + encoder): the
    // dash presentation is exactly the LC stream after the 1024 priming.
    let (_, base) = collect(LC, &opts)?;
    let n = info.samples as usize;
    for ch in 0..2 {
        assert_eq!(&pcm[ch][..], &base[ch][1024..1024 + n], "dash ch{ch} pcm");
    }
    Ok(())
}

#[test]
fn sidx_inconsistent_base_is_malformed() {
    match decode_with(LC_SIDX, &DecodeOptions::audio()) {
        Err(AacError::Malformed(_)) => {}
        Err(e) => panic!("sidx fixture: {e:?}"),
        Ok(_) => panic!("declared range outside mdat must not decode"),
    }
}

#[test]
fn encrypted_is_typed_unsupported() {
    match decode_with(CENC, &DecodeOptions::audio()) {
        Err(AacError::Unsupported(UnsupportedFeature::FragmentedMp4(_))) => {}
        Err(e) => panic!("cenc fixture: {e:?}"),
        Ok(_) => panic!("encrypted fMP4 must not decode"),
    }
}

#[test]
fn two_audio_tracks_is_typed_unsupported() {
    match decode_with(TWO_TRACK, &DecodeOptions::audio()) {
        Err(AacError::Unsupported(UnsupportedFeature::FragmentedMp4(_))) => {}
        Err(e) => panic!("2track fixture: {e:?}"),
        Ok(_) => panic!("multi-track fMP4 must not decode"),
    }
}

#[test]
fn seekable_reader_matches_slice() -> Result<()> {
    let opts = DecodeOptions::audio();
    for (tag, data) in [("lc", LC.to_vec()), ("dash", dash_concat())] {
        let (info, pcm) = collect(data.as_slice(), &opts)?;
        let mut spcm: Vec<Vec<f32>> = Vec::new();
        let sinfo = decode_seek_streaming(Cursor::new(data), &opts, |f| {
            if spcm.is_empty() {
                spcm = f.planar.iter().map(|p| p.to_vec()).collect();
            } else {
                for (dst, src) in spcm.iter_mut().zip(f.planar.iter()) {
                    dst.extend_from_slice(src);
                }
            }
            Ok(())
        })?;
        assert_eq!(sinfo.samples, info.samples, "{tag} seek samples");
        assert_eq!(sinfo.aac_frames, info.aac_frames, "{tag} seek frames");
        assert_eq!(spcm, pcm, "{tag} seek pcm");
    }
    Ok(())
}

#[test]
fn m4a_seek_presents_fmp4_sample_exactly() -> Result<()> {
    let opts = DecodeOptions::audio();
    let (info, _) = collect(LC, &opts)?;
    let mut src = M4aSeek::open(Cursor::new(LC), opts.clone())?;
    assert_eq!(src.presentation_len(), info.samples);
    let at = 50_000u64;
    assert_eq!(src.seek(at as i64)?, at);
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let got = src.decode(|f| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    })?;
    assert_eq!(got.samples, info.samples - at);
    // The flat fixture is the same AU sequence with a 1024-sample elst skip:
    // seeking it at `at - 1024` lands on the same target AU with the same
    // preroll restart, so both seek paths must be bit-identical. (Seeking
    // against linear decode has the codec's preroll-settle error, measured
    // at ≤ 2 LSB on the flat corpus in stream_m4a_seek_tests.)
    let mut flat = M4aSeek::open(Cursor::new(LC_FLAT), opts)?;
    flat.seek(at as i64 - 1024)?;
    let mut fplanes: Vec<Vec<f32>> = Vec::new();
    let fgot = flat.decode(|f| {
        if fplanes.is_empty() {
            fplanes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in fplanes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    })?;
    // The flat elst also caps the coded tail, so only the shared window
    // (flat's presentation rest) is comparable.
    let n = fgot.samples as usize;
    assert!(n < got.samples as usize);
    for ch in 0..2 {
        assert_eq!(
            &planes[ch][..n],
            &fplanes[ch][..],
            "fmp4 seek == flat seek at the same coded AU, ch{ch}"
        );
    }
    Ok(())
}

#[test]
fn probe_reports_fmp4_init() -> Result<()> {
    let p = probe(LC)?;
    assert_eq!(p.container, crate::ProbeContainer::M4a);
    assert_eq!(p.profile, crate::ProbeProfile::Lc);
    assert_eq!(p.meta.output_rate, 48_000);
    assert!(matches!(p.duration, ProbeDuration::Unknown));
    let dash = probe(&dash_concat())?;
    assert!(
        matches!(dash.trim, ProbeTrim::Exact { priming: 1024, .. }),
        "dash init elst priming: {:?}",
        dash.trim
    );
    assert!(matches!(dash.duration, ProbeDuration::Unknown));
    Ok(())
}

#[test]
fn fmp4_respects_the_index_budget() {
    let opts = DecodeOptions::audio().with_memory(MemoryBudgets {
        max_index_entries: 10,
        ..MemoryBudgets::default()
    });
    match decode_with(LC, &opts) {
        Err(AacError::Limit {
            kind: crate::BudgetKind::Metadata,
            ..
        }) => {}
        Err(e) => panic!("index budget: {e:?}"),
        Ok(_) => panic!("119 samples must exceed a 10-entry index budget"),
    }
}

#[test]
fn push_feed_still_rejects_flat_isobmff() {
    // TASK-125: fragmented init (`mvex`) decodes through push feed; a flat
    // moov keeps the random-access rejection.
    let mut dec = crate::Decoder::new(DecodeOptions::speech());
    match dec.feed(LC_FLAT, |_| Ok(())) {
        Err(AacError::Unsupported(UnsupportedFeature::M4aPush)) => {}
        other => panic!("push feed of flat M4A: {other:?}"),
    }
}

#[test]
fn malformed_kind_for_fragment_structure() {
    // mfhd sequence gap: flip the second fragment's sequence number.
    let mut data = LC.to_vec();
    let needle: &[u8] = b"mfhd";
    let hits: Vec<usize> = data
        .windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == needle).then_some(i))
        .collect();
    assert!(hits.len() >= 2, "fixture has several fragments");
    let seq_at = hits[1] + 8; // mfhd body: version/flags(4) + sequence(4)
    let seq = u32::from_be_bytes(data[seq_at..seq_at + 4].try_into().unwrap());
    data[seq_at..seq_at + 4].copy_from_slice(&(seq + 5).to_be_bytes());
    match decode_with(&data, &DecodeOptions::audio()) {
        Err(AacError::Malformed(MalformedKind::Syntax)) => {}
        Err(e) => panic!("mfhd gap: {e:?}"),
        Ok(_) => panic!("mfhd sequence gap must not decode"),
    }
}
