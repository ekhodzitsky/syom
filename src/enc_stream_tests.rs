//! Push `Encoder`: byte-identity with one-shot encode for any chunking,
//! tail-flush parity, tallies, error paths, callback abort.

use crate::{AacError, EncodeContainer, EncodeInfo, EncodeOptions, Encoder, Result, encode_with};

/// Deterministic stereo fixture: a 440 Hz sine in L, LCG noise in R.
fn fixture(rate: u32, n: usize) -> Vec<Vec<f32>> {
    let l: Vec<f32> = (0..n)
        .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin())
        .collect();
    let mut state = 0x2F6E_2B1Du32;
    let r: Vec<f32> = (0..n)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            0.3 * (((state >> 9) as f32 / (1u32 << 23) as f32) * 2.0 - 1.0)
        })
        .collect();
    vec![l, r]
}

/// Streaming-encode `pcm` fed in `chunk`-sample pieces; returns the
/// concatenated ADTS bytes and the final tallies.
fn stream_encode(pcm: &[Vec<f32>], rate: u32, chunk: usize) -> Result<(Vec<u8>, EncodeInfo)> {
    stream_encode_with(pcm, rate, chunk, &EncodeOptions::adts())
}

/// `stream_encode` under explicit options (lookahead parity tests).
fn stream_encode_with(
    pcm: &[Vec<f32>],
    rate: u32,
    chunk: usize,
    opts: &EncodeOptions,
) -> Result<(Vec<u8>, EncodeInfo)> {
    let mut enc = Encoder::new(rate, pcm.len(), opts)?;
    let mut out = Vec::new();
    let mut cb = |f: crate::EncodedFrame<'_>| {
        out.extend_from_slice(f.au);
        Ok(())
    };
    let n = pcm[0].len();
    let mut off = 0usize;
    while off < n {
        let end = (off + chunk).min(n);
        let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[off..end]).collect();
        let used = enc.feed(&planes, &mut cb)?;
        assert_eq!(used, end - off, "feed consumes the whole chunk");
        off = end;
    }
    let info = enc.finish(&mut cb)?;
    Ok((out, info))
}

#[test]
fn byte_identity_with_one_shot_any_chunking() -> Result<()> {
    let pcm = fixture(48_000, 10_000); // not a multiple of 1024: exercises the tail
    let want = encode_with(&pcm, 48_000, &EncodeOptions::adts())?;
    for chunk in [1, 7, 1024, 4097, 10_000] {
        let (got, info) = stream_encode(&pcm, 48_000, chunk)?;
        assert_eq!(got, want, "chunk {chunk}: streaming != one-shot bytes");
        assert_eq!(info.samples, 10_000, "chunk {chunk}: samples tally");
        assert_eq!(info.aac_frames, 11, "chunk {chunk}: content+drain");
        assert_eq!(info.bytes as usize, want.len(), "chunk {chunk}: byte tally");
    }
    Ok(())
}

#[test]
fn byte_identity_mono() -> Result<()> {
    let pcm = vec![fixture(44_100, 5_000).swap_remove(0)];
    let want = encode_with(&pcm, 44_100, &EncodeOptions::adts())?;
    for chunk in [3, 1024, 5_000] {
        let (got, _) = stream_encode(&pcm, 44_100, chunk)?;
        assert_eq!(got, want, "mono chunk {chunk}: streaming != one-shot");
    }
    Ok(())
}

#[test]
fn exact_multiple_still_emits_overlap_drain() -> Result<()> {
    let pcm = fixture(48_000, 3 * 1024);
    let (adts, info) = stream_encode(&pcm, 48_000, 999)?;
    assert_eq!(info.aac_frames, 4, "3 content + 1 drain");
    assert_eq!(info.remainder, 0);
    assert_eq!(info.priming, 1024);
    assert_eq!(info.coded_samples, 4 * 1024);
    assert_eq!(adts, encode_with(&pcm, 48_000, &EncodeOptions::adts())?);
    Ok(())
}

/// Fixture with a click early in a frame (the lookahead case) riding on
/// the standard sine/noise bed.
fn click_fixture(rate: u32, n: usize) -> Vec<Vec<f32>> {
    let mut pcm = fixture(rate, n);
    let mut lcg = 0x1234_5678u32;
    for k in 0..32 {
        lcg = lcg.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let u = (lcg >> 9) as f32 / (1u32 << 23) as f32 * 2.0 - 1.0;
        pcm[0][3 * 1024 + 100 + k] += 0.4 * u;
    }
    pcm
}

