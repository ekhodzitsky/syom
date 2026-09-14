# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Isolated scoring lab (`lab/score`): independent-decode alignment, valid
  duration, actual ES bitrate, SNR/max-abs. Silence, delay, truncation,
  channel-swap and channel mismatch are diagnostics, not high quality
  scores. PEAQ BS.1387 and ViSQOL are unavailable on this host and are
  not claimed. Ordinary tests never link the scorer.
- Isolated glint-audio 0.11.0 lab (`lab/glint`): crates.io pins
  `df09912e…` / `b8c79457…`, MIT, C++17 native core vendored in
  `glint-audio-sys`. Wrapper vs C ABI copy overhead is a separate lane.
  AAC-LC only; no Cargo feature drops MP3/Opus. `GLINT_MODE=fixed` is
  off. Quality `speed|normal|best`. Ordinary tests never link glint.
- Isolated FAAC 1.31.1 encode lab (`lab/faac`): knik0/faac tag `faac-1.31.1`,
  SHA-256 `3191bf1b…`, LGPL kept off the product crate. In-process ADTS
  AAC-LC (`LOW`, MPEG-4, float PCM, TNS off). Requested `bitRate` is per
  channel and maps to `quantqual`; a 2 s 440 Hz sine at 64/128/192 kbps
  per channel all land ~43–44 kbps actual. Independent syom decode is
  finite 97280 samples/ch vs 96000 input. HE encode is unavailable on
  this release. Ordinary tests never link FAAC.
- Isolated fdk-aac-rust 0.2.3 lab (`lab/fdk-aac-rust`): crates.io pin
  `607e6ba5…`, Fraunhofer license kept off the product crate. Default
  `ffi` feature is native FDK C++ (blocked here). Measured path is
  `--no-default-features` (Rust port). LC sine48 decode matches 13312
  samples at 48 kHz; HE/PS ADTS via `AacLcDecoder` is core-rate only.
  Not a second independent FDK oracle (same 2.0.3 lineage as TASK-7).
  Ordinary tests never link it.
- Independent memory budgets (TASK-23 / F07): named constants and checked
  accounting (`MemoryBudgets`) for finite compressed input (1 GiB),
  collected planar f32 (4 GiB), channels (8), M4A index/metadata
  (2^20 entries / 16 MiB), and resident workspace (8 MiB). Duration is
  not a memory cap. Streaming `feed` must not inherit a lifetime
  compressed-byte cap (enforcement: TASK-24 collection, TASK-25
  streaming). `speech()` Mono / 7200 s / 48 kHz is unchanged.
- One-shot decode and M4A demux enforce those collection budgets before
  `reserve`/`resize` (TASK-24). Over-budget output, channel count, table
  entries, and box depth are `AacError::Limit` (`planned == max` allowed,
  `max + 1` not). `DecodeOptions::memory` holds the numbers; `speech()`
  duration and `ChannelMode` are unchanged. Streaming workspace remains
  TASK-25.
- Streaming `Decoder::feed` no longer inherits the 1 GiB lifetime
  compressed-byte cap (F07 / TASK-25). Resident buffer, declared AU
  length, and codec workspace are fenced independently of duration;
  one-shot `decode` still uses the finite 1 GiB input budget. Chunked
  vs whole-slice feeds stay sample-identical.
- Truncated or malformed SBR `fill_element` payloads are decode errors
  even before the first successful HE frame (F14 / TASK-39). A first
  SBR payload without `bs_header_flag` is `SbrFreqBandInvalid`, not LC
  success. HE declared without FIL still 2×-upsamples; a missing FIL
  after HE stays 2×. Truncated `ps_data()` is `PsDataInvalid`, not a
  silent hold. he48/ps48 goldens and chunked feeds are unchanged.
- Offline AAC evaluation corpus (`corpus/manifest.json`) with in-tree golden
  hashes, deterministic boundary PCM, named licensed natural excerpts (not
  vendored), a coverage table, and `scripts/verify_corpus.py` that fails on
  bit-flips, missing required assets, train/holdout recording overlap, and
  inconsistent metadata.

### Changed

- Decoder Criterion benches (`benches/aac.rs`) run an untimed equivalent-PCM
  preflight (`syom::decode_cmp::run_preflight`) before timing: native rate,
  channel count, finite samples and consumed length must match. A failed
  candidate aborts the group. Unavailable profiles and mismatched lengths
  (HE vs core-only) are non-comparable and produce no throughput number.
  The primary lane is planar split at native rate; speech downmix and
  discard-output are separately named. Historical BENCH.md decode rows
  stay labeled until a matched-output baseline replaces them. Product
  `decode` / `speech()` defaults are unchanged.
