# syom

just aac.

Pure-Rust **AAC-LC / HE-AAC** codec. Zero crates on the product path.
Decode ADTS, LATM/LOAS, and M4A/ISOBMFF `mp4a`; **encode** AAC-LC to ADTS
or M4A. PCM is planar `f32` at the bitstream's native sample rate
(HE = 2× core).

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

Encoding: `syom::encode(&planes, 48_000)?` gives an ADTS stream (AAC-LC,
mono/stereo, 128 kbps). `encode_with` takes `EncodeOptions`:
`EncodeContainer::Adts` (default) or `M4a`, and `with_bitrate_bps`.
`syom::write("clip.m4a", &planes, 48_000, ...)` via `write_with`.
The encoder is long-window LC (no block switching yet), KBD analysis, a
Bark-spreading psy model with flat 18 dB SMR, per-frame M/S, and a
CBR-ish rate loop. The committed lavc goldens prove ffmpeg decodes the
output bit-exact-close (≤ 1 LSB s16 vs our own decode).

Streaming: `Decoder::new(opts)` + `feed(chunk, |frame| ...)` for ADTS/LATM
byte streams (frames may straddle chunks; M4A is rejected — `moov` needs
random access), or `decode_streaming(bytes, &opts, cb)` for any in-memory
container. Each callback gets one AAC `Frame` of borrowed planar f32 (valid
for the callback only; return `Err` to abort) and `finish` yields
`StreamInfo` tallies. Peak PCM RAM is one frame. Encode has the mirror
shape: `Encoder::new(rate, channels, &opts)` + `feed(planes, |frame| ...)`
takes PCM chunks of any size and fires per ADTS-wrapped access unit
(byte-exact with one-shot `encode_with`; M4A rejected — `stco` needs
finish-time sizes), `finish` encodes the zero-padded tail and yields
`EncodeInfo`.

Channels: mono, stereo, and multichannel AAC-LC 3.0 / 4.0 / 5.0 / 5.1
(`channel_configuration` 3–6) plus in-band PCE streams. Split mode emits one
plane per channel in the libavcodec layout order — 5.1 is FL FR FC LFE BL BR —
or PCE declaration order (front, side, back, LFE) for PCE streams. Speech
mono is the arithmetic mean of the non-LFE planes (stereo reduces to
`0.5·(L+R)`).

Correctness vs FFmpeg libavcodec native s16: max abs ≤ 1 LSB, SNR ≥ 70 dB
on committed goldens (LC lecture / 44.1 / ADTS / TNS / PNS, LC 3.0–5.1,
HE ADTS / M4A, LATM; plus encoder output decoded by lavc). Runtime does
not spawn ffmpeg.

No resample.

## License

MIT. AAC-LC engine is original (ISO/IEC 14496-3 / 13818-7). See
[NOTICE](NOTICE) for the AAC patent disclaimer (Via LA).
