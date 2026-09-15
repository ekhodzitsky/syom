# TASK-103 library line coverage

`cargo llvm-cov --lib --summary-only --ignore-filename-regex '_tests\.rs$'`
on rustc 1.97.1, linux-x64. Test files excluded. Not a coverage-only
test suite: the instrumented run is the ordinary `--lib` tests.

| | regions | functions | **lines** |
|---|---:|---:|---:|
| covered / total | 27272 / 29738 | 1248 / 1342 | **16204 / 17322** |
| | 91.71% | 93.00% | **93.55%** (≥90% gate) |

## Uncovered critical parser / limit / lifecycle (justified)

| file | line % | why leftover is not a silent gap |
|---|---:|---|
| `error.rs` | 64% | `Display` / unused match arms on `#[non_exhaustive]` variants |
| `engine/adts_crc.rs` | 74% | multi-RDB CRC layout (TASK-17 no-go until 13818-7) |
| `isomp4_io.rs` | 76% | seek-error / short-read IO tails |
| `engine/ps_stereo.rs` | 78% | 34-band hybrid / unused mix helpers |
| `stream/au.rs` | 79% | `from_asc` error tails already fenced |
| `engine/skip.rs` | ~78% | FIL skip of reserved extension types |
| `engine/enc_section_dp.rs` | n/a (dead) | TASK-73 research, not the production greedy path |

ISO 14496-26 bitstreams were not obtained; those rows stay unqualified
(see `corpus/conformance/go-nogo.md`).
