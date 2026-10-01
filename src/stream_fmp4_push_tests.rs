//! TASK-125: push (`Decoder::feed`) delivery of bounded fragmented MP4 —
//! arbitrary chunking of init + fragments, per-AU delivery inside `mdat`,
//! bounded resident buffering, typed truncation/lifecycle outcomes. The
//! reference for every parity assertion is the one-shot slice decode of
//! the same bytes (TASK-124, stream_fmp4_tests.rs).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, DecodeOptions, Decoder, LifecycleState, MalformedKind, MemoryBudgets, Result,
    StreamInfo, UnsupportedFeature,
};

const LC: &[u8] = include_bytes!("goldens/fmp4_lc.mp4");
const LC_DBMOOF: &[u8] = include_bytes!("goldens/fmp4_lc_dbmoof.mp4");
const LC_OMIT: &[u8] = include_bytes!("goldens/fmp4_lc_omit.mp4");
const LC_V1: &[u8] = include_bytes!("goldens/fmp4_lc_v1.mp4");
const LC_CMAF: &[u8] = include_bytes!("goldens/fmp4_lc_cmaf.mp4");
const LC_FLAT: &[u8] = include_bytes!("goldens/fmp4_lc_flat.m4a");
const HE: &[u8] = include_bytes!("goldens/fmp4_he.mp4");
const HE2: &[u8] = include_bytes!("goldens/fmp4_he2.mp4");
const DASH_INIT: &[u8] = include_bytes!("goldens/fmp4_dash_init.m4s");
const DASH_SEG1: &[u8] = include_bytes!("goldens/fmp4_dash_seg1.m4s");
const DASH_SEG2: &[u8] = include_bytes!("goldens/fmp4_dash_seg2.m4s");
const CENC: &[u8] = include_bytes!("goldens/fmp4_cenc.mp4");
const TWO_TRACK: &[u8] = include_bytes!("goldens/fmp4_2track.mp4");

fn dash_concat() -> Vec<u8> {
    [DASH_INIT, DASH_SEG1, DASH_SEG2].concat()
}

