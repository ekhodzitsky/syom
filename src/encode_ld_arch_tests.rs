//! TASK-132: LD encode is an accepted opt-in epic, not an encoder.

#![allow(clippy::unwrap_used, clippy::expect_used)]

const ARCH: &str = include_str!("../lab/profiles/LD_ENC.md");

#[test]
fn architecture_doc_is_a_go_for_opt_in_only() {
    for needle in [
        "not an encoder",
        "## Decision",
        "**Go opt-in AAC-LD encode**",
        "**No-go as a 0.x default**",
        "**No-go ADTS**",
        "**No-go the 480-sample grid**",
        "**No-go lookahead**",
        "**No-go ELD, LD-SBR and USAC.**",
        "## Scope (v1)",
        "ONLY_LONG",
        "## Signal path",
        "## Delay",
        "## Bit budget",
        "6144",
        "## Profile signalling",
        "## Quality targets",
        "## Staged interfaces",
        "512",
        "Forbidden",
    ] {
        assert!(ARCH.contains(needle), "missing {needle:?}");
    }
    assert!(ARCH.contains("Product `encode` stays AAC-LC"));
    assert!(!ARCH.contains("copy the FDK encoder source into"));
}
