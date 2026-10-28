# syom 0.8 production readiness audit

Later [0.8.1 candidate qualification](CANDIDATE-0.8.1.md) adds real recordings
and fixes a PNS seek defect absent from these original short fixtures.

The [unreleased fill fix](FILL-FIX.md) resolves this audit's padded-LC
allocation finding. Results below describe the original 0.8.0 revision;
the followup preserves them and records new verification separately.

Recorded 2026-10-28 on Linux x86_64. Engine revision:
`997bf56efda8700792dbc4b0136a36ad90eca328` (`v0.8.0`). Codec sources,
root Cargo.toml and root Cargo.lock are unchanged. Changes accompanying
this audit affect documentation and isolated lab tools.

**Conditional go for a controlled backend AAC decoder.** Core checks,
MSRV, AddressSanitizer, exact chunked output, long-stream retained heap
and M4A seeking pass on this host. **No-go for a zero-allocation decode
contract:** the packaged consumer allocation gate fails. General encoder
quality leadership, complete ISO conformance, physical mobile runtime
qualification and production traffic qualification remain open.

## Verification results

| Check | Result | Evidence |
|---|---|---|
| Library and integration tests | 1121 + 1 passed, 7 ignored diagnostics | runs/checks.txt |
| Documentation examples | 28 passed | runs/checks.txt |
| fmt, Clippy all targets with warnings denied | passed | runs/checks.txt |
| Rust 1.89.0 library and integration tests | passed | runs/checks.txt |
| Nightly ASan library and integration tests | 1121 + 1 passed; leak detection enabled | runs/checks.txt |
| Package audit and isolated package compile | passed; zero runtime dependencies | runs/checks.txt |
| Corpus manifest and hashes | passed: 82 entries, 24 registered natural excerpts | runs/checks.txt |
| Packaged consumer scenarios | allocation gate failed; other cells pass | runs/consumer.log |
| Consumer extraction patterns | kover/sluh model flows passed | runs/consumers.log |
| Long streams and seek | passed | runs/long-flows.log |

The corpus check validates registered metadata and available hashes. It
permits explicitly missing external recordings; it does not establish
that the 24 natural excerpts exist in this checkout. No user production
media directory was supplied for this audit. Ordinary tests use offline
committed oracles and do not invoke external decoders.

Three existing dead-code warnings appear when syom is built as a release
dependency (`enc_section_dp` research helpers). Root Clippy passes. They
are not new package build failures.

## Current Rust decoder comparison

Published versions were resolved from crates.io on 2026-10-28:
rusty_aac **2.0.1**, Symphonia **0.6.1**, oxideav-aac **0.1.7**.
The previous README compared rusty_aac 0.5.0. Its millisecond-scale
figures must not be attributed to current rusty_aac.

The adapters derive from `benches/peers.rs`, with strict ADTS consumption
and decoder error reporting. The isolated Cargo.lock pins all transitive
dependencies (including oxideav-core 0.1.42). Engine initialization,
decoding, planar f32 collection, layout conversion and destruction are
inside the timer; input reads and preflight checksums are outside.
Every timed candidate passes `syom::decode_cmp::run_preflight` for equal
rate, channels, samples per channel and finite output. A failed candidate
aborts its whole group. Different checksums are recorded, not treated as
waveform equivalence or perceptual quality.

Both protocols use release thin LTO, one codegen unit, CPU affinity 0,
and rotate peer order per repetition. The host is an AMD Ryzen AI 9 HX
370, rustc 1.97.1. The four-peer pass uses 4 warmup + 21 measurements;
the separate fast-peer pass uses 8 + 101 and omits oxideav to avoid its
long calls dominating the run. The host was busy: the four-peer pass
started at load average 20.97 and contains large scheduling outliers.
No outliers were removed. Values from the two protocols are kept separate.

Fast-peer pass, median / p95 in microseconds:

