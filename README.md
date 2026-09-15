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
| HE-AAC v1/v2 (SBR/PS) decode | yes | signalled | LC core only | ADTS SBR/PS |
| HE-AAC v1 encode | opt-in (`with_he`) | no | no | advertised |
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
    assert!(matches!(
        syom::probe(&[]).unwrap_err(),
        syom::AacError::NeedMore { .. }
    ));
    let speech = syom::DecodeOptions::speech();
    assert_eq!(speech.channel_mode, syom::ChannelMode::Mono);
    let audio = syom::DecodeOptions::audio();
    assert_eq!(audio.channel_mode, syom::ChannelMode::Split);
    let _ = syom::DecodeOptions::unbounded();
    Ok(())
}
```

Errors are matchable without scraping `Display` (`AacError` is
`#[non_exhaustive]` — keep a `_` arm):

```rust
fn kind(e: syom::AacError) -> &'static str {
    match e {
        syom::AacError::NotAac => "not-aac",
        syom::AacError::Unsupported(_) => "unsupported",
        syom::AacError::Truncated { .. } => "truncated",
        syom::AacError::NeedMore { .. } => "need-more",
        syom::AacError::Malformed(_) => "malformed",
        syom::AacError::Limit { .. } | syom::AacError::TooLong { .. } => "limit",
        syom::AacError::InvalidPcm(_) | syom::AacError::InvalidLimits(_) => "invalid",
        syom::AacError::Lifecycle { .. } => "lifecycle",
        _ => "other",
    }
}
assert_eq!(kind(syom::decode(&[]).unwrap_err()), "not-aac");
```

`probe` / `probe_with` inspect container, profile, rates, and layout
without decoding PCM. ADTS/LATM duration is unknown (VBR); M4A `elst`
is exact. Then `decode` as usual.

From a path: `syom::read("clip.m4a")?` (speech-mono, 2 h). Keep coded
layout with lecture caps: `decode_with(bytes, &DecodeOptions::audio())`.
No duration/rate ceiling: `DecodeOptions::unbounded()`.
Streaming `Frame::meta` (`Copy`) names planes (5.1 = FL FR FC LFE BL BR)
and core vs output rate; ADTS priming stays `None`.

Encoding: `syom::encode(&planes, 48_000)?` gives an ADTS stream (AAC-LC,
mono/stereo, 128 kbps). `bitrate_bps` is an ABR target (payload/valid
±3% on ≥10 s non-silent tracks; leftover budget is unused bytes after
`ID_END`; silence may undershoot) with
`adts_buffer_fullness = 0x7FF` (no CBR reservoir). `encode_with` takes
`EncodeOptions`:
`EncodeContainer::Adts` (default) or `M4a`, `with_bitrate_bps`,
`with_lookahead` (one-frame attack lookahead: better pre-echo suppression
on early-in-frame onsets, one extra frame of latency, default off), and
`with_ath` (Terhardt absolute-threshold floor, default off; TASK-68
no-go as 0.x default), and `with_tonality` (Johnston SFM `target_q`
scale, default off; TASK-69 no-go as 0.x default), `with_short_tns`
(per-window short TNS, default off; TASK-72 no-go as 0.x default), and
`with_short_group` (short-window grouping, default off; TASK-71 no-go as
0.x default), and `with_band_refine` (leftover-bit band scalefactor
refine, default off; TASK-74 no-go as 0.x default), and `with_pns`
(perceptual noise substitution, default off; TASK-75 no-go as 0.x
default), and `with_intensity` (intensity stereo, default off; TASK-76
no-go as 0.x default), and `with_he` (HE-AAC v1: SBR on an LC core at
half the input rate; 16–48 kHz input, mono/stereo, `bitrate_bps` is the
whole-stream budget; ADTS signals SBR implicitly, M4A / `Encoder::asc`
carry the explicit two-rate config; priming 3018 output samples; default
off — `encode` stays LC).
`syom::write("clip.m4a", &planes, 48_000, ...)` via `write_with`.
`with_quality(0..=10)` switches to quality VBR (fixed allowed noise, no
target rate; LC only; 6 dB per level, level 5 = the psy target).
Presets (settings, not claims; `lab/quality/CURVES.md`):
`EncodeOptions::default()` LC 128 kbps causal, `high_quality()` LC
192 kbps, `low_rate()` HE-AAC v1 48 kbps for full-band content.
The encoder is LC with block switching (an attack detector walks
OnlyLong → LongStart → EightShort → LongStop on transients), KBD
analysis, a Bark-spreading psy model (18 dB SMR) whose masked thresholds
set every band's quantizer step (noise-to-mask allocation; under a bit
shortage a water level drops the quietest noise-like bands first),
**per-band** M/S, long-frame TNS, and an ABR rate loop. Optional
`with_lookahead(true)` is off by default. Committed lavc goldens
(`src/goldens/enc48{,m,t,l}.*`) show ffmpeg decodes the output within
≤ 2 LSB s16 / ≥ 55 dB (typically 1 LSB / ~80 dB). Tests never spawn
ffmpeg.

Streaming: `Decoder::new(opts)` + `feed(chunk, |frame| ...)` for ADTS/LATM
byte streams (frames may straddle chunks; M4A is rejected — `moov` needs
random access), `Decoder::from_asc(asc, opts)` + `decode_au(payload, cb)`
when a demuxer already has AudioSpecificConfig and complete access units,
`decode_read` / `decode_read_streaming` for a generic `std::io::Read`
(ADTS/LOAS), `decode_seek` / `decode_seek_streaming` for seekable M4A
(`moov` plus one access unit; `mdat` is not slurped),
`M4aSeek::seek` for presentation-sample access with codec preroll,
or `decode_streaming(bytes, &opts, cb)` for any in-memory container.
Each callback gets one AAC `Frame` of borrowed planar f32 (valid
for the callback only; return `Err` to abort) and `finish` yields
`StreamInfo` tallies. A parser, limit, or callback error **fails** the
instance; a successful `finish` **finishes** it; further `feed`/`finish`
error until `reset()`, which keeps prepared workspace capacity
(filterbank slots, spectral/PCM planes, encoder KBD/psy) and does not
use a global cache. After warmup, speech and stereo borrowed-frame
callbacks do not allocate. `finish` takes `&mut self`. Peak PCM RAM is
one frame. Encode has the mirror shape: `Encoder::new` plus `feed` takes PCM
chunks of any size and fires per ADTS-wrapped access unit (byte-exact
with one-shot `encode_with`; M4A rejected — `stco` needs finish-time
sizes), `finish` encodes the zero-padded tail and yields `EncodeInfo`.
The same open / failed / finished / `reset` contract applies; counters
include a frame already handed to a callback that then returned an error.

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
