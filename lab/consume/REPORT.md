# TASK-102 — realistic consumer flows and one-engine integration

Measured 2026-09-22, Linux x86_64, dev-pin rustc 1.97.1. Subject is the
**packaged** syom 0.6.0 crate (`cargo package --allow-dirty --no-verify`;
360 942 B, sha256 `d9ff6bf4c8cc6aa8…`, manifest audited: no dependencies,
no build.rs, rust-version 1.88).

## Method and scope

`lab/consume` is an isolated consumer crate (not a workspace member).
`run.py` packages syom, unpacks the `.crate` into `lab/consume/vendor/syom`
(gitignored), audits the packaged manifest, then builds and runs two
binaries against **those packaged sources** — never the working tree:

```sh
python3 lab/consume/run.py
```

Fixtures are synthesized and coded by syom itself. This lab measures
*integration cost* (calls, concepts, copies, allocations, error usability);
codec conformance stays with the goldens and the offline native oracles.
No external codec runs in any cell, and ordinary workspace tests never
build this lab. Allocation counts come from a counting global allocator
compiled into each bin.

## AC#1 — consumer flows (`flows` bin, verbatim output)

```
cell bytes-decode: 1 call each way; speech = 1 ch, full = 2 ch @ 48000 Hz Mpeg(2), SNR L 30.6 / R 36.3 dB, no resample
cell file-reader-decode: read(path) + read_with; decode_read over ADTS (16864 B) and LOAS (1917 B); sniff/probe agree (ADTS duration Unknown: no index)
cell raw-au: Encoder::raw + asc (2 B) -> 11 AUs; Decoder::from_asc + decode_au -> 11264 samples; frame meta Some(Mpeg(1)); wrap_adts_au re-frames
cell sink-encode: encode_write to a generic Write (48 calls, 16864 B, byte-exact with one-shot); failing sink -> typed Io with exact prefix; encode_write_m4a/decode_seek roundtrip is sample-exact (48000 samples, priming Some(1024))
cell bounded-streaming: 1319-B chunks == one-shot (481280 samples); 10-s stream trips a 5-s cap with TooLong: Audio file too long (5s). Maximum supported: 5s.; reset() + second pass: 0 allocations after warm-up
cell error-handling: garbage -> NotAac | truncated-adts-tail -> Ok, -1024 samples (documented drop) | truncated-raw-au -> Truncated | aac-main-profile -> Unsupported: audioObjectType 1 | tiny-asc -> NotAac | |x|>1 -> InvalidPcm(Amplitude) | NaN -> InvalidPcm | 7-planes -> Encode | failed instance -> Lifecycle until reset(), then recovers
```

Covered: bytes / file / `Read` decode, stereo full fidelity vs speech mix,
raw AU + ASC both directions, generic `Write` sink encode with exact error
prefix, bounded push streaming (chunked == one-shot sample-exact; duration
cap fires mid-`feed`, before the whole stream is buffered), and a typed,
distinguishable error outcome for every misuse class (`NotAac`,
`Truncated`, `Unsupported`, `Malformed`, `InvalidPcm`, `Encode`,
`Lifecycle`, `TooLong`).

Streaming allocation behavior: after one warm pass + `reset()`, a second
chunked pass over a same-format stream performs **0 heap allocations**
(borrowed frame callbacks, reused workspace). Measured nuance: `finish()`
takes the resident push buffer (`std::mem::take` in `stream.rs`), so a
reset instance re-grows it during the first chunks of the next stream
(≤ 2 allocations, ≤ 4 KiB) — steady state within the stream is unaffected.

## AC#2 — kover / sluh call patterns (`consumers` bin)

Real call sites (read-only inspection; neither repo modified):

- **kover** treats AAC as a helper *process*
  (`kover-infer/src/extract_proc.rs`, `kover-core/src/media/mod.rs`:
  "`HELPER IN.mp4 -o OUT.pcm`. Output: 16 kHz s16le mono", "AAC is syom").
  Per extract: one `Command` spawn, two temp files, any failure collapses
  to an exit status mapped to a single untyped `KoverError::Media`.
