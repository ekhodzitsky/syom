# Parser fuzz pin (TASK-48)

Isolated from the `syom` package. **Not invoked by `cargo test`.**
Ordinary tests replay `corpus/fuzz/` plus goldens; they must not spawn
this binary.

## Harness

Std-only mutational fuzzer (`lab/fuzz`). No libFuzzer, honggfuzz, AFL,
or `cargo-fuzz`. Product `[dependencies]` stay empty. This crate is not
a workspace member.

```sh
make -C lab/fuzz
# or:
cargo run --release --manifest-path lab/fuzz/Cargo.toml --bin syom_fuzz -- --seconds 30
```

Seeds: committed goldens (`sine48.adts`, `latm48.latm`, `sine441.m4a`,
`he48.adts`) plus `corpus/fuzz/*.bin` (provenance: `corpus/fuzz/SEEDS.md`).

Decode uses `DecodeOptions::speech()` (lecture caps + memory budgets).
Mutations are capped at 64 KiB.

## Sanitizer

Host toolchain is rustc 1.97.1 stable (`rust-toolchain.toml`).
`-Zsanitizer=address` needs nightly and is not part of this pin.
The campaign runs with `catch_unwind`. A debug pass and a release
pass are recorded in REPORT. This is not proof of exhaustive safety
(TASK-103). Ordinary `cargo test` **must not** invoke this binary.
