# fdk-aac-rust 0.2.3 evaluation (TASK-5)

Recorded 2026-09-14, rustc 1.97.1, Linux x86_64. Isolated lab
`lab/fdk-aac-rust`, `--no-default-features`. `FDK_AAC_SOURCE_DIR` =
fdk-aac @ `d8e6b1a3aa606c450241632b64b703f21ea31ce3`. Afterburner off.
Not invoked by `cargo test`.

## Lineage

Same Fraunhofer FDK 2.0.3 family as native `lab/fdk` (TASK-7). Do **not**
count this port as a second independent FDK oracle. Label any later
timing/quality cell `fdk-aac-rust-0.2.3` (Rust port), never `FDK`.

## Smoke (rust-only `AacLcDecoder` / `ConfiguredPureRustEncoder`)

| Cell | Result |
|---|---|
| LC decode `sine48.adts` | **ok** 48000 Hz, 1 ch, 13 frames, 13312 samples, finite |
| LC decode `tns48.adts` | **ok** 48000 Hz, 1 ch, 1024 samples |
| HE-ADTS `he48.adts` via `from_adts_header` | **core-only** 24000 Hz, 1 ch, 9216 samples — not 2× HE |
| HE-v2 ADTS `ps48.adts` via `from_adts_header` | **core-only** 24000 Hz, 1 ch, 26624 samples — not PS stereo |
| LC encode 1024 zeros @ 48 kHz 128 kbps ADTS | **ok** 13-byte frame, encoder_delay=2048 |
| Default `ffi` feature | **blocked** (C++ / `g++`, same as TASK-7) |

syom `decode` of `sine48.adts` is also 48000 Hz / 13312 samples. HE/PS
goldens are 2× at 48 kHz in syom; this helper is LC-core of those
bitstreams. Full-band HE/PS on the rust-only ADTS helper is an
**unavailable** cell here.

## Wrapper / build cost

- rust-only `cargo build --release` of this smoke: ~9 s after crate fetch
  (fdk-aac-rust compile), no C++.
- Default `ffi` would compile `fdk-aac-sys` + C++ FDK (not measured).
- Runtime: in-process iterator over ADTS frames (`decode_adts_frame_f32`).
- Network: crates.io for the crate; GitHub or `FDK_AAC_SOURCE_DIR` for tables.

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` | **no-go** (Fraunhofer license, build-time git fetch) |
| Ordinary `cargo test` | **no-go** |
| Independent FDK oracle | **no-go** (shared 2.0.3 lineage with TASK-7) |
| Named competitor cell (Rust FDK port) | **go** once a matched-output HE path exists; LC decode/encode smoke is recorded |
| Performance vs native FDK | **no-go** until ffi or a documented rust-only HE path is comparable |