- **sluh** ingests WAVE via vendored ryf (`sluh/src/wav.rs`
  `load_16k_mono_s16`: ryf → mono f32 → Lanczos-3 → s16le 16 kHz); AAC
  reaches it only as already-extracted PCM.

Faithful minimal reproductions (verbatim output):

```
cell kover-extract: 33639 B M4A in memory -> 64000 B s16le 16 kHz mono; native 48000 Hz / 96000 valid samples retained until the consumer's own resample; one-shot extract cost 319 allocs / 1412513 B heap; zero processes, zero temp files, zero WAV wraps
cell kover-error: garbage M4A -> NotAac (helper path: exit status -> KoverError::Media, cause lost)
cell sluh-ingest: AAC -> speech mono f32 (native) -> consumer resample -> 64000 B s16le; s16 quantization happens once, at the ASR boundary
cell native-layout: same bytes as full fidelity -> (2, 48000, Mpeg(2), Some(1024)) (2 planes, sample-exact 96000 via elst, priming Some(1024)); the helper contract discards all of this
cell probe: duration Exact { samples: 96000 }, trim Exact { priming: 1024, remainder: 256 } — exact before any PCM is decoded
cell borrowed-encode: &[&[f32]] planes encode byte-identical to owned (33312 B) — no consumer-side copy
```

Concept/copy comparison for the kover extract seam (same input: M4A bytes
in memory; same output: 16 kHz s16le mono):

| | kover helper contract (today) | in-library syom |
|---|---|---|
| Process spawns | 1 per extract | 0 |
| Temp files | 2 (`.mp4` in, `.pcm` out) | 0 |
| PCM representation | s16le fixed 16 kHz mono (quantized before any consumer choice) | planar f32 at native rate; s16 once at the ASR boundary |
| Rate / layout knowledge | hard-coded in the helper | exact from the stream (`probe`, `StreamInfo`, `FrameMeta` labels) |
| Timeline | helper-side assumption | sample-exact via `elst` (priming 1024 reported) |
| Failure detail | exit status → `KoverError::Media` (cause lost) | typed `AacError` (matchable class) |
| New concepts | `Command`, temp paths, `KOVER_EXTRACT` binding | `DecodeOptions::speech()` |

The engine does no hidden conversion: nothing is resampled or quantized on
the way out, and borrowed planes make the encode side copy-free. The one
accidental-copy risk found is in the *helper contract*, not the library.

## AC#3 — vs the TASK-18 baseline and peers

TASK-18 (2026-09-14, syom 0.6.0 release API; `corpus/footprint/report.md`)
ranked five usability gaps. Follow-up measured today against the packaged
crate:

| TASK-18 gap | Status now | Evidence |
|---|---|---|
| No public raw AU + ASC entry | **closed** (TASK-52) | `raw-au` cell: `Encoder::raw` + `asc()` → `Decoder::from_asc` + `decode_au`; matches rusty_aac's `with_config_bytes` capability |
| Encode input `&[Vec<f32>]` (one extra copy) | **closed** (TASK-55) | `borrowed-encode` cell: `&[&[f32]]` byte-identical to owned |
| Push `Decoder` rejects M4A | **improved** (TASK-57) | `decode_seek` / `M4aSeek` sample-exact on `Read + Seek`; push stays ADTS/LATM-only, documented |
| Speech default mono surprises stereo consumers | unchanged (D3) — and **worse than documented**, see finding | `bytes-decode` cell deviation, TASK-122 |
| rust-version 1.97 harder than peers | **improved** (TASK-99) | packaged manifest: 1.88 (peers: oxideav-aac 1.80, rusty_aac / symphonia 1.85) |

Added since the baseline, exercised here: `encode_write` to any `Write`
with an exact prefix on sink error (TASK-59), `encode_write_m4a`
(TASK-60), per-frame `FrameMeta` channel labels (TASK-61), and the typed
error taxonomy (TASK-51) that the `error-handling` cell matches on.

