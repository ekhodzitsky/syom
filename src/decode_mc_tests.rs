//! Multichannel (3.0 / 4.0 / 5.0 / 5.1) AAC-LC lavc goldens + speech mono rule.
//!
//! Fixtures: 0.4 s, 48 kHz, one sine per channel — FL 440, FR 880, FC 330,
//! LFE 100, BL/BC 550, BR 660 Hz — WAV planes in layout order, encoded with
//! ffmpeg's native AAC-LC encoder (aac_at/afconvert drop 4.0's back center).
//! lavc decodes each config to its default layout, so the golden plane order
//! is FL FR FC / FL FR FC BC / FL FR FC BL BR / FL FR FC LFE BL BR; syom
//! emits the same order (see `engine::channel_map`).
//!
//! Generated offline from a scratch dir (oracle; tests never spawn ffmpeg):
//!
//!   python3 - <<'EOF'   # mc51_src.wav (mc30/40/50: truncate the freq list)
//!   import numpy as np, wave
//!   sr = 48000; t = np.arange(int(sr*0.4))/sr
//!   fr = [440, 880, 330, 100, 550, 660]          # FL FR FC LFE BL BR
//!   d = np.stack([0.16*32767*np.sin(2*np.pi*f*t) for f in fr], 1).astype('<i2')
//!   w = wave.open('mc51_src.wav','wb'); w.setnchannels(len(fr))
//!   w.setsampwidth(2); w.setframerate(sr); w.writeframes(d.tobytes()); w.close()
//!   EOF
//!   ffmpeg -guess_layout_max 0 -i mc51_src.wav -af aformat=channel_layouts=5.1 \
//!       -c:a aac -b:a 256000 mc51.m4a
//!   ffmpeg -i mc51.m4a -c:a copy -f adts mc51.adts
//!   ffmpeg -i mc51.adts -f s16le -acodec pcm_s16le mc51.s16      # untrimmed
//!   ffmpeg -i mc51.m4a  -f s16le -acodec pcm_s16le mc51_m4a.s16  # elst-trimmed
//!
//! mc30/mc40/mc50 likewise with layouts 3.0/4.0/5.0 (m4a golden minted for
//! mc51 only, to cover the ISOBMFF + ASC path).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::decode_tests::assert_native_matches_lavc_with;
use super::{AacError, DecodeOptions};

#[test]
fn mc30_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc30.adts"),
        include_bytes!("goldens/mc30.s16"),
        48_000,
        "mc30-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn mc40_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc40.adts"),
        include_bytes!("goldens/mc40.s16"),
        48_000,
        "mc40-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn mc50_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc50.adts"),
        include_bytes!("goldens/mc50.s16"),
        48_000,
        "mc50-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn mc51_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc51.adts"),
        include_bytes!("goldens/mc51.s16"),
        48_000,
        "mc51-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn mc51_m4a_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc51.m4a"),
        include_bytes!("goldens/mc51_m4a.s16"),
        48_000,
        "mc51-m4a",
        &DecodeOptions::unbounded(),
    )
}

/// Speech caps on 5.1 must not blow up: the one mono rule is the arithmetic
/// mean of the non-LFE planes (canonical 5.1 order: LFE is plane 3).
#[test]
fn mc51_speech_mono_is_mean_of_non_lfe_planes() -> Result<(), AacError> {
    let split = crate::decode_with(
        include_bytes!("goldens/mc51.adts"),
        &DecodeOptions::unbounded(),
    )?;
    assert_eq!(split.channels.len(), 6, "5.1 split plane count");
    for (input, label) in [
        (&include_bytes!("goldens/mc51.adts")[..], "adts"),
        (&include_bytes!("goldens/mc51.m4a")[..], "m4a"),
    ] {
        let mono = crate::decode_with(input, &DecodeOptions::speech())?;
        assert_eq!(mono.channels.len(), 1, "{label} speech not mono");
        let m = &mono.channels[0];
        assert!(
            m.iter().any(|s| s.abs() > 1e-3),
            "{label} speech mono silent"
        );
        if label == "adts" {
            assert_eq!(m.len(), split.channels[0].len(), "{label} mono length");
            let non_lfe = [0usize, 1, 2, 4, 5];
            for (i, &mv) in m.iter().enumerate() {
                let mean = non_lfe.iter().map(|&c| split.channels[c][i]).sum::<f32>() / 5.0;
                assert!(
                    (mv - mean).abs() <= 1e-6,
                    "{label} sample {i}: {mv} vs non-LFE mean {mean}"
                );
            }
        }
    }
    Ok(())
}

// ---- 7.1 (TASK-62) -------------------------------------------------------
//
// `mc71`: channel_configuration 7 from ffmpeg (`channel_layouts=7.1`, 0.3 s,
// 256 kbps, same recipe as mc51): elements SCE C, CPE FL/FR, CPE SL/SR,
// CPE BL/BR, LFE; tones FL 440, FR 880, FC 330, LFE 100, BL 550, BR 660,
// SL 770, SR 220. lavc and syom present FL FR FC LFE BL BR SL SR.
// `mc71p`: the same source as ffmpeg's `7.1(wide)` PCE stream (cfg 0):
// front CPE0 + SCE0, side SCE1 (the 100 Hz plane is an SCE, not an LFE
// element), back CPE1 + CPE2 — an "alternative 7.1" that decodes in PCE
// declaration order with no LFE (nothing is guessed from the plane count).

const MC71_TONES: [u32; 8] = [440, 880, 330, 100, 550, 660, 770, 220];

