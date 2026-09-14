//! Bounded `read` / `read_file_capped` (TASK-22).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{AacError, DecodeOptions, decode, read, read_file_capped, read_with};
use std::io::Write;

const SINE: &[u8] = include_bytes!("goldens/sine48.adts");

fn tmp(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "syom-read-{}-{}-{}",
        name,
        std::process::id(),
        bytes.len()
    ));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn exact_limit_reads_and_limit_plus_one_is_too_long() {
    let p = tmp("exact", SINE);
    let n = SINE.len() as u64;
    let got = read_file_capped(&p, n).unwrap();
    assert_eq!(got, SINE);
    let err = read_file_capped(&p, n - 1).unwrap_err();
    assert!(matches!(err, AacError::TooLong { .. }), "{err}");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn empty_file_is_not_aac_after_capped_read() {
    let p = tmp("empty", &[]);
    let got = read_file_capped(&p, 64).unwrap();
    assert!(got.is_empty());
    assert!(matches!(decode(&got).unwrap_err(), AacError::NotAac));
    let _ = std::fs::remove_file(&p);
}

#[test]
fn missing_file_is_io_error() {
    let p = std::env::temp_dir().join("syom-read-does-not-exist-xyz.adts");
    let err = read_file_capped(&p, 64).unwrap_err();
    assert!(matches!(err, AacError::Io(_)), "{err}");
    assert!(std::error::Error::source(&err).is_some());
}

#[test]
fn valid_existing_file_still_decodes_via_read() {
    let p = tmp("ok", SINE);
    let a = read(&p).unwrap();
    let b = read_with(&p, &DecodeOptions::speech()).unwrap();
    assert_eq!(a.sample_rate, 48_000);
    assert_eq!(a.channels[0].len(), b.channels[0].len());
    let _ = std::fs::remove_file(&p);
}

#[test]
fn growing_file_is_still_capped() {
    // Metadata may be 4 bytes; we still refuse to keep more than `limit`.
    let p = tmp("grow", &[0, 1, 2, 3]);
    let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
    f.write_all(&[4, 5, 6, 7, 8]).unwrap();
    drop(f);
    // Now 9 bytes on disk; limit 4 must fail without returning 9 bytes.
    let err = read_file_capped(&p, 4).unwrap_err();
    assert!(matches!(err, AacError::TooLong { .. }), "{err}");
    let _ = std::fs::remove_file(&p);
}