#[test]
fn lookahead_byte_identity_with_one_shot_any_chunking() -> Result<()> {
    let pcm = click_fixture(48_000, 10_000); // tail + early-in-frame click
    let opts = EncodeOptions::adts().with_lookahead(true);
    let want = encode_with(&pcm, 48_000, &opts)?;
    // Sanity: the option actually changes the bitstream.
    assert_ne!(
        want,
        encode_with(&pcm, 48_000, &EncodeOptions::adts())?,
        "lookahead should shift the window sequence"
    );
    for chunk in [1, 7, 1024, 4097, 10_000] {
        let (got, info) = stream_encode_with(&pcm, 48_000, chunk, &opts)?;
        assert_eq!(got, want, "chunk {chunk}: lookahead streaming != one-shot");
        assert_eq!(info.samples, 10_000, "chunk {chunk}: samples tally");
        assert_eq!(info.aac_frames, 11, "chunk {chunk}: content+drain");
        assert_eq!(info.bytes as usize, want.len(), "chunk {chunk}: byte tally");
    }
    Ok(())
}

#[test]
fn lookahead_trails_by_one_frame_and_flushes_at_finish() -> Result<()> {
    let pcm = fixture(48_000, 1024 + 100);
    let opts = EncodeOptions::adts().with_lookahead(true);
    let mut enc = Encoder::new(48_000, pcm.len(), &opts)?;
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    let mut seen = Vec::new();
    enc.feed(&planes, |f| {
        seen.push(f.samples);
        Ok(())
    })?;
    // The first completed frame is held for the lookahead decision.
    assert!(
        seen.is_empty(),
        "feed emits nothing before the second frame"
    );
    let info = enc.finish(|f| {
        seen.push(f.samples);
        Ok(())
    })?;
    assert_eq!(seen, [1024, 100, 0], "held full, tail, overlap drain");
    assert_eq!(info.samples, 1124);
    assert_eq!(info.aac_frames, 3);
    assert_eq!(info.remainder, 924);
    let full = fixture(48_000, 3 * 1024);
    let opts = EncodeOptions::adts().with_lookahead(true);
    let (got, info) = stream_encode_with(&full, 48_000, 1024, &opts)?;
    assert_eq!(info.aac_frames, 4);
    assert_eq!(got, encode_with(&full, 48_000, &opts)?);
    Ok(())
}

#[test]
fn tail_frame_reports_remainder_samples() -> Result<()> {
    let pcm = fixture(48_000, 1024 + 100);
    let opts = EncodeOptions::adts();
    let mut enc = Encoder::new(48_000, pcm.len(), &opts)?;
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    let mut seen = Vec::new();
    enc.feed(&planes, |f| {
        seen.push(f.samples);
        Ok(())
    })?;
    let info = enc.finish(|f| {
        seen.push(f.samples);
        Ok(())
    })?;
    assert_eq!(seen, [1024, 100, 0], "full frame, remainder, drain");
    assert_eq!(info.samples, 1124);
    assert_eq!(info.aac_frames, 3);
    Ok(())
}

#[test]
fn streamed_adts_decodes_like_one_shot() -> Result<()> {
    let pcm = fixture(48_000, 8_000);
    let (adts, _) = stream_encode(&pcm, 48_000, 313)?;
    let dec = crate::decode_with(&adts, &crate::DecodeOptions::unbounded())?;
    assert_eq!(dec.sample_rate, 48_000);
    assert_eq!(dec.channels.len(), 2);
    assert_eq!(dec.channels[0].len(), 9 * 1024);
    Ok(())
}

#[test]
fn constructor_error_paths() -> Result<()> {
    let good = EncodeOptions::adts();
    let cases = [
        Encoder::new(47_000, 2, &good).err(),
        Encoder::new(48_000, 0, &good).err(),
        Encoder::new(48_000, 3, &good).err(),
        Encoder::new(48_000, 2, &good.clone().with_bitrate_bps(0)).err(),
        Encoder::new(48_000, 1, &good.clone().with_bitrate_bps(1_000_000)).err(),
        Encoder::new(48_000, 2, &good.with_container(EncodeContainer::M4a)).err(),
    ];
    for e in cases {
        let e = e.ok_or(AacError::NotAac)?;
        assert!(
            matches!(e, AacError::Encode(_) | AacError::Unsupported(_)),
            "expected Encode/Unsupported, got {e:?}"
        );
        assert!(!e.to_string().is_empty(), "stable Display");
    }
    // The M4A rejection names the one-shot escape hatch, like the push
    // Decoder's ISOBMFF error names decode_streaming.
    let e = Encoder::new(48_000, 2, &EncodeOptions::m4a())
        .err()
        .ok_or(AacError::NotAac)?;
    assert!(
        matches!(
            e,
            AacError::Unsupported(crate::UnsupportedFeature::EncodeM4aStreaming)
        ),
        "{e}"
    );
    Ok(())
}

