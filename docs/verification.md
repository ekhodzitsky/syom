# Verification and release status

[Documentation](README.md) · [Usage](usage-and-validation.md) ·
[Development](../CONTRIBUTING.md)

## Release and checkout

| Source | Status | What the evidence covers |
|---|---|---|
| Registry syom 0.10.1 | Published 2026-11-06 | AVX-512 FFT update; [archive, source/tag and registry checks](../lab/release-audit/RELEASE-0.10.1.md) |
| syom 0.10.2 candidate | Publication pending | Standard iTunSMPB field order, bounded metadata parsing, duplicate-tag handling, probe and presentation-seek consistency |

Version 0.10.2 contains the standard iTunSMPB fixes. Version 0.10.1 can
return padded audio for Apple M4A files whose only trim metadata is iTunSMPB;
its own generated tags round-trip but use the wrong field order. Regenerate
old malformed tags when upgrading, and recreate cached checkpoint blobs.

The [changelog](../CHANGELOG.md) records the 0.10.2 changes. Publication and
registry example checks are pending until the release candidate is uploaded.
A validated checkout, a published crate and a deployed consumer are separate
states.

## Hosted checks

GitHub Actions are enabled. The [workflow](../.github/workflows/ci.yml)
runs formatting, offline Python policies, line/changelog checks, Clippy,
library/integration tests and doctests on Linux x64/ARM64, macOS ARM64
and Windows x64. A separate job tests Rust 1.89; Linux x64 audits the package.
The macOS job also runs the isolated native Apple gapless lane.

[Run 42588281199](https://github.com/ekhodzitsky/syom/actions/runs/42588281199)
passed for `d3f33d4` on 2026-11-09, including native Apple execution.
This is evidence for that source, not a retroactive rerun of the registry
archive. Future workflow runs retain their own source and host identities.
The README CI badge follows `main`; a passing PR run is a separate record.
Use [AGENTS.md](../AGENTS.md) for the canonical local checklist.

## Native Apple gapless interoperability

The [native report](../lab/release-audit/runs/issue9-reaudit-20261109/native-macos-arm64.json)
records macOS ARM64 26.7.2 (25G332), frontend/framework identities, commands,
source/encoded hashes and actual afinfo output.

| Native AAC-LC input | Valid samples | Priming | Remainder |
|---|---:|---:|---:|
| 44.1 kHz stereo | 352800 | 2112 | 416 |
| 48 kHz mono | 48013 | 2112 | 51 |

Both original afconvert files already lacked an edit list; tested bytes
equal native encoded bytes. Decode, probe, slice/seekable streaming,
presentation completion and exact seeking matched those counts.
Every channel was bit-identical across syom playback paths. No comparison
of decoded PCM against AudioToolbox, HE/PS qualification, quality ranking
or timing claim follows from these two LC cases.

The lane is isolated from ordinary Cargo tests. Its [pin and commands](../lab/apple/PIN.md#gapless-interoperability-lane-2026-11-09)
describe native execution and the authored Linux self-test. JSON evidence
is retained; generated audio stays in ignored output. The broader historical
Apple quality/timing qualification remains [separate](../lab/apple/REPORT.md).

## Local audit and consumer evidence

The [repeat audit](../lab/release-audit/runs/issue9-reaudit-20261109/audit.json)
and [final local checks](../lab/release-audit/runs/issue9-reaudit-20261109/final/checks.json)
record the implementation scope, Linux host, commands, source/package hashes,
MSRV, packaged consumers and parser/state fuzz. Failure diagnoses and their
resolutions remain available as compact summaries; routine stdout and superseded
runs are omitted. These are bounded tests, not exhaustive safety
or full ISO conformance certification.

The published release has its own [registry receipt](../lab/release-audit/RELEASE-0.10.1.md).
The [TASK-145 qualification](../lab/release-audit/TASK145.md) covers its exact
Git-pinned library and a separate gigastt adapter patch. Two CPU 1 confirmations
pass the six-case timing gate and 29 PCM cases retain exact bits.
A CPU 0 scan failure remains unexplained. The raw HTTP equality canary remains
false; independent presentation references explain accepted differences.
Admission/load/fault gates pass; split-mode RSS remains higher than the standard
decoder. The [subsequent review](../lab/release-audit/TASK145-REVIEW.md) strengthens
verification without rewriting those measurements. This is a qualified handoff,
not evidence of a deployed service.

### Historical consumer checks

| Evidence | Original scope |
|---|---|
| [0.8.1 recording qualification](../lab/release-audit/CANDIDATE-0.8.1.md) | 19 cases from 16 recordings against FFmpeg and the recorded gigastt adapter |
| [0.8.1 publication](../lab/release-audit/PUBLISHED-0.8.1.md) | Padded-LC allocation fix and PNS-history preservation during M4A seek |
| [HTTP ASR canary](../lab/release-audit/HTTP-CANARY-0.8.1.md) | Normalized transcripts and durations matched on 32 speech-format cases |
| [Load/fault qualification](../lab/release-audit/HTTP-LOAD-0.8.1.md) | 224 requests per binary at 1/2/4/8 clients, including timeouts and cancellation recovery |
| [0.9.0 candidate](../lab/release-audit/REPORT.md#candidate-090--cold-start-fix) | Exact comparisons, parser/state fuzz, cold 128 KiB stack checks |

These use their original sources, adapters and workloads. They are not new
measurements of the current checkout. The [consumer PR snapshot](https://github.com/ekhodzitsky/gigastt/blob/c64de2e19997ba7c4ecfe1f184488f85274a5b1f/docs/measurements/aac-20261029/README.md)
also retains its original scope.

## Badges and measurement gaps

The registry and docs badges describe the distribution channel. The CI badge
shows the last hosted `main` result. MSRV is declared in [Cargo.toml](../Cargo.toml)
and tested separately from the ordinary development toolchain. None of these
badges is a codec quality or conformance certificate.

The [2026-11-02 local library-line report](../lab/baseline/COVERAGE.md#local-measurement-2026-11-02)
measures **25802 / 27830 lines (92.71%)** for its recorded runtime sources.
It excludes test files, integration tests, doctests, benches, dependencies and
std. Branch/MC/DC coverage was not enabled. It is a dated measurement rather
than a rolling score; the root README therefore does not present it as a
current coverage badge. The earlier 93.88% measurement remains in that report.
Different source populations prevent a direct coverage-regression claim.

Physical Android/iOS runtime, full ISO conformance and listening qualification
remain open. [Mobile reports](../lab/mobile/REPORT.md), [corpus provenance](../corpus/README.md)
and [listening protocol](../lab/listen/REPORT.md) describe those limits.
Encoder quality leadership remains unproven; the dated [quality comparison](../lab/quality/REPORT.md)
includes LC speech cases that trail FDK. Historical [decoding measurements](comparison.md#historical-decoding-measurements)
retain both favorable and unfavorable results.