fn collect(data: &[u8], opts: &DecodeOptions) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let info = crate::decode_streaming(data, opts, |f| {
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

/// Push `data` through `Decoder::feed` in `chunk`-sized pieces, then finish.
fn push(data: &[u8], opts: &DecodeOptions, chunk: usize) -> Result<(StreamInfo, Vec<Vec<f32>>)> {
    let mut dec = Decoder::new(opts.clone());
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut on_frame = |f: crate::Frame<'_>| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    };
    for c in data.chunks(chunk.max(1)) {
        dec.feed(c, &mut on_frame)?;
    }
    let info = dec.finish(&mut on_frame)?;
    Ok((info, planes))
}

fn assert_parity(data: &[u8], opts: &DecodeOptions, tag: &str) -> Result<()> {
    let (info, pcm) = collect(data, opts)?;
    for chunk in [data.len(), 1, 7, 1024] {
        let (pinfo, ppcm) = push(data, opts, chunk)?;
        assert_eq!(
            pinfo.sample_rate, info.sample_rate,
            "{tag} chunk {chunk} rate"
        );
        assert_eq!(
            pinfo.channels, info.channels,
            "{tag} chunk {chunk} channels"
        );
        assert_eq!(
            pinfo.aac_frames, info.aac_frames,
            "{tag} chunk {chunk} frames"
        );
        assert_eq!(pinfo.samples, info.samples, "{tag} chunk {chunk} samples");
        assert_eq!(pinfo.priming, info.priming, "{tag} chunk {chunk} priming");
        assert_eq!(
            pinfo.remainder, info.remainder,
            "{tag} chunk {chunk} remainder"
        );
        assert_eq!(ppcm, pcm, "{tag} chunk {chunk} pcm");
    }
    Ok(())
}

#[test]
fn push_lc_matches_one_shot_under_arbitrary_chunking() -> Result<()> {
    let opts = DecodeOptions::audio();
    for (tag, data) in [
        ("lc", LC),
        ("dbmoof", LC_DBMOOF),
        ("omit", LC_OMIT),
        ("v1", LC_V1),
        ("cmaf", LC_CMAF),
    ] {
        assert_parity(data, &opts, tag)?;
    }
    Ok(())
}

#[test]
fn push_he_matches_one_shot() -> Result<()> {
    let opts = DecodeOptions::audio();
    assert_parity(HE, &opts, "he")?;
    assert_parity(HE2, &opts, "he2")?;
    Ok(())
}

#[test]
fn push_speech_mono_matches_one_shot() -> Result<()> {
    let opts = DecodeOptions::speech();
    assert_parity(LC, &opts, "lc-speech")?;
    Ok(())
}

#[test]
fn push_dash_segments_across_feeds_trim_priming() -> Result<()> {
    let opts = DecodeOptions::audio();
    let (info, pcm) = collect(&dash_concat(), &opts)?;
    assert_eq!(info.priming, Some(1024));
    // One feed per DASH segment: init, then media segments one at a time.
    let mut dec = Decoder::new(opts);
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let mut on_frame = |f: crate::Frame<'_>| {
        if planes.is_empty() {
            planes = f.planar.iter().map(|p| p.to_vec()).collect();
        } else {
            for (dst, src) in planes.iter_mut().zip(f.planar.iter()) {
                dst.extend_from_slice(src);
            }
        }
        Ok(())
    };
    dec.feed(DASH_INIT, &mut on_frame)?;
    assert_eq!(dec.frames(), 0, "init alone delivers no frames");
    dec.feed(DASH_SEG1, &mut on_frame)?;
    dec.feed(DASH_SEG2, &mut on_frame)?;
    let pinfo = dec.finish(&mut on_frame)?;
    assert_eq!(pinfo.aac_frames, info.aac_frames);
    assert_eq!(pinfo.samples, info.samples);
    assert_eq!(pinfo.priming, info.priming);
    assert_eq!(planes, pcm);
    Ok(())
}

#[test]
fn push_resident_buffer_is_bounded_per_fragment() -> Result<()> {
    // fmp4_lc.mp4 is 32 090 bytes of init + 3 fragments + mfra; per-AU
    // delivery and per-fragment tables must keep the resident buffer far
    // below the file size (no lifetime growth).
    let mut dec = Decoder::new(DecodeOptions::audio());
    for c in LC.chunks(509) {
        dec.feed(c, |_| Ok(()))?;
    }
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.aac_frames, 119);
    let cap = dec.test_buf_cap();
    assert!(
        cap <= 16 * 1024,
        "resident buffer {cap} B must stay at one box/AU scale"
    );
    // Reuse after reset: same stream again, no capacity growth.
    dec.reset();
    for c in LC.chunks(509) {
        dec.feed(c, |_| Ok(()))?;
    }
    dec.finish(|_| Ok(()))?;
    assert_eq!(dec.test_buf_cap(), cap, "reset keeps a steady-state buffer");
    Ok(())
}

#[test]
fn push_long_synthetic_stream_stays_bounded() -> Result<()> {
    // Eight replays of the dbmoof fragment section (moof-relative
    // addressing, so copies resolve against their own mdat; mfhd sequence
    // numbers are rewritten to stay continuous): 952 AUs through one push
    // session. The resident buffer must stay at one box/AU scale.
    let init_end = 728; // dbmoof: ftyp 28 + moov 700 (walk_mp4 inventory)
    let frag_end = 31_957; // three moof/mdat pairs, mfra trailer excluded
    let section = &LC_DBMOOF[init_end..frag_end];
    let mfhd_hits: Vec<usize> = section
        .windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == b"mfhd").then_some(i))
        .collect();
    assert_eq!(mfhd_hits.len(), 3, "three fragments per round");
    let rounds = 8u64;
    let mut dec = Decoder::new(DecodeOptions::audio());
    let mut frames = 0u64;
    dec.feed(&LC_DBMOOF[..init_end], |_| Ok(()))?;
    assert_eq!(dec.frames(), 0, "init alone delivers no frames");
    let mut seq = 0u32;
    for _ in 0..rounds {
        let mut s = section.to_vec();
        for &h in &mfhd_hits {
            seq += 1;
            s[h + 8..h + 12].copy_from_slice(&seq.to_be_bytes());
        }
        for c in s.chunks(997) {
            dec.feed(c, |f| {
                frames += 1;
                let _ = f;
                Ok(())
            })?;
        }
    }
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.aac_frames, 119 * rounds);
    assert_eq!(frames, 119 * rounds);
    let cap = dec.test_buf_cap();
    assert!(
        cap <= 16 * 1024,
        "resident buffer {cap} B after {rounds} rounds: no lifetime growth"
    );
    Ok(())
}

