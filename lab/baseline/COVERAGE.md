# TASK-103 library line coverage

`cargo llvm-cov --lib --ignore-filename-regex '_tests\.rs$'` on rustc
1.97.1, linux-x64, 2026-09-29 (1078 passed, 7 ignored). Test files,
benches and the standard library are excluded. Not a coverage-only
test suite: the instrumented run is the ordinary `--lib` tests.

| | regions | functions | **lines** |
|---|---:|---:|---:|
| covered / total | 35458 / 38545 | 1663 / 1782 | **20941 / 22306** |
| | 91.99% | 93.32% | **93.88%** (≥90% gate) |

## Uncovered critical parser / limit / lifecycle (justified)

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
