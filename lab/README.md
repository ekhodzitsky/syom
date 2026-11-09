# Offline native comparison lab

[Documentation index](../docs/README.md) · [Development workflow](../CONTRIBUTING.md)

These optional labs are outside the published crate and root Cargo workspace.
Ordinary Cargo checks do not download media or build/run the native, Java and
browser lab drivers. Rust peer crates used in tests and benchmarks are
development dependencies; the published library has no runtime dependencies.
Each experiment has its own prerequisites, source pins, output paths and scope.

## Current evidence

| Question | Record |
|---|---|
| What was published? | [0.10.1 source, tag, checksum and registry checks](release-audit/RELEASE-0.10.1.md) |
| What changed since publication? | [Release/checkout boundary and hosted checks](../docs/verification.md#release-and-checkout) |
| Did native Apple tag-only playback run? | [macOS LC gapless report](apple/REPORT.md#2026-11-09-narrow-native-gapless-interoperability) |
| How did the real consumer behave? | [TASK-145 decoder/HTTP/load qualification](release-audit/TASK145.md) and [audit](release-audit/TASK145-REVIEW.md) |
| What is the checkpoint contract? | [User contract](../docs/persistent-checkpoints.md) and [0.10.0 qualification](release-audit/TASK144.md) |
| Which corpus and provenance are available? | [Committed inventory](../corpus/README.md) |
| Which platforms were actually exercised? | [Hosted source/host records](../docs/verification.md#hosted-checks), [earlier platform records](baseline/PLATFORM.md) and [mobile limits](mobile/REPORT.md) |

The current crate is published; the optional consumer patch is a separately
qualified handoff, not a deployment. Linux checks do not establish other-platform
parity; later [hosted results](../docs/verification.md#hosted-checks) cover their
recorded source/hosts. CPU 0 variability, the raw HTTP equality failure and higher split-mode
RSS remain in the consumer report. The accepted presentation differences use
independent references, not a waived equality result.

## Experiments and reproduction

| Adapter | Pin / purpose | Evidence |
|---|---|---|
| [libavcodec](libavcodec/) | FFmpeg 9.0.1 native `aac` | [Global pins](PIN.md) |
| [fdk](fdk/) | fdk-aac v2.0.3 | [Pin](fdk/PIN.md) · [Report](fdk/REPORT.md) |
| [faad2](faad2/) | FAAD2 2.11.3 (`FAAD_FMT_FLOAT`) | [Pin](faad2/PIN.md) |
| [fdk-aac-rust](fdk-aac-rust/) | fdk-aac-rust 0.2.3 (Rust FDK port) | [Pin](fdk-aac-rust/PIN.md) · [Report](fdk-aac-rust/REPORT.md) |
| [faac](faac/) | FAAC 1.31.1 (`libfaac` ADTS LC) | [Pin](faac/PIN.md) · [Report](faac/REPORT.md) |
| [glint](glint/) | glint-audio 0.11.0 (C++17 AAC-LC) | [Pin](glint/PIN.md) · [Report](glint/REPORT.md) |
| [score](score/) | alignment / SNR diagnostics (not PEAQ) | [Pin](score/PIN.md) · [Report](score/REPORT.md) |
| [baseline](baseline/) | matched-output LC/HE/PS timings | [Pin](baseline/PIN.md) · [Report](baseline/REPORT.md) |
| [prime](prime/) | encoder priming / omitted tail | [Report](prime/REPORT.md) |
| [quality](quality/) | LC SNR/rate; [0.9.0 HE development recheck](quality/HE-REQUALIFY-2026-11-02.md), with full-reference scoring and explicit rate limits | [Pin](quality/PIN.md) · [Report](quality/REPORT.md) |
| [footprint](footprint/) | minimal decode/encode consumers, size budgets, MSRV and package policy | [Report](footprint/REPORT.md) |
| [wasm](wasm/) | wasm32-unknown-unknown smoke under Node, native reference | [Report](wasm/REPORT.md) |
| [consume](consume/) | consumer flows and kover/sluh call patterns against the packaged crate | [Report](consume/REPORT.md) |
| [listen](listen/) | preregistered MUSHRA/BS.1116 protocol (dry-run only) | [Pin](listen/PIN.md) · [Report](listen/REPORT.md) |
| [fuzz](fuzz/) | std mutational parser + stateful stream fuzz | [Pin](fuzz/PIN.md) · [Report](fuzz/REPORT.md) |
| [apple](apple/) | Native LC gapless CI smoke; broader quality/timing/HE qualification pending | [Pin](apple/PIN.md) · [Report](apple/REPORT.md) |
| [aacjs](aacjs/) | AAC.js 0.1.3 / Aurora.js 0.4.9 browser-JS LC peer | [Pin](aacjs/PIN.md) · [Report](aacjs/REPORT.md) |
| [profiles](profiles/) | LD/ELD/USAC feasibility: FDK v2.0.3, libxaac v0.1.13, exhale v1.2.2 | [Pin](profiles/PIN.md) · [Report](profiles/REPORT.md) |
| [oxideav](oxideav/) | oxideav-aac 0.1.7, pure-Rust LC + HE v1 encoder peer | [Pin](oxideav/PIN.md) · [Report](oxideav/REPORT.md) |
| [fmp4](fmp4/) | fragmented-MP4 fixtures, timeline oracles and bounded-input contract | [Pin](fmp4/PIN.md) · [Report](fmp4/REPORT.md) |
| [frame960](frame960/) | 960/120-frame feasibility and offline oracle probes | [Pin](frame960/PIN.md) · [Report](frame960/REPORT.md) |
| [mobile](mobile/) | Android/iOS compile checks and integration evidence; no device runtime qualification | [Pin](mobile/PIN.md) · [Report](mobile/REPORT.md) · [Integration](mobile/INTEGRATION.md) |
| [jcodec](jcodec/) | JCodec 0.2.5 / JAAD AAC peer | [Pin](jcodec/PIN.md) · [Report](jcodec/REPORT.md) |

See [PIN.md](PIN.md) for checksums, configure flags, ISA and lanes.
See [CONTRIBUTING.md](../CONTRIBUTING.md) for local prerequisites, command
side effects and the distinction between checks, generators and measurements.

```sh
# after a local FFmpeg 9.0.1 prefix exists:
make -C lab/libavcodec FFMPEG_PREFIX=/path/to/prefix
python3 lab/smoke.py --driver lab/libavcodec/avc_driver
python3 lab/faad2/smoke.py lab/faad2/faad_driver
```

## Historical qualification

| Version | Original evidence |
|---|---|
| 0.10.0 | [Publication and archive receipt](release-audit/REPORT.md#release-0100), persistent LC state and strict completion |
| 0.9.0 | [Publication](release-audit/REPORT.md#release-090), [cold-start qualification](release-audit/REPORT.md#candidate-090--cold-start-fix) and [exact-seek read/equality records](release-audit/runs/exact-seek-0.9.0/report.json) |
| 0.8.1 | [Publication](release-audit/PUBLISHED-0.8.1.md), [recording qualification](release-audit/CANDIDATE-0.8.1.md), [HTTP canary](release-audit/HTTP-CANARY-0.8.1.md) and [load/fault campaign](release-audit/HTTP-LOAD-0.8.1.md) |
| 0.8.0 | [Original readiness audit](release-audit/REPORT.md#historical-080-audit) and [fill fix](release-audit/FILL-FIX.md) |

These reports retain their original source, version, protocol and limitations.
The older [consumer PR snapshot](https://github.com/ekhodzitsky/gigastt/blob/c64de2e19997ba7c4ecfe1f184488f85274a5b1f/docs/measurements/aac-20261029/README.md)
is historical adapter evidence; use TASK-145 for the later qualified patch.
Running a path-dependent lab against today's checkout produces new evidence,
not a reproduction of an older binary without its recorded sources and flags.
