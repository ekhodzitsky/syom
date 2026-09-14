# TASK-18: API integration cost and deployment footprint

Measured 2026-09-14. Host: Linux 7.0.0-31-generic x86_64, 24 threads.
`rustc`/`cargo` **1.97.1**. Numbers are one-run lab values, not a
leaderboard. Peers are identified by crate version, not by invented
quality scores.

## Equal scenarios (concepts / copies / setup)

Count is *named types + required calls*, not source lines.

| Scenario | syom 0.6.0 | rusty_aac 0.5.0 | oxideav-aac 0.1.7 | symphonia 0.6 AAC |
|---|---|---|---|---|
| Bytes → PCM | 1 call: `decode(&[u8])` → owned planar `f32` | `AacDecoder::new` + per-packet `decode` + collect interleaved f32 | `oxideav-core` runtime + register AAC + `decode_all` (ADTS) | Probe + FormatReader + CodecRegistry + packet loop |
| File → PCM | 1 call: `read(path)` | open + read-all + decoder loop | same as bytes after load | MediaSourceStream + probe |
| Stereo kept | extra: `decode_with` + `DecodeOptions::unbounded()` (default `speech()` is **mono**) | decoder output channels as coded | as coded | as coded |
| Bounded streaming | `Decoder::new` + `feed` + `finish` (3); callback borrows one frame | packet-at-a-time decoder; caller owns buffer | not a byte-stream push API | incremental format reader |
| Raw network AU + ASC | **gap** (TASK-52) | `AacDecoder::with_config` / `with_config_bytes` | ASC parse exists; full AU decode is the crate’s engine path | codec params from demux |
| Encode to memory | 1 call: `encode(&planes, rate)` ADTS 128 k | `AacEncoder` + `AacEncoderConfig` + pull packets + ADTS writer | encoder modules; not a one-call ADTS helper | no AAC encode |
| Encode to file | `write` / `write_with` | caller writes packets | caller writes | n/a |
| Errors | `Result<T, AacError>` (`Display`) | crate `Error` | crate `Error` | `symphonia_core::errors::Error` |

**Copies (syom):** one-shot decode owns `Vec<Vec<f32>>`. Streaming
`Frame` is borrowed for the callback only. Encode takes `&[Vec<f32>]`
(one extra owned layer vs `&[&[f32]]` — TASK-55).

## Footprint (AAC-only product path)

| Item | Value |
|---|---|
| Product `[dependencies]` | **empty** |
| Declared `rust-version` | 1.97 (edition 2024). Peers: rusty_aac 1.85, oxideav-aac 1.80, symphonia 1.85 |
| `cargo build --release` of syom lib | 1.94 s (already-warm target dir) |
| `libsyom.rlib` | 3 796 632 B (includes ~2.0 MiB rustc `lib.rmeta`) |
| Linked `.o` text | 671 403 B (655.7 KiB) |
| Linked `.o` data | 208 112 B (203.2 KiB) |
| Minimal consumer (decode golden + encode 2048 zeros) | first `--release` **1.97 s**, RSS 366 648 KiB; incremental 0.02 s |
| Consumer ELF (unstripped, pie) | 1 393 584 B; text 981 136; data 222 328; bss 2 248 |

`[profile.bench]` LTO does **not** apply to this lib/consumer
measurement (default `release`). Re-run with explicit lto if a
distribution binary is the cell.

## Usability gaps (ranked)

1. **Default `DecodeOptions::speech()` is mono.** Music/stereo
   preservation needs `unbounded()` or `with_channel_mode(Split)`.
   Call site: `decode_with(bytes, &DecodeOptions::unbounded())`.
2. **No public raw AU + ASC entry** (rusty_aac has `with_config_bytes`).
3. **Push `Decoder` rejects M4A** (`moov` needs random access).
4. **Encode input is `&[Vec<f32>]`**, not borrowed slices.
5. **Rust 1.97 / edition 2024** is a harder floor than oxideav-aac 1.80.

Hidden `decode_cmp` / `encode_cmp` / `mem_iso` are bench-only
(`#[doc(hidden)]`); they are not consumer concepts.

## Go / no-go

| Proposal | Decision | Why |
|---|---|---|
| Keep empty product `[dependencies]` | **go** | Measured: rlib has no third-party linkage; consumer builds with path-dep only. |
| Change `speech()` default to Split / “full fidelity” | **no-go** | Decision D3: keep current defaults until a versioned API task (TASK-50). Document Split as the stereo call site. |
| Drop HE from the default crate to shrink text | **no-go** | Would split the product; not justified by this footprint cell. |
| Public AU+ASC decode | **go later** | TASK-52. Real gap vs rusty_aac for muxers. |
| Borrowed encode planes | **go later** | TASK-55. One extra copy on the encode path. |
| Streaming M4A | **go later** | TASK-57. |
| Lower MSRV below 1.97 | **no-go now** | Edition 2024 is declared; not a silent downgrade. |

Re-measure after a default or MSRV change. Do not treat rlib byte
counts as a CI gate (they include rustc metadata and move with
codegen).
