# Library line coverage

## Local measurement 2026-11-02

**92.71% of library lines covered: 25802 / 27830.** Local Linux x86_64,
rustc 1.97.1, cargo-llvm-cov 0.9.1. All 166 reported source files are
byte-identical to runtime revision `13d8fd7`; accompanying changes are
documentation/tooling only. Full source hashes, all per-file metrics and
raw test output are retained in [the measurement record](runs/coverage-20261102/report.json).

| Metric | Covered / total | Percent |
|---|---:|---:|
| Lines | 25802 / 27830 | 92.71% |
| Functions | 2002 / 2147 | 93.25% |
| Regions | 44496 / 48756 | 91.26% |

Scope: ordinary library unit tests, **1167 passed, 7 existing ignored**.
Files ending `_tests.rs` are excluded from the report; shipped hidden
comparison helpers are included. Integration tests, doctests, benchmarks,
dependencies and std are not measured. Only executable source locations
enter LLVM's denominator; this is not a percentage of all repository text.
Branch coverage and MC/DC were not enabled, so zero branch counts in the
export do not mean measured 0% branch coverage. The run does not replace
independent waveform, hostile-input, cold-process or platform qualification.

```sh
CARGO_TARGET_DIR="$PWD/target" CARGO_INCREMENTAL=0 cargo llvm-cov --lib \
  --ignore-filename-regex '_tests\.rs$' --json --summary-only \
  --output-path target/documentation-coverage-20261102.json
```

The tool and llvm-tools component were already installed. The run used the
local shared build tree and dispatched no GitHub Actions. Coverage is a
source-pinned snapshot, not a rolling guarantee about future commits.
The earlier 93.88% used a different source population; these percentages
are not an equal-denominator regression measurement.

## Historical measurement 2026-09-29

The following TASK-103 result and its original gap rationale are preserved
as the historical report. Rust versions, thresholds and missing-vector
notes below refer to that run.

`cargo llvm-cov --lib --ignore-filename-regex '_tests\.rs$'` on rustc
1.97.1, linux-x64, 2026-09-29 (1078 passed, 7 ignored). Test files,
benches and the standard library are excluded. Not a coverage-only
test suite: the instrumented run is the ordinary `--lib` tests.

| | regions | functions | **lines** |
|---|---:|---:|---:|
| covered / total | 35458 / 38545 | 1663 / 1782 | **20941 / 22306** |
| | 91.99% | 93.32% | **93.88%** (≥90% gate) |

### Uncovered critical parser / limit / lifecycle (justified)

| file | line % | why leftover is not a silent gap |
|---|---:|---|
| `error.rs` | 66% | `Display` / unused match arms on `#[non_exhaustive]` variants |
| `engine/adts_crc.rs` | 74% | multi-RDB CRC layout (TASK-17 no-go until 13818-7) |
| `isomp4_io.rs` | 82% | seek-error / short-read IO tails |
| `engine/ps_stereo.rs` | 78% | 34-band hybrid / unused mix helpers |
| `stream/au.rs` | 79% | `from_asc` error tails already fenced |
| `engine/skip.rs` | 96% | one line left; region coverage 78% is reserved FIL extension types |
| `engine/enc_section_dp.rs` | 96% | TASK-73 research helpers; the production planner stays greedy |

ISO 14496-26 bitstreams were not obtained; those rows stay unqualified
(see `corpus/conformance/go-nogo.md`).
