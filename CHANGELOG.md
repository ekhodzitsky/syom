# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- PS first-envelope H-matrix interpolation now starts one slot before the
  frame (`(n + 1)/(n_0 + 1)`, Annex 8.A / §8.6.4.6.4) instead of at slot 0
  (`n/n_0`). HE-AACv2 output now matches lavc within 1 LSB on real stereo
  content (was up to 11 LSB / 67 dB SNR whenever the stereo cues moved;
  dual-mono fixtures could not see it).

### Added

- `goldens/ps48.*` HE-AACv2 fixture with real stereo content (440 Hz L /
  880 Hz R, explicit in-band `EXTENSION_ID_PS`) plus lavc s16 goldens for
  the M4A (elst-trimmed) and ADTS containers, and tests enforcing 1 LSB /
  SNR ≥ 70 dB, non-degenerate stereo output, and explicit PS signalling.

### Changed

- Compatible crate versions in `Cargo.lock` (`cc` 1.4.5,
  `wasm-bindgen` 0.2.127). Product `[dependencies]` stay empty.

## [0.3.0] - 2026-09-03

### Changed

- Product is a zero-dep AAC library (`cargo add syom`). Output is planar
  `f32` at native rate. Caps: `DecodeOptions::speech()` / `unbounded()`.
- Crate root is decode / sniff / options. ISO-BMFF demux (`parse_aac_track`)
  is crate-private.
- LC decode is f32 from spectrum through IMDCT/OLA. ICS/spectrum
  buffers are reused across frames. On the committed LC fixtures,
  in-process wall time is ≤ Symphonia and ≪ rusty_aac; allocs and
  peak RSS are ≤ both linked decode peers (`BENCH.md`).

### Added

- HE-AAC v1/v2 (SBR/PS) and LATM/LOAS on the same decode path. Committed
  lavc native goldens (max abs ≤ 1 LSB, SNR ≥ 70 dB) for lecture M4A,
  44.1 M4A, ADTS sine, TNS, PNS-heavy LC, HE ADTS/M4A/LATM, and LC LATM.
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



[Unreleased]: https://github.com/ekhodzitsky/syom/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ekhodzitsky/syom/releases/tag/v0.3.0
