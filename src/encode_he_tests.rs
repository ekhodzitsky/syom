//! TASK-90: public HE v1 encode — options and typed errors, ADTS / M4A /
//! raw signalling, push-vs-one-shot parity, timeline, lavc goldens.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::encode_tests::{
    assert_decode_matches_lavc_n, assert_decode_matches_lavc_pub, lavc_fixture,
};
use crate::engine::asc::write_he;
use crate::{
    AacError, DecodeOptions, EncodeContainer, EncodeOptions, Encoder, ProbeProfile,
    UnsupportedFeature, decode_with, encode_with, mux_raw_he_m4a, probe, wrap_adts_au,
};

const PRIMING: u64 = 3018;

fn he(bps: u32) -> EncodeOptions {
    EncodeOptions::adts().with_bitrate_bps(bps).with_he(true)
}

fn push_all(
    opts: &EncodeOptions,
    pcm: &[Vec<f32>],
    chunk: usize,
) -> (Vec<Vec<u8>>, Vec<usize>, crate::EncodeInfo, Vec<u8>) {
    let mut enc = Encoder::new(48_000, pcm.len(), opts).unwrap();
    let asc = enc.asc().to_vec();
    let mut aus = Vec::new();
    let mut samples = Vec::new();
    let n = pcm[0].len();
    let mut at = 0;
    while at < n {
        let end = (at + chunk).min(n);
        let planes: Vec<&[f32]> = pcm.iter().map(|p| &p[at..end]).collect();
        enc.feed(&planes, |f| {
            aus.push(f.au.to_vec());
            samples.push(f.samples);
            Ok(())
        })
        .unwrap();
        at = end;
    }
    let info = enc
        .finish(|f| {
            aus.push(f.au.to_vec());
            samples.push(f.samples);
            Ok(())
        })
        .unwrap();
    (aus, samples, info, asc)
}

#[test]
fn he_is_off_by_default_and_rates_channels_bitrates_are_typed() {
    assert!(!EncodeOptions::default().he);
    let pcm = vec![vec![0.1f32; 4096]];
    let e = encode_with(&pcm, 96_000, &he(48_000)).unwrap_err();
    assert!(
        matches!(
            e,
            AacError::Unsupported(UnsupportedFeature::EncodeHeRate(96_000))
        ),
        "{e}"
    );
    let e = encode_with(&pcm, 8_000, &he(48_000)).unwrap_err();
    assert!(
        matches!(
            e,
            AacError::Unsupported(UnsupportedFeature::EncodeHeRate(8_000))
        ),
        "{e}"
    );
    assert!(matches!(
        Encoder::new(8_000, 1, &he(48_000)).err(),
        Some(AacError::Unsupported(_))
    ));
    // Whole-stream budget must fit the core: 6144 · 24000 / 1024 = 144 kbps mono.
    assert!(encode_with(&pcm, 48_000, &he(144_000)).is_ok());
    let e = encode_with(&pcm, 48_000, &he(150_000)).unwrap_err();
    assert!(matches!(e, AacError::Encode(_)), "{e}");
    let three = vec![vec![0.1f32; 4096]; 3];
    assert!(matches!(
        encode_with(&three, 48_000, &he(48_000)).unwrap_err(),
        AacError::Encode(_)
    ));
    let raw = he(48_000).with_container(EncodeContainer::Raw);
    assert!(matches!(
        encode_with(&pcm, 48_000, &raw).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::EncodeRawOneShot)
    ));
    assert!(matches!(
        Encoder::new(48_000, 1, &he(48_000).with_container(EncodeContainer::M4a)).err(),
        Some(AacError::Unsupported(
            UnsupportedFeature::EncodeM4aStreaming
        ))
    ));
    let lc = encode_with(&pcm, 48_000, &EncodeOptions::adts()).unwrap();
    assert_eq!(
        probe(&lc).unwrap().meta.core_rate,
        48_000,
        "LC default untouched"
    );
}

#[test]
fn asc_is_the_iso_two_rate_vector() {
    // ISO 14496-3 Amd 2 explicit SBR, 24 kHz core / 48 kHz output mono:
    // the independently authored conformance vector `2b098800`.
    assert_eq!(write_he(6, 3, 1), vec![0x2b, 0x09, 0x88, 0x00]);
    let (_, _, _, asc) = push_all(&he(24_000), &[vec![0.0f32; 2048]], 2048);
    assert_eq!(asc, vec![0x2b, 0x09, 0x88, 0x00]);
    let (_, _, _, asc) = push_all(&he(48_000), &[vec![0.0f32; 2048], vec![0.0f32; 2048]], 2048);
    assert_eq!(
        asc,
        vec![0x2b, 0x11, 0x88, 0x00],
        "stereo: channelConfiguration 2"
    );
    let parsed = crate::engine::asc::AudioSpecificConfig::parse(&asc)
        .unwrap()
        .0;
    assert_eq!(
        (
            parsed.aot,
            parsed.sample_rate,
            parsed.output_sample_rate,
            parsed.channel_configuration
        ),
        (2, 24_000, 48_000, 2)
    );
    assert!(parsed.sbr_present);
}

