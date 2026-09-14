//! TASK-17: independently authored syntax vectors drive shipped parsers.
//! Ordinary tests never spawn ffmpeg/FDK and never need ISO 14496-26.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

use crate::engine::adts::AdtsHeader;
use crate::engine::asc::AudioSpecificConfig;
use crate::engine::bits::BitReader;
use crate::engine::channel_map::parse_pce;
use crate::engine::error::Error;
use crate::engine::latm::MuxCfg;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn unhex(s: &str) -> Vec<u8> {
    assert!(s.len().is_multiple_of(2), "{s}");
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn json_str(obj: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\": \"");
    let i = obj.find(&pat)?;
    let rest = &obj[i + pat.len()..];
    let e = rest.find('"')?;
    Some(rest[..e].to_string())
}

fn json_bool(obj: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{key}\": ");
    let i = obj.find(&pat)?;
    let rest = obj[i + pat.len()..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_u(obj: &str, key: &str) -> Option<u64> {
    let pat = format!("\"{key}\": ");
    let i = obj.find(&pat)?;
    let rest = obj[i + pat.len()..].trim_start();
    if rest.starts_with(['"', 't', 'f', 'n', '{', '[']) {
        return None;
    }
    let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    n.parse().ok()
}

struct Vector {
    id: String,
    layer: String,
    agreement: String,
    hex: String,
    independent: String,
}

fn load_vectors() -> Vec<Vector> {
    let text = std::fs::read_to_string(repo_root().join("corpus/conformance/vectors.json"))
        .expect("vectors.json");
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find("\"id\": \"") {
        rest = &rest[i + 7..];
        let e = rest.find('"').unwrap();
        let id = rest[..e].to_string();
        let chunk_end = rest[e..]
            .find("\"id\": \"")
            .map(|j| e + j)
            .unwrap_or(rest.len());
        let obj = &rest[..chunk_end];
        out.push(Vector {
            id,
            layer: json_str(obj, "layer").unwrap(),
            agreement: json_str(obj, "agreement").unwrap(),
            hex: json_str(obj, "hex").unwrap(),
            independent: json_str(obj, "independent")
                .map(|_| String::new())
                .unwrap_or_default(),
        });
        // Keep the independent object text: from "independent":
        if let Some(p) = obj.find("\"independent\":") {
            out.last_mut().unwrap().independent = obj[p..].to_string();
        }
        rest = &rest[e..];
    }
    out
}

fn err_name(e: Error) -> &'static str {
    match e {
        Error::UnsupportedFrameLength => "UnsupportedFrameLength",
        Error::UnsupportedAot(_) => "UnsupportedAot",
        Error::AdtsLayerNonZero => "AdtsLayerNonZero",
        Error::AdtsReservedSampleRateIndex => "AdtsReservedSampleRateIndex",
        _ => "other",
    }
}

fn asc_matches(bytes: &[u8], ind: &str) -> bool {
    let Ok((asc, _)) = AudioSpecificConfig::parse(bytes) else {
        return false;
    };
    json_u(ind, "aot").is_none_or(|v| u64::from(asc.aot) == v)
        && json_u(ind, "sample_rate").is_none_or(|v| u64::from(asc.sample_rate) == v)
        && json_u(ind, "output_sample_rate").is_none_or(|v| u64::from(asc.output_sample_rate) == v)
        && json_u(ind, "channel_configuration")
            .is_none_or(|v| u64::from(asc.channel_configuration) == v)
        && json_bool(ind, "sbr_present").is_none_or(|v| asc.sbr_present == v)
        && json_bool(ind, "ps_present").is_none_or(|v| asc.ps_present == v)
        && json_u(ind, "pce_front_sce")
            .is_none_or(|v| asc.pce.as_ref().map(|p| p.front.len() as u64) == Some(v))
}

