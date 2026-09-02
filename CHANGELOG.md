# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Product is a zero-dep AAC library (`cargo add syom`), not a 16 kHz
  extract CLI. WAV-in, MP4 PCM, resample, clap, tracing, thiserror, and
  rustfft are gone. Output is planar `f32` at native rate. Caps:
  `DecodeOptions::speech()` / `unbounded()`.
- LC decode is f32 from spectrum through IMDCT/OLA. ICS/spectrum
  buffers are reused across frames. On the committed LC fixtures,
  in-process wall time is ≤ Symphonia and ≪ rusty_aac; allocs and
  peak RSS are ≤ both linked decode peers (`BENCH.md`).

### Added

- HE-AAC v1/v2 (SBR/PS) and LATM/LOAS on the same decode path. Committed
  lavc native goldens (max abs ≤ 1 LSB, SNR ≥ 70 dB) for lecture M4A,
  44.1 M4A, ADTS sine, TNS, PNS-heavy LC, HE ADTS/M4A, and LATM.
  Runtime does not spawn ffmpeg.
- Owned IMDCT/FFT (no rustfft). PNS polarity matches lavc PCM.

### Fixed

- MP4 audio `elst.media_time` is honoured (AAC encoder delay).
- Implicit SBR `0x2b7` probe requires `sbrPresentFlag == 1` so LC ASC
  padding `56 e5 00` is not treated as HE.
- `speech()` / `decode()` apply SBR on HE-AAC. The mono fast path no
  longer returns core-rate silence.
- `unbounded()` / Split emits two planes on mono LC+SBR (lavc implicit
  HE-AACv2 PS), including LATM/LOAS. Goldens are interleaved lavc s16.
- After SBR is active, a malformed SBR fill is `Err`, not dropped.
- Mono mix-down after SBR/PS is one rule (ADTS, M4A, LATM). LATM
  honours the duration cap while decoding, not after the whole PCM.

## [0.2.0] - 2026-09-01

### Added

- MP4 PCM sound tracks (`sowt`, `twos`, `ipcm`, `lpcm`, `raw `) extract to
  16 kHz s16le mono. Other sample fourccs stay `Media`.

## [0.1.0] - 2026-09-01

### Added

- CLI `syom FILE -o OUT`: MP4/M4A/ADTS AAC and PCM WAV to 16 kHz s16le
  mono (WAV or raw `.pcm`). AAC engine vendored from oxideav-aac (MIT)
  via kover-aac / gigastt-aac; rustfft IMDCT. This repo is the AAC
  home: kover and sluh spawn the binary; they do not link the decoder.

[Unreleased]: https://github.com/ekhodzitsky/syom/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ekhodzitsky/syom/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/ekhodzitsky/syom/releases/tag/v0.1.0
