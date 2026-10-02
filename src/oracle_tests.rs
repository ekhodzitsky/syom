//! Oracle provenance: ordinary tests never invoke ffmpeg/FDK.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::process::Command;

fn repo_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn provenance_verifier_passes_offline() {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/verify_oracle_provenance.py"))
        .output()
        .expect("python3");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    assert!(text.contains("no ffmpeg invoked"), "{text}");
}

#[test]
fn synth_oracle_regenerates_without_ffmpeg() {
    let man = std::fs::read_to_string(repo_root().join("corpus/oracles/provenance.json")).unwrap();
    assert!(man.contains("\"id\": \"synth-oracle-impulse-0\""));
    assert!(man.contains("pns_stochastic"));
    assert!(man.contains("deterministic"));
    assert!(
        man.contains("ffmpeg/libavcodec version") || man.contains("provenance_gap"),
        "historical gaps must stay explicit"
    );
    // Regeneration is the python verifier's sha256_of(settings) path.
    // Linux keeps a minimal PATH so a shell-out to ffmpeg would fail.
    // That path cannot start CPython on Windows; leave PATH alone there.
    let mut cmd = Command::new("python3");
    cmd.arg(repo_root().join("scripts/verify_oracle_provenance.py"))
        .env_remove("MINT_GOLDENS");
    if cfg!(not(windows)) {
        cmd.env("PATH", "/usr/bin:/bin");
    }
    let out = cmd.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
}

#[test]
fn mint_tests_are_opt_in_and_source_does_not_spawn_ffmpeg() {
    let enc = std::fs::read_to_string(repo_root().join("src/encode_tests.rs")).unwrap();
    assert!(enc.contains("MINT_GOLDENS"));
    assert!(!enc.contains("Command::new(\"ffmpeg\")") && !enc.contains("Command::new(\"fdk"));
    let lib_tests = std::fs::read_to_string(repo_root().join("src/decode_tests.rs")).unwrap();
    assert!(!lib_tests.contains("Command::new(\"ffmpeg\")"));
}