- Invalid `DecodeOptions` duration/rate limits (NaN, −∞, negative duration,
  zero `max_sample_rate`, zero `max_decode_sample_rate` with a finite
  duration) now error as `AacError::InvalidLimits` before decode work.
  `+∞` remains the explicit unbounded contract. NaN/−∞ no longer open an
  unlimited frame budget.
- `read` / `read_with` check the compressed file size (and cap the subsequent
  read) against `DEFAULT_MAX_INPUT_BYTES` before allocating the whole file.
- README install version is 0.6; oxideav-aac is documented as an ADTS
  LC+SBR/PS decoder (not a parser); encoder M/S is per-band; BENCH.md is
  linked as historical unequal-work evidence.
- Oracle provenance ledger (`corpus/oracles/provenance.json`) for committed
  goldens: SHA-256, comparison policy (deterministic vs PNS), mint command
  when known, and explicit gaps where ffmpeg versions were never recorded.
  No silent remint; ordinary tests still do not spawn ffmpeg.
- Encoder Criterion groups run an untimed preflight (`encode_cmp`) that
  records ADTS/payload bytes, decoded duration and achieved bitrate, aborts
  on failed encode/decode, and labels rate mismatch >1% as non-matched
  (not an equal-rate cell). HE encode remains an unavailable separate cell.
- Isolated native libavcodec lab (`lab/libavcodec`, FFmpeg 9.0.1 pin):
  in-process ADTS access-unit decode, container decode, and LC ADTS encode.
  Not a workspace member; ordinary tests never link or spawn ffmpeg.
- Isolated native FDK lab (`lab/fdk`, fdk-aac v2.0.3 pin, Fraunhofer license
  kept off the product crate): LC ADTS decode/encode adapter, afterburner
  off, delay reported. In-process smoke (zig-c++ GNU, 170 TUs): sine48
  13312 samples, HE/PS goldens length-match syom at 48 kHz, LC encode
  128 kbps is 32426 B / 97280 samples (independent lavc). syom rejects
  that FDK ADTS FIL (`extension_payload invalid`). Ordinary tests never
  link FDK.
- Isolated-process memory runner (`benches/mem_iso.rs`): one OS process per
  peer/case, reporting baseline RSS, peak RSS delta, peak live heap, alloc
  count/bytes and retained live separately for one-shot vs streaming. The
  in-process `mem` bench is unchanged historical.
- LATM/LOAS `AudioMuxElement` delivers every subframe (`numSubFrames+1`
  access units) in payload order. A missing extra subframe is an error,
  not a silent one-AU success. Single-subframe goldens are unchanged.
- LATM `latmGetValue` is the 2-bit length prefix used by FFmpeg/FDK/FAAD2
  (`bytesForValue` then `(n+1)*8` bits). `audioMuxVersion=1` other-data
  length uses that form; an ASC that overruns `ascLen` is an error.
- ADTS `protection_absent=0` frames verify `crc_check` with the ISO/IEC
  11172-3 CRC-16 (poly `0x8005`, init `0xFFFF`, no inversion) over the
  13818-7 header and raw-data-block protected bits. Mismatch is
  `AdtsCrcMismatch`, distinct from truncation. CRC-free streams are
  unchanged. The MPEG-4 Audio §1.8.4.5 generator is not used here.
- ADTS frames with `number_of_raw_data_blocks_in_frame` > 1 decode every
  `raw_data_block()` (1..=4). CRC-present payloads skip the header
  position table (`7+2N` bytes) and per-block 16-bit fields; CRC values
  are not checked (TASK-29). Single-block goldens are unchanged.
- Footprint/API cost (TASK-18): empty product deps, Rust 1.97, a
  measured release rlib/consumer binary, and a go/no-go that keeps
  `speech()` mono until a versioned API decision. Artifact:
  `corpus/footprint/`.
- Isolated FAAD2 2.11.3 decode lab (`lab/faad2`, GPL-2.0-or-later kept
  off the product crate): ADTS in-process `FAAD_FMT_FLOAT`, metadata and
  a classified disagreement report vs FFmpeg 9.0.1. Ordinary tests never
  link or spawn FAAD2.
