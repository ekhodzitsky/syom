# In-process one-core speed, 2026-10-16

**Historical encoder protocol correction (2026-11-01):** the original FDK
adapter performed only one flush call and omitted delayed tail AUs. Its
encoder measurements below do not represent complete input coverage.
They are preserved as historical observations. The new full-drain protocol
and byte-preserving LC performance increment are recorded in the appendix
below and [raw evidence](peer_speed/runs/lc-encode-20261101/report.json).
The historical decode cells are unaffected by the encoder drain correction.

TASK-112.2 (LC encode vs fdk-aac 2.0.3) and TASK-112.3 (decode vs
libfdk and libavcodec on the same output shape). This file is the log.
The README speed table was not edited. BENCH.md was not edited.
Codec sources under `src/` were not edited for this measurement.

## Gate

Encode: **not passed.** syom is slower on both cells. No encoder change
was kept, because the acceptance gate is a median at or below fdk-aac
and these medians are not.

Decode: **passed** on every cell both peers can emit. Both peers emitted
the syom shape on all three cells, so none were left blank.

## Host and protocol

- Commit measured: `9c5364f259ddf62a3e0f579a6c38896d49f1c954`
- Worktree was not clean. Pre-existing diffs are comments plus the HE
  payload handoff in `src/engine/enc_he.rs` (no intermediate `Vec`).
  LC golden tests `encoder_goldens_match_auto_and_scalar_fft`,
  `encoder_64k_scalar_matches_auto`, `encoder_goldens_match_sse2_only_fft`
  and HE byte tests `lavc_decodes_our_he_adts_and_m4a`,
  `lavc_matches_our_decode_of_our_he_adts` passed on that tree
  (`CARGO_TARGET_DIR=/tmp/syom-t112-speed`). This pass did not remint
  goldens.
- Machine: Linux x86_64, AMD Ryzen AI 9 HX 370, governor `performance`,
  cpu0 about 5.11–5.15 GHz during the run.
- Load average: `2.81 3.70 2.84` before and after (the run is shorter
  than the 1-minute window). Not a quiet host. These milliseconds stand
  alone; they are not the README figures and not the BENCH.md table.
- Placement: `taskset -c 0` for every timed process. libavcodec
  `threads=1`. The fdk adapter opens one decoder transport layer and
  does not set an encoder thread count.
- Reps: 4 warmup calls discarded, then 21 measured. Median is the 11th
  sorted sample (0-based index 10). No outlier was dropped.
- Clock: `CLOCK_MONOTONIC` / `Instant` around the in-process call only.
  Process start, PCM synthesis, and the file read are outside that
  region. Nothing was subtracted from a process-wall time.
- Each measured rep includes producing the output buffer and releasing
  it. Encode is one-shot (open, encode, flush), not a warmed encoder
  reused across reps.

## Build

syom: rustc 1.97.1 (`8bab26f4f`), `lab/baseline/peer_speed` release
profile, thin LTO, `codegen-units = 1`, `opt-level = 3`.
`CARGO_TARGET_DIR=/tmp/syom-t112-speed`.

fdk-aac 2.0.3 (tarball sha256
`e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78`),
static, `zig-c++ -target x86_64-linux-gnu -O2 -fno-exceptions -fno-rtti`,
the flags in `lab/fdk/PIN.md`. Afterburner off. AOT 2. ADTS transmux.
Timed function: `fdk_encode_pcm_adts` / `fdk_decode_adts`.

libavcodec: FFmpeg 9.0.1 (tarball sha256
`cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635`),
AAC-only static prefix from `lab/PIN.md` (`--disable-x86asm`,
`--extra-cflags=-fPIC -O2`, gcc 15.2.0). Engine id
`ffmpeg-9.0.1-native-aac threads=1`. Timed function: `avc_decode_au`
(access units, not the container demux lane).

The checked-in `lab/fdk/fdk_driver` and `lab/libavcodec/avc_driver`
binaries have no repeat loop, so the timers above call the same adapter
functions from `lab/baseline/peer_speed`. `cargo test` does not build
that package and does not spawn ffmpeg or fdk.

Decode note: the fdk and libavcodec adapter calls hash planar f32
(FNV-1a) before returning. The syom timer does not. That extra walk
makes the C peers a little slower. The gaps below are larger than that
walk, so it does not change the decode gate.

## Encode (TASK-112.2)

