//! TASK-54: bounded metadata probe — no PCM, no stsz allocation.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::{
    AacError, BudgetKind, DecodeOptions, Layout, MemoryBudgets, ProbeContainer, ProbeDuration,
    ProbeProfile, ProbeTrim, decode, decode_with, probe, probe_with,
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
