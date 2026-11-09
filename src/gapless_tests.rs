//! File-level `iTunSMPB`: ADTS tag round-trip, split feeds, and an M4A
//! with the tag and no `elst`.

use crate::gapless::test_support::{append_tag, find_edts, m4a_tag_no_elst};
use crate::gapless::{self, Delay};
use crate::{
    AacError, DecodeOptions, Decoder, EncodeOptions, Encoder, Frame, ProbeContainer, ProbeDuration,
    ProbeTrim, Result, decode, decode_with, encode_with, probe, wrap_adts_au,
};

fn impulse(n: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    v[0] = 0.9;
    v
}

fn peak(x: &[f32]) -> usize {
    x.iter()
        .enumerate()
        .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn push_adts(pcm: &[f32], opts: &EncodeOptions) -> Result<Vec<u8>> {
    let mut enc = Encoder::new(48_000, 1, opts)?;
    let mut out = Vec::new();
    enc.feed(&[pcm], |f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    enc.finish(|f| {
        out.extend_from_slice(f.au);
        Ok(())
    })?;
    Ok(out)
}

#[test]
fn lc_1124_adts_trims_to_the_source_and_keeps_the_impulse() -> Result<()> {
    let pcm = impulse(1124);
    let opts = EncodeOptions::adts();
    let adts = encode_with(&[&pcm], 48_000, &opts)?;
    assert!(adts.starts_with(b"ID3"));
    let expected = b"iTunSMPB\0 00000000 00000400 0000039C 0000000000000464";
    assert!(adts.windows(expected.len()).any(|w| w == expected));
    let dec = decode(&adts)?;
    assert_eq!(dec.channels[0].len(), 1124);
    assert_eq!(dec.priming, Some(1024));
    assert_eq!(dec.remainder, Some(924));
    let meta = probe(&adts)?;
    assert!(matches!(
        meta.duration,
        ProbeDuration::Exact { samples: 1124 }
    ));
    assert!(matches!(
        meta.trim,
        ProbeTrim::Exact {
            priming: 1024,
            remainder: 924
        }
    ));
    assert_eq!(peak(&dec.channels[0]), 0);
    let split = decode_with(&adts, &DecodeOptions::unbounded())?;
    assert_eq!(split.channels[0], dec.channels[0]);
    assert_eq!(split.priming, Some(1024));
    assert_eq!(split.remainder, Some(924));
    let raw = push_adts(&pcm, &opts)?;
    assert_eq!(gapless::strip_id3(&adts), raw.as_slice());
    assert_eq!(raw[0], 0xff);
    let padded = decode(&raw)?;
    assert_eq!(padded.channels[0].len(), 3 * 1024);
    assert_eq!(padded.priming, None);
    assert_eq!(padded.remainder, None);
    assert_eq!(peak(&padded.channels[0]), 1024);
    Ok(())
}

#[test]
fn he_v1_adts_round_trip_uses_3018_output_samples() -> Result<()> {
    let n = 2048usize;
    let pcm = impulse(n);
    let opts = EncodeOptions::adts().with_he(true).with_bitrate_bps(32_000);
    let adts = encode_with(&[&pcm], 48_000, &opts)?;
    let dec = decode(&adts)?;
    assert_eq!(dec.channels[0].len(), n);
    assert_eq!(dec.priming, Some(3018));
    assert_eq!(dec.remainder, Some(1078));
    let mut sink = Vec::new();
    crate::encode_write(&mut sink, &[&pcm], 48_000, &opts)?;
    assert_eq!(sink, adts);
    Ok(())
}

#[test]
fn tag_split_across_feeds_matches_one_shot() -> Result<()> {
    let pcm = impulse(1124);
    let adts = encode_with(&[&pcm], 48_000, &EncodeOptions::adts())?;
    let one = decode(&adts)?;
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut pcm_out = Vec::new();
    dec.feed(&adts[..9], |f: Frame<'_>| {
        pcm_out.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert!(pcm_out.is_empty(), "a split tag is not a frame");
    dec.feed(&adts[9..], |f: Frame<'_>| {
        pcm_out.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    let info = dec.finish(|f: Frame<'_>| {
        pcm_out.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert_eq!(info.priming, Some(1024));
    assert_eq!(info.remainder, Some(924));
    assert_eq!(pcm_out, one.channels[0]);
    Ok(())
}

#[test]
fn huge_syncsafe_size_is_a_limit_not_an_allocation() -> Result<()> {
    let hdr = [b'I', b'D', b'3', 4, 0, 0, 0x7f, 0x7f, 0x7f, 0x7f];
    let Err(err) = decode(&hdr) else {
        return Err(AacError::format("huge tag decoded"));
    };
    assert!(matches!(err, AacError::Limit { .. }), "{err}");
    let Err(err) = probe(&hdr) else {
        return Err(AacError::format("huge tag probed"));
    };
    assert!(matches!(err, AacError::Limit { .. }), "{err}");
    Ok(())
}

#[test]
fn probe_keeps_a_bare_id3_and_a_non_adts_payload_as_not_aac() -> Result<()> {
    let Err(bare) = probe(b"ID3XXXXX") else {
        return Err(AacError::format("bare ID3 probed"));
    };
    assert!(matches!(bare, AacError::NotAac));
    let Err(trunc) = decode(b"ID3XXXXX") else {
        return Err(AacError::format("truncated tag decoded"));
    };
    assert!(matches!(trunc, AacError::Truncated { .. }));
    let mut junk = gapless::tag_bytes(1124, false);
    junk.extend_from_slice(b"HELLO!!!!");
    let Err(not) = probe(&junk) else {
        return Err(AacError::format("non-adts payload probed"));
    };
    assert!(matches!(not, AacError::NotAac));
    let adts = encode_with(&[&impulse(1124)], 48_000, &EncodeOptions::adts())?;
    assert_eq!(probe(&adts)?.container, ProbeContainer::Adts);
    let stale = gapless::prefix_delay(gapless::strip_id3(&adts).to_vec(), 1024, 0, 100);
    let dec = decode(&stale)?;
    assert_eq!(dec.channels[0].len(), 3 * 1024);
    assert_eq!(dec.priming, None);
    let stale_meta = probe(&stale)?;
    assert!(matches!(stale_meta.duration, ProbeDuration::Unknown));
    assert!(matches!(stale_meta.trim, ProbeTrim::Unknown));
    Ok(())
}

#[test]
fn push_feed_of_tagged_lc_emits_the_source_before_finish() -> Result<()> {
    let pcm = impulse(2048);
    let adts = encode_with(&[&pcm], 48_000, &EncodeOptions::adts())?;
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut during = Vec::new();
    dec.feed(&adts, |f: Frame<'_>| {
        during.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert_eq!(during.len(), 2048);
    assert_eq!(peak(&during), 0);
    let mut tail = Vec::new();
    let info = dec.finish(|f: Frame<'_>| {
        tail.extend_from_slice(f.planar[0]);
        Ok(())
    })?;
    assert!(tail.is_empty());
    assert_eq!(info.samples, 2048);
    assert_eq!(info.priming, Some(1024));
    assert_eq!(info.remainder, Some(0));
    Ok(())
}

#[test]
fn huge_priming_does_not_stall_a_push_feed() -> Result<()> {
    let pcm = impulse(2048);
    let raw = push_adts(&pcm, &EncodeOptions::adts())?;
    let tagged = gapless::prefix_delay(raw, 100_000, 0, 2048);
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut during = 0usize;
    dec.feed(&tagged, |f: Frame<'_>| {
        during += f.samples;
        Ok(())
    })?;
    assert_eq!(during, 3 * 1024);
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!((info.priming, info.remainder), (None, None));
    assert_eq!(info.samples, (3 * 1024) as u64);
    let one = decode(&tagged)?;
    assert_eq!(one.channels[0].len(), 3 * 1024);
    assert_eq!(one.priming, None);
    assert!(matches!(probe(&tagged)?.trim, ProbeTrim::Unknown));
    Ok(())
}

#[test]
fn push_feed_trims_a_modest_stale_tag_and_reports_none() -> Result<()> {
    let pcm = impulse(2048);
    let raw = push_adts(&pcm, &EncodeOptions::adts())?;
    let tagged = gapless::prefix_delay(raw, 1024, 0, 100);
    let one = decode(&tagged)?;
    assert_eq!(one.channels[0].len(), 3 * 1024);
    assert_eq!(one.priming, None);
    let mut dec = Decoder::new(DecodeOptions::speech());
    let mut during = 0usize;
    dec.feed(&tagged, |f: Frame<'_>| {
        during += f.samples;
        Ok(())
    })?;
    assert_eq!(during, 100);
    let info = dec.finish(|_| Ok(()))?;
    assert_eq!(info.samples, 100);
    assert_eq!((info.priming, info.remainder), (None, None));
    Ok(())
}

#[test]
fn au_bytes_match_wrap_adts_au() -> Result<()> {
    let pcm = impulse(1124);
    let tagged = encode_with(&[&pcm], 48_000, &EncodeOptions::adts())?;
    let raw = gapless::strip_id3(&tagged);
    let mut wrapped = Vec::new();
    let mut pos = 0usize;
    while pos + 7 <= raw.len() {
        let len = (((raw[pos + 3] as usize) & 3) << 11)
            | ((raw[pos + 4] as usize) << 3)
            | ((raw[pos + 5] as usize) >> 5);
        assert!(len >= 7 && pos + len <= raw.len(), "frame {pos}");
        let payload = &raw[pos + 7..pos + len];
        wrapped.extend_from_slice(&wrap_adts_au(payload, 48_000, 1)?);
        pos += len;
    }
    assert_eq!(pos, raw.len());
    assert_eq!(wrapped.as_slice(), raw);
    Ok(())
}

#[test]
fn m4a_without_elst_trims_from_itunsmpb() -> Result<()> {
    let pcm = impulse(1124);
    let delay = gapless::delay_lc(1124);
    let file = m4a_tag_no_elst(&pcm, &delay)?;
    assert!(!file.windows(4).any(|w| w == b"edts" || w == b"elst"));
    assert!(file.windows(8).any(|w| w == b"iTunSMPB"));
    let dec = decode(&file)?;
    assert_eq!(dec.channels[0].len(), 1124);
    assert_eq!(dec.priming, Some(1024));
    assert_eq!(dec.remainder, Some(924));
    assert_eq!(peak(&dec.channels[0]), 0);
    let mut streamed = Vec::new();
    let info = crate::decode_seek_streaming(
        std::io::Cursor::new(&file),
        &DecodeOptions::unbounded(),
        |f| {
            streamed.extend_from_slice(f.planar[0]);
            Ok(())
        },
    )?;
    assert_eq!(streamed, dec.channels[0]);
    assert_eq!(info.priming, dec.priming);
    assert_eq!(info.remainder, dec.remainder);
    let mut bare = encode_with(&[&pcm], 48_000, &EncodeOptions::m4a())?;
    let (at, len, ancestors) = find_edts(&bare).ok_or_else(|| AacError::format("no edts"))?;
    for anc in &ancestors {
        let sz = u32::from_be_bytes(
            bare[*anc..*anc + 4]
                .try_into()
                .map_err(|_| AacError::format("box"))?,
        ) as usize;
        bare[*anc..*anc + 4].copy_from_slice(&((sz - len) as u32).to_be_bytes());
    }
    bare.drain(at..at + len);
    let open = decode(&bare)?;
    assert_eq!(open.channels[0].len(), 3 * 1024);
    assert_eq!(open.priming, None);
    Ok(())
}

#[test]
fn apple_itunsmpb_example_has_the_documented_counts() -> Result<()> {
    let mut file = b"\0\0\0\x08moov".to_vec();
    append_tag(
        &mut file,
        b" 00000000 00000840 000001A0 0000000000056220 00000000",
    )?;
    assert_eq!(
        gapless::itunsmpb_in(&file),
        Some(Delay {
            priming: 2112,
            remainder: 416,
            source: 352800
        })
    );
    Ok(())
}

#[test]
fn m4a_probe_uses_only_matching_itunsmpb_without_elst() -> Result<()> {
    let pcm = impulse(1124);
    let delay = gapless::delay_lc(1124);
    let file = m4a_tag_no_elst(&pcm, &delay)?;
    let meta = probe(&file)?;
    assert_eq!(meta.duration, ProbeDuration::Exact { samples: 1124 });
    assert_eq!(
        meta.trim,
        ProbeTrim::Exact {
            priming: 1024,
            remainder: 924
        }
    );
    for source in [1125, u64::MAX] {
        let stale = Delay { source, ..delay };
        let file = m4a_tag_no_elst(&pcm, &stale)?;
        assert_eq!(probe(&file)?.trim, ProbeTrim::Unknown);
        assert_eq!(decode(&file)?.priming, None);
    }
    let mut edited = encode_with(&[&pcm], 48_000, &EncodeOptions::m4a())?;
    append_tag(&mut edited, b" 00000000 00000000 0000079C 0000000000000464")?;
    assert_eq!(probe(&edited)?.trim, meta.trim);
    assert_eq!(decode(&edited)?.priming, Some(1024));
    Ok(())
}
