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
pipeline. [rusty_aac](https://crates.io/crates/rusty_aac) 0.5 is LC (SBR
signalled, not reconstructed). [oxideav-aac](https://crates.io/crates/oxideav-aac)
0.1.7 decodes ADTS LC with SBR/PS (no ISOBMFF demux). **syom is the AAC
crate**: one call, LC + HE v1/v2, duration/rate caps, no extra dependencies.

| | syom | rusty_aac 0.5 | symphonia 0.6 | oxideav-aac 0.1.7 |
|---|---|---|---|---|
| AAC-LC | yes | yes | yes | yes (ADTS) |
| HE-AAC v1/v2 (SBR/PS) | yes | signalled | LC core only | ADTS SBR/PS |
| ADTS / M4A / LATM | yes | ADTS + AU | via formats | ADTS only |
| Default deps | **none** | none | several | oxideav-core |
| Output | planar `f32`, native rate | interleaved f32 | packets | interleaved i16 |
| Duration / rate caps | yes (`speech` / `unbounded`) | no | no | — |
| One-call `decode(&[u8])` | yes | no | no | `decode_all` |

Wall-time tables in [BENCH.md](BENCH.md) are **historical unequal-work
rows** (speech mix vs discarded or LC-core output). They are not a
matched-PCM leaderboard; see that file and `syom::decode_cmp`.

## Install

```toml
[dependencies]
syom = "0.6"
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
`EncodeContainer::Adts` (default) or `M4a`, `with_bitrate_bps`, and
`with_lookahead` (one-frame attack lookahead: better pre-echo suppression
on early-in-frame onsets, one extra frame of latency, default off).
`syom::write("clip.m4a", &planes, 48_000, ...)` via `write_with`.
The encoder is LC with block switching (an attack detector walks
OnlyLong → LongStart → EightShort → LongStop on transients), KBD
analysis, a Bark-spreading psy model with flat 18 dB SMR, **per-band**
M/S, long-frame TNS, and a CBR-ish rate loop. Optional
`with_lookahead(true)` is off by default. Committed lavc goldens
(`src/goldens/enc48{,m,t,l}.*`) show ffmpeg decodes the output within
≤ 2 LSB s16 / ≥ 55 dB (typically 1 LSB / ~80 dB). Tests never spawn
ffmpeg.

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
