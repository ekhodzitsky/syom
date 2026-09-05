# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Streaming decode API: `Decoder` (resumable push decoder for ADTS and
  LATM/LOAS byte streams — `feed` arbitrary chunks, the `on_frame`
  callback fires once per decoded AAC frame, `finish` flushes and drops a
  partial trailing frame) and `decode_streaming` (slice-based, any
  container incl. M4A/ISOBMFF). Frames are delivered as `Frame` — planar
  f32 in [-1, 1] borrowing decoder scratch, valid for the callback only —
  with `StreamInfo` tallies at the end. The callback returns `Result` so
  consumers can abort mid-stream. Peak PCM RAM is O(frame). Push decoding
  rejects M4A input (`moov` needs random access).

### Changed

- One-shot `decode_with` now layers on the streaming core. Two visible
  consequences: ADTS input with leading junk resyncs to the first valid
  frame instead of answering `NotAac`, and an over-long LATM stream fails
  mid-decode with `TooLong` instead of a decode-class "latm: too long".
- M4A `elst` encoder delay is skipped pre-emission (a skip counter over
  decoded frames) instead of a post-hoc PCM shift; decoded output is
  unchanged.

### Fixed

- Multichannel frames (≥ 3 channels or a PCE) decode each channel through its
  own filterbank state; previously the shared L/R filterbanks bled
  overlap-add tails across channels, and `speech()` mono on > 2 channels
  returned only the last element (e.g. LFE on 5.1). Speech mono is now one
  documented rule: the arithmetic mean of the decoded non-LFE planes.
- LATM/LOAS `StreamMuxConfig()` `crcCheckSum` is verified (CRC-8,
  §1.8.4.5) instead of discarded; a mismatch rejects the stream.
- PS first-envelope H-matrix interpolation now starts one slot before the
  frame (`(n + 1)/(n_0 + 1)`, Annex 8.A / §8.6.4.6.4) instead of at slot 0
  (`n/n_0`). HE-AACv2 output now matches lavc within 1 LSB on real stereo
  content (was up to 11 LSB / 67 dB SNR whenever the stereo cues moved;
  dual-mono fixtures could not see it).

### Added

- Multichannel AAC-LC: ADTS/ASC `channel_configuration` 3–6 (3.0 / 4.0 /
  5.0 / 5.1) with correct Center/LFE placement, and in-band
  `program_config_element()` (channelConfiguration 0) channel mapping. Split
  mode emits planes in the libavcodec layout order (5.1 = FL FR FC LFE BL
  BR) or, for PCE streams, in PCE declaration order (front, side, back,
  LFE). CCE elements are consumed without disturbing other channels; full
  CCE gain-element application remains out of scope.
- `goldens/mc{30,40,50,51}.*` multichannel fixtures (per-channel sines) with
  lavc s16 goldens; tests enforce per-plane peak ≥ 1000, max abs ≤ 1 LSB and
  SNR ≥ 70 dB in split mode, plus the speech mono rule on 5.1.

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
