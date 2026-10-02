//! Offline corpus verifier: same `scripts/verify_corpus.py` used by hand.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn verify(args: &[&str]) -> (bool, String) {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/verify_corpus.py"))
        .args(args)
        .output()
        .expect("python3 verify_corpus.py");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

#[test]
fn verifier_accepts_committed_manifest_offline() {
    let (ok, text) = verify(&[]);
    assert!(ok, "verifier failed: {text}");
    assert!(text.contains("OK:"), "{text}");
    assert!(text.contains("natural excerpts"), "{text}");
}

#[test]
fn verifier_detects_modified_bytes() {
    let dir = std::env::temp_dir().join(format!("syom-corpus-flip-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let golden = repo_root().join("src/goldens/sine48.adts");
    let flipped = dir.join("sine48.adts");
    let mut bytes = std::fs::read(&golden).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xff;
    std::fs::write(&flipped, &bytes).unwrap();
    let orig = std::fs::read_to_string(repo_root().join("corpus/manifest.json")).unwrap();
    // Point the sine48 row at the flipped copy; digest stays the committed one.
    // Forward slashes: a Windows path's backslashes are not valid JSON escapes.
    let flipped_json = flipped.to_string_lossy().replace('\\', "/");
    let patched = orig.replace(
        "\"path\": \"src/goldens/sine48.adts\"",
        &format!("\"path\": \"{flipped_json}\""),
    );
    let man = dir.join("manifest.json");
    std::fs::write(&man, patched.as_bytes()).unwrap();
    let (ok, text) = verify(&["--manifest", man.to_str().unwrap()]);
    assert!(!ok, "bit-flip must fail: {text}");
    assert!(
        text.contains("sha256 mismatch") || text.contains("modified bytes"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verifier_detects_train_holdout_overlap_and_missing_asset() {
    let dir = std::env::temp_dir().join(format!("syom-corpus-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let man_path = dir.join("manifest.json");
    let orig = std::fs::read_to_string(repo_root().join("corpus/manifest.json")).unwrap();
    let overlapped = orig.replace(
        "\"recording_id\": \"openslr12-spk-1580\"",
        "\"recording_id\": \"openslr12-spk-1089\"",
    );
    std::fs::write(&man_path, overlapped.as_bytes()).unwrap();
    let (ok, text) = verify(&["--manifest", man_path.to_str().unwrap()]);
    assert!(!ok, "overlap must fail: {text}");
    assert!(
        text.contains("overlaps") || text.contains("both dev and holdout"),
        "{text}"
    );

    let missing = orig.replace("src/goldens/sine48.adts", "src/goldens/does-not-exist.adts");
    std::fs::write(&man_path, missing.as_bytes()).unwrap();
    let (ok, text) = verify(&["--manifest", man_path.to_str().unwrap()]);
    assert!(!ok, "missing asset must fail: {text}");
    assert!(text.contains("missing required asset"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verifier_detects_inconsistent_sample_metadata() {
    let dir = std::env::temp_dir().join(format!("syom-corpus-meta-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let man_path = dir.join("manifest.json");
    let orig = std::fs::read_to_string(repo_root().join("corpus/manifest.json")).unwrap();
    let orig = orig.replacen("\"valid_samples\": 13312", "\"valid_samples\": -1", 1);
    std::fs::write(&man_path, orig.as_bytes()).unwrap();
    let (ok, text) = verify(&["--manifest", man_path.to_str().unwrap()]);
    assert!(!ok, "negative samples must fail: {text}");
    assert!(text.contains("valid_samples"), "{text}");
    let _ = std::fs::remove_dir_all(&dir);
}
