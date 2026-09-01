# syom — Agent Guide

> On-device audio extract. One CLI. No cloud. No ffmpeg.
>
> **съём.** SYOM: Speech Yielded from Original Media.
>
> MP4/M4A (AAC `mp4a` or PCM `sowt`/`twos`/`ipcm`/`lpcm`/`raw `)/ADTS/WAV → 16 kHz s16le mono. Opus is a later pack, not a fork.

Repository: https://github.com/ekhodzitsky/syom  
License: MIT (engine oxideav-aac MIT). AAC patents: see NOTICE.  
Status: private.

## Product

`syom FILE -o OUT.wav` writes 16 kHz mono PCM (WAV or raw `.pcm`).
No models. No network. Missing input is a hard error.

Forbidden: cloud decode, ffmpeg, Symphonia, API keys, telemetry.

## CLI (v1)

```sh
syom lecture.mp4 -o lecture.wav
syom lecture.mp4 -o lecture.pcm
```

No `serve`. No microphone. stdout is WAV only with `-o -`.

JSON is not the contract. Bytes are. kover spawns this binary.

## Stack

- Rust 2024, pin in `rust-toolchain.toml`.
- Decoder crate `syom-aac` (vendored oxideav-aac + rustfft IMDCT).
- No Python, no ffmpeg, no `symphonia` on the product path.
- Remux / video bitstream stay in kover. This repo does not copy video.

## Checks

```sh
cargo fmt --check
cargo test --workspace --lib --bins
cargo clippy --all-targets -- -D warnings
python3 scripts/check-file-size.py
```

No `unwrap` / `expect` in `syom` (clippy deny). `syom-aac` engine is
vendored (`#![allow(clippy::all)]`). Tests return `Result` and use `?`.
`thiserror` in the library surface. `tracing`, never `println!` (stdout
may be WAV). Implementation files ≤ 400 lines. Test files ≤ 500.
`lib.rs` / `main.rs` are module trees only. Tests live in sibling
`foo_tests.rs`. Engine / `isomp4.rs` are exempt from the line cap.

## Git

Do not commit on `main` after bootstrap. One concern per branch.
Isolated worktree while a PR is open.