| Fixture | syom | rusty_aac | Symphonia |
|---|---:|---:|---:|
| LC ADTS mono, 48000 Hz, 13312 samples | 59.6 / 68.3 | 152.3 / 160.7 | 135.2 / 143.7 |
| HE ADTS stereo, 48000 Hz, 18432 samples | 190.8 / 198.2 | 354.9 / 366.9 | core-only, excluded |
| HE v2 ADTS stereo, 48000 Hz, 53248 samples | 1211.1 / 1304.8 | 1120.2 / 1195.4 | core-only, excluded |
| LC 5.1 ADTS, 48000 Hz, 20480 samples | 601.3 / 633.1 | 678.3 / 697.4 | decoder unavailable |

syom has the lower median on these LC and HE v1 cells; rusty_aac has the
lower median on HE v2. This supports profile-specific observations, not
an all-content or all-platform speed claim. The four-peer medians and
all raw samples are in [runs/peers.log](runs/peers.log); fast-peer raw
samples are in [runs/peers-fast.log](runs/peers-fast.log).

M4A: rusty_aac/oxideav adapters do not demux the container; that does not
mean their raw decoders cannot decode its AAC access units. On
`sine441.m4a`, syom emits 11025 samples and Symphonia 12288, so they are
not timed against each other. On `lecture.m4a`, Symphonia returns
`unsupported feature: aac: predictor data`, aborting the group. Both
benchmark commands therefore exit **1** after reporting the earlier
valid groups. That is a recorded peer failure, not a passing entire matrix.
oxideav's `he48.adts` output is mono while syom's is dual-mono stereo,
so it is excluded from that cell.

Native FDK/libavcodec comparisons and encoder tuning were not rebuilt in
this audit. The [2026-10-16 in-process report](../baseline/SOTA-SPEED-2026-10-16.md)
remains historical: syom LC encode was 2.46x / 1.65x slower than FDK at
128 / 24 kbps on its two synthetic stereo cells. Its libavcodec build
also disabled x86 assembly. Those results do not establish current
production superiority. Encoder speed without matched-rate quality is
not a quality ranking.

## Bounded fuzz and sanitizer results

Six workers each ran for 180 seconds: two parser and two stateful release
workers (seeds 2801/2802), plus one parser and one stateful ASan worker
(seed 2803). All exited 0 with no reported panic, sanitizer error or
wrapper timeout. Actual aggregate child CPU time was **978.08 seconds**,
not 24 CPU-hours. This is a bounded regression campaign; the earlier
[24 CPU-hour campaign](../baseline/CAMPAIGN.md) remains separate history.

| Worker | Iterations | Panics |
|---|---:|---:|
| release parser 2801 | 12054 | 0 |
| release parser 2802 | 11120 | 0 |
| release stateful 2801 | 224295 | 0 |
| release stateful 2802 | 206077 | 0 |
| ASan parser 2803 | 10697 | 0 |
| ASan stateful 2803 | 204336 | 0 |
| Total | **668579** | **0** |

The parser seed set adds LC960 LOAS/M4A, a generated CBR stream and
validated authored Main/SSR/LTP streams. A raw ASC/AU mutation path
exercises the LD480 grid. Parser decode checks finite PCM and uses a
10-second duration limit, 16 MiB output, 1 MiB metadata and 16384 index
entries; mutated inputs are at most 64 KiB. Probe also runs. Stateful
workers exercise ABR and CBR encoders, feed/finish/reset, failing callbacks,
invalid PCM and the expanded container/profile inputs. This is an in-tree
mutator, not coverage-guided libFuzzer/AFL. Branch coverage was not
remeasured; historical 93.88% line coverage is not a current 0.8.0 result.

Per-worker output and statuses are in `runs/*280*.log` and
[runs/fuzz-summary.json](runs/fuzz-summary.json). The ASan toolchain is
nightly 1.100.0 (574ff7d98); rust-src and `-Zbuild-std` instrument the
standard library. Leak detection is enabled. The stable and ASan logs
refer to the same unchanged product codec revision.

## Long stream and integration findings

`flows` checks exact PCM hashes and counts across chunks of 1, 7, 1319
and 65536 bytes for LC, HE v1, HE v2, 5.1, fMP4 and LD fixtures. All pass.
A synthetic 120-second M4A preserves the source length and matches linear
decode at five sample positions, including positions inside a frame.