Same PCM both sides. Voice-like / speech-like stereo from
`voice()` in `lab/quality/src/he_qual.rs`, which CORE_TUNE.md identifies
as the voice-like pair (seeds 3 and 4, f0 150 Hz and 113 Hz), not the
later `speech2` seeds. 2.0 s, 48 kHz, planar f32le, 96000 samples per
channel, 768000 bytes. LC ADTS, not HE. 128 kbps is
`EncodeOptions::adts().with_bitrate_bps(128_000)`. 24 kbps is
`with_bitrate_bps(24_000)`.

| cell | syom median | fdk-aac 2.0.3 median | syom min–max | fdk min–max | syom at or below fdk |
|---|---:|---:|---|---|---|
| LC 128 kbps stereo | 11.2015 ms | 4.5610 ms | 11.1403–11.4792 | 4.5440–4.8359 | no |
| LC 24 kbps speech-like stereo | 5.4477 ms | 3.3049 ms | 5.4358–5.7036 | 3.2856–3.5824 | no |

Output size is not the gate. syom wrote 33255 and 6920 ADTS bytes;
fdk-aac wrote 32777 and 6690. fdk delay 2048, afterburner 0.

syom is slower on both cells (about 2.46× at 128 kbps and 1.65× at
24 kbps). The encode gate is not passed. Encoder decision math,
`FORMANT_RATIO`, search offsets, SIMD FMA, and the bitstream were left
as they were.

## Decode (TASK-112.3)

`DecodeOptions::unbounded()` (planar split). A peer counts only when
sample rate, channel count, and samples per channel match syom.
Checksums differ and are not the shape.

| cell | shape (all three) | syom median | libfdk median | libavcodec median | syom at or below both |
|---|---|---:|---:|---:|---|
| LC ADTS mono `src/goldens/sine48.adts` | 48000 Hz, 1 ch, 13312 | 0.0534 ms | 0.2630 ms | 0.1786 ms | yes |
| HE-AAC v2 stereo `src/goldens/ps48.adts` | 48000 Hz, 2 ch, 53248 | 1.1230 ms | 2.9011 ms | 1.6440 ms | yes |
| LC 5.1 `src/goldens/mc51.adts` | 48000 Hz, 6 ch, 20480 | 0.5260 ms | 2.0879 ms | 1.1856 ms | yes |

libavcodec consumed the whole file (4452, 2440, and 13294 bytes).
libfdk's printed `aot` is 2 on all three, including `ps48.adts`, while
the PCM is full-rate stereo of 53248 samples (not the 26624-sample
core) and the reported delay is 3730. That is the reconstructed shape,
so the cell is not blank. Same for 5.1: 6 channels, 20480 samples, not
a mono downmix.

Ranges (min–max, ms): syom 0.0527–0.0630, 1.1077–1.1402, 0.5111–0.6947;
libfdk 0.2600–0.2877, 2.8780–3.1955, 2.0721–2.3904; libavcodec
0.1763–0.2407, 1.6225–1.8785, 1.1516–1.4215.

No peer was blank. syom's median is below both peers on every cell.
The decode gate is passed. Decoder math was not changed.

## Commands

```sh
export CARGO_TARGET_DIR=/tmp/syom-t112-speed
export FDK_SRC=/tmp/syom-t112-peers/fdk-aac-2.0.3
export FDK_PREFIX=/tmp/syom-t112-peers/fdk-prefix
python3 lab/baseline/peer_speed/build_fdk.py
# fdk_bench link: gcc -O2 on fdk_adapt.c + fdk_bench.c, then
# zig-c++ -target x86_64-linux-gnu -O2 ... -lfdk-aac -lm
# avc_bench link: gcc -O2 against the FFmpeg 9.0.1 prefix
#   -lavformat -lavcodec -lswresample -lavutil -lm -lpthread
cargo build --release --manifest-path lab/baseline/peer_speed/Cargo.toml

BIN=/tmp/syom-t112-speed/release/syom-peer-speed
FDK=/tmp/syom-t112-peers/fdk_bench
AVC=/tmp/syom-t112-peers/avc_bench
PCM=/tmp/syom-t112-peers/voice-like.f32

taskset -c 0 "$BIN" encode-voice 4 21 "$PCM"
taskset -c 0 "$FDK" bench-encode 48000 2 128000 2 "$PCM" 4 21
taskset -c 0 "$FDK" bench-encode 48000 2 24000 2 "$PCM" 4 21
taskset -c 0 "$BIN" decode src/goldens/sine48.adts 4 21
taskset -c 0 "$FDK" bench-decode src/goldens/sine48.adts 4 21
taskset -c 0 "$AVC" bench-decode src/goldens/sine48.adts 4 21
taskset -c 0 "$BIN" decode src/goldens/ps48.adts 4 21
taskset -c 0 "$FDK" bench-decode src/goldens/ps48.adts 4 21
taskset -c 0 "$AVC" bench-decode src/goldens/ps48.adts 4 21
taskset -c 0 "$BIN" decode src/goldens/mc51.adts 4 21
taskset -c 0 "$FDK" bench-decode src/goldens/mc51.adts 4 21
taskset -c 0 "$AVC" bench-decode src/goldens/mc51.adts 4 21
```