Refreshed scenario rows against the TASK-18 peers (concepts = named types
+ required calls, as in the baseline):

| Scenario | syom (packaged 0.6.0) | rusty_aac 0.5.0 | oxideav-aac 0.1.7 | symphonia 0.6 AAC |
|---|---|---|---|---|
| Bytes → PCM | 1 call `decode` | decoder + packet loop + collect | runtime + register + `decode_all` | probe + reader + registry + packet loop |
| Stereo kept | `decode_with` + `DecodeOptions::audio()` | as coded | as coded | as coded |
| Bounded streaming | `Decoder` feed/finish, borrowed frames, **0 steady-state allocs after warm-up (measured)** | packet-at-a-time | no push API | incremental reader |
| Raw AU + ASC | `from_asc` / `decode_au` / `Encoder::raw` (was: gap) | `with_config_bytes` | engine path | codec params from demux |
| Encode to memory | 1 call `encode`, borrowed planes | encoder + config + pull + ADTS writer | modules | none |
| Encode to any sink | `encode_write` / `encode_write_m4a`, typed sink errors | caller writes | caller writes | n/a |
| Errors | typed `AacError`, matchable classes (measured per class) | crate `Error` | crate `Error` | `symphonia_core` error |

Remaining friction (recorded, not blocking):

1. The `speech()` default is mono — a music consumer must opt into
   `audio()`/`unbounded()` (D3 defers the default to a versioned decision).
2. Push streaming is ADTS/LATM only; M4A needs `Read + Seek`.
3. `finish()` frees the resident push buffer, so reusing a reset instance
   for another stream re-allocates it once (≤ 4 KiB) — worth a note if a
   consumer pools decoders.
4. A truncated ADTS *tail* is `Ok` minus one frame (documented drop in
   `pump.rs`); strict consumers must compare sample counts. Raw-AU
   truncation is a hard `Truncated`.
5. `Display` strings are fixed English text; the contract is the typed
   class (the enum is `#[non_exhaustive]` — keep a `_` arm).

## Finding: LC stereo speech mono returned the left plane (TASK-122, fixed)

The `bytes-decode` cell exposed a product bug. Contract: speech mono is
the mean of the non-LFE planes (`src/decode.rs` docs).
Measured on every transport (ADTS one-shot, push streaming, M4A): for LC
stereo (CPE, `channel_configuration` 2) the speech path returns the **left
plane untouched** — corr(speech, mean) = 0.83, corr(speech, L) = 1.0000,
corr(speech, R) = 0.0002. HE-AAC stereo and LC 5.1 correctly return the
mean (corr = 1.0000). Mechanism: the mono entry points set
`fast_mono = true`, `push_pcm` then keeps only `pcm_l`, and the stereo
mix-down in `engine/decode_cpe.rs` is guarded by `!fast_mono`; the
`mono_mix` path in `engine/decode.rs` runs only when a plane order exists
(PCE / multichannel).

**Resolution (TASK-122, 2026-09-23):** the CPE right channel is folded
into `pcm_l` as the running mean right after synthesis in
`engine/decode_cpe.rs`, so the fast-mono path emits the documented mean on
every transport with no extra allocation. The cell now hard-fails below
corr 0.999 instead of printing a KNOWN DEVIATION marker; re-measured
2026-09-23: no marker, cell passes (regression pinned by
`src/decode_speech_tests.rs`).

## AC#4 — packaged-crate independence, no external codecs

- Both bins compile and run from `vendor/syom`, the unpacked `.crate`
  (`run.py` output above: manifest audit passes, 360 942 B).
- `lab/consume` is not a workspace member; `cargo test --workspace --lib`
  never builds it, and no cell spawns ffmpeg/FDK/FAAD2 or any other codec.
- The packaged crate builds warning-free as a dependency except three
  pre-existing dead-code warnings in `engine/enc_section_dp.rs`
  (stage-3 code paths; present before this task).

## Reproduce

```sh
python3 lab/consume/run.py   # packages syom, builds vendor/syom, runs both bins
```