fn adts_matches(bytes: &[u8], ind: &str) -> bool {
    let Ok((h, off)) = AdtsHeader::parse(bytes) else {
        return false;
    };
    json_bool(ind, "protection_absent").is_none_or(|v| h.protection_absent == v)
        && json_u(ind, "profile").is_none_or(|v| u64::from(h.profile) == v)
        && json_u(ind, "sampling_frequency_index")
            .is_none_or(|v| u64::from(h.sampling_frequency_index) == v)
        && json_u(ind, "channel_configuration")
            .is_none_or(|v| u64::from(h.channel_configuration) == v)
        && json_u(ind, "aac_frame_length").is_none_or(|v| u64::from(h.aac_frame_length) == v)
        && json_u(ind, "number_of_raw_data_blocks_in_frame")
            .is_none_or(|v| u64::from(h.number_of_raw_data_blocks_in_frame) == v)
        && json_u(ind, "payload_offset").is_none_or(|v| off as u64 == v)
}

fn latm_matches(bytes: &[u8], ind: &str) -> bool {
    let mut br = BitReader::new(bytes);
    let Ok(cfg) = MuxCfg::parse(&mut br) else {
        return false;
    };
    json_u(ind, "aot").is_none_or(|v| u64::from(cfg.asc.aot) == v)
        && json_u(ind, "sample_rate").is_none_or(|v| u64::from(cfg.asc.sample_rate) == v)
        && json_u(ind, "channel_configuration")
            .is_none_or(|v| u64::from(cfg.asc.channel_configuration) == v)
        && json_u(ind, "frame_length_type").is_none_or(|v| u64::from(cfg.frame_length_type) == v)
        && json_u(ind, "num_sub_frames_field").is_none_or(|v| u64::from(cfg.num_sub_frames) == v)
}

fn pce_matches(bytes: &[u8], ind: &str) -> bool {
    let mut br = BitReader::new(bytes);
    let Ok(p) = parse_pce(&mut br) else {
        return false;
    };
    json_u(ind, "front").is_none_or(|v| p.front.len() as u64 == v)
        && json_u(ind, "side").is_none_or(|v| p.side.len() as u64 == v)
        && json_u(ind, "back").is_none_or(|v| p.back.len() as u64 == v)
        && json_u(ind, "lfe").is_none_or(|v| p.lfe.len() as u64 == v)
}

fn independent_match(v: &Vector, bytes: &[u8]) -> bool {
    if json_str(&v.independent, "error").is_some() {
        return false;
    }
    match v.layer.as_str() {
        "asc" => asc_matches(bytes, &v.independent),
        "adts" => adts_matches(bytes, &v.independent),
        "latm" => latm_matches(bytes, &v.independent),
        "pce" => pce_matches(bytes, &v.independent),
        _ => false,
    }
}

fn reject_ok(v: &Vector, bytes: &[u8]) -> bool {
    let want = json_str(&v.independent, "error").unwrap();
    let got = match v.layer.as_str() {
        "asc" => AudioSpecificConfig::parse(bytes).err().map(err_name),
        "adts" => AdtsHeader::parse(bytes).err().map(err_name),
        "latm" => MuxCfg::parse(&mut BitReader::new(bytes))
            .err()
            .map(err_name),
        _ => None,
    };
    got == Some(want.as_str())
}

#[test]
fn verifier_accepts_inventory_offline() {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/verify_conformance_inventory.py"))
        .env_remove("MINT_GOLDENS")
        .output()
        .expect("python3");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(text.contains("no ffmpeg invoked"), "{text}");
    assert!(text.contains("ISO 14496-26 not obtained"), "{text}");
}

#[test]
fn shipped_parsers_on_authored_vectors() {
    let vecs = load_vectors();
    assert!(vecs.len() >= 12, "missing authored vectors");
    for v in &vecs {
        let bytes = unhex(&v.hex);
        match v.agreement.as_str() {
            "match" => assert!(
                independent_match(v, &bytes),
                "{} must match independent fields via shipped parser",
                v.id
            ),
            "match-reject" => assert!(
                reject_ok(v, &bytes),
                "{} must reject as {}",
                v.id,
                json_str(&v.independent, "error").unwrap()
            ),
            "diverge" => assert!(
                !independent_match(v, &bytes),
                "{} unexpectedly matches independent layout; update agreement if TASK follow-on landed",
                v.id
            ),
            "partial" | "local" => {
                // Header parse must not panic. Partial gaps stay catalogued.
                let _ = independent_match(v, &bytes);
            }
            other => panic!("{}: unknown agreement {other}", v.id),
        }
    }
}