/// Higher-ranked no-op sink (a closure literal would pin one lifetime).
fn sink(_: crate::EncodedFrame<'_>) -> Result<()> {
    Ok(())
}

#[test]
fn feed_error_paths() -> Result<()> {
    let opts = EncodeOptions::adts();
    // Wrong plane count does not consume PCM: the encoder stays open.
    let mut enc = Encoder::new(48_000, 2, &opts)?;
    let a = [0.0f32; 8];
    let e = enc.feed(&[&a], sink).err().ok_or(AacError::NotAac)?;
    assert!(
        matches!(e, AacError::InvalidPcm(crate::PcmReject::ChannelCount)),
        "plane count: {e:?}"
    );
    assert!(!enc.is_failed());
    enc.feed(&[&a, &a], sink)?;
    // Unequal plane lengths.
    let mut enc = Encoder::new(48_000, 2, &opts)?;
    let b = [0.0f32; 9];
    let e = enc.feed(&[&a, &b], sink).err().ok_or(AacError::NotAac)?;
    assert!(
        matches!(e, AacError::InvalidPcm(crate::PcmReject::PlaneLength)),
        "unequal lengths: {e:?}"
    );
    assert!(!enc.is_failed());
    // Non-finite sample fails the stream until reset.
    let mut enc = Encoder::new(48_000, 1, &opts)?;
    let nan = [f32::NAN; 8];
    let e = enc.feed(&[&nan], sink).err().ok_or(AacError::NotAac)?;
    assert!(
        matches!(e, AacError::InvalidPcm(crate::PcmReject::NonFinite)),
        "NaN: {e:?}"
    );
    assert!(enc.is_failed());
    let good = [0.0f32; 2048];
    assert!(enc.feed(&[&good], sink).is_err(), "NaN is sticky");
    enc.reset()?;
    let mut out = 0usize;
    enc.feed(&[&good], |f| {
        out += f.au.len();
        Ok(())
    })?;
    let info = enc.finish(sink)?;
    assert_eq!(info.aac_frames, 3);
    assert!(out > 0);
    Ok(())
}

#[test]
fn finish_without_input_is_empty_error() -> Result<()> {
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts())?;
    let e = enc.finish(sink).err().ok_or(AacError::NotAac)?;
    assert!(
        matches!(e, AacError::InvalidPcm(crate::PcmReject::Empty)),
        "empty finish: {e:?}"
    );
    assert!(enc.is_failed());
    assert!(enc.finish(sink).is_err(), "empty finish is sticky");
    enc.reset()?;
    // Feeding empty slices is a no-op, not "input".
    let empty: [&[f32]; 1] = [&[]];
    assert_eq!(enc.feed(&empty, sink)?, 0);
    let e = enc.finish(sink).err().ok_or(AacError::NotAac)?;
    assert!(
        matches!(e, AacError::InvalidPcm(crate::PcmReject::Empty)),
        "empty feeds: {e:?}"
    );
    Ok(())
}

#[test]
fn callback_abort_propagates() -> Result<()> {
    let pcm = fixture(48_000, 4_096);
    let opts = EncodeOptions::adts();
    let mut enc = Encoder::new(48_000, 2, &opts)?;
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    let mut fired = 0u32;
    let e = enc
        .feed(&planes, |_| {
            fired += 1;
            Err(AacError::encode("stop"))
        })
        .err()
        .ok_or(AacError::NotAac)?;
    assert_eq!(e.to_string(), "stop", "callback error surfaces verbatim");
    assert_eq!(fired, 1, "aborted on the first frame");
    assert_eq!(enc.frames(), 1, "callback-seen frame is counted");
    assert_eq!(enc.samples(), 1024, "consumed the aborted frame only");
    assert!(enc.is_failed());
    assert!(enc.feed(&planes, sink).is_err());
    assert!(enc.finish(sink).is_err());
    // Fresh encoder with a pending tail: finish's callback fires and aborts.
    let mut enc = Encoder::new(48_000, 2, &opts)?;
    let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[..100]).collect();
    enc.feed(&planes, sink)?;
    let e = enc
        .finish(|_| Err(AacError::encode("stop at finish")))
        .err()
        .ok_or(AacError::NotAac)?;
    assert_eq!(e.to_string(), "stop at finish");
    assert!(enc.is_failed());
    assert_eq!(enc.frames(), 1);
    Ok(())
}

#[test]
fn info_fields_match_construction() -> Result<()> {
    let pcm = fixture(32_000, 2_048);
    let (adts, info) = stream_encode(&pcm, 32_000, 2_048)?;
    assert_eq!(info.sample_rate, 32_000);
    assert_eq!(info.channels, 2);
    assert_eq!(info.samples, 2_048);
    assert_eq!(info.aac_frames, 3);
    assert_eq!(info.priming, 1024);
    assert_eq!(info.remainder, 0);
    assert_eq!(info.coded_samples, 3 * 1024);
    assert_eq!(info.bytes as usize, adts.len());
    Ok(())
}