/// Strongest of the fixture tones over the last 4096 samples (Goertzel).
fn dominant_tone(plane: &[f32]) -> u32 {
    let tail = &plane[plane.len().saturating_sub(4096)..];
    let mag = |f: u32| {
        let w = 2.0 * std::f64::consts::PI * f64::from(f) / 48_000.0;
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &v) in tail.iter().enumerate() {
            let ph = w * i as f64;
            re += f64::from(v) * ph.cos();
            im += f64::from(v) * ph.sin();
        }
        re.hypot(im)
    };
    MC71_TONES
        .iter()
        .copied()
        .max_by(|&a, &b| mag(a).total_cmp(&mag(b)))
        .unwrap()
}

#[test]
fn mc71_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc71.adts"),
        include_bytes!("goldens/mc71.s16"),
        48_000,
        "mc71-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn mc71_pce_adts_native_matches_lavc_golden() -> Result<(), AacError> {
    assert_native_matches_lavc_with(
        include_bytes!("goldens/mc71p.adts"),
        include_bytes!("goldens/mc71p.s16"),
        48_000,
        "mc71p-adts",
        &DecodeOptions::unbounded(),
    )
}

#[test]
fn mc71_planes_carry_the_standard_identities_and_speech_mono_skips_lfe() -> Result<(), AacError> {
    use crate::{Channel, Layout, mpeg_channels};
    let adts = &include_bytes!("goldens/mc71.adts")[..];
    let m4a = &include_bytes!("goldens/mc71.m4a")[..];
    let split = crate::decode_with(adts, &DecodeOptions::unbounded())?;
    assert_eq!(split.layout, Layout::Mpeg(7));
    assert_eq!(split.channels.len(), 8);
    let tones: Vec<u32> = split.channels.iter().map(|p| dominant_tone(p)).collect();
    assert_eq!(tones, MC71_TONES, "FL FR FC LFE BL BR SL SR by tone");
    for input in [adts, m4a] {
        let probe = crate::probe(input)?;
        assert_eq!(probe.meta.layout, Layout::Mpeg(7));
        assert_eq!(probe.meta.labels().collect::<Vec<_>>(), mpeg_channels(7));
        assert_eq!(probe.meta.lfe_index(), Some(3));
        let mut labels = Vec::new();
        crate::decode_streaming(input, &DecodeOptions::unbounded(), |f| {
            assert_eq!(f.meta.layout, Layout::Mpeg(7));
            assert_eq!(f.planar.len(), 8);
            labels = f.meta.labels().collect();
            Ok(())
        })?;
        assert_eq!(labels, mpeg_channels(7));
        assert_eq!(labels[6], Channel::SideLeft);
        let mono = crate::decode_with(input, &DecodeOptions::speech())?;
        assert_eq!(mono.channels.len(), 1);
        if input.len() == adts.len() {
            let m = &mono.channels[0];
            assert_eq!(m.len(), split.channels[0].len());
            let non_lfe = [0usize, 1, 2, 4, 5, 6, 7];
            for (i, &mv) in m.iter().enumerate() {
                let mean = non_lfe.iter().map(|&c| split.channels[c][i]).sum::<f32>() / 7.0;
                assert!((mv - mean).abs() <= 1e-6, "sample {i}: {mv} vs {mean}");
            }
        }
    }
    Ok(())
}

#[test]
fn mc71_pce_stream_is_declaration_order_without_lfe_guessing() -> Result<(), AacError> {
    use crate::{Channel, Layout};
    let adts = &include_bytes!("goldens/mc71p.adts")[..];
    let split = crate::decode_with(adts, &DecodeOptions::unbounded())?;
    assert_eq!(split.layout, Layout::Pce);
    assert_eq!(split.channels.len(), 8);
    let tones: Vec<u32> = split.channels.iter().map(|p| dominant_tone(p)).collect();
    assert_eq!(
        tones, MC71_TONES,
        "declaration order equals the lavc order here"
    );
    let mut labels = Vec::new();
    crate::decode_streaming(adts, &DecodeOptions::unbounded(), |f| {
        labels = f.meta.labels().collect();
        assert_eq!(f.meta.lfe_index(), None, "100 Hz plane is an SCE, not LFE");
        Ok(())
    })?;
    assert_eq!(
        labels,
        vec![
            Channel::FrontLeft,
            Channel::FrontRight,
            Channel::FrontCenter,
            Channel::Other,
            Channel::BackLeft,
            Channel::BackRight,
            Channel::Other,
            Channel::Other,
        ]
    );
    let mono = crate::decode_with(adts, &DecodeOptions::speech())?;
    let m = &mono.channels[0];
    for (i, &mv) in m.iter().enumerate() {
        let mean = split.channels.iter().map(|c| c[i]).sum::<f32>() / 8.0;
        assert!((mv - mean).abs() <= 1e-6, "mean of all 8 planes at {i}");
    }
    Ok(())
}

#[test]
fn channel_configuration_8_to_15_is_a_typed_unsupported_error() {
    use crate::{AacError, UnsupportedFeature};
    let au =
        crate::encode_with(&[vec![0.0f32; 4096]], 48_000, &crate::EncodeOptions::adts()).unwrap();
    let payload = &au[7..]; // first ADTS frame body is enough to reach the check
    for cfg in [8u8, 11, 12, 14, 15] {
        let asc = crate::engine::asc::write_lc(3, cfg);
        let loas = crate::wrap_loas_au(payload, &asc).unwrap();
        let want = |e: AacError| matches!(e, AacError::Unsupported(UnsupportedFeature::ChannelConfiguration(c)) if c == cfg);
        assert!(want(crate::probe(&loas).unwrap_err()), "probe cfg {cfg}");
        assert!(
            want(crate::decode_with(&loas, &DecodeOptions::unbounded()).unwrap_err()),
            "decode cfg {cfg}"
        );
    }
}
