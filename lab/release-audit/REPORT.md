# syom 0.8 production readiness audit

Version [0.10.0 / TASK-144](TASK144.md) has local persistent-state and strict
completion qualification. [Release preparation and registry verification](#release-0100)
are recorded separately; historical candidate reports retain their original status.

## Release evidence index

| Status | Evidence |
|---|---|
| 0.10.0 published release, qualified on Linux x86_64 | [Release preflight and registry receipt](#release-0100) |
| 0.9.0 published release, qualified on Linux x86_64 | [Publication and archive verification](#release-090) and [original cold-start qualification](#candidate-090--cold-start-fix) |
| 0.8.1 published release | [Publication verification](PUBLISHED-0.8.1.md) and [recording qualification](CANDIDATE-0.8.1.md) |
| 0.8.0 historical audit | Original findings below, measured on `997bf56` on 2026-10-28 |

Publication, consumer deployment and SOTA eligibility are separate statuses.
The historical zero-allocation no-go below was fixed before the published
0.8.1 release; it is not the current candidate's allocation result.

## Historical 0.8.0 audit

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

## Candidate 0.9.0 — cold-start fix

This ordinary release candidate is independent of the SOTA eligibility
work in TASK-112. The strict FDK encoder-speed gate remains open; the
historical tables above and in the speed report retain their original
versions and results. Version 0.9.0 is not published by this qualification.

Baseline: `80c2c33`, the byte-preserving encoder increment. Its isolated
optimized fresh-process regression fails on a 128 KiB worker stack for
all six tested inputs: LC, HE v1, explicit HE v2, implicit PS, multichannel
LC and LD. Initializing `LazyLock<[HuffTree; 11]>` constructs eleven inline
12 KiB lookup pairs, exceeding the stack before normal decoding begins.
A warm process or the full debug suite's 512 KiB test stack can hide this.

The candidate keeps each lookup as a fixed-size boxed array, constructed
from a heap-filled vector. Fixed lengths preserve lookup bounds and table
contents; no full lookup array is built on the stack. There is no new
unsafe code, product dependency, public API or per-frame allocation.
The change adds 22 once-per-process retained heap allocations and moves
135168 lookup payload bytes from inline static storage to the heap.
Box pointers add 176 bytes across the eleven trees on x86_64, plus any
allocator overhead. These figures are not RSS measurements.

`tests/cold_stack.rs` starts a fresh process for each of the six profiles,
then makes the first decode on a worker thread without prewarming tables.
Optimized builds use 128 KiB, debug builds 512 KiB. The candidate passes
all six and compares cold/warm PCM exactly. The same new regression test
fails all six against unchanged baseline production sources. Original
baseline failure logs remain preserved, not rewritten as passing results.

The same-core warm decode guard runs CPU0 in ABBA/BAAB order, five warmups
and 31 measured complete decode/free calls per process (124 observations
per variant/cell), retaining every sample. Observed candidate/baseline
pooled ratios: LC sine 0.9561, lecture 0.9408, HE v1 1.0126, HE v2 0.9942,
multichannel 0.9601, LD 0.9724. The worst observed slowdown is 1.3%, within
the 3% guard. This is a busy shared Linux x86_64 host; these observations
are not a universal speed ranking or Windows/macOS/ARM qualification.

[Source-pinned qualification evidence](runs/candidate-0.9.0/report.json)
retains the cold failures/successes, all warm timing observations, source
hashes, exact PCM and encoder comparisons, local checks and package audit.

Final local checks pass on the frozen candidate sources: fmt, size,
changelog, clippy all targets with warnings denied, **1167 unit tests**
(7 existing ignored diagnostics), **two integration tests**, 29 doctests,
and package audit. The fresh-process integration test covers six separate
cold children. Full Rust **1.89** lib/tests and the release cold regression
pass. The formerly failing isolated optimized latency test also passes.

Actual output files were compared against unchanged `80c2c33`: **48 decoded
fixtures have identical f32 bits and output shapes**, eight additional
transport fixtures retain identical typed errors, and **60 encoder
configurations match byte for byte**. Both same-version libraries were
explicitly cleaned and rebuilt in one shared target directory before this
comparison, avoiding stale dependency reuse. Initial unqualified snapshot
logs are kept separately and are not used as evidence.

The packaged consumer passes all flows, including exact seek and warm
zero-allocation streaming. Parser fuzz seed1430 completed 2658 iterations;
stateful fuzz seed1431 completed 50000 iterations; both had zero panics.
`cargo publish --dry-run --locked --allow-dirty` verifies the packaged
library and stops before uploading. The local candidate archive contains
183 files, remains below the 512 KiB compressed budget, declares Rust 1.89,
and has no product dependencies or build script. Its dirty VCS metadata
and archive hash distinguish it from any future published package.

The registry check before qualification reports 0.8.1 as the latest release.
No publication or deployment was performed; no new SOTA claim is made. The ordinary candidate is locally qualified within the stated
Linux scope; current Windows/macOS/ARM validation remains outstanding.

## Release 0.9.0

Release preparation dated 2026-11-02. This release includes the exact-seek
API additions, byte-preserving encoder optimizations and cold Huffman stack
fix described in the [changelog](../../CHANGELOG.md). Runtime algorithms
remain those qualified by the preceding candidate; release changes update
installation, version/date labels and public API documentation.

The [release preflight](runs/release-0.9.0/preflight.json) records the local
checks and source hashes. No Windows/macOS/ARM parity, listening superiority
or SOTA claim is added. Hosted GitHub checks remain paused. The existing
workflow triggers main-branch pushes only; release commit messages retain
`[skip ci]`, and tag pushes do not match its branch filter.

Registry publication, downloaded-archive comparison and the exact source
commit/tag are recorded separately after the upload. Historical candidate
reports above preserve their original unpublished status at measurement time.

### Registry receipt

[syom 0.9.0](https://crates.io/crates/syom/0.9.0) was published on
**2026-11-02 at 10:07:32 UTC** (13:07:32 Moscow). Annotated tag `v0.9.0`
points to release source commit `1bb81520e4bda91f17e4e8b79d7c5cab93666a4f`.
The publication-receipt follow-up changes documentation/evidence outside the package only;
it does not change packaged sources.

The registry download is **461255 bytes**, SHA-256:

```text
712d524186d1f1a9d2eff429118a02047a0e784479a7ea5fec7feccbe5a1c911
```

The registry checksum, downloaded bytes and actual uploaded archive match.
Packaged VCS metadata names the clean release commit. All **175 runtime
Rust files**, README, CHANGELOG, license/notices and original manifest
byte-match that commit. The normalized manifest declares Rust 1.89 and no
runtime/build dependencies. The
[machine-readable verification](runs/release-0.9.0/verification.json)
retains source hashes, timestamps, artifact provenance and registry checks.

The exact README example then passed against **registry 0.9.0** on current
Rust and MSRV 1.89, printing `1 channel at 48000 Hz`. Before publication,
the clean-commit dry-run, required local gates, full MSRV tests, six cold
128 KiB release regressions and packaged consumers passed. None of these
Linux checks establishes fresh Windows/macOS/ARM parity or SOTA eligibility.
No consumer deployment or GitHub Actions dispatch was performed.


## Release 0.10.0

Release preparation dated 2026-11-04. Persistent AAC-LC checkpoints and strict
M4A completion are qualified by [TASK-144](TASK144.md). Release preparation
updates version/date labels and the README example; runtime Rust implementation
files remain byte-identical to that candidate (only guide documentation changes).
The original candidate measurements and limits remain preserved.

Local release checks and source hashes are retained in
[runs/release-0.10.0/preflight.json](runs/release-0.10.0/preflight.json).
Registry publication, source commit, tag and downloaded-archive verification
are appended after upload. Qualification is Linux x86_64 only. No SOTA,
Windows/macOS/ARM parity or downstream consumer migration claim is added.
GitHub Actions remain paused; release commits use `[skip ci]`.

### Registry receipt

[syom 0.10.0](https://crates.io/crates/syom/0.10.0) was published on
**2026-11-04 at 13:22:40 UTC** (16:22:40 Moscow). Annotated tag `v0.10.0`
points to clean release source commit
`eef30caa6b1596ee4ba15db81492bd087747426e`; both remote tag and peeled commit
were verified. This receipt follow-up updates evidence outside the package.

The registry archive is **468580 bytes**, SHA-256:

```text
864eacba963d7f6db2815289d4a8572f62a8784ee61f71a013bf9a9b935491ce
```

The API checksum, downloaded archive and clean pre-upload package match.
Packaged VCS metadata identifies the clean release commit. All **179 shipped
Rust sources**, README, CHANGELOG, license/notices and original Cargo manifest
byte-match that commit. The normalized manifest declares Rust 1.89, zero
runtime/build dependencies and no build script. The exact README example
passed against registry 0.10.0 on Rust 1.97.1 and MSRV 1.89.0, printing
`1 channel at 48000 Hz`.

[Machine-readable verification](runs/release-0.10.0/verification.json) retains
publication time, source/tag identity, checksums and source comparison.
[Publication log](runs/release-0.10.0/publish.log),
[clean package verification](runs/release-0.10.0/clean-package.log) and
[clean dry run](runs/release-0.10.0/clean-dry-run.log) are retained.
The first archive inspection found an older precommit `.crate` left by the
package check: Cargo's dry-run used a separate temporary package. A fresh
`cargo package --locked` supplied the clean archive checked before upload;
no stale candidate archive was published. Historical candidate reports keep
their original unpublished status and are superseded only by this receipt.
GitHub Actions were not dispatched; no consumer deployment/migration occurred.