#[test]
fn match_vectors_cover_asc_adts_latm() {
    let vecs = load_vectors();
    let layers: Vec<_> = vecs
        .iter()
        .filter(|v| v.agreement == "match")
        .map(|v| v.layer.as_str())
        .collect();
    assert!(layers.contains(&"asc"));
    assert!(layers.contains(&"adts"));
    assert!(layers.contains(&"latm"));
}

#[test]
fn two_rate_explicit_sbr_is_iso_amd2_not_local_one_rate() {
    let vecs = load_vectors();
    let two = vecs
        .iter()
        .find(|v| v.id == "asc-explicit-sbr-two-rate-24-48")
        .unwrap();
    assert_eq!(two.hex, "2b098800");
    assert_eq!(two.agreement, "match");
    let bytes = unhex(&two.hex);
    assert!(
        asc_matches(&bytes, &two.independent),
        "Amd 2 two-rate explicit SBR must parse"
    );
    let local = vecs
        .iter()
        .find(|v| v.id == "asc-explicit-sbr-one-rate-syom-local")
        .unwrap();
    assert!(
        AudioSpecificConfig::parse(&unhex(&local.hex)).is_err(),
        "one-rate AOT5 is not Table 1.13"
    );
}

#[test]
fn implicit_sbr_vector_is_the_he48_m4a_esds_prefix() {
    let m4a = include_bytes!("goldens/he48.m4a");
    let needle = unhex("130856e598");
    assert!(
        m4a.windows(needle.len()).any(|w| w == needle.as_slice()),
        "authored implicit SBR ASC must appear in committed he48.m4a"
    );
    let (asc, _) = AudioSpecificConfig::parse(&needle).unwrap();
    assert!(asc.sbr_present);
    assert_eq!(asc.sample_rate, 24_000);
    assert_eq!(asc.output_sample_rate, 48_000);
}

#[test]
fn adts_multi_rdb_count_is_parsed_and_decode_ignores_it() {
    let hdr = include_str!("../corpus/conformance/vectors.json");
    assert!(hdr.contains("\"id\": \"adts-lc-two-rdb\""));
    let bytes = unhex("fff14c4000fffd");
    let (h, off) = AdtsHeader::parse(&bytes).unwrap();
    assert_eq!(h.number_of_raw_data_blocks_in_frame, 2);
    assert_eq!(off, 7);
    let decode = std::fs::read_to_string(repo_root().join("src/engine/decode.rs")).unwrap();
    assert!(decode.contains("_num_raw_data_blocks"));
}

#[test]
fn unsupported_is_not_malformed_for_960_and_main() {
    let fl = AudioSpecificConfig::parse(&unhex("118c")).unwrap_err();
    assert!(matches!(fl, Error::UnsupportedFrameLength));
    let main = AudioSpecificConfig::parse(&unhex("0988")).unwrap_err();
    assert!(matches!(main, Error::UnsupportedAot(1)));
    let bad = AdtsHeader::parse(&unhex("fff34c4000fffc")).unwrap_err();
    assert!(matches!(bad, Error::AdtsLayerNonZero));
}

#[test]
fn clauses_record_unpaid_iso_and_latm_get_value_gap() {
    let clauses =
        std::fs::read_to_string(repo_root().join("corpus/conformance/clauses.json")).unwrap();
    assert!(clauses.contains("\"iso_14496_26_obtained\": false"));
    assert!(clauses.contains("latm_value is 8-bit+escape"));
    let gonogo =
        std::fs::read_to_string(repo_root().join("corpus/conformance/go-nogo.md")).unwrap();
    assert!(gonogo.contains("No conformance certificate"));
    assert!(gonogo.contains("TASK-26"));
    assert!(gonogo.contains("TASK-30"));
}