- Encode PCM must be finite and in `[-1, 1]`. NaN, infinities, and
  `|x| > 1` are `Encode` errors on both one-shot and push APIs. There is
  no silent clip. ±0, subnormals, and full-scale ±1 remain valid.
- LC encode enforces 6144 bits per channel per 1024-sample frame,
  independent of the ADTS 8184-byte length ceiling. A requested bitrate
  that cannot fit that cap is `Encode` (not an over-limit frame). The
  rate loop still drops top bands to stay inside the cap; encode_cmp
  reports achieved vs requested bitrate. Default 128 kbps at 48 kHz and
  encoder goldens are unchanged.
- M4A/LATM ASC SBR/PS flags seed the decoder: `sbr_present` activates HE
  without waiting for a FIL payload. Output rate is the declared 1× or 2×
  core rate (not a hardcoded 2×). 1× keeps core sample count; 2× without
  payload is pure upsample. Other ratios are `Format`. ADTS still discovers
  SBR from the bitstream. Downsampled high-band reconstruction stays 32-band
  QMF (not on the product path).
- SBR QMF and header history are keyed by channel-element `(kind, tag)`.
  FIL SBR attaches to the preceding SCE/CPE/LFE. Duplicate or
  missing-element SBR extensions are `Format`. HE reconstruction runs
  before speech downmix; mixed output is the non-LFE mean of the
  reconstructed planes. Stereo and multielement HE keep independent
  state. 1× passthrough and he48 goldens are unchanged.
- Independent CCE is applied after IMDCT (`dest += gain * cce_pcm`) with
  CCE overlap keyed by element tag. Unity independent coupling of a
  silent SCE matches the coupling ICS decoded as SCE across frames.
- Dependent CCE is reconstructed on spectral coefficients (FFmpeg
  `apply_dependent_coupling`) at BEFORE_TNS or BETWEEN_TNS_AND_IMDCT.
  A silent CCE is a no-op; unity coupling of a silent SCE matches the
  coupling-channel ICS decoded as SCE. Missing targets are `Format`.
  Non-CCE streams are unchanged.
- Multichannel filterbank overlap is keyed by channel-element `(kind, tag)`,
  not bitstream encounter order. Legal reordering keeps each identity's
  history; identities missing from a frame are dropped so a later tag reuse
  or mono/stereo/layout change starts cold. Duplicate identities in one
  access unit are `Format`. cfg 1–2 still use `fb_l`/`fb_r`.
- In-band and ASC-seeded PCE must declare LC, match the stream sample-rate
  index, and list exactly the channel elements in the access unit. Missing,
  extra, or wrong-type tags, and layouts above 5.1, are `Format` errors
  rather than silent extra planes or synthesized silence. Default
  `channel_configuration` 1–6 mapping is unchanged.
- ASC `channel_configuration=0` now parses the embedded
  `program_config_element` (LC object type, matching core rate, unique
  tags). M4A/LATM seed the decoder map from it; in-band PCE still wins.
  No ISO 14496-26 PCE bitstream is claimed.
- HE `AudioSpecificConfig` parse follows Amd 2/9: explicit AOT 5/29
  consume a core rate and an extension rate (not one rate halved);
  implicit `0x2b7` reads `extensionSamplingFrequencyIndex`; `0x548`
  sets PS. Downsampled SBR (1×) is distinct from dual-rate (2×).
- AAC syntax inventory (`corpus/conformance/`): clause acquisition status
  (ISO 14496-3/26 and 13818-7 full text not obtained), advertised-tool
  coverage matrix, and independently authored ASC/ADTS/LATM/PCE examples
  with expected fields or errors. No conformance certificate. Parser
  follow-ons that can start without purchasing ISO text are listed in
  `go-nogo.md`. Ordinary tests never fetch standards or spawn ffmpeg.

## [0.6.0] - 2026-09-06

### Added

