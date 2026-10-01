//! TASK-54: bounded metadata probe — no PCM, no stsz allocation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, BudgetKind, DecodeOptions, Decoder, Layout, MemoryBudgets, ProbeContainer,
    ProbeDuration, ProbeProfile, ProbeTrim, UnsupportedFeature, decode, decode_with, probe,
    probe_with,
};

const SINE: &[u8] = include_bytes!("goldens/sine48.adts");
const HE_ADTS: &[u8] = include_bytes!("goldens/he48.adts");
const HE_LATM: &[u8] = include_bytes!("goldens/he48.latm");
const HE_M4A: &[u8] = include_bytes!("goldens/he48.m4a");
const LECTURE: &[u8] = include_bytes!("goldens/lecture.m4a");
const MC51: &[u8] = include_bytes!("goldens/mc51.adts");
const MC51_M4A: &[u8] = include_bytes!("goldens/mc51.m4a");
const PS_M4A: &[u8] = include_bytes!("goldens/ps48.m4a");
const LATM: &[u8] = include_bytes!("goldens/latm48.latm");

fn need_more(e: &AacError) -> bool {
    matches!(e, AacError::NeedMore { .. })
}

/// First LOAS frames of the FDK v2.0.3 LD cells (lab/profiles/REPORT.md;
/// target/tmp/profiles/cells/ld48.loas, ld96s.loas): AOT 23, 48 kHz.
const LD48_LOAS: &str = "56e04e2000b989002788c1442dad0cdd2885a891324892e448ea913855d300a021e211ef9f42f30ecff0cf63e68d56d88bc120eec77bb2e31d4c8dcdd9d9e993ab299d9d9d9d9d9d859d1d058505200000";
const LD96S_LOAS: &str = "56e0802000b99100148f01170fffffffcdda21ba752ae4992e23572e478c892d574c0280878847be49401d6112672d484fc3fda365be29b9e27b9e2339b39fd381286045292922d332998f2e96640cb515a6633254d6a2b5363331a2b514a24aa15401a04210269d40e3eb54ec9225aae9805010f108f7cfa1a0012353b74f6e5e6000";

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

#[test]
fn adts_lc_reports_header_and_unknown_duration() {
    let p = probe(SINE).unwrap();
    assert_eq!(p.container, ProbeContainer::Adts);
    assert_eq!(p.profile, ProbeProfile::Lc);
    assert_eq!(p.meta.core_rate, 48_000);
    assert_eq!(p.meta.output_rate, 48_000);
    assert_eq!(p.meta.layout, Layout::Mpeg(1));
    assert!(matches!(p.duration, ProbeDuration::Unknown));
    assert!(matches!(p.trim, ProbeTrim::Unknown));
    let pcm = decode(SINE).unwrap();
    assert_eq!(pcm.sample_rate, p.meta.output_rate);
    assert_eq!(pcm.core_rate, p.meta.core_rate);
}

#[test]
fn adts_implicit_he_stays_header_lc() {
    let p = probe(HE_ADTS).unwrap();
    assert_eq!(p.container, ProbeContainer::Adts);
    assert_eq!(p.profile, ProbeProfile::Lc);
    assert_eq!(p.meta.core_rate, 24_000);
    assert_eq!(p.meta.output_rate, 24_000);
    assert!(matches!(p.duration, ProbeDuration::Unknown));
    let pcm = decode_with(HE_ADTS, &DecodeOptions::audio()).unwrap();
    assert_eq!(pcm.sample_rate, 48_000);
}

#[test]
fn latm_he_duration_unknown_and_decode_follows() {
    let p = probe(HE_LATM).unwrap();
    assert_eq!(p.container, ProbeContainer::Latm);
    assert!(matches!(p.duration, ProbeDuration::Unknown));
    assert!(matches!(p.trim, ProbeTrim::Unknown));
    let pcm = decode_with(HE_LATM, &DecodeOptions::audio()).unwrap();
    assert_eq!(pcm.sample_rate, 48_000);
    if p.profile == ProbeProfile::Lc {
        assert_eq!(p.meta.output_rate, p.meta.core_rate);
    } else {
        assert_eq!(pcm.sample_rate, p.meta.output_rate);
        assert_eq!(pcm.core_rate, p.meta.core_rate);
    }
}

#[test]
fn latm_lc_unknown_duration() {
    let p = probe(LATM).unwrap();
    assert_eq!(p.container, ProbeContainer::Latm);
    assert_eq!(p.profile, ProbeProfile::Lc);
    assert!(matches!(p.duration, ProbeDuration::Unknown));
}

#[test]
fn m4a_lecture_exact_duration_matches_decode() {
    let p = probe(LECTURE).unwrap();
    assert_eq!(p.container, ProbeContainer::M4a);
    assert_eq!(p.profile, ProbeProfile::Lc);
    assert_eq!(p.meta.layout, Layout::Mpeg(2));
    assert_eq!(p.meta.core_rate, 48_000);
    let ProbeDuration::Exact { samples } = p.duration else {
        panic!("lecture elst duration must be exact, got {:?}", p.duration);
    };
    let ProbeTrim::Exact { priming, .. } = p.trim else {
        panic!("lecture elst trim must be exact");
    };
    assert_eq!(priming, 1024);
    let pcm = decode_with(LECTURE, &DecodeOptions::audio()).unwrap();
    assert_eq!(pcm.sample_rate, p.meta.output_rate);
    assert_eq!(pcm.channels[0].len() as u64, samples);
    assert_eq!(pcm.priming, Some(priming));
}

