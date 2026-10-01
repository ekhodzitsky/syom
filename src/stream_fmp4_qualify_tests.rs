//! TASK-126: fMP4 conformance qualification — the third-party GPAC-muxed
//! BBB vector end to end (pinned bit-exact against the raw-AU path, whose
//! lavc parity TASK-127 measured), tfdt-less accumulation PCM parity, and
//! typed errors for truncated init/moof/mdat and mid-fragment cuts
//! (lab/fmp4/REPORT.md §2 cells). Timeline/dts pins live in
//! `isomp4_frag_timeline_tests`; goldens are pinned in lab/fmp4/GOLDENS.md.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, DecodeOptions, DecodeOptions as Opts, Decoder, ProbeContainer, ProbeDuration,
    ProbeProfile, Result, StreamInfo, decode_streaming, decode_with, probe,
};

/// GPAC-muxed Akamai BBB DASH HE-AAC v1 vector (sha256 pinned in
/// lab/fmp4/PIN.md and lab/fmp4/GOLDENS.md): init + first media segment.
const BBB_INIT: &[u8] = include_bytes!("goldens/fmp4_bbb_init.m4a");
const BBB_SEG1: &[u8] = include_bytes!("goldens/fmp4_bbb_seg1.m4a");
/// TASK-127 golden: the segment's 94 raw AUs, extracted verbatim.
const BBB_AU: &[u8] = include_bytes!("goldens/bbb_fil.au");
/// tfdt stripped by lab/fmp4/strip_tfdt.py: the decode-time accumulator
/// must reproduce the tfdt-carrying dbmoof timeline (oracle-pinned).
const NOTFDT: &[u8] = include_bytes!("goldens/fmp4_lc_notfdt.mp4");
const LC_DBMOOF: &[u8] = include_bytes!("goldens/fmp4_lc_dbmoof.mp4");
const LC: &[u8] = include_bytes!("goldens/fmp4_lc.mp4");

fn collect(data: &[u8]) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let info = decode_streaming(data, &Opts::audio(), |f| {
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

/// `.au` blob: u32-LE AU count, then per AU u32-LE length + AU bytes.
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

fn bbb_concat() -> Vec<u8> {
    [BBB_INIT, BBB_SEG1].concat()
}

#[test]
fn bbb_third_party_vector_matches_the_raw_au_path() -> Result<()> {
    let (info, pcm) = collect(&bbb_concat())?;
    assert_eq!((info.sample_rate, info.channels), (48_000, 2));
    assert_eq!(info.aac_frames, 94);
    assert_eq!(info.samples, 94 * 2048);
    assert_eq!(info.priming, None, "BBB carries no elst: priming presented");
    // The fMP4 lane must resolve exactly the AUs TASK-127 extracted and
    // pinned against lavc (≤ 1 LSB): bit-exact against the raw-AU path.
    let asc = [0x2b, 0x11, 0x88, 0x00]; // AOT 5, 24 kHz core / 48 kHz out
    let mut dec = Decoder::from_asc(&asc, DecodeOptions::audio())?;
    let mut apcm: Vec<Vec<f32>> = Vec::new();
    for au in au_blob(BBB_AU) {
        dec.decode_au(au, |f| {
            if apcm.is_empty() {
                apcm = f.planar.iter().map(|p| p.to_vec()).collect();
            } else {
                for (dst, src) in apcm.iter_mut().zip(f.planar.iter()) {
                    dst.extend_from_slice(src);
                }
            }
            Ok(())
        })?;
    }
    dec.finish(|_| Ok(()))?;
    assert_eq!(pcm, apcm, "fMP4-resolved AUs != bbb_fil.au");
    Ok(())
}

#[test]
fn bbb_probe_reports_he_from_the_init() -> Result<()> {
    let p = probe(&bbb_concat())?;
    assert_eq!(p.container, ProbeContainer::M4a);
    assert_eq!(p.profile, ProbeProfile::HeAac);
    assert_eq!(p.meta.output_rate, 48_000);
    assert!(
        matches!(p.duration, ProbeDuration::Unknown),
        "no elst/mehd duration cap: {:?}",
        p.duration
    );
    Ok(())
}

#[test]
fn tfdt_less_accumulation_is_pcm_identical() -> Result<()> {
    let (info, pcm) = collect(NOTFDT)?;
    let (binfo, base) = collect(LC_DBMOOF)?;
    assert_eq!(info.aac_frames, binfo.aac_frames);
    assert_eq!(info.samples, binfo.samples);
    assert_eq!(info.priming, binfo.priming);
    assert_eq!(pcm, base, "accumulated dts changed the presentation");
    Ok(())
}

/// Byte offset of the `n`th `needle` fourcc occurrence.
fn find(data: &[u8], needle: &[u8; 4], n: usize) -> usize {
    data.windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == needle).then_some(i))
        .nth(n)
        .unwrap_or_else(|| panic!("{needle:?} occurrence {n}"))
}

/// Truncation outcome on the slice path: typed `Truncated`, never a
/// silent prefix decode and never the flat path's `NotAac` collapse once
/// an fMP4 init (`moov` with `mvex`) is visible.
fn expect_truncated(data: &[u8], what: &str) {
    match decode_with(data, &DecodeOptions::audio()) {
        Err(AacError::Truncated { .. }) => {}
        other => panic!("{what}: expected Truncated, got {other:?}"),
    }
}

#[test]
fn mid_fragment_cuts_are_typed_truncated() {
    let mvex = find(LC, b"mvex", 0);
    let moof0 = find(LC, b"moof", 0) - 4; // fourcc is 4 B into the box header
    let moof1 = find(LC, b"moof", 1) - 4;
    let mdat0 = find(LC, b"mdat", 0) - 4;
    // Inside the init moov, past the visible mvex: a truncated fMP4 init.
    expect_truncated(&LC[..mvex + 20], "cut inside init moov after mvex");
    // Inside the first moof and inside the first mdat (mid-AU).
    expect_truncated(&LC[..moof0 + 24], "cut inside first moof");
    expect_truncated(&LC[..mdat0 + 24], "cut inside first mdat payload");
    // Mid-stream: inside the second fragment.
    expect_truncated(&LC[..moof1 + 24], "cut inside second moof");
    // A box header dangling after the last complete box.
    expect_truncated(&[LC, &[0, 0, 0, 40]].concat(), "dangling header");
}

#[test]
fn truncated_init_before_mvex_is_indistinguishable_from_flat() {
    // A cut inside the init moov before `mvex` is visible carries no fMP4
    // signal at all: same `NotAac` class as a truncated flat M4A.
    let moov = find(LC, b"moov", 0) - 4;
    match decode_with(&LC[..moov + 16], &DecodeOptions::audio()) {
        Err(AacError::NotAac) | Err(AacError::Truncated { .. }) => {}
        other => panic!("early init cut: {other:?}"),
    }
}
