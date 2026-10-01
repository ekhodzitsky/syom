//! TASK-18: footprint artifact matches the shipped crate (empty deps, MSRV).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cargo_toml() -> String {
    fs::read_to_string(root().join("Cargo.toml")).unwrap()
}

#[test]
fn product_dependencies_are_empty() {
    let t = cargo_toml();
    let start = t.find("[dependencies]").expect("deps table");
    let rest = &t[start + "[dependencies]".len()..];
    let next = rest.find("\n[").unwrap_or(rest.len());
    let body = rest[..next].to_string();
    assert!(
        body.contains("Intentionally empty"),
        "product [dependencies] must stay empty: {body:?}"
    );
    assert!(!body.contains("symphonia"));
    assert!(!body.contains("rusty_aac"));
    assert!(!body.contains("oxideav"));
}

#[test]
fn msrv_is_1_88_and_the_dev_pin_is_1_97() {
    // TASK-99: the declared minimum is what the code needs (let chains);
    // development and CI stay pinned for stable fmt / clippy output.
    let cargo = cargo_toml();
    assert!(cargo.contains("rust-version = \"1.88\""));
    assert!(
        cargo.contains("\"!/src/**/*_tests.rs\""),
        "package include list"
    );
    assert!(cargo.contains("edition = \"2024\""));
    let pin = fs::read_to_string(root().join("rust-toolchain.toml")).unwrap();
    assert!(pin.contains("1.97.1"));
}

#[test]
fn speech_default_still_mono() {
    let o = crate::DecodeOptions::speech();
    assert_eq!(o.channel_mode, crate::ChannelMode::Mono);
}

fn json_u64(raw: &str, key: &str) -> u64 {
    let needle = format!("\"{key}\": ");
    let i = raw.find(&needle).unwrap_or_else(|| panic!("missing {key}"));
    let rest = &raw[i + needle.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().unwrap_or_else(|_| panic!("bad {key}"))
}

#[test]
fn footprint_manifest_records_go_nogo_and_sizes() {
    let raw = fs::read_to_string(root().join("corpus/footprint/manifest.json")).unwrap();
    assert!(raw.contains("\"dependencies\": {}"));
    assert!(raw.contains("\"keep_zero_product_deps\": \"go\""));
    assert!(raw.contains("\"change_speech_default_to_split\": \"no-go\""));
    assert!(raw.contains("TASK-18"));
    assert!(json_u64(&raw, "rlib_bytes") > 1_000_000);
    assert!(json_u64(&raw, "object_text_bytes") > 100_000);
    assert!(json_u64(&raw, "binary_bytes_unstripped") > 500_000);
    let report = fs::read_to_string(root().join("corpus/footprint/report.md")).unwrap();
    assert!(report.contains("decode(&[u8])"));
    assert!(report.contains("no-go"));
}

#[test]
fn documented_consumer_calls_still_decode_and_encode() {
    let adts = include_bytes!("goldens/sine48.adts");
    let pcm = crate::decode(adts).unwrap();
    assert!(!pcm.channels.is_empty());
    assert!(pcm.channels.iter().all(|c| c.iter().all(|x| x.is_finite())));
    let out = crate::encode(&[vec![0.0f32; 2048]], 48_000).unwrap();
    assert!(out.len() > 7);
    assert!(out.starts_with(b"ID3"));
    assert_eq!(crate::gapless::strip_id3(&out)[0], 0xff);
}