#[test]
fn push_adts_raw_and_one_shot_agree() {
    let pcm = lavc_fixture();
    let one = encode_with(&pcm, 48_000, &he(48_000)).unwrap();
    let (aus, samples, info, _) = push_all(&he(48_000), &pcm, 1000);
    assert_eq!(
        aus.concat().as_slice(),
        crate::gapless::strip_id3(&one),
        "push ADTS == one-shot ADTS"
    );
    assert_eq!(
        samples.iter().sum::<usize>(),
        pcm[0].len(),
        "every source sample attributed once"
    );
    assert!(samples.iter().all(|&s| s <= 2048));
    assert_eq!(info.priming, PRIMING);
    assert_eq!(info.coded_samples, info.aac_frames * 2048);
    assert_eq!(info.remainder, info.coded_samples - PRIMING - info.samples);
    assert_eq!(info.bytes as usize, crate::gapless::strip_id3(&one).len());
    let (raw, _, _, _) = push_all(&he(48_000).with_container(EncodeContainer::Raw), &pcm, 5000);
    let rewrapped: Vec<u8> = raw
        .iter()
        .flat_map(|au| wrap_adts_au(au, 24_000, 2).unwrap())
        .collect();
    assert_eq!(
        rewrapped.as_slice(),
        crate::gapless::strip_id3(&one),
        "raw AUs wrapped at the core rate == ADTS"
    );
    let m4a = encode_with(
        &pcm,
        48_000,
        &he(48_000).with_container(EncodeContainer::M4a),
    )
    .unwrap();
    let refs: Vec<&[u8]> = raw.iter().map(Vec::as_slice).collect();
    assert_eq!(
        mux_raw_he_m4a(&refs, 48_000, 2, pcm[0].len() as u64).unwrap(),
        m4a,
        "raw AUs muxed == one-shot M4A"
    );
    assert!(matches!(
        mux_raw_he_m4a(&refs, 96_000, 2, 10).unwrap_err(),
        AacError::Unsupported(_)
    ));
    let (la, _, li, _) = push_all(&he(48_000).with_lookahead(true), &pcm, 777);
    let one_la = encode_with(&pcm, 48_000, &he(48_000).with_lookahead(true)).unwrap();
    assert_eq!(la.concat(), crate::gapless::strip_id3(&one_la));
    assert_eq!(li.aac_frames, info.aac_frames);
}

#[test]
fn he_tail_shorter_than_the_sbr_delay_still_presents() {
    // 3018 priming + 3127 source = 6145 decoded samples. Three AUs are
    // 6144, so M4A used to reject the file and ADTS dropped the tail.
    let n = 3127usize;
    let pcm = vec![vec![0.0f32; n]];
    for look in [false, true] {
        let opts = he(32_000).with_lookahead(look);
        let adts = encode_with(&pcm, 48_000, &opts).unwrap();
        let da = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(
            da.channels[0].len(),
            n,
            "lookahead {look}: tagged ADTS presents the tail"
        );
        let bare = crate::gapless::strip_id3(&adts);
        let padded = decode_with(bare, &DecodeOptions::unbounded()).unwrap();
        assert!(
            padded.channels[0].len() as u64 >= PRIMING + n as u64,
            "lookahead {look}: untagged ADTS dropped the tail ({})",
            padded.channels[0].len()
        );
        let m4a = encode_with(&pcm, 48_000, &opts.with_container(EncodeContainer::M4a)).unwrap();
        let dm = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(dm.channels[0].len(), n, "lookahead {look}");
        assert_eq!(dm.priming, Some(PRIMING));
    }
}

