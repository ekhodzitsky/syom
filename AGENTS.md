# syom — Agent Guide

> just aac. Pure-Rust AAC-LC / HE-AAC decoder. Zero `[dependencies]`.
>
> **съём.** SYOM: Speech Yielded from Original Media — the AAC crate.
>
> `cargo add syom`. One call, planar `f32` at native rate.

Repository: https://github.com/ekhodzitsky/syom  
License: MIT (AAC-LC engine original; HE SBR/PS from in-tree tables). AAC patents: see NOTICE.  
Status: private.

## Product

Library, not a 16 kHz extract CLI. Empty product `[dependencies]`. No C/FFI,
ffmpeg, rustfft, clap, tracing, thiserror, Symphonia on the product path.

```rust
let pcm = syom::decode(&bytes)?; // planar f32, bitstream rate
```

`decode` / `decode_bytes` / `decode_with` / `read` / `read_with` plus
`sniff_aac` / `sniff_is_adts` / `sniff_is_latm` / `sniff_is_isobmff`.
Caps: `DecodeOptions::speech()` (default) / `unbounded()`. Encode is not v1.
CLI is examples only.

AAC-LC + HE-AAC v1/v2 (SBR/PS) from ADTS, M4A/ISOBMFF `mp4a`+ASC, LATM/LOAS.
Honour `elst.media_time`. Own IMDCT/FFT. Own error enum.

ffmpeg / fdk-aac are **offline oracles**. Tests never spawn them.

## Checks

```sh
cargo fmt --check
cargo test --lib --bins --doc
cargo clippy --all-targets -- -D warnings
python3 scripts/check-file-size.py
python3 scripts/check-changelog.py
```

No `unwrap` / `expect` in lib (clippy deny). Tests return `Result` and use
`?`. Impl files ≤ 400 lines. Test files ≤ 500. `lib.rs` is a module tree.
Tests live in sibling `*_tests.rs`. Exempt like isomp4: `isomp4.rs`, HE
`sbr_*` / `ps_*` / `extension_payload` / `crc`.

## Git

Do not commit on `main` after bootstrap. One concern per branch.
Isolated worktree while a PR is open.

## Versioning

Keep a Changelog + SemVer. Crate version is `syom` (`Cargo.toml` package).

- Pre-1.0: new input/output formats bump **y**; fixes bump **z**.
- Every user-visible PR adds a bullet under `## [Unreleased]`.

kover/sluh spawn or link this crate. Do not grow a second AAC decoder there.