- Encoder TNS (Temporal Noise Shaping) on long-window frames (`enc_tns`):
  per channel, an all-pole LPC (autocorrelation + Levinson–Durbin, order
  ≤ 12 trimmed at trailing |k| < 0.1) of the MDCT spectrum over the psy
  model's coded-band span is quantized to the 4-bit TNS coefficient table
  and the analysis (whitening) FIR runs on the raw L/R spectra before the
  M/S transform — the mirror of the decoder's tool order (TNS inverse
  after the M/S undo). The span is emitted as an order-0 spacer filter
  plus the active filter; every band in the span is force-coded (the
  whitened residual carries real energy across the whole span — a dropped
  band would feed zeros into the decoder's recursion). The psy coded-band
  decisions and masking thresholds come from the original (pre-TNS)
  spectrum via a snapshot with the M/S transform replayed onto it; peaks
  and quantization use the filtered residual. A filter is emitted only
  when the quantized filter's measured prediction gain clears 2 dB —
  steady tones/sweeps barely whiten and stay off (as with lavc); tremolo
  and speech onsets fire. Short frames keep `tns_data_present = 0` (v1).
  The decision path is libm-free (`det_math` + IEEE-exact ops), so the
  byte-exact golden asserts still pin cross-platform output. A/B (test
  knob `set_tns`): lecture speech at 64 kbps stereo +4.0 dB overall SNR
  (+6 dB worst segment), tremolo neutral. Re-minted goldens: `enc48{,m,t}`
  (the `enc48` fixture's silence tail is now an 880 Hz / 8 Hz tremolo so
  the byte-exact golden exercises TNS-on mid-stream).
- Opt-in one-frame attack lookahead for the encoder:
  `EncodeOptions::with_lookahead(true)` (default off — the causal behavior
  and all existing goldens are unchanged). The attack detector runs one
  frame ahead of the encode, so an attack anywhere in frame N+1 makes
  frame N a LongStart (its start-window slope covers the pre-attack tail)
  and frame N+1 an EightShort: a click landing in the first ~448 samples
  of a frame — the causal detector's weak spot, previously coded by the
  LongStart's flat-region long transform — is now coded entirely on short
  windows. Pre-echo in the 10 ms before such an early click after silence
  drops a further 32.7 dB in the A/B measurement (lookahead vs causal,
  both with block switching on). Costs one extra frame (1024 samples) of
  latency in both one-shot and push encode; the push `Encoder` holds the
  frame and flushes it at `finish`, staying byte-exact with one-shot
  `encode_with` for the same options and any feed chunking. New lavc
  golden `src/goldens/enc48l.*` (transient fixture, lookahead on) pins the
  bitstream byte-exactly and decode-matches ffmpeg within the usual
  tolerance.

### Changed

- Encoder M/S stereo is now decided per scalefactor band instead of
  whole-pair per frame. A band goes M/S when its side energy is 3× under
  the weaker original channel (`3·Σs² < min(Σl², Σr²)`, f64 sums — the
  per-band refinement of the previous whole-frame energy comparison);
  mixed frames emit `ms_mask_present = 1` with the per-band `ms_used`
  bitmask (per (window, band), 8 groups of 1 window, on short frames),
  and unanimous frames still collapse to the 2-bit whole-pair masks 0/2.
  Bands where M/S costs more than L/R (e.g. a channel-quiet band, where
  L/R codes one channel and M/S would code two) now stay L/R. The decision
  runs once per frame before the rate loop on the MDCT spectra and is
  platform-deterministic (`+` / `*` only). Re-minted goldens: `enc48t.*`
  is byte-identical (fully correlated fixture stays all-M/S); `enc48{,m}.*`
  drifted only in the decorrelated noise-burst frames.

## [0.5.0] - 2026-09-06

### Added

- Streaming encode API: push `Encoder` (resumable, ADTS only) —
  `Encoder::new(sample_rate, channels, &EncodeOptions)`, then `feed` planar
  f32 chunks of any size and the `on_frame` callback fires once per
  completed AAC frame with a borrowed ADTS-wrapped `EncodedFrame`;
  `finish` encodes the zero-padded tail frame and returns `EncodeInfo`
  tallies (rate / channels / `aac_frames` / `samples` / `bytes`). Output
  is byte-exact with one-shot `encode_with` on the same PCM for any feed
  chunking. M4A is rejected at construction (`stco` / sample sizes need
  the finished totals; use `encode_with`), mirroring the push `Decoder`'s
  ISOBMFF rejection.
- Encoder block switching: a deterministic attack detector (high-passed
  128-sample sub-block energy surge, >8× the running two-frame mean, OR'd
  across channels for the shared CPE window) drives a causal
  OnlyLong → LongStart → EightShort → LongStop state machine — no
  lookahead, no added latency. Transients are now coded on eight 128-bin
  short windows (no `scale_factor_grouping`: 8 groups of 1 window);
  steady content stays OnlyLong byte-for-byte (the `enc48{,m}` goldens
  did not change). Pre-echo in the 10 ms before a click after silence
  drops 19.3 dB in the A/B measurement. A new transient lavc golden
  (`src/goldens/enc48t.*` — castanet clicks over a tone bed) pins the
  short-window bitstream byte-exactly and decode-matches ffmpeg within
  the usual tolerance (measured 1 LSB s16 / ~71 dB SNR).

### Fixed

- Encoder output is now byte-identical across platforms (fixes the two
  `lavc_matches_our_decode_of_our_*` oracle tests on x86_64 CI). Every
  transcendental in the encode decision path routes through the new
  `engine/det_math` — `exp2`/`log2`/`atan`/`sincos` built from
  exactly-rounded ops over literals, `|x|^0.75` via two sqrts — the
  oracle fixture's sweep sine uses `det_math::sincos`, and the aarch64
  NEON MDCT/IMDCT butterflies no longer fuse multiply-add (a 1-ulp
  mismatch with the scalar x86_64 path). The `enc48{,m}` goldens were
  re-minted; the oracle tests keep a byte-exact tripwire plus the
  decode-equivalence tolerance check on the fresh encode.

## [0.4.0] - 2026-09-06

### Added

- AAC-LC encoder: `encode` / `encode_with` / `write` / `write_with` take
  planar f32 PCM (mono or stereo, any ADTS-table sample rate) and produce
  ADTS or M4A (`EncodeOptions`: `EncodeContainer::Adts` default / `M4a`,
  `bitrate_bps` default 128 kbps). Long windows only (KBD analysis, no
  block switching yet), per-frame whole-pair M/S, a Bark-spreading psy
  model (flat 18 dB SMR), CBR-ish via a per-frame global scalefactor
  offset search plus a bounded one-frame bit credit. Errors surface as
  the new `AacError::Encode` variant. Committed lavc goldens
  (`src/goldens/enc48{,m}.*`) pin ffmpeg-decoded PCM against our own
  decode (≤ 2 LSB s16, ≥ 55 dB SNR; measured 1 LSB / ~80 dB).
- Streaming decode API: `Decoder` (resumable push decoder for ADTS and
  LATM/LOAS byte streams — `feed` arbitrary chunks, the `on_frame`
  callback fires once per decoded AAC frame, `finish` flushes and drops a
  partial trailing frame) and `decode_streaming` (slice-based, any
  container incl. M4A/ISOBMFF). Frames are delivered as `Frame` — planar
  f32 in [-1, 1] borrowing decoder scratch, valid for the callback only —
  with `StreamInfo` tallies at the end. The callback returns `Result` so
  consumers can abort mid-stream. Peak PCM RAM is O(frame). Push decoding
  rejects M4A input (`moov` needs random access).
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
- `goldens/ps48.*` HE-AACv2 fixture with real stereo content (440 Hz L /
  880 Hz R, explicit in-band `EXTENSION_ID_PS`) plus lavc s16 goldens for
  the M4A (elst-trimmed) and ADTS containers, and tests enforcing 1 LSB /
  SNR ≥ 70 dB, non-degenerate stereo output, and explicit PS signalling.

### Changed

- One-shot `decode_with` now layers on the streaming core. Two visible
  consequences: ADTS input with leading junk resyncs to the first valid
  frame instead of answering `NotAac`, and an over-long LATM stream fails
  mid-decode with `TooLong` instead of a decode-class "latm: too long".
- M4A `elst` encoder delay is skipped pre-emission (a skip counter over
  decoded frames) instead of a post-hoc PCM shift; decoded output is
  unchanged.
- Bench refresh (BENCH.md, 2026-09-05): symphonia dev-dep bumped to 0.6
  and oxideav-aac 0.1.7 added as a decode peer (the published tarball
  does decode — ADTS LC + SBR + PS). syom leads LC ADTS/M4A wall, HE
  (~30×/18× vs oxideav), and 5.1.
- Compatible crate versions in `Cargo.lock` (`cc` 1.4.5,
  `wasm-bindgen` 0.2.127). Product `[dependencies]` stay empty.

### Fixed

- One-shot mono decode no longer pays streaming-layer overhead: ADTS/LATM
  mono frames decode straight into the output plane (no per-frame scratch
  or callback copy), `decode_streaming` iterates the input slice zero-copy
  instead of buffering it, and ADTS output planes are pre-sized from a
  frame-header walk. LC ADTS wall 225 → 216 µs (symphonia 0.6: 222 µs),
  allocs/iter 32 → 27, alloc bytes/iter 146 → 70 KiB (BENCH.md,
  2026-09-05).

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
