//! TASK-14: drive `scripts/listen_protocol.py` (not a second protocol).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(args: &[&str]) -> (bool, String) {
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/listen_protocol.py"))
        .args(args)
        .output()
        .expect("python3 listen_protocol.py");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("syom-listen-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn dry_run_blinds_holdout_and_screens_synthetic_listener() {
    let dir = tmp("ok");
    let out = dir.to_str().unwrap();
    let (ok, text) = run(&["dry-run", "--out", out, "--seed", "task-14"]);
    assert!(ok, "dry-run failed: {text}");
    assert!(text.contains("SYNTHETIC-NOT-LISTENING-EVIDENCE"), "{text}");
    assert!(text.contains("not_listening_evidence"), "{text}");

    let result = std::fs::read_to_string(dir.join("result.json")).unwrap();
    for needle in [
        "\"ok\": true",
        "\"items\": 8",
        "\"mushra_items\": 7",
        "\"bs1116_items\": 1",
        "\"valid_listeners\": 8",
        "\"excluded_listeners\": 1",
        "\"n_qualification\": 69",
        "\"n_exploratory\": 20",
        "\"noninferior_mushra\": true",
        "SYNTHETIC-NOT-LISTENING-EVIDENCE",
    ] {
        assert!(result.contains(needle), "result missing {needle}: {result}");
    }

    let design = std::fs::read_to_string(dir.join("design.json")).unwrap();
    for rec in [
        "openslr12-spk-1580",
        "openslr12-spk-1995",
        "openslr12-spk-2094",
        "commonvoice-ar-corpus",
        "commonvoice-ja-corpus",
        "librivox-war-of-the-worlds",
        "musopen-chopin-op28",
        "fma-cc0-percussion",
    ] {
        assert!(design.contains(rec), "missing holdout {rec}");
    }
    assert!(design.contains("lavc9-native-aac"), "{design}");
    assert!(design.contains("syom-lc"), "{design}");
    assert!(design.contains("apple-audiotoolbox"), "{design}");
    assert!(design.contains("no-host"), "{design}");
    assert!(design.contains("\"bitrate_bps\": 64000"), "{design}");
    assert!(design.contains("\"bitrate_bps\": 128000"), "{design}");
    assert!(design.contains("BS.1534-3"), "{design}");
    assert!(design.contains("BS.1116-3"), "{design}");
    assert!(design.contains("planning_sd_mushra"), "{design}");

    let scores = std::fs::read_to_string(dir.join("scores.synthetic.json")).unwrap();
    assert!(
        scores.contains("SYNTHETIC-NOT-LISTENING-EVIDENCE"),
        "{scores}"
    );
    let analysis = std::fs::read_to_string(dir.join("analysis.json")).unwrap();
    assert!(
        analysis.contains("\"not_listening_evidence\": true"),
        "{analysis}"
    );
    assert!(analysis.contains("\"session_ok\": true"), "{analysis}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn packet_filenames_and_durations_do_not_leak_codec() {
    let dir = tmp("blind");
    let out = dir.to_str().unwrap();
    let (ok, text) = run(&["dry-run", "--out", out, "--seed", "task-14"]);
    assert!(ok, "{text}");

    let mut n_meta = 0usize;
    for ent in walkdir(&dir.join("packet")) {
        let name = ent.file_name().unwrap().to_string_lossy().to_lowercase();
        for tok in ["syom", "lavc", "ffmpeg", "fdk", "faac", "glint", "apple"] {
            assert!(!name.contains(tok), "filename leaks {tok}: {name}");
        }
        if name.ends_with(".meta.json") {
            n_meta += 1;
            let body = std::fs::read_to_string(&ent).unwrap().to_lowercase();
            for tok in ["syom", "lavc", "ffmpeg", "fdk", "faac", "glint", "apple"] {
                assert!(!body.contains(tok), "meta leaks {tok}");
            }
            assert!(body.contains("n_samples"), "{ent:?}");
        }
    }
    assert!(n_meta >= 8 * 3, "expected blinded metas, got {n_meta}");

    let design = std::fs::read_to_string(dir.join("design.json")).unwrap();
    // Script fails the run if every UI order equals filename order.
    assert!(design.contains("\"ui_order\""), "{design}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_rejects_codec_filename_and_unlabeled_scores() {
    let dir = tmp("leak");
    let out = dir.to_str().unwrap();
    let (ok, text) = run(&["dry-run", "--out", out, "--seed", "task-14"]);
    assert!(ok, "{text}");

    let src = dir.join("packet/item00/c00.meta.json");
    let leaked = dir.join("packet/item00/syom.meta.json");
    std::fs::rename(&src, &leaked).unwrap();
    let (ok, text) = run(&["verify", "--out", out]);
    assert!(!ok, "codec filename must fail: {text}");
    assert!(
        text.contains("leaks syom") || text.contains("FAIL"),
        "{text}"
    );
    std::fs::rename(&leaked, &src).unwrap();

    let scores_path = dir.join("scores.synthetic.json");
    let orig = std::fs::read_to_string(&scores_path).unwrap();
    let stripped = orig.replace("SYNTHETIC-NOT-LISTENING-EVIDENCE", "real-listeners");
    std::fs::write(&scores_path, stripped).unwrap();
    let (ok, text) = run(&["verify", "--out", out]);
    assert!(!ok, "unlabeled scores must fail: {text}");
    assert!(
        text.contains("SYNTHETIC") || text.contains("scores missing"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn protocol_doc_locks_method_anchors_and_nogo_execution() {
    let proto = std::fs::read_to_string(repo_root().join("lab/listen/PROTOCOL.md")).unwrap();
    for needle in [
        "BS.1534-3",
        "BS.1116-3",
        "3.5 kHz",
        "7 kHz",
        "−23 LUFS",
        "15%",
        "n = 69",
        "fixed n",
        "SYNTHETIC",
        "not listening evidence",
        "openslr12-spk-1580",
        "musopen-chopin-op28",
    ] {
        assert!(proto.contains(needle), "PROTOCOL missing {needle}");
    }
    let report = std::fs::read_to_string(repo_root().join("lab/listen/REPORT.md")).unwrap();
    assert!(report.contains("NO-GO"), "{report}");
    assert!(report.contains("human"), "{report}");
    assert!(report.contains("gap-until-obtained"), "{report}");
    assert!(report.contains("Apple"), "{report}");
    assert!(
        report.contains("not listening evidence") || report.contains("NOT listening"),
        "{report}"
    );
    let pin = std::fs::read_to_string(repo_root().join("lab/listen/PIN.md")).unwrap();
    assert!(pin.contains("Not invoked by"), "{pin}");
}

fn walkdir(root: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for ent in std::fs::read_dir(&dir).unwrap() {
            let p = ent.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}
