//! Preflight tests drive `decode_cmp::run_preflight` — the same function
//! Criterion uses before timing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::decode_cmp::{
    Candidate, Collect, Lane, Pcm, Status, run_preflight, syom_candidate, syom_collect,
};
use super::{ChannelMode, DecodeOptions};

#[path = "../benches/peers.rs"]
mod peers;

const LC_ADTS: &[u8] = include_bytes!("goldens/sine48.adts");
const LC_M4A: &[u8] = include_bytes!("goldens/sine441.m4a");
const HE_ADTS: &[u8] = include_bytes!("goldens/he48.adts");
const MC_ADTS: &[u8] = include_bytes!("goldens/mc51.adts");
const MALFORMED: &[u8] = &[0u8; 32];

fn failing(_bytes: &[u8], _lane: Lane) -> Collect {
    Collect::Failed("injected failure".into())
}

fn short_pcm(_bytes: &[u8], _lane: Lane) -> Collect {
    Collect::Pcm(Pcm {
        sample_rate: 48_000,
        planes: vec![vec![0.0; 4]],
    })
}

fn nonfinite(_bytes: &[u8], _lane: Lane) -> Collect {
    Collect::Pcm(Pcm {
        sample_rate: 48_000,
        planes: vec![vec![f32::NAN; 8]],
    })
}

fn fail_cand() -> Candidate {
    Candidate {
        id: "failing",
        version: "0",
        collect: failing,
    }
}

fn adts_peers() -> Vec<Candidate> {
    vec![
        syom_candidate(),
        peers::rusty_candidate(),
        peers::oxideav_candidate(),
        peers::symphonia_adts_candidate(),
    ]
}

#[test]
fn failing_candidate_aborts_before_timed() {
    let pf = run_preflight(
        "sine48.adts",
        LC_ADTS,
        Lane::PlanarSplit,
        &[syom_candidate(), fail_cand()],
    );
    assert!(pf.aborted(), "{}", pf.report());
    assert!(
        pf.timed_ids().is_none(),
        "abort must not produce timed ids: {}",
        pf.report()
    );
    assert!(
        pf.row("failing").is_some_and(|r| r.status.is_failed()),
        "{}",
        pf.report()
    );
    let report = pf.report();
    assert!(report.contains("abort:"), "{report}");
    assert!(!report.contains("timed:"), "{report}");
}

#[test]
fn nonfinite_output_aborts_before_timed() {
    let pf = run_preflight(
        "sine48.adts",
        LC_ADTS,
        Lane::PlanarSplit,
        &[
            syom_candidate(),
            Candidate {
                id: "nan",
                version: "0",
                collect: nonfinite,
            },
        ],
    );
    assert!(pf.aborted(), "{}", pf.report());
    assert!(pf.timed_ids().is_none(), "{}", pf.report());
    assert!(
        pf.row("nan").is_some_and(
            |r| matches!(&r.status, Status::Failed { reason } if reason.contains("non-finite"))
        ),
        "{}",
        pf.report()
    );
}

#[test]
fn malformed_fixture_aborts_before_timed() {
    let pf = run_preflight("malformed", MALFORMED, Lane::PlanarSplit, &adts_peers());
    assert!(pf.aborted(), "{}", pf.report());
    assert!(pf.timed_ids().is_none(), "{}", pf.report());
    assert!(
        pf.row("syom").is_some_and(|r| r.status.is_failed()),
        "{}",
        pf.report()
    );
}

#[test]
fn lc_adts_planar_split_is_comparable() {
    let pf = run_preflight("sine48.adts", LC_ADTS, Lane::PlanarSplit, &adts_peers());
    assert!(!pf.aborted(), "{}", pf.report());
    let timed = pf.timed_ids().expect("lc adts should time");
    for id in ["syom", "rusty_aac", "oxideav-aac", "symphonia"] {
        let row = pf.row(id).unwrap_or_else(|| panic!("{id} missing"));
        assert!(
            row.status.is_comparable(),
            "{id} should be comparable: {}",
            pf.report()
        );
        assert!(timed.contains(&id), "{id} not timed: {}", pf.report());
    }
    let Status::Comparable { shape, checksum } = &pf.row("syom").unwrap().status else {
        panic!("{}", pf.report());
    };
    assert_eq!(shape.sample_rate, 48_000, "{}", pf.report());
    assert_eq!(shape.channels, 1, "{}", pf.report());
    assert_eq!(shape.samples, 13_312, "{}", pf.report());
    assert_ne!(*checksum, 0, "consumed output checksum");
    let report = pf.report();
    assert!(report.contains(env!("CARGO_PKG_VERSION")), "{report}");
    assert!(report.contains(peers::RUSTY_AAC), "{report}");
    assert!(report.contains(peers::OXIDEAV_AAC), "{report}");
    assert!(report.contains(peers::SYMPHONIA), "{report}");
    assert!(report.contains("48000 Hz 1 ch 13312 samples"), "{report}");
    assert!(report.contains("checksum="), "{report}");
    assert!(report.contains("lane=planar_split"), "{report}");
}

#[test]
fn he_adts_core_only_is_noncomparable() {
    let pf = run_preflight("he48.adts", HE_ADTS, Lane::PlanarSplit, &adts_peers());
    assert!(!pf.aborted(), "{}", pf.report());
    let timed = pf.timed_ids().expect("he group should not abort");
    assert!(
        pf.row("syom").is_some_and(|r| r.status.is_comparable()),
        "{}",
        pf.report()
    );
    let rusty = pf.row("rusty_aac").expect("rusty row");
    assert!(
        matches!(rusty.status, Status::NonComparable { .. }),
        "core-only HE must be non-comparable: {}",
        pf.report()
    );
    assert!(!timed.contains(&"rusty_aac"), "{}", pf.report());
    let report = pf.report();
    assert!(report.contains("non-comparable"), "{report}");
    assert!(!report.contains("throughput"), "{report}");
    assert!(report.contains("timed:"), "{report}");
}

