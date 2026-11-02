//! Each profile starts in a new process, before any decoder table is warm.
#![allow(clippy::unwrap_used)]

use std::process::Command;
use syom::{DecodeOptions, decode_with};

#[test]
fn cold_decode_on_small_stack() {
    if let Ok(profile) = std::env::var("SYOM_COLD_STACK_PROFILE") {
        let data: &[u8] = match profile.as_str() {
            "lc" => include_bytes!("../src/goldens/sine48.adts"),
            "he1" => include_bytes!("../src/goldens/he48e.adts"),
            "he2" => include_bytes!("../src/goldens/ps48.adts"),
            "implicit-ps" => include_bytes!("../src/goldens/he48.adts"),
            "mc" => include_bytes!("../src/goldens/mc51.adts"),
            "ld" => include_bytes!("../src/goldens/ld64m.loas"),
            _ => panic!("unknown cold-stack profile"),
        };
        // Debug codec frames need more space; the optimized contract is 128 KiB.
        let stack_kib = if cfg!(debug_assertions) { 512 } else { 128 };
        let cold = std::thread::Builder::new()
            .stack_size(stack_kib * 1024)
            .spawn(move || decode_with(data, &DecodeOptions::unbounded()).unwrap())
            .unwrap()
            .join()
            .unwrap();
        assert!(!cold.channels.is_empty());
        assert!(
            cold.channels
                .iter()
                .all(|p| !p.is_empty() && p.iter().all(|x| x.is_finite()))
        );
        let warm = decode_with(data, &DecodeOptions::unbounded()).unwrap();
        assert_eq!(cold.sample_rate, warm.sample_rate);
        assert_eq!(cold.channels, warm.channels);
        eprintln!("cold-stack {profile}: {stack_kib} KiB; cold/warm PCM equal");
        return;
    }

    let mut failures = Vec::new();
    for profile in ["lc", "he1", "he2", "implicit-ps", "mc", "ld"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "cold_decode_on_small_stack", "--nocapture"])
            .env("SYOM_COLD_STACK_PROFILE", profile)
            .output()
            .unwrap();
        eprintln!("{profile}: {}", output.status);
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        if !output.status.success() {
            failures.push(profile);
        }
    }
    assert!(failures.is_empty(), "cold-stack failures: {failures:?}");
}
