# syom

AAC to planar `f32`.

[![crates.io](https://img.shields.io/crates/v/syom.svg)](https://crates.io/crates/syom)
[![docs.rs](https://img.shields.io/docsrs/syom)](https://docs.rs/syom)
[![CI (main)](https://github.com/ekhodzitsky/syom/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/ekhodzitsky/syom/actions/workflows/ci.yml?query=branch%3Amain)
[![Rust 1.89+](https://img.shields.io/badge/MSRV-1.89-blue.svg)](https://github.com/ekhodzitsky/syom/blob/main/Cargo.toml)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/ekhodzitsky/syom/blob/main/LICENSE)

An AAC codec in Rust. Decode LC, HE-AAC v1/v2 and AAC-LD from files,
memory or streams; encode LC, HE v1/v2 and LD. ADTS, M4A, LATM/LOAS and
bounded fragmented MP4. Output is one `f32` plane per channel at the
stream's native sample rate. No runtime dependencies or native codec
installation. No resampling.

<a id="quick-start"></a>

## Examples

After [installing syom](#install), generate a tone, encode it and decode it:

```rust
fn main() -> syom::Result<()> {
    let pcm: Vec<f32> = (0..48_000)
        .map(|i| 0.3 * (i as f32 * 0.06).sin())
        .collect();
    let adts = syom::encode(&[&pcm], 48_000)?;
    let audio = syom::decode_with(&adts, &syom::DecodeOptions::audio())?;
    assert_eq!(audio.channels[0].len(), pcm.len());
    assert_eq!((audio.channels.len(), audio.sample_rate), (1, 48_000));
    println!("{} channel at {} Hz", audio.channels.len(), audio.sample_rate);
    Ok(())
}
```

Expected output: `1 channel at 48000 Hz`.

**Read an existing file.** `DecodeOptions::audio()` preserves coded channels.
The shorter `read()` and `decode()` helpers use a bounded speech mono mix.

```rust,no_run
fn main() -> syom::Result<()> {
    let audio = syom::read_with("in.m4a", &syom::DecodeOptions::audio())?;
    println!("{} channels at {} Hz", audio.channels.len(), audio.sample_rate);
    Ok(())
}
```

**Decode chunks as they arrive.** This example supplies its own tagged ADTS.
The callback borrows PCM; copy samples you need to retain.

```rust
fn main() -> syom::Result<()> {
    let adts = syom::encode(&[vec![0.1f32; 1124]], 48_000)?;
    let mut decoder = syom::Decoder::new(syom::DecodeOptions::audio());
    let mut samples = 0usize;
    for chunk in adts.chunks(127) {
        decoder.feed(chunk, |frame| {
            samples += frame.samples;
            Ok(())
        })?;
    }
    let info = decoder.finish(|frame| {
        samples += frame.samples;
        Ok(())
    })?;
    assert_eq!(samples, 1124);
    assert_eq!(info.samples, samples as u64);
    Ok(())
}
```

Push decode takes ADTS, LATM/LOAS and fragmented MP4. Flat M4A needs random
access: use `decode_seek_streaming` or `M4aSeek`. Push encode uses
`Encoder::feed`.
[API choices and limits](https://github.com/ekhodzitsky/syom/blob/main/docs/usage-and-validation.md).

## Decode

| Profile | ADTS | M4A | LATM/LOAS | Bounded fMP4 |
|---|---|---|---|---|
| AAC-LC | Yes | Yes | Yes | Yes |
| HE-AAC v1/v2 | Yes | Yes | Yes | Yes |
| AAC-LD, 512/480 samples | No | Yes | Yes | ASC path; no separate LD fMP4 golden |

Mono through 7.1; no seven-plane layout. Speech mode mixes non-LFE channels.
960-sample LC is supported in M4A, LATM/LOAS and raw access units.
Raw AUs need an AudioSpecificConfig supplied by the caller.
Encrypted media, DRM, AAC-ELD, USAC and multi-track fMP4 are outside scope.
[Format details](https://github.com/ekhodzitsky/syom/blob/main/docs/usage-and-validation.md#formats-and-channel-layout).

## Encode

LC defaults to 128 kbps ABR with per-band M/S. Quality VBR, HE v1/v2,
AAC-LD and surround LC are opt-in. Output: ADTS, M4A or LATM/LOAS;
push encoding can return raw AUs. No fragmented-MP4 encoding.
AAC-LD encode uses 512-sample frames and cannot use ADTS.
[Encoding guide](https://docs.rs/syom/0.10.3/syom/guide/).

<a id="choosing-a-library"></a>

## Compared with

Pinned integration snapshot; peer versions were reviewed on 2026-11-02.

| Library | Integration boundary | HE v1/v2 decode |
|---|---|---|
| syom 0.10.3 | Built-in AAC containers; planar `f32` | Yes |
| rusty_aac 2.0.1 | Caller demuxes M4A; interleaved `f32` | Yes |
| Symphonia 0.6.1 | Multi-format media pipeline | LC core only |
| oxideav-aac 0.1.7 | AAC codec using oxideav-core; interleaved `i16` | Yes |

These boundaries affect container handling, channel layout and buffer ownership.
[Comparison and primary sources](https://github.com/ekhodzitsky/syom/blob/main/docs/comparison.md)
describe the pinned versions and limitations, including LD support.

<a id="measured-decoding-time"></a>

## Decoding measurements

Historical **syom 0.8.0** medians, not measurements of 0.10.3. Lower is better.

| ADTS workload | syom 0.8.0 | rusty_aac 2.0.1 | Symphonia 0.6.1 |
|---|---:|---:|---:|
| LC mono, 48 kHz, 13312 samples | 59.6 µs | 152.3 µs | 135.2 µs |
| HE v2 stereo, 48 kHz, 53248 samples | 1.211 ms | 1.120 ms | Core-only; excluded |

Linux x86_64, Ryzen AI 9 HX 370, rustc 1.97.1, release thin LTO, one core;
8 warmups and 101 calls, 2026-10-28. Matched planar output shape; checksums
differ. The busy-host results vary by profile and establish no quality ranking.
[Method and all cells](https://github.com/ekhodzitsky/syom/blob/main/docs/comparison.md#historical-decoding-measurements) ·
[Older benchmarks](https://github.com/ekhodzitsky/syom/blob/main/BENCH.md).

## Install

```toml
[dependencies]
syom = "0.10"
```

Rust **1.89+**. Version: **0.10.3**. No runtime crates, build script
or native codec backend.

For a new project:

```sh
cargo new syom-demo --bin
cd syom-demo
cargo add syom@0.10.3
```

Put an example above in `src/main.rs` and run `cargo run --release`.
The file-reading example needs your own `in.m4a`; the other two generate
their own input.

<a id="status-and-limits"></a>

## Scope

**0.10.3 accepts fractional M4A edit durations**, including ffmpeg's
millisecond movie clock at 44.1 kHz. Small rounded overhangs are capped
at the available media; missing packets remain errors.

**0.10.2 fixes standard Apple iTunSMPB handling** and aligns decoding,
probe and presentation seeking for tag-only M4A. Regenerate tags written
with syom's older incorrect field order. [Release verification](https://github.com/ekhodzitsky/syom/blob/main/docs/verification.md#release-and-checkout).

Authenticate persistent LC checkpoints against the immutable source,
options and exact build. Recreate cached blobs when upgrading from an earlier version.
Tag-only sources remain checkpoint-ineligible. Exact seeking can replay
the coded prefix.
[Checkpoint contract](https://github.com/ekhodzitsky/syom/blob/main/docs/persistent-checkpoints.md).

CI exercises Linux x64/ARM64, macOS ARM64, Windows x64 and Rust 1.89.
Native Apple AAC-LC gapless smoke is recorded for mono and stereo.
Physical-mobile runtime, full ISO conformance and listening qualification
remain open. Encoder quality leadership is unproven.
[Validation and dated coverage](https://github.com/ekhodzitsky/syom/blob/main/docs/verification.md).

[Documentation](https://github.com/ekhodzitsky/syom/blob/main/docs/README.md) ·
[API guide](https://docs.rs/syom/0.10.3/syom/guide/) ·
[Contributing](https://github.com/ekhodzitsky/syom/blob/main/CONTRIBUTING.md) ·
[Changelog](https://github.com/ekhodzitsky/syom/blob/main/CHANGELOG.md) ·
[Issues](https://github.com/ekhodzitsky/syom/issues) ·
[MIT](https://github.com/ekhodzitsky/syom/blob/main/LICENSE) ·
[AAC patent notice](https://github.com/ekhodzitsky/syom/blob/main/NOTICE)
