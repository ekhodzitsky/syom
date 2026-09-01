# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