## Byte-preserving LC increment, 2026-11-01

**TASK-112.2 remains In Progress.** This increment improves LC encoding,
but the required FDK tie/win is not established on every cell. No README
speed ranking or SOTA claim is updated. Version stays the existing
**unpublished 0.9.0 candidate**; no tag, publication or deployment occurs.

### Corrected complete-input protocol

Baseline production revision: `e8ceb7eb4ed31220ae7b2f3995d4ef52e5983920`.
An isolated checkout builds those unchanged product sources with the same
current lab harness as the candidate. Candidate source hashes are retained
in [source-manifest.json](peer_speed/runs/lc-encode-20261101/source-manifest.json);
the sources and measurements are committed together. Both use release,
thin LTO, one codegen unit and rustc 1.97.1 on Linux x86_64, Ryzen AI 9 HX 370.

FDK is pinned 2.0.3, generic x86_64 `-O2`, afterburner off, ADTS AOT2.
The adapter includes float-to-INT_PCM conversion inside the measured call.
Same source PCM does not establish equivalent encoder quality.
The corrected adapter drains to `AACENC_ENCODE_EOF`, checks errors and
bounds the drain loop. On the 96000-sample LC input the historical output
covers only `97280 - 2048 = 95232` samples after declared delay; full drain
covers `98304 - 2048 = 96256`. HE v1/v2 omission was also independently
verified. Old outputs are exact prefixes of the complete outputs. See
[drain evidence](peer_speed/runs/lc-encode-20261101/fdk-drain-smoke.json)
and [native build provenance](peer_speed/runs/lc-encode-20261101/native-manifest.json).
Earlier encoder quality comparisons using this adapter need fresh complete-tail
qualification before being promoted; historical numbers are preserved.

Each timed process is pinned to CPU0. Four warmups are followed by 31
measured complete one-shot open/encode/flush/free calls. All six orders of
baseline/candidate/FDK run for all four cells. The table pools all 186
observations per engine/cell; even-sized medians average the middle pair.
No outlier is removed. Process startup, file reads, PCM generation, hashing
and independent FFmpeg validation are outside the in-process wall timer.
Every block retains host load/frequency and all individual observations.
The host is busy, with substantial scheduling modes; these observations
are not a universal speed ranking or proof of performance on other hosts.

| Cell | Baseline ms | Candidate ms | FDK full-drain ms | Candidate vs baseline |
|---|---:|---:|---:|---:|
| original voice stereo 128k, 2s | 12.12985 | 9.02285 | 6.02835 | 25.6% faster |
| original voice stereo 24k, 2s | 6.08920 | 4.78765 | 3.73905 | 21.4% faster |
| CORE_TUNE lecture stereo 24k, 0.25s | 0.36725 | 0.43765 | 0.39745 | 19.2% slower |
| CORE_TUNE lecture mean-mono 24k, 0.25s | 0.31035 | 0.20765 | 0.26695 | 33.1% faster |

The quarter-second stereo pooled result is worse and is not hidden. Because
these very short blocks were scheduling-sensitive, both short cells were
then repeated with all six orders and **301 observations per process** on
the same binaries, retaining all data again. Pooled medians over 1806 samples:

| Short-cell stability check | Baseline ms | Candidate ms | FDK ms |
|---|---:|---:|---:|
| lecture stereo 24k | 0.33750 | 0.31405 | 0.36910 |
| lecture mean-mono 24k | 0.22870 | 0.20580 | 0.20690 |