#[test]
fn push_cut_at_a_fragment_boundary_finishes_cleanly() -> Result<()> {
    // fmp4_lc inventory: the second mdat's payload ends exactly at 25 442 —
    // a cut on the fragment boundary is a complete (shorter) stream, not a
    // truncation.
    let mut dec = Decoder::new(DecodeOptions::audio());
    dec.feed(&LC[..25_442], |_| Ok(()))?;
    let info = dec.finish(|_| Ok(()))?;
    assert!(info.aac_frames > 0, "two full fragments decode");
    assert!(info.aac_frames < 119, "the third fragment is absent");
    Ok(())
}

#[test]
fn push_mid_mdat_cut_is_typed_truncated() {
    // 20 000 bytes land inside the second fragment's mdat: samples of the
    // current fragment are still owed at finish.
    let mut dec = Decoder::new(DecodeOptions::audio());
    dec.feed(&LC[..20_000], |_| Ok(())).expect("prefix feeds");
    match dec.finish(|_| Ok(())) {
        Err(AacError::Truncated { .. }) => {}
        Err(e) => panic!("mid-mdat cut: {e:?}"),
        Ok(info) => panic!("mid-mdat cut decoded cleanly: {info:?}"),
    }
    assert!(dec.is_failed());
}

#[test]
fn push_mid_box_cut_is_typed_truncated() {
    // 740 bytes end 8 bytes into the first moof (moof starts at 732).
    let mut dec = Decoder::new(DecodeOptions::audio());
    dec.feed(&LC[..740], |_| Ok(())).expect("prefix feeds");
    match dec.finish(|_| Ok(())) {
        Err(AacError::Truncated { .. }) => {}
        Err(e) => panic!("mid-moof cut: {e:?}"),
        Ok(info) => panic!("mid-moof cut decoded cleanly: {info:?}"),
    }
}

#[test]
fn push_mfhd_gap_is_malformed_and_fails_the_instance() -> Result<()> {
    let mut data = LC.to_vec();
    let hits: Vec<usize> = data
        .windows(4)
        .enumerate()
        .filter_map(|(i, w)| (w == b"mfhd").then_some(i))
        .collect();
    assert!(hits.len() >= 2, "fixture has several fragments");
    let seq_at = hits[1] + 8;
    let seq = u32::from_be_bytes(data[seq_at..seq_at + 4].try_into().unwrap());
    data[seq_at..seq_at + 4].copy_from_slice(&(seq + 5).to_be_bytes());
    let mut dec = Decoder::new(DecodeOptions::audio());
    for c in data.chunks(511) {
        match dec.feed(c, |_| Ok(())) {
            Err(AacError::Malformed(MalformedKind::Syntax)) => break,
            Err(e) => panic!("mfhd gap: {e:?}"),
            Ok(_) => {}
        }
    }
    assert!(dec.is_failed(), "mfhd gap must surface by end of feed");
    match dec.feed(b"x", |_| Ok(())) {
        Err(AacError::Lifecycle {
            state: LifecycleState::Failed,
        }) => {}
        other => panic!("feed after failure: {other:?}"),
    }
    // reset restores the existing push rules: the pristine stream decodes.
    dec.reset();
    let mut frames = 0u64;
    for c in LC.chunks(511) {
        dec.feed(c, |f| {
            frames += 1;
            let _ = f;
            Ok(())
        })?;
    }
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.aac_frames, 119);
    Ok(())
}

