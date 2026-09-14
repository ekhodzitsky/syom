//! TASK-15: matched-output baseline artifacts and the shipped preflight.

#![allow(clippy::unwrap_used, clippy::expect_used)]

fn root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn bench_md_has_matched_output_and_keeps_historical() {
    let md = std::fs::read_to_string(root().join("BENCH.md")).unwrap();
    assert!(md.contains("matched-output"), "{md}");
    assert!(md.contains("historical"), "archive tables stay labeled");
    assert!(md.contains("run_preflight"), "{md}");
    assert!(md.contains("13312"), "LC sample count");
    assert!(md.contains("lab/baseline/REPORT.md"), "{md}");
}

#[test]
fn baseline_harness_calls_shipped_preflight() {
    let src = std::fs::read_to_string(root().join("benches/baseline.rs")).unwrap();
    assert!(src.contains("run_preflight"));
    assert!(src.contains("const REPS: usize = 20"));
    assert!(src.contains("syom::decode_cmp"));
    assert!(src.contains("Lane::PlanarSplit"));
    assert!(src.contains("Lane::SpeechDownmix"));
    assert!(src.contains("Lane::DiscardOutput"));
}

#[test]
fn baseline_report_records_ci_memory_and_go_nogo() {
    let report = std::fs::read_to_string(root().join("lab/baseline/REPORT.md")).unwrap();
    assert!(report.contains("median_ns"));
    assert!(report.contains("p95_ns"));
    assert!(report.contains("bootstrap"));
    assert!(report.contains("he_adts"));
    assert!(report.contains("ps_adts"));
    assert!(report.contains("mc_adts"));
    assert!(report.contains("peak_live"));
    assert!(report.contains("first_output"));
    assert!(report.contains("perf_event_paranoid"));
    assert!(report.contains("no-go"));
    assert!(report.contains("TASK-80"));
    assert!(report.contains("TASK-77"));
    let reset = std::fs::read_to_string(root().join("lab/baseline/RESET.md")).unwrap();
    assert!(reset.contains("Encoder::new"));
    assert!(reset.contains("go"));
    assert!(report.contains("13312"));
    let pin = std::fs::read_to_string(root().join("lab/baseline/PIN.md")).unwrap();
    assert!(pin.contains("AMD Ryzen"));
    assert!(pin.contains("taskset"));
}

#[test]
fn product_does_not_depend_on_baseline_lab() {
    let cargo = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    assert!(!cargo.contains("lab/baseline"));
    assert!(cargo.contains("name = \"baseline\""));
}