This supports a short-cell improvement in that run, not an excuse to replace
the first table or declare the full FDK gate passed. Both long voice cells
remain slower than FDK. The original voice-like fixture and actual lecture
are distinct; neither is relabelled as the other.

### Retained changes and evidence

- Reuse invariant short-window analysis and long-frame peak/noise/scalefactor
  terms. Reuse quantized long bands only at the same normalized sf, resetting
  every frame and invalidating explicit refinement mutations.
- Preserve section choices with exact integer prefix sums, including missing
  books, saturated costs and the lower-book tie rule.
- Leave the chosen rate-probe build resident. Write bit fields by bytes,
  emit the fitted AU directly once, and append EXT_FILL at the original
  pre-END bit position rather than serializing the AAC body again.
- Compute four independent TNS autocorrelation lags together while preserving
  the scalar operation order within every sum. FIR batching and forced
  Huffman inlining regressed their micro cases and were rejected; raw data
  and the rejected FIR patch are retained.

Full output bytes match `e8ceb7e` in **60 configurations**, including rate,
layout, CBR/VBR, PNS/intensity, grouping/TNS/refinement/lookahead, HE and LD
variations. This is an actual file-byte comparison, not hash equality alone.
Existing auto/scalar/SSE2 goldens stay unchanged. Independent references
cover cached analysis, quantizer mutations, greedy costs, rate selection,
credit/fullness, band drops, and every padding bit alignment.

The broader `syom_speed` corpus retains all 15 cells (speech/music/noise/click,
ABR128/64 and q5) in baseline/candidate/candidate/baseline order, five warmups
and 31 measured calls. All byte counts/hashes match. The two paired tables
show median gains of **27.0% and 27.4%**, with all 15 cells improving in each
pair. Their bootstrap intervals describe those samples; shared-host systematic
noise remains. Full tables are in the evidence directory.

The tradeoff is **8008 extra fixed heap bytes per LC encoder** and one
constructor allocation; the caches add no per-frame allocations. The section
prefix scratch uses 4992 stack bytes. Isolated footprint release cells
(fat LTO, panic=abort) pass LC/HEv2/MC encoding on 128 KiB stacks.

An extra isolated release latency test fails on first cold decoder Huffman
initialization on **both original e8ceb7e and the retained candidate**.
That pre-existing limitation is recorded, not fixed or disguised by warming
it here. A passing full debug suite (whose test stack is 512 KiB) does not
establish blanket cold decoder support on 128 KiB. No Windows/macOS/ARM
parity is established by these Linux checks.

Local gates pass: fmt, file-size, changelog, clippy all-targets with warnings
denied, 1156 unit tests (7 existing ignored), the allocation integration test,
29 doctests and package audit. Rust 1.89 full lib/tests, all packaged consumer
cells and stateful fuzz seed1122 (8683 iterations, zero panics) also pass.
No hosted GitHub checks are dispatched.

[Full report and reproduction commands](peer_speed/runs/lc-encode-20261101/report.json)
retain source/tool hashes, every measured repetition, exploratory snapshots,
byte comparisons, failed cold-stack tests and accepted/rejected experiments.

```sh
# Build the current harness against both unchanged e8ceb7e and candidate sources.
# Use the same lab files/Cargo release profile in the isolated baseline checkout.
cargo build --release --manifest-path lab/baseline/peer_speed/Cargo.toml
BIN=lab/baseline/peer_speed/target/release/syom-peer-speed
"$BIN" prepare-lc target/lc-speed/fixtures
# Build the pinned FDK source/link the C timer using native-manifest.json commands.
python3 lab/baseline/peer_speed/compare_encode.py \
  --baseline BASELINE_BIN --candidate "$BIN" --fdk FDK_BENCH \
  --fixtures target/lc-speed/fixtures --output target/lc-speed/reproduce \
  --rounds 6 --reps 31
# Separate short-cell stability campaign; keep the original results as well.
python3 lab/baseline/peer_speed/compare_encode.py \
  --baseline BASELINE_BIN --candidate "$BIN" --fdk FDK_BENCH \
  --fixtures target/lc-speed/fixtures --output target/lc-speed/short-reproduce \
  --rounds 6 --reps 301 --cells lecture24 lecture-m24
"$BIN" byte-corpus target/lc-speed/byte-corpus/candidate
# Run byte-corpus on the baseline too, then compare all output file bytes.
```
