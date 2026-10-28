# LC fill decoding allocation fix

Unreleased followup to the 2026-10-28 [0.8.0 audit](REPORT.md), based on
`997bf56efda8700792dbc4b0136a36ad90eca328`. The padded-LC consumer gate
now passes for ABR and CBR, with and without ID3: **zero allocations after
warmup** in each recorded case. This removes the audit's fill-allocation
finding; it does not qualify every profile for a real-time audio callback.

## Decoder change

`ingest_fil` recognizes the opaque `EXT_FILL` type on a copy of the bit
reader, then skips its bounded body directly. Those padding bits have no
value constraints or side information. Other extension types still use
the existing structured parser, preserving SBR, CRC, DRC and normative
`EXT_FILL_DATA` validation. Empty FIL remains legal; a body shorter than
the declared count still fails. No PCM math, encoder decisions or golden
fixtures change.

Three regression tests cover escaped counts through 269 bytes, all eight
bit offsets, exact alignment of the next syntax element, every byte
truncation of 1/8/269-byte padding, invalid patterned fill and reserved
extension types. Existing SBR/DRC and encoder/decoder golden tests pass.

The packaged consumer runner now clears the **release** syom dependency
before rebuilding extracted sources. Cargo archives normalize file mtimes;
without this step, replacing a package with the same version could reuse
old compiled code. This was observed while verifying the fix. The final
consumer log records a rebuild and both consumer binaries passing.

## Verification

| Check | Result |
|---|---|
| Library and integration tests | 1124 + 1 passed, 7 diagnostic tests ignored |
| Documentation examples | 28 passed |
| Rust 1.89 library and integration tests | 1124 + 1 passed |
| Nightly AddressSanitizer and leak detection | 1124 + 1 passed |
| fmt, Clippy with warnings denied, file size, changelog | passed |
| Packaged consumer | all cells passed, ABR/CBR tagged/untagged allocation assertions enforced |
| Package audit and isolated package compile | passed; zero runtime dependencies |
| Long stream and seek rerun | exact chunked PCM, five exact M4A seeks, stable retained heap beyond 600 seconds |

The old ABR consumer recorded 915 allocations / 138934 cumulative bytes.
The fixed ABR and CBR cases each record **0 allocations / 0 cumulative
bytes**, both tagged and untagged. Counts cover the second stream after
workspace and input-buffer warmup, including finish; they do not mean
constructing or warming a decoder requires no allocations.

On the long-stream rerun, live heap at warmup and the last feed is 188077
bytes in both cases; peak incremental live heap is 19910 bytes, relative
to a baseline that includes the decoder and packet. These allocator
measurements are not RSS. Inputs are the same golden/synthetic streams as
the initial audit, not production recordings.

Evidence is preserved in [runs/fill-fix](runs/fill-fix), separate from the
original release measurements. The benchmark table in README still
refers to original 0.8.0; no new throughput ranking is claimed for this
patch. Natural recordings, blind encoder listening, full ISO conformance
and physical mobile qualification remain open.

## Fuzz rerun

Six independent parser/stateful workers, including two AddressSanitizer
workers with leak detection, each ran for 180 seconds on the fixed code.
All exited 0 with no reported panic, sanitizer error or wrapper timeout.
Aggregate child CPU time was 1076.33 seconds. The in-tree mutator and seed
families match the original audit; this bounded regression run does not
replace a long coverage-guided campaign.

| Worker | Iterations | Panics |
|---|---:|---:|
| release-syom_fuzz-2801 | 13111 | 0 |
| release-syom_fuzz-2802 | 13637 | 0 |
| release-syom_fuzz_state-2801 | 270262 | 0 |
| release-syom_fuzz_state-2802 | 251260 | 0 |
| asan-syom_fuzz-2803 | 12630 | 0 |
| asan-syom_fuzz_state-2803 | 233397 | 0 |
| Total | **794297** | **0** |

## Reproduce

```sh
python3 lab/consume/run.py
cargo test --workspace --lib --tests
cargo test --doc
cargo clippy --all-targets -- -D warnings
CARGO_TARGET_DIR=target/release-audit-msrv cargo +1.89.0 test --lib --tests
CARGO_TARGET_DIR=target/release-audit-asan RUSTFLAGS='-Zsanitizer=address' ASAN_OPTIONS=detect_leaks=1 cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --lib --tests
```

The bounded fuzz reproduction command remains in the original audit.
The source hashes and exact verification commands for this working-tree
patch are stored with the followup results.
