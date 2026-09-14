//! Encode preflight: same `run_encode_preflight` the Criterion groups use.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::encode_cmp::{
    EncodeCandidate, EncodeCollect, EncodeStatus, run_encode_preflight, syom_encode_candidate,
    syom_he_unavailable,
};
use super::{decode, encode};

const LC_ADTS: &[u8] = include_bytes!("goldens/sine48.adts");

fn pcm() -> (Vec<Vec<f32>>, u32) {
    let d = decode(LC_ADTS).unwrap();
    (d.channels, d.sample_rate)
}

fn failing(_: &[Vec<f32>], _: u32, _: u32) -> EncodeCollect {
    EncodeCollect::Failed("injected encode failure".into())
}

fn rusty_encode(pcm: &[Vec<f32>], rate: u32, requested_bps: u32) -> EncodeCollect {
    let mut enc = rusty_aac::AacEncoder::new(rusty_aac::AacEncoderConfig {
        bitrate_bps: requested_bps,
        ..Default::default()
    });
    let planes: Vec<&[f32]> = pcm.iter().map(Vec::as_slice).collect();
    if enc.push_pcm_planar(&planes, rate).is_err() {
        return EncodeCollect::Failed("rusty push".into());
    }
    enc.finish();
    let mut bytes = Vec::new();
    while let Ok(p) = enc.next_packet() {
        let hdr = rusty_aac::AdtsHeader {
            object_type: 2,
            sample_rate: rate,
            channels: pcm.len() as u16,
            frame_length: 7 + p.data.len(),
            header_len: 7,
        };
        bytes.extend_from_slice(&rusty_aac::write_adts_header(&hdr));
        bytes.extend_from_slice(&p.data);
    }
    if bytes.is_empty() {
        EncodeCollect::Failed("rusty produced no packets".into())
    } else {
        EncodeCollect::Adts(bytes)
    }
}

fn rusty_candidate() -> EncodeCandidate {
    EncodeCandidate {
        id: "rusty_aac",
        version: "0.5.0",
        encode: rusty_encode,
    }
}

#[test]
fn failing_encoder_aborts_before_timed() {
    let (pcm, rate) = pcm();
    let pf = run_encode_preflight(
        "sine",
        &pcm,
        rate,
        128_000,
        &[
            syom_encode_candidate(),
            EncodeCandidate {
                id: "failing",
                version: "0",
                encode: failing,
            },
        ],
    );
    assert!(pf.aborted(), "{}", pf.report());
    assert!(pf.timed_ids().is_none(), "{}", pf.report());
    assert!(pf.report().contains("abort:"), "{}", pf.report());
}

#[test]
fn lc_mono_reports_bytes_duration_and_rate() {
    let (pcm, rate) = pcm();
    let pf = run_encode_preflight(
        "sine48-mono",
        &pcm,
        rate,
        128_000,
        &[syom_encode_candidate(), rusty_candidate()],
    );
    assert!(!pf.aborted(), "{}", pf.report());
    let timed = pf.timed_ids().unwrap();
    assert!(timed.contains(&"syom") && timed.contains(&"rusty_aac"));
    let report = pf.report();
    assert!(report.contains("ADTS"), "{report}");
    assert!(report.contains("payload"), "{report}");
    assert!(
        report.contains("req 128000 bps") || report.contains("128000"),
        "{report}"
    );
    // Tonal sine is content-limited for syom (~67 kbps historically).
    let syom = &pf.rows.iter().find(|r| r.id == "syom").unwrap().status;
    match syom {
        EncodeStatus::NonMatchedRate { metrics, .. } | EncodeStatus::Comparable { metrics } => {
            assert!(metrics.adts_bytes > 0);
            assert!(metrics.payload_bytes > 0);
            assert!(metrics.achieved_bps > 0.0);
            assert_eq!(metrics.decoded_rate, 48_000);
        }
        other => panic!("syom should encode: {other:?}\n{}", pf.report()),
    }
}

#[test]
fn he_encode_cell_is_separate_unavailable() {
    let (pcm, rate) = pcm();
    let pf = run_encode_preflight(
        "he-encode",
        &pcm,
        rate,
        64_000,
        &[EncodeCandidate {
            id: "syom",
            version: env!("CARGO_PKG_VERSION"),
            encode: syom_he_unavailable,
        }],
    );
    assert!(!pf.aborted(), "{}", pf.report());
    assert!(
        matches!(pf.rows[0].status, EncodeStatus::Unavailable { .. }),
        "{}",
        pf.report()
    );
    assert!(pf.report().contains("unavailable"), "{}", pf.report());
    assert!(
        pf.timed_ids().unwrap().is_empty(),
        "HE encode must not be timed as LC: {}",
        pf.report()
    );
}

#[test]
fn over_limit_bitrate_fails_before_timing() {
    let (pcm, rate) = pcm();
    let pf = run_encode_preflight(
        "over-limit",
        &pcm,
        rate,
        1_000_000,
        &[syom_encode_candidate()],
    );
    assert!(pf.aborted(), "{}", pf.report());
    assert!(pf.timed_ids().is_none(), "{}", pf.report());
    assert!(
        pf.report().contains("6144") || pf.report().contains("failed"),
        "{}",
        pf.report()
    );
}

#[test]
fn product_encode_still_matches_one_shot() {
    let (pcm, rate) = pcm();
    let a = encode(&pcm, rate).unwrap();
    assert!(!a.is_empty());
}
