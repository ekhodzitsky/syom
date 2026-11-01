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

## LC Huffman/planner follow-up — 2026-11-01

This second byte-preserving increment starts at `4d44195`. The version is
still the **unpublished 0.9.0 candidate**. TASK-112.2 remains In Progress:
the complete-input FDK gate fails on both long voice cells. Historical
measurements above are retained; the following results are a new campaign,
not values to splice into an earlier table.

The pre-review phase-two candidate, unchanged complete-drain FDK 2.0.3
adapter and four source PCM cells use
the previous protocol: CPU0, four warmups, 31 measured complete calls, all
six engine orders, 186 observations per engine/cell. Same release profile,
Linux x86_64 host and rustc 1.97.1. All samples, host load/frequency and
binary hashes are in [comparison.json](peer_speed/runs/lc-huffman-20261101/comparison.json).
Its exact source manifest is `phase2-source-manifest.json`. Final review
subsequently found that rolling totals read cost rows for uncoded bands,
where the original planner skipped those rows. The final candidate restores
that behavior and is measured separately below.
FDK build provenance remains in the preceding increment's native manifest.

| Cell | 4d44195 ms | Candidate ms | FDK full-drain ms | Candidate vs baseline |
|---|---:|---:|---:|---:|
| original voice stereo 128k, 2s | 8.29645 | 7.71620 | 5.41200 | 7.0% faster |
| original voice stereo 24k, 2s | 4.72345 | 4.58650 | 3.88055 | 2.9% faster |
| CORE_TUNE lecture stereo 24k, 0.25s | 0.34810 | 0.33280 | 0.40715 | 4.4% faster |
| CORE_TUNE lecture mean-mono 24k, 0.25s | 0.21950 | 0.20315 | 0.22735 | 7.4% faster |

Retained changes:

- Accumulate two bounded Huffman book costs in independent u16 lanes of a
  u32. Compile-time table bounds prevent carries. Only 64-bit targets,
  bands of at most 1024 coefficients and maximum magnitude at most seven
  use this path; other cases retain scalar costs. Immutable tables add
  1904 bytes, with no allocation or unsafe code. Tuple-reference tests
  include exhaustive small quads/pairs and both sides of the length gate.
- Select the Huffman codebook once per band and combine codeword/sign bits
  in one write. Bounded escape words also use one write; larger magnitudes
  retain the generic sequence. The old tuple writer remains the independent
  test oracle, including invalid tuples, partial tails, extreme magnitudes
  and all bit alignments.
- Specialize section header costs and build exact u64 prefix sums with
  rolling totals instead of copying the previous row on every band. The
  all-range reference retains missing-book, near-u32-limit and tie cases.
- Store only active rows/columns of each psychoacoustic spreading matrix.
  This preserves coefficient bits and scalar accumulation order. The two
  48 kHz LC matrices shrink from 20808 to 10388 payload bytes; two fat
  pointers add 16 bytes on x86_64, for **10404 fewer logical bytes per LC
  encoder**. This is not an RSS or allocator-rounded measurement. There
  is no independent proven speed gain from this storage change.

The linked peer harness gains 1488 `.text` bytes and 1968 `.rodata` bytes
under the same release profile. These are binary-section measurements,
not a claim about every downstream consumer's code size. Dense matrix
storage adds no allocation count; packed costs and emission add no heap.

Actual file comparisons match `4d44195` across all **60 configurations**;
all existing encoder goldens remain unchanged. Thus the preceding
increment's equality to `e8ceb7e` is also preserved. Evidence includes an
independent fixed-matrix reference across LC/short/LD rates, zero-band and
custom layouts, and independent Huffman tuple/reference tests.

Rejected experiments are retained separately: skipping inactive-band
invariant calculations did not help short cells; a short-window scalefactor
cache added 1920 bytes for approximately flat timing; a compact u32 section
prefix tier regressed common planner micro cases. Earlier u64 packed-cost
variants and a magnitude-12 fast path were not retained. Planner shortcuts
that avoided prefix construction also failed to beat the retained planner
consistently. Micro results describe their cases, not end-to-end guarantees.

The first broad 15-cell ABBA campaign has substantial regressions, including
click64 and click-q5. Both complete raw pairs and bootstrap output are
retained as `lc2-guard-*` in the evidence directory. Independent dense-only
experiments also show an approximately 1.5x timing mode in both baseline
and candidate, so these observations require separate attribution; they
are not silently discarded as outliers. An additional interleaved,
per-cell campaign is recorded below.

The full local phase-two source checks passed: fmt, size, changelog,
clippy all-targets with warnings denied, **1164 unit tests** (7 existing
ignored), one allocation integration test, 29 doctests and package audit.
Rust **1.89** lib/tests, packaged consumers, stateful fuzz seed1123
(25229 iterations, zero panics) and isolated release LC/HEv2/MC encoder
128 KiB stack cells also passed. These validate Linux x86_64 only. The
pre-existing cold decoder stack failure recorded above remains separate.
GitHub CI was not dispatched; nothing was published or deployed.

