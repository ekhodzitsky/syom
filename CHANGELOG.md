# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Lecture fixture native-rate PCM is checked against a committed lavc
  golden (ffmpeg 8.1.1 native AAC, 48 kHz mono s16, minted once offline).
  Max abs ≤ 1 LSB, SNR ≥ 70 dB. Runtime does not spawn ffmpeg.
- 44.1 kHz mono M4A (elst 1024) and 48 kHz mono ADTS LC sine fixtures vs
  lavc native goldens, same 1 LSB / 70 dB bar. A hand-built 48 kHz ADTS
  with order-1 TNS is checked the same way. PNS uses lavc's LCG seed
  (`0x1f2e3d4c`). Runtime does not spawn ffmpeg.

### Fixed

- MP4 audio `elst.media_time` is honoured (AAC encoder delay). The lecture
  fixture drops the 1024-sample priming frame before the 16 kHz resample.
  ADTS and PCM tracks without a non-zero edit are unchanged.

### Changed

- AAC-LC engine is original (ISO/IEC 14496-3 / 13818-7), not oxideav-aac.
  HE-AAC SBR/PS (AOT 5 / 29) is `Media`. rustfft IMDCT stays, with
  pre/post twiddles owned here and tested against the naive §4.6.11 sum.

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