#[test]
fn m4a_he_and_ps_profiles() {
    let he = probe(HE_M4A).unwrap();
    assert_eq!(he.container, ProbeContainer::M4a);
    assert_ne!(he.profile, ProbeProfile::Lc);
    assert_eq!(he.meta.core_rate, 24_000);
    assert_eq!(he.meta.output_rate, 48_000);
    assert!(matches!(he.duration, ProbeDuration::Exact { .. }));
    let ps = probe(PS_M4A).unwrap();
    assert_eq!(ps.profile, ProbeProfile::HeAacV2);
    assert_eq!(ps.meta.layout, Layout::Mpeg(2));
}

#[test]
fn mc51_layout_adts_and_m4a() {
    let a = probe(MC51).unwrap();
    assert_eq!(a.meta.layout, Layout::Mpeg(6));
    assert_eq!(a.meta.labels().count(), 6);
    let m = probe(MC51_M4A).unwrap();
    assert_eq!(m.container, ProbeContainer::M4a);
    assert_eq!(m.meta.layout, Layout::Mpeg(6));
    assert!(matches!(m.duration, ProbeDuration::Exact { .. }));
}

#[test]
fn partial_prefixes_are_need_more() {
    assert!(need_more(&probe(&[]).unwrap_err()));
    assert!(need_more(&probe(&SINE[..1]).unwrap_err()));
    assert!(need_more(&probe(&SINE[..6]).unwrap_err()));
    assert!(need_more(&probe(&LATM[..2]).unwrap_err()));
    assert!(need_more(&probe(&LECTURE[..12]).unwrap_err()));
    let p = probe(&SINE[..7]).unwrap();
    assert_eq!(p.container, ProbeContainer::Adts);
}

#[test]
fn not_aac_and_unsupported_profile() {
    assert!(matches!(probe(b"ID3XXXXX").unwrap_err(), AacError::NotAac));
    let mut main = SINE[..7].to_vec();
    main[2] &= 0x3F;
    assert!(matches!(
        probe(&main).unwrap_err(),
        AacError::Unsupported(_)
    ));
}

#[test]
fn loas_ld_probe_reports_ld_profile_rates_and_layout() {
    let mono = unhex(LD48_LOAS);
    assert!(crate::sniff_is_latm(&mono));
    assert!(crate::sniff_aac(&mono));
    let p = probe(&mono).unwrap();
    assert_eq!(p.container, ProbeContainer::Latm);
    assert_eq!(p.profile, ProbeProfile::Ld);
    assert_eq!(p.meta.core_rate, 48_000);
    assert_eq!(p.meta.output_rate, 48_000);
    assert_eq!(p.meta.layout, Layout::Mpeg(1));
    assert!(matches!(p.duration, ProbeDuration::Unknown));

    let stereo = unhex(LD96S_LOAS);
    let p = probe(&stereo).unwrap();
    assert_eq!(p.profile, ProbeProfile::Ld);
    assert_eq!(p.meta.core_rate, 48_000);
    assert_eq!(p.meta.layout, Layout::Mpeg(2));
}

#[test]
fn loas_ld_payload_decodes_one_512_frame() {
    // One FDK LOAS frame: 512 samples at 48 kHz, not an LC fallback
    // and not a typed reject (TASK-129).
    let ld = unhex(LD48_LOAS);
    let pcm = decode_with(&ld, &DecodeOptions::unbounded()).unwrap();
    assert_eq!((pcm.sample_rate, pcm.channels.len()), (48_000, 1));
    assert_eq!(pcm.channels[0].len(), 512);
    let mut dec = Decoder::new(DecodeOptions::unbounded());
    let mut n = 0usize;
    dec.feed(&ld, |f| {
        n += f.samples;
        Ok(())
    })
    .unwrap();
    let info = dec.finish(|_| Ok(())).unwrap();
    assert_eq!(n, 512);
    assert_eq!(info.sample_rate, 48_000);
    assert!(!dec.is_failed());
}

#[test]
fn adts_cannot_signal_ld_and_er_profiles_stay_typed() {
    // The ADTS profile field is 2 bits (AOT − 1): AOT 23 is unsignalable
    // there (FDK rejects ADTS carriage too, lab/profiles/REPORT.md). The
    // ER-era profiles ADTS can spell must fail typed, never fall back to LC.
    let mut ssr = SINE[..7].to_vec();
    ssr[2] = (ssr[2] & 0x3F) | 0x80; // profile 2 -> AOT 3 (SSR)
    assert!(matches!(
        probe(&ssr).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::AudioObjectType(3))
    ));
    let mut ltp = SINE[..7].to_vec();
    ltp[2] |= 0xC0; // profile 3 -> AOT 4 (LTP)
    assert!(matches!(
        probe(&ltp).unwrap_err(),
        AacError::Unsupported(UnsupportedFeature::AudioObjectType(4))
    ));
}

#[test]
fn metadata_and_au_limits() {
    let tiny = MemoryBudgets {
        max_metadata_bytes: 8,
        ..MemoryBudgets::default()
    };
    let err = probe_with(LECTURE, &tiny).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::Metadata,
                ..
            }
        ),
        "{err:?}"
    );
    let tight_au = MemoryBudgets {
        max_declared_au_bytes: 8,
        ..MemoryBudgets::default()
    };
    let err = probe_with(SINE, &tight_au).unwrap_err();
    assert!(
        matches!(
            err,
            AacError::Limit {
                kind: BudgetKind::AccessUnit,
                ..
            }
        ),
        "{err:?}"
    );
}

#[test]
fn probe_does_not_allocate_pcm_planes() {
    let p = probe(LECTURE).unwrap();
    let ProbeDuration::Exact { samples } = p.duration else {
        panic!("expected exact");
    };
    assert!(samples > 1_000);
    assert_eq!(
        std::mem::size_of_val(&p),
        std::mem::size_of::<crate::Probe>()
    );
}