Exact source and binary hashes, all observations, micro input sources,
rejected patches and validation logs are retained in
[the follow-up evidence directory](peer_speed/runs/lc-huffman-20261101/report.json).
Reproduce the four-cell comparison with the previous `compare_encode.py`
command, using `4d44195` plus the same current lab harness for baseline.

### Reviewed source freeze and final local checks

Review restored the optional uncoded-row behavior before the accepted source
freeze. Three new reference checks cover empty costs under an all-false
mask, omitted trailing uncoded rows, and rejection of a missing coded row.
The earlier timings above remain explicitly pre-review measurements.

On the reviewed source, all required local gates passed again: fmt,
file-size, changelog, clippy all-targets `-D warnings`, **1167 unit tests**
(7 existing ignored), one integration test, 29 doctests and package audit.
Full Rust 1.89 lib/tests also passed. The packaged consumer passed every
flow, including exact seek and zero-allocation warm streaming. Stateful
fuzz seed1124 completed **37535 iterations** in 30026 ms with zero panics.
The isolated fat-LTO release encoder cells passed on 128 KiB stacks for
LC, HEv2 and multichannel. Actual full-byte comparison again matched all
60 baseline outputs. See [validation.json](peer_speed/runs/lc-huffman-20261101/validation.json)
and [reviewed byte comparison](peer_speed/runs/lc-huffman-20261101/reviewed-byte-identity.json).

The reviewed peer binary adds 1504 `.text` bytes and 1968 `.rodata` bytes
relative to the baseline (the pre-review binary added 1488/1968). Exact
[binary section sizes](peer_speed/runs/lc-huffman-20261101/binary-sections.json)
and [source hashes](peer_speed/runs/lc-huffman-20261101/source-manifest.json)
identify the final code separately from every exploratory prototype.

### Final reviewed speed campaign

After the correctness fix and all builds/checks finished, the same four-cell
FDK comparison was repeated on the final binary, with no concurrent local
build/test jobs. Again, all six orders, four warmups and 31 measurements
per process are retained: 186 samples per engine/cell, no outlier removal.
A preliminary launch failed because the copied binary lacked execute
permission; permission was restored and the complete campaign restarted.
This infrastructure retry is recorded in `launch-note.json`.

| Cell | 4d44195 ms | Reviewed candidate ms | FDK full-drain ms | Candidate vs baseline |
|---|---:|---:|---:|---:|
| original voice stereo 128k, 2s | 7.96235 | 7.57500 | 5.12370 | 4.9% faster |
| original voice stereo 24k, 2s | 4.57235 | 4.36785 | 3.77915 | 4.5% faster |
| CORE_TUNE lecture stereo 24k, 0.25s | 0.33585 | 0.32160 | 0.39125 | 4.2% faster |
| CORE_TUNE lecture mean-mono 24k, 0.25s | 0.20360 | 0.19005 | 0.21450 | 6.7% faster |

[All final comparison observations](peer_speed/runs/lc-huffman-20261101/reviewed-comparison.json)
identify the exact binary hashes and independent complete-stream validation.
The long voice cells still take approximately 48% and 16% more time than
FDK respectively on this run. Same PCM does not imply matched encoder
quality. **The strict speed gate remains unmet.**

The guard harness now accepts an optional exact cell name as its second
argument; the original no-filter invocation still emits the same 15 cells
and byte/hash identities. Unknown names fail explicitly. This allows
shorter alternating blocks rather than an entire corpus per variant.
See [CLI compatibility checks](peer_speed/runs/lc-huffman-20261101/guard-cli-checks/report.json).

An initial interleaved campaign on the pre-review source retained two small
pooled regressions: click64 +2.22% and lecture-st64 +0.86%. Its full
[observations/provenance](peer_speed/runs/lc-huffman-20261101/guards-before-review/report.json)
remain available. The final reviewed source then ran all 15 exact cells in
ABBA/BAAB order, five warmups plus seven measured calls per process,
**120 blocks total** (28 observations per engine/cell). All sizes/FNVs match.
All 15 final pooled medians improve in that run, but music/noise retain
large pair-level scheduling swings, including ratios from about 0.650 to
1.267. The extrema are not a stable speed guarantee. Click128/64/q5 final
pooled times fall by 8.38%/6.40%/4.25%; the earlier regressions are retained,
not reclassified or removed. [Full final guard data](peer_speed/runs/lc-huffman-20261101/guards-final/report.json)
include every sample, host reading and pair ratio.

```sh
python3 lab/baseline/peer_speed/compare_encode.py \
  --baseline BASELINE_4D44195_BIN --candidate REVIEWED_BIN --fdk FDK_BENCH \
  --fixtures target/lc-speed/fixtures --output target/lc-speed/reproduce \
  --rounds 6 --reps 31
# Build the same current speed harness against both product revisions;
# repeat all cells in baseline/candidate/candidate/baseline/candidate/baseline/baseline/candidate order.
taskset -c 0 QUALITY_BIN 7 click-mono-lc64
```

The full report preserves both source freezes and rejected experiments.
No speed ranking is changed in README. Candidate 0.9.0 remains unpublished;
no tag, hosted CI or consumer deployment occurs.