#[test]
fn mismatched_length_is_noncomparable_not_throughput() {
    let pf = run_preflight(
        "sine48.adts",
        LC_ADTS,
        Lane::PlanarSplit,
        &[
            syom_candidate(),
            Candidate {
                id: "short",
                version: "0",
                collect: short_pcm,
            },
        ],
    );
    assert!(!pf.aborted(), "{}", pf.report());
    let timed = pf.timed_ids().expect("mismatch is not abort");
    assert!(
        matches!(
            pf.row("short").unwrap().status,
            Status::NonComparable { .. }
        ),
        "{}",
        pf.report()
    );
    assert!(!timed.contains(&"short"), "{}", pf.report());
    assert!(timed.contains(&"syom"), "{}", pf.report());
}

#[test]
fn speech_and_discard_are_named_distinct_lanes() {
    let split = run_preflight(
        "sine48.adts",
        LC_ADTS,
        Lane::PlanarSplit,
        &[syom_candidate()],
    );
    let speech = run_preflight(
        "sine48.adts",
        LC_ADTS,
        Lane::SpeechDownmix,
        &[syom_candidate()],
    );
    let discard = run_preflight(
        "sine48.adts",
        LC_ADTS,
        Lane::DiscardOutput,
        &[syom_candidate()],
    );
    assert_eq!(Lane::PlanarSplit.as_str(), "planar_split");
    assert_eq!(Lane::SpeechDownmix.as_str(), "speech_downmix");
    assert_eq!(Lane::DiscardOutput.as_str(), "discard_output");
    assert!(
        split.report().contains("lane=planar_split"),
        "{}",
        split.report()
    );
    assert!(
        speech.report().contains("lane=speech_downmix"),
        "{}",
        speech.report()
    );
    assert!(
        discard.report().contains("lane=discard_output"),
        "{}",
        discard.report()
    );
    let mc_split = run_preflight("mc51.adts", MC_ADTS, Lane::PlanarSplit, &[syom_candidate()]);
    let mc_speech = run_preflight(
        "mc51.adts",
        MC_ADTS,
        Lane::SpeechDownmix,
        &[syom_candidate()],
    );
    let Status::Comparable { shape: split_s, .. } = &mc_split.row("syom").unwrap().status else {
        panic!("{}", mc_split.report());
    };
    let Status::Comparable {
        shape: speech_s, ..
    } = &mc_speech.row("syom").unwrap().status
    else {
        panic!("{}", mc_speech.report());
    };
    assert!(split_s.channels > 1, "split keeps 5.1 planes");
    assert_eq!(speech_s.channels, 1, "speech downmix is mono");
    assert_eq!(split_s.samples, speech_s.samples);
}

#[test]
fn lc_m4a_marks_adts_only_peers_noncomparable() {
    let pf = run_preflight(
        "sine441.m4a",
        LC_M4A,
        Lane::PlanarSplit,
        &[
            syom_candidate(),
            peers::rusty_candidate(),
            peers::oxideav_candidate(),
            peers::symphonia_m4a_candidate(),
        ],
    );
    assert!(!pf.aborted(), "{}", pf.report());
    let timed = pf.timed_ids().expect("m4a should time syom");
    assert!(timed.contains(&"syom"), "{}", pf.report());
    assert!(
        matches!(
            pf.row("rusty_aac").unwrap().status,
            Status::NonComparable { .. }
        ),
        "{}",
        pf.report()
    );
    assert!(
        matches!(
            pf.row("oxideav-aac").unwrap().status,
            Status::NonComparable { .. }
        ),
        "{}",
        pf.report()
    );
    assert!(!timed.contains(&"rusty_aac") && !timed.contains(&"oxideav-aac"));
}

#[test]
fn discard_lane_counts_match_preflight_samples() {
    let pf = run_preflight("sine48.adts", LC_ADTS, Lane::DiscardOutput, &adts_peers());
    assert!(!pf.aborted(), "{}", pf.report());
    let Status::Comparable { shape, .. } = &pf.row("syom").unwrap().status else {
        panic!("{}", pf.report());
    };
    let n = shape.samples;
    assert_eq!(super::decode_cmp::syom_discard(LC_ADTS).unwrap(), n);
    assert_eq!(peers::rusty_discard(LC_ADTS).unwrap(), n);
    assert_eq!(peers::oxideav_discard(LC_ADTS).unwrap(), n);
    assert_eq!(peers::symphonia_adts_discard(LC_ADTS).unwrap(), n);
}

#[test]
fn speech_default_unchanged() {
    assert_eq!(DecodeOptions::speech().channel_mode, ChannelMode::Mono);
    assert_eq!(DecodeOptions::default().channel_mode, ChannelMode::Mono);
    let speech = syom_collect(LC_ADTS, Lane::SpeechDownmix);
    let Collect::Pcm(pcm) = speech else {
        panic!("speech collect failed");
    };
    assert_eq!(pcm.planes.len(), 1);
}

#[test]
fn peer_versions_match_dev_dependencies() {
    let toml = include_str!("../Cargo.toml");
    assert!(toml.contains("rusty_aac = \"0.5.0\""));
    assert!(toml.contains("oxideav-aac = \"0.1.7\""));
    assert!(toml.contains("symphonia = { version = \"0.6\""));
}

#[test]
fn bench_md_labels_historical_decode_rows() {
    let md = include_str!("../BENCH.md");
    assert!(
        md.contains("historical"),
        "decode tables must stay labeled historical"
    );
    assert!(
        md.contains("non-comparable"),
        "preflight policy should be documented"
    );
}