#[test]
fn finished_is_sticky_until_reset() -> Result<()> {
    let pcm = fixture(48_000, 2048);
    let opts = EncodeOptions::adts();
    let mut enc = Encoder::new(48_000, 2, &opts)?;
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, sink)?;
    let info = enc.finish(sink)?;
    assert!(enc.is_finished());
    assert!(!enc.is_failed());
    assert_eq!(enc.frames(), info.aac_frames);
    assert!(enc.finish(sink).is_err());
    assert!(enc.feed(&planes, sink).is_err());
    enc.reset()?;
    let mut out = Vec::new();
    enc.feed(&planes, |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    let info2 = enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    assert_eq!(out, encode_with(&pcm, 48_000, &opts)?);
    assert_eq!(info2.aac_frames, info.aac_frames);
    Ok(())
}

#[test]
fn reset_clears_held_lookahead_state() -> Result<()> {
    let pcm = fixture(48_000, 3 * 1024);
    let opts = EncodeOptions::adts().with_lookahead(true);
    let mut enc = Encoder::new(48_000, 2, &opts)?;
    let prefix: Vec<&[f32]> = pcm.iter().map(|p| &p[..1024]).collect();
    enc.feed(&prefix, sink)?;
    assert_eq!(enc.frames(), 0, "first frame is held");
    enc.reset()?;
    let mut out = Vec::new();
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    assert_eq!(out, encode_with(&pcm, 48_000, &opts)?);
    Ok(())
}

#[test]
fn reset_does_not_leak_rate_credit_into_next_session() -> Result<()> {
    let loud = [vec![0.9f32; 2048]];
    let quiet = [(0..2048)
        .map(|i| 0.05 * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / 48_000.0).sin())
        .collect::<Vec<f32>>()];
    let mut enc = Encoder::new(48_000, 1, &EncodeOptions::adts())?;
    let planes: Vec<&[f32]> = loud.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, sink)?;
    enc.finish(sink)?;
    enc.reset()?;
    let mut out = Vec::new();
    let planes: Vec<&[f32]> = quiet.iter().map(Vec::as_slice).collect();
    enc.feed(&planes, |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    assert_eq!(out, encode_with(&quiet, 48_000, &EncodeOptions::adts())?);
    Ok(())
}

#[test]
fn reset_same_pcm_is_byte_exact_twice() -> Result<()> {
    let pcm = fixture(48_000, 3 * 1024);
    let want = encode_with(&pcm, 48_000, &EncodeOptions::adts())?;
    let mut enc = Encoder::new(48_000, 2, &EncodeOptions::adts())?;
    for pass in 0..2 {
        enc.reset()?;
        let mut out = Vec::new();
        let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
        enc.feed(&planes, |f| {
            out.extend_from_slice(f.au);
            Ok(())
        })?;
        enc.finish(|f| {
            out.extend_from_slice(f.au);
            Ok(())
        })?;
        assert_eq!(out, want, "pass {pass}");
    }
    Ok(())
}

#[test]
fn two_encoders_reset_are_reentrant() -> Result<()> {
    let a_pcm = fixture(48_000, 2048);
    let b_pcm = fixture(44_100, 2048);
    let opts = EncodeOptions::adts();
    let mut a = Encoder::new(48_000, 2, &opts)?;
    let mut b = Encoder::new(44_100, 2, &opts)?;
    let pa: Vec<&[f32]> = a_pcm.iter().map(Vec::as_slice).collect();
    let pb: Vec<&[f32]> = b_pcm.iter().map(Vec::as_slice).collect();
    a.feed(&pa, sink)?;
    a.finish(sink)?;
    b.feed(&pb, sink)?;
    b.finish(sink)?;
    a.reset()?;
    b.reset()?;
    let mut oa = Vec::new();
    let mut ob = Vec::new();
    a.feed(&pa, |f| {
        oa.extend_from_slice(f.au);
        Ok(())
    })?;
    a.finish(|f| {
        oa.extend_from_slice(f.au);
        Ok(())
    })?;
    b.feed(&pb, |f| {
        ob.extend_from_slice(f.au);
        Ok(())
    })?;
    b.finish(|f| {
        ob.extend_from_slice(f.au);
        Ok(())
    })?;
    assert_eq!(oa, encode_with(&a_pcm, 48_000, &opts)?);
    assert_eq!(ob, encode_with(&b_pcm, 44_100, &opts)?);
    Ok(())
}