#[test]
fn m4a_signals_two_rates_and_presents_exactly_n_samples() {
    let pcm = lavc_fixture();
    let n = pcm[0].len();
    let adts = encode_with(&pcm, 48_000, &he(48_000)).unwrap();
    let m4a = encode_with(
        &pcm,
        48_000,
        &he(48_000).with_container(EncodeContainer::M4a),
    )
    .unwrap();
    let p = probe(&m4a).unwrap();
    assert_eq!(p.profile, ProbeProfile::HeAac);
    assert_eq!((p.meta.core_rate, p.meta.output_rate), (24_000, 48_000));
    assert_eq!(
        probe(&adts).unwrap().profile,
        ProbeProfile::Lc,
        "ADTS header stays LC (implicit)"
    );
    let track = crate::isomp4::parse_aac_track(&m4a).unwrap();
    assert_eq!(track.edit_start, PRIMING);
    assert_eq!(track.presentation_samples(), Some(n as u64));
    let da = decode_with(&adts, &DecodeOptions::unbounded()).unwrap();
    let dm = decode_with(&m4a, &DecodeOptions::unbounded()).unwrap();
    assert_eq!(
        (dm.sample_rate, dm.core_rate, dm.channels.len()),
        (48_000, 24_000, 2)
    );
    assert_eq!(
        dm.channels[0].len(),
        n,
        "M4A presentation is N at the output rate"
    );
    assert_eq!(dm.priming, Some(PRIMING));
    assert_eq!(da.channels[0].len(), n);
    assert_eq!(da.priming, Some(PRIMING));
    for ch in 0..2 {
        let (a, m) = (&da.channels[ch], &dm.channels[ch]);
        let max = a
            .iter()
            .zip(m.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(0.0f32, f32::max);
        assert_eq!(max, 0.0, "ch{ch}: M4A != tagged ADTS");
    }
}

/// Offline mint: `MINT_GOLDENS=1 cargo test --lib mint_lavc_he_m4a_golden`, then
/// `ffmpeg -y -i src/goldens/he48em.m4a -f s16le src/goldens/he48em.lavc.s16`.
#[test]
fn mint_lavc_he_m4a_golden() {
    if std::env::var("MINT_GOLDENS").is_err() {
        return;
    }
    let m4a = encode_with(
        &lavc_fixture(),
        48_000,
        &he(48_000).with_container(EncodeContainer::M4a),
    )
    .unwrap();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/goldens");
    std::fs::write(dir.join("he48em.m4a"), m4a).unwrap();
}

#[test]
fn lavc_decodes_our_he_adts_and_m4a() {
    let pcm = lavc_fixture();
    let adts = encode_with(&pcm, 48_000, &he(48_000)).unwrap();
    assert_eq!(
        crate::gapless::strip_id3(&adts),
        &include_bytes!("goldens/he48e.adts")[..],
        "public path == committed HE golden"
    );
    assert_decode_matches_lavc_pub(&adts, include_bytes!("goldens/he48e.lavc.s16"));
    let m4a = encode_with(
        &pcm,
        48_000,
        &he(48_000).with_container(EncodeContainer::M4a),
    )
    .unwrap();
    let golden = include_bytes!("goldens/he48em.m4a");
    assert_eq!(
        m4a.as_slice(),
        &golden[..],
        "M4A drifted; re-mint with MINT_GOLDENS=1"
    );
    // ffmpeg honours the edit list (skips priming); we present N samples.
    assert_decode_matches_lavc_n(
        &m4a,
        include_bytes!("goldens/he48em.lavc.s16"),
        pcm[0].len(),
    );
}

#[test]
fn presets_are_explicit_settings_not_claims() {
    let hq = EncodeOptions::high_quality();
    assert_eq!(
        (hq.bitrate_bps, hq.he, hq.lookahead, hq.container),
        (192_000, false, false, EncodeContainer::Adts)
    );
    let lr = EncodeOptions::low_rate();
    assert_eq!(
        (lr.bitrate_bps, lr.he, lr.lookahead, lr.container),
        (48_000, true, false, EncodeContainer::Adts)
    );
    let d = EncodeOptions::default();
    assert_eq!((d.bitrate_bps, d.he, d.lookahead), (128_000, false, false));
    let pcm = lavc_fixture();
    let a = encode_with(&pcm, 48_000, &hq).unwrap();
    let b = encode_with(&pcm, 48_000, &lr).unwrap();
    let c = encode_with(&pcm, 48_000, &d).unwrap();
    assert!(
        a.len() > c.len() && c.len() > b.len(),
        "{} {} {}",
        a.len(),
        c.len(),
        b.len()
    );
    assert_eq!(
        decode_with(&b, &DecodeOptions::audio())
            .unwrap()
            .sample_rate,
        48_000
    );
    // Presets compose with the other builders.
    let m4a = encode_with(
        &pcm,
        48_000,
        &EncodeOptions::low_rate().with_container(EncodeContainer::M4a),
    )
    .unwrap();
    assert_eq!(probe(&m4a).unwrap().profile, ProbeProfile::HeAac);
}
