# syom

AAC to planar `f32`.

[![crates.io](https://img.shields.io/crates/v/syom.svg)](https://crates.io/crates/syom)
[![docs.rs](https://img.shields.io/docsrs/syom)](https://docs.rs/syom)
[![ci](https://github.com/ekhodzitsky/syom/actions/workflows/ci.yml/badge.svg)](https://github.com/ekhodzitsky/syom/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

An AAC codec in Rust. AAC-LC, HE-AAC v1/v2, and AAC-LD. ADTS, LATM/LOAS,
M4A, and bounded fragmented MP4. Encode defaults to AAC-LC at 128 kbps,
ADTS or M4A. HE and LD are opt-in. Output is planar `f32` at the
bitstream rate (HE is twice the core rate). The product path pulls no
crates. ffmpeg is an offline oracle, not a runtime dependency.

## Examples

```rust
fn main() -> syom::Result<()> {
    let pcm: Vec<f32> = (0..48_000).map(|i| 0.3 * (i as f32 * 0.06).sin()).collect();
    let adts = syom::encode(&[&pcm], 48_000)?;
    let back = syom::decode_with(&adts, &syom::DecodeOptions::audio())?;
    assert_eq!((back.channels.len(), back.sample_rate), (1, 48_000));
    Ok(())
}
```

`decode` is the speech default (mono mix, 2 h at up to 48 kHz; higher
rates keep that sample budget). `DecodeOptions::audio()`
keeps the coded channels. From a path: `syom::read`. Push decode and
encode: `Decoder::feed`, `Encoder::feed`. The
[guide](https://docs.rs/syom/latest/syom/guide/) has one compiling
example per task.

| task | section |
|---|---|
| one-call encode / decode, speech vs full fidelity | 1 |
| borrowed PCM, `Read` / `Write` / `Seek` | 2 |
| priming, tail, exact length | 3 |
| push `Encoder` / `Decoder`, bounded memory | 4 |
| raw access units and `AudioSpecificConfig` | 5 |
| probe, sample-exact M4A seek | 6 |
| LC, quality VBR, HE v1/v2, surround | 7 |
| limits and typed errors | 8 |
| profiles and containers | 9 |
| behaviour changes since 0.6.0 | 10 |

## Decode and encode

| | ADTS | M4A | LATM/LOAS | fMP4 |
|---|---|---|---|---|
| AAC-LC decode | yes | yes | yes | yes |
| HE-AAC v1/v2 decode | yes | yes | yes | yes |
| AAC-LD decode (512) | no | yes | yes | yes |
| LC encode | yes | yes | yes | no |
| HE v1/v2 encode | yes | yes | yes | no |
| AAC-LD encode | no | yes | yes | no |

LC encode is the default; M/S is per-band. `with_he`, `with_he_v2`, and
`with_ld` are opt-in. ADTS cannot signal AAC-LD. Fragmented MP4 uses the
same ASC path as M4A, so AAC-LD decodes there too; there is no separate
LD fMP4 golden. Surround LC is 3, 4, 5, 6, or 8 planes
(`channel_configuration` 3–7). Seven planes have no layout.
No resample.

## Compared with Rust AAC libraries

Capabilities below refer to the pinned published versions, not their main
branches. Raw-AU support does not imply a built-in M4A demuxer.

| | syom 0.8.0 | rusty_aac 2.0.1 | symphonia 0.6.1 | oxideav-aac 0.1.7 |
|---|---|---|---|---|
| Job | AAC codec | AAC codec | media pipeline | AAC codec |
| Runtime deps | none | none | several | oxideav-core |
| HE-AAC v1/v2 decode | yes | yes | LC core only | yes |
| AAC-LD decode | yes | yes | no | yes, with documented TNS limitations |
| Built-in M4A demux | yes | caller supplies raw AUs | yes | caller supplies raw AUs |
| LATM/LOAS decode | yes | yes | no built-in LATM reader | yes |
| Encode profiles | LC, HE v1/v2, LD | LC | none | LC, HE v1 |
| Decoded output | planar f32 | interleaved f32 | audio buffers | interleaved i16 |

Sources: [rusty_aac](https://docs.rs/crate/rusty_aac/2.0.1),
[Symphonia](https://github.com/pdeljanov/Symphonia),
[oxideav-aac](https://docs.rs/crate/oxideav-aac/0.1.7).

## Speed

Linux x86_64, AMD Ryzen AI 9 HX 370, rustc 1.97.1, `taskset -c 0`,
release thin LTO, 8 warmup + 101 measured reps, 2026-10-28.
Same bitstream and collected planar f32 shape; adapters include layout
conversion. Checksums differ; matching shape is not a quality score.
Median wall times from the Rust fast-peer pass:

| Workload | syom 0.8.0 | rusty_aac 2.0.1 | symphonia 0.6.1 |
|---|---|---|---|
| LC ADTS, 48 kHz mono, 13312 samples | 59.6 µs | 152.3 µs | 135.2 µs |
| HE ADTS, 48 kHz stereo, 18432 samples | 190.8 µs | 354.9 µs | core-only, excluded |
| HE-AAC v2 ADTS, 48 kHz stereo, 53248 samples | 1.211 ms | 1.120 ms | core-only, excluded |
| LC 5.1 ADTS, 48 kHz, 20480 samples | 601.3 µs | 678.3 µs | decoder unavailable |

This busy-host sample favors syom on LC/HE v1 and rusty_aac on HE v2;
it is not a universal speed ranking. The separate four-peer run has
substantial scheduling noise. Both runs, exact inputs, protocol and
limitations are in the [release audit](lab/release-audit/REPORT.md).
[BENCH.md](BENCH.md) retains historical results against older peers.
Encoder quality superiority is not established; the earlier in-process
LC encode comparison was slower than FDK (see the audit).

## Production scope

The local [0.8.1 candidate](lab/release-audit/CANDIDATE-0.8.1.md) passes
19 cases from 16 recordings against FFmpeg and the actual gigastt decode
adapter. It fixes padded-LC allocations and PNS history during M4A seek.
The [paired HTTP ASR canary](lab/release-audit/HTTP-CANARY-0.8.1.md)
matches normalized baseline transcripts and durations on 32 speech-format cases.
The [local load/fault qualification](lab/release-audit/HTTP-LOAD-0.8.1.md)
also passes 224 measured requests per binary at 1/2/4/8 clients, queue and
watchdog timeouts, and recovery after client cancellation.
Cold or stateful seeks can replay the prefix; known noise-free LC/LD
prefixes retain short preroll. Tagged/untagged LC ABR/CBR consumer checks
allocate zero times after warmup. Encoder listening, physical mobile,
full ISO conformance and live-service rollout qualification remain open. Use
`DecodeOptions::audio()` to keep channels; `decode()` defaults to mono.

## Install

```toml
[dependencies]
syom = "0.8"
```

rustc **1.89**.

[guide](https://docs.rs/syom/latest/syom/guide/) | [BENCH.md](BENCH.md) | [CHANGELOG](CHANGELOG.md)

No resample. MIT. AAC patents: [NOTICE](NOTICE).
