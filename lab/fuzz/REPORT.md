# TASK-48 initial parser-fuzz campaign

Not invoked by `cargo test`. This is not proof of exhaustive safety.

## Toolchain / host

- rustc 1.97.1 (`8bab26f4f 2026-07-14`), `x86_64-unknown-linux-gnu`
- Host: Linux x86_64
- Harness: `lab/fuzz` std mutator, `catch_unwind` around
  `syom::decode_with(..., DecodeOptions::speech())` + sniff
- No libFuzzer / AFL / `-Zsanitizer` (stable pin; AddressSanitizer
  needs nightly)

## Seeds (11)

Valid goldens: `sine48.adts` (4452 B), `latm48.latm` (2352 B),
`sine441.m4a` (4907 B), `he48.adts` (832 B).

Malformed (`corpus/fuzz/SEEDS.md`): empty, ADTS truncated / length 5 /
reserved srate, ASC Main AOT, LOAS truncated, M4A truncated ftyp.

## Runs

| profile | wall | iters | crashes (panics) |
|---|---:|---:|---:|
| debug | 15.005 s | 7312 | 0 |
| release | 30.000 s | 224252 | 0 |

Command: `cargo run --release --manifest-path lab/fuzz/Cargo.toml --bin syom_fuzz -- --seconds 30 --iters 2000000`
(the iter cap was not reached; wall-clock stopped the run).

## Findings

None minimized. No panic on this campaign. Ordinary tests already
assert the known-invalid ADTS length seed reaches
`Error::AdtsFrameLengthTooSmall`.

## Limits

Speech decode caps and a 64 KiB mutation ceiling bound work. This
campaign is an initial smoke, not TASK-103 qualification. Stateful
`Decoder`/`Encoder` fuzz is TASK-49.
