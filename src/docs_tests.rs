//! README / BENCH wording must match the crate version and peer evidence.

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[test]
fn readme_install_matches_package_version() {
    let readme = include_str!("../README.md");
    let ver = env!("CARGO_PKG_VERSION");
    let major_minor = ver.rsplit_once('.').map(|(a, _)| a).unwrap_or(ver);
    assert!(
        readme.contains(&format!("syom = \"{major_minor}\"")),
        "install example must use {major_minor}"
    );
    assert!(
        !readme.contains("syom = \"0.3\""),
        "stale 0.3 install example"
    );
    assert!(
        !readme.to_ascii_lowercase().contains("is a parser"),
        "oxideav must not be labeled only a parser"
    );
    assert!(readme.contains("per-band"), "encoder M/S is per-band");
    assert!(readme.contains("BENCH.md"), "link historical benches");
}

#[test]
fn bench_md_is_labeled_historical() {
    let md = include_str!("../BENCH.md");
    assert!(md.contains("historical"));
    assert!(md.contains("non-comparable") || md.contains("unequal"));
}
