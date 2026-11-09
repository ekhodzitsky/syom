# syom 0.10.3 release verification

Published to [crates.io](https://crates.io/crates/syom/0.10.3) on 2026-11-09
at `2026-11-09T19:50:46.121040Z`. Annotated tag
[`v0.10.3`](https://github.com/ekhodzitsky/syom/releases/tag/v0.10.3)
identifies clean source commit `2be19a2abbaea2261f82819d5e90bba5b8d28ed9`.

## Archive and source

- Archive: **474,095 bytes**, **194 text files**.
- SHA-256: `38d60edf8102a376deb2c2d673dafdb5ca0d40a7ffe7ae38958da9ffba3e6d40`.
- Registry checksum, downloaded archive and clean pre-upload dry-run archive match.
- All 191 shipped source, README, changelog, license/notice and original manifest
  files byte-match the release commit. VCS metadata names that commit without
  a dirty flag. The normalized manifest declares Rust 1.89, no runtime/build
  dependencies and no build script.
- `cargo publish --dry-run --locked --registry crates-io`, clean
  `cargo package --locked` and `cargo publish --locked --registry crates-io`
  exited zero. Cargo's registry-staging archive was used for the dry-run comparison.

## Validation

Local Linux x86_64, rustc 1.97.1 (`8bab26f4f`) and MSRV 1.89.0 (`29483883e`).
All [required local commands](../../AGENTS.md) passed after the version bump:
86 Python tests, 1,216 library tests (7 ignored), 5 integration tests and
31 doctests. Rust tests and doctests also passed on MSRV. Formatting,
strict Clippy, documentation links, file-size, changelog and package checks passed.

[CI run 42686059672](https://github.com/ekhodzitsky/syom/actions/runs/42686059672)
passed Linux x64/ARM64, Windows x64, macOS ARM64 and MSRV for source
`b4b3124aacbfb8147c28c15b2d03d555e6fb2f04`. The tagged source differs only
in the README's registry installation version. Runtime source and Cargo
versions are identical. The macOS artifact reports `native_executed: true`,
`status: passed`, on macOS 26.7.2 ARM64; this verifies the narrow native
AAC-LC gapless lane, not AudioToolbox waveform-quality or speed superiority.

`python3 lab/consume/run.py` passed all four consumer binaries and strict
completion against packaged 0.10.3. Isolated parser/state fuzz used
`cargo run --release --manifest-path lab/fuzz/Cargo.toml --locked --bin <bin>
-- --seconds 30 --seed 11`: `syom_fuzz` completed 1,433 iterations and
`syom_fuzz_state` completed 38,197, both exit 0 with zero panics.
These bounded runs are not exhaustive safety or ISO conformance certification.

The [issue implementation and repeated audits](ISSUE-11.md) retain their
original candidate identities, regression diagnoses and agent-youtube
qualification: 435 consumer tests pass, with three byte-identical full/resumed
PCM comparisons across separate processes. No consumer deployment or dependency
upgrade is implied by this library release.

After publication, `python3 scripts/check-readme-example.py` and its
`--msrv` variant passed against registry **0.10.3** with the checksum above.
All three examples compiled; two executed and file I/O remained compile-only.
No local path or Git dependency substituted for the registry crate.

## Upgrade boundaries

Fractional movie-clock edit durations round down; only sub-tick,
sub-millisecond overhangs are capped. Exact media-time conversion and strict
missing-payload/completion checks remain in force. Damaged-frame skipping or
concealment is not introduced. Recreate persistent checkpoints when upgrading;
cross-version restoration remains unsupported.

Routine stdout, archives, generated media and verification copies remain in
ignored `target/release-0.10.3/` and CI artifacts.
