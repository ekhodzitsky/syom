# syom 0.10.2 release verification

Published to [crates.io](https://crates.io/crates/syom/0.10.2) on 2026-11-09
at `2026-11-09T10:15:12.395552Z`. Annotated tag
[`v0.10.2`](https://github.com/ekhodzitsky/syom/releases/tag/v0.10.2)
identifies clean source commit `959f792f6d99736571520d65d94da2b0355bdcfe`.

## Archive and source

- Archive: **472,791 bytes**, **193 UTF-8 text files**.
- SHA-256: `2e3d5eef487d6298fef71a102527efb86f1cca79e75feed5e0c17b3f44b60ee8`.
- The crates.io checksum, downloaded archive and pre-upload dry-run archive match.
- All 190 shipped runtime-source, README, changelog, license/notice and original
  manifest files byte-match the release commit. VCS metadata identifies that
  commit with no dirty flag. The normalized manifest declares Rust 1.89, no
  runtime/build dependencies and no build script.
- `cargo publish --dry-run --locked --registry crates-io` passed from the clean
  source; `cargo publish --locked --registry crates-io` completed successfully.

## Validation scope

Local host: `Linux-7.0.0-42-generic-x86_64-with-glibc2.43`, x86_64; Rust 1.97.1 and 1.89.0.
The full [required local checks](../../AGENTS.md) passed: 86 Python tests,
1205 library tests (7 ignored), 5 integration tests and 31 doctests; the Rust
tests and doctests also passed on MSRV. Formatting, Clippy, documentation links,
file-size, changelog and package checks passed.

`python3 lab/consume/run.py` passed against the packaged 0.10.2 crate, including
exact seeking, bounded presentation and checkpoint resume. The native Apple
consumer built on MSRV; `python3 lab/apple/gapless.py --self-test --out target/apple-gapless-self-test`
passed on Linux with `native_executed: false`.

Parser/state fuzz: build with
`cargo build --release --manifest-path lab/fuzz/Cargo.toml --locked`, then run
`python3 lab/release-audit/run_fuzz.py --seconds 30 --out target/release-fuzz`.
All four workers exited zero, with zero panics:

| Worker | Seed | Observed iterations |
|---|---:|---:|
| parser | 2801 | 1657 |
| parser | 2802 | 1393 |
| state | 2801 | 38941 |
| state | 2802 | 41995 |

Wall-limited iteration counts vary. These are bounded checks, not exhaustive
safety or ISO conformance certification.

The runtime source is byte-identical to platform-tested commit `8eaff2d`:
[CI 42595782757](https://github.com/ekhodzitsky/syom/actions/runs/42595782757)
passed Linux x64/ARM64, Windows x64, macOS ARM64 and MSRV, including native
Apple AAC-LC. The release changes the package version, documentation and
path-dependency lockfiles. Prior [native Apple measurements](../apple/REPORT.md#2026-11-09-narrow-native-gapless-interoperability)
retain their original source and scope; they are not relabeled as release runs.

After publication, both `python3 scripts/check-readme-example.py` and its
`--msrv` variant passed against registry **0.10.2** with the checksum above:
all three Rust examples compiled; two executed, and file I/O remained
compile-only. No path/Git patch substituted for the registry crate.

## Upgrade boundaries

Regenerate tags written with syom's previous incorrect iTunSMPB field order.
Recreate persistent checkpoint caches when upgrading from earlier versions;
cross-version restoration is unsupported. Tag-only sources remain excluded
from persistent checkpoints. No consumer deployment is implied.

Routine stdout, downloaded archives and build products remain in ignored
`target/` or CI artifacts. This compact receipt preserves release identity,
commands and meaningful results without committing raw logs.