#[test]
fn push_oversized_box_is_a_typed_limit() {
    // Init moov declares 17 MiB (over the 16 MiB metadata budget).
    let mut data = LC.to_vec();
    let moov_at = 32; // ftyp is 32 B; walk_mp4 inventory pins moov at 32
    assert_eq!(&data[moov_at + 4..moov_at + 8], b"moov");
    data[moov_at..moov_at + 4].copy_from_slice(&(17u32 << 20).to_be_bytes());
    let mut dec = Decoder::new(DecodeOptions::audio());
    match dec.feed(&data, |_| Ok(())) {
        Err(AacError::Limit {
            kind: crate::BudgetKind::Metadata,
            ..
        }) => {}
        Err(e) => panic!("oversized moov: {e:?}"),
        Ok(_) => panic!("17 MiB moov must hit the metadata budget"),
    }
    assert!(dec.is_failed());
}

#[test]
fn push_box_size_smaller_than_header_is_malformed() {
    let mut data = LC.to_vec();
    data[32..36].copy_from_slice(&4u32.to_be_bytes()); // moov size 4 < 8
    let mut dec = Decoder::new(DecodeOptions::audio());
    match dec.feed(&data, |_| Ok(())) {
        Err(AacError::Malformed(MalformedKind::Syntax)) => {}
        Err(e) => panic!("size<hdr moov: {e:?}"),
        Ok(_) => panic!("box size below its header must not decode"),
    }
}

#[test]
fn push_mdat_declared_past_end_is_typed_truncated() {
    // First mdat (at 1028, size 12049) declares 0x7FFF_FFFE: the resolved
    // samples can never arrive.
    let mut data = LC.to_vec();
    assert_eq!(&data[1032..1036], b"mdat");
    data[1028..1032].copy_from_slice(&0x7FFF_FFFEu32.to_be_bytes());
    let mut dec = Decoder::new(DecodeOptions::audio());
    dec.feed(&data, |_| Ok(())).expect("declared mdat fits");
    match dec.finish(|_| Ok(())) {
        Err(AacError::Truncated { .. }) => {}
        Err(e) => panic!("mdat overflow: {e:?}"),
        Ok(info) => panic!("endless mdat decoded cleanly: {info:?}"),
    }
}

#[test]
fn push_fmp4_finished_instance_rejects_feed_until_reset() -> Result<()> {
    let mut dec = Decoder::new(DecodeOptions::audio());
    dec.feed(LC, |_| Ok(()))?;
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.aac_frames, 119);
    assert!(dec.is_finished());
    match dec.feed(b"x", |_| Ok(())) {
        Err(AacError::Lifecycle {
            state: LifecycleState::Finished,
        }) => {}
        other => panic!("feed after finish: {other:?}"),
    }
    dec.reset();
    dec.feed(LC, |_| Ok(()))?;
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.aac_frames, 119);
    Ok(())
}

#[test]
fn push_flat_m4a_still_needs_random_access() {
    let mut dec = Decoder::new(DecodeOptions::speech());
    match dec.feed(LC_FLAT, |_| Ok(())) {
        Err(AacError::Unsupported(UnsupportedFeature::M4aPush)) => {}
        Err(e) => panic!("flat m4a push: {e:?}"),
        Ok(_) => panic!("flat m4a (no mvex) must stay M4aPush"),
    }
    assert!(dec.is_failed());
}

#[test]
fn push_out_of_envelope_fmp4_is_typed_unsupported() {
    for (tag, data) in [("cenc", CENC), ("2track", TWO_TRACK)] {
        let mut dec = Decoder::new(DecodeOptions::audio());
        let mut outcome = None;
        for c in data.chunks(1024) {
            if let Err(e) = dec.feed(c, |_| Ok(())) {
                outcome = Some(e);
                break;
            }
        }
        match outcome {
            Some(AacError::Unsupported(UnsupportedFeature::FragmentedMp4(_))) => {}
            Some(e) => panic!("{tag}: {e:?}"),
            None => panic!("{tag}: out-of-envelope fMP4 must not feed cleanly"),
        }
        assert!(dec.is_failed(), "{tag} lifecycle");
    }
}

