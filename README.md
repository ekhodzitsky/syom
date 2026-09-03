# syom

just aac.

Pure-Rust **AAC-LC / HE-AAC** decoder. Zero crates on the product path.
Read ADTS, LATM/LOAS, and M4A/ISOBMFF `mp4a`. Output is planar `f32` at the
bitstream's native sample rate (HE = 2× core).

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![deps](https://img.shields.io/badge/deps-zero-success.svg)](Cargo.toml)

## Why

[symphonia](https://github.com/pdeljanov/Symphonia) is a multi-format
pipeline. [rusty_aac](https://crates.io/crates/rusty_aac) is LC (SBR
signalled, not reconstructed). [oxideav-aac](https://crates.io/crates/oxideav-aac)
on crates.io is a parser. **syom is the AAC crate**: one call, LC + HE v1/v2,
hard RAM/duration caps, no extra dependencies.

| | syom | rusty_aac | symphonia | oxideav-aac 0.1 |
|---|---|---|---|---|
| AAC-LC | yes | yes | yes | parse only |
| HE-AAC v1/v2 (SBR/PS) | yes | signalled | no | tables |
| ADTS / M4A / LATM | yes | ADTS + AU | via formats | ADTS parse |
| Default deps | **none** | none | several | oxideav-core |
| Output | planar `f32`, native rate | interleaved | packets | — |
| Duration / RAM caps | yes (`speech` / `unbounded`) | no | no | — |
| One-call `decode(&[u8])` | yes | no | no | no |

## Install

```toml
[dependencies]
syom = "0.3"
```

Requires **Rust 1.97**, edition 2024.

## Quick start

```rust
fn main() -> syom::Result<()> {
    assert!(syom::decode(&[]).is_err());
    assert!(!syom::sniff_aac(b"ID3"));
    let speech = syom::DecodeOptions::speech();
    assert_eq!(speech.channel_mode, syom::ChannelMode::Mono);
    let _ = syom::DecodeOptions::unbounded();
    Ok(())
}
```

From a path: `syom::read("clip.m4a")?`. Caps:
`decode_with(bytes, &DecodeOptions::speech().with_channel_mode(syom::ChannelMode::Split))`.

Correctness vs FFmpeg libavcodec native s16: max abs ≤ 1 LSB, SNR ≥ 70 dB
on committed goldens (LC lecture / 44.1 / ADTS / TNS / PNS, HE ADTS / M4A,
LATM). Runtime does not spawn ffmpeg.

Encode is not v1. No resample.

## License

MIT. AAC-LC engine is original (ISO/IEC 14496-3 / 13818-7). See
[NOTICE](NOTICE) for the AAC patent disclaimer (Via LA).