A repeated LC frame stream emits **28807168 samples**, over 600 seconds.
Live process heap after warmup and at the last feed is **188077 bytes**
in both cases. Peak incremental live heap is **19910 bytes** relative to
this cell's baseline, which already includes the decoder object and
packet. These are in-process allocator counters, not peak RSS or total
process memory. Repeated frames exercise duration scaling, not content
diversity. No PCM is collected in the long-stream callback.

The old packaged-consumer harness assumed ADTS had no trim metadata and
sliced away another 1024 samples. In 0.8.0 the one-shot encoder prefixes
ID3/iTunSMPB and decode already trims it. The harness now validates exact
source length and probe's exact duration. Transport sniffers still
inspect offset zero: a caller must peel ID3 before using `sniff_is_adts`
or `sniff_aac` as a hard ingestion filter. `decode` handles the tag.

The allocation assertion remains enforced and fails: **915 allocations /
138934 cumulative bytes** after warmup on the 10-second padded LC case,
with or without ID3. An allocation backtrace reaches
`extension_payload::read_packed_bits` through `parse_fill`: EXT_FILL
padding is copied into temporary vectors. The test intentionally stays
red until the zero-allocation requirement is restored. Bounded retained
memory and zero allocation churn are different requirements. No codec
change or golden regeneration was made to hide this finding.

An untagged ADTS stream cut inside its final frame still succeeds with
1024 fewer samples. Strict completeness consumers must compare expected
length or validate framing; successful decode alone does not prove a
complete upload. The malformed Main sample in the old lab is now labeled
as malformed payload rather than as an unsupported AAC profile.

## Supported production envelope

| Use | Audit disposition |
|---|---|
| Backend LC/HE decode with explicit limits and supported containers | Conditional go on the tested host; qualify the consumer's real files before broad rollout |
| Music or multichannel output | Use DecodeOptions::audio; default decode mixes to mono |
| Bounded long streaming and LC M4A seek | Passed in the recorded synthetic and golden cases |
| Audio callback requiring no allocations | No-go: padded LC consumer gate fails |
| New CBR, Main/LTP, SSR and 960/480 frame paths | Tested and fuzzed within authored/golden coverage; broader independent qualification remains open; SSR short windows unsupported |
| LC/HE/LD encoder superiority | Not established; no completed blind holdout study |
| Arbitrary MP4, DRM, multitrack fragments | Outside the documented bounded fMP4 envelope |
| Physical iOS/Android runtime guarantees | Not qualified by this Linux audit |
| ELD or USAC decoding/encoding | Unsupported |

Existing TASK-120 / TASK-112.4 cover the natural holdout and listening
work, TASK-123 covers physical mobile devices, and TASK-112 remains open
for the full release/SOTA qualification. The fill allocation finding is
tracked separately as TASK-140. A controlled backend deployment and a
universal-best claim have different acceptance criteria.

## Reproduce

From the repository root:

```sh
cargo build --release --locked --manifest-path lab/release-audit/Cargo.toml
taskset -c 0 lab/release-audit/target/release/syom-release-audit
taskset -c 0 lab/release-audit/target/release/syom-release-audit --fast
lab/release-audit/target/release/flows
python3 lab/consume/run.py
cargo run --release --manifest-path lab/consume/Cargo.toml --bin consumers
cargo build --release --locked --manifest-path lab/fuzz/Cargo.toml
CARGO_TARGET_DIR=target/release-audit-fuzz-asan RUSTFLAGS='-Zsanitizer=address' cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu --release --locked --manifest-path lab/fuzz/Cargo.toml
python3 lab/release-audit/run_fuzz.py --seconds 180 --out target/release-audit-rerun --asan-bin target/release-audit-fuzz-asan/x86_64-unknown-linux-gnu/release
CARGO_TARGET_DIR=target/release-audit-asan RUSTFLAGS='-Zsanitizer=address' ASAN_OPTIONS=detect_leaks=1 cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
```

The consumer runner exits 1 on the allocation gate and stops before its
second binary; run `consumers` separately as above. Nightly requires the
rust-src component. Toolchain identifiers and fixture SHA256 values are
in [runs/host.json](runs/host.json). Registry crate checksums are in
Cargo.lock. The 7 ignored library tests print lab timing/alignment tables
and are not counted as passing tests.