#[test]
fn push_fmp4_respects_the_moof_metadata_budget() {
    let opts = DecodeOptions::audio().with_memory(MemoryBudgets {
        max_metadata_bytes: 128, // the fixture's moofs are ~300 B
        ..MemoryBudgets::default()
    });
    let mut dec = Decoder::new(opts);
    match dec.feed(LC, |_| Ok(())) {
        Err(AacError::Limit {
            kind: crate::BudgetKind::Metadata,
            ..
        }) => {}
        Err(e) => panic!("moof budget: {e:?}"),
        Ok(_) => panic!("296 B moof must exceed a 128 B metadata budget"),
    }
}

#[test]
fn push_one_feed_uses_the_metadata_cap_after_sniff() -> Result<()> {
    // The resident cap widens to the metadata budget only once the bytes
    // sniff as fMP4. A single feed that starts unsniffed must still accept
    // a moov above the ADTS buffered cap (here 256) and match one-shot.
    let opts = DecodeOptions::audio().with_memory(MemoryBudgets {
        max_buffered_input_bytes: 256,
        ..MemoryBudgets::default()
    });
    let (info, pcm) = collect(LC, &opts)?;
    let (pinfo, ppcm) = push(LC, &opts, LC.len())?;
    assert_eq!(pinfo, info);
    assert_eq!(ppcm, pcm);
    Ok(())
}

#[test]
fn push_box_size_overflow_is_malformed() {
    // First mdat (1028) rewritten as a 64-bit size of u64::MAX: the absolute
    // end does not fit, which is a typed error, not a wrap or a panic.
    let mut data = LC.to_vec();
    assert_eq!(&data[1032..1036], b"mdat");
    data[1028..1032].copy_from_slice(&1u32.to_be_bytes());
    data[1036..1044].copy_from_slice(&u64::MAX.to_be_bytes());
    let mut dec = Decoder::new(DecodeOptions::audio());
    match dec.feed(&data, |_| Ok(())) {
        Err(AacError::Malformed(MalformedKind::Syntax)) => {}
        Err(e) => panic!("mdat box size overflow: {e:?}"),
        Ok(_) => panic!("box end must not wrap"),
    }
    assert!(dec.is_failed());
    match dec.feed(b"x", |_| Ok(())) {
        Err(AacError::Lifecycle {
            state: LifecycleState::Failed,
        }) => {}
        other => panic!("feed after box-size overflow: {other:?}"),
    }

    // Same overflow on the top-level walk, before any moof is resolved.
    let mut top = LC[..32].to_vec();
    top.extend_from_slice(&1u32.to_be_bytes());
    top.extend_from_slice(b"free");
    top.extend_from_slice(&u64::MAX.to_be_bytes());
    let mut dec = Decoder::new(DecodeOptions::audio());
    match dec.feed(&top, |_| Ok(())) {
        Err(AacError::Malformed(MalformedKind::Syntax)) => {}
        Err(e) => panic!("top-level box size overflow: {e:?}"),
        Ok(_) => panic!("top-level box end must not wrap"),
    }
    assert!(dec.is_failed());
}

#[test]
fn push_moof_table_is_freed_per_fragment() -> Result<()> {
    // dbmoof moof is ~300 B; the largest trun is 47 samples (47 * 16 = 752).
    // 1024 fits one fragment's table and that moof, not two accumulated.
    let opts = DecodeOptions::audio().with_memory(MemoryBudgets {
        max_metadata_bytes: 1024,
        ..MemoryBudgets::default()
    });
    let mut dec = Decoder::new(opts);
    for c in LC_DBMOOF.chunks(1000) {
        dec.feed(c, |_| Ok(()))?;
    }
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.aac_frames, 119);
    Ok(())
}
