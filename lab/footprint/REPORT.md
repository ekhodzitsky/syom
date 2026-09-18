# TASK-99 — toolchain, package and footprint budgets

Measured 2026-09-18, Linux x86_64 (Ryzen AI 9 HX 370).

## Toolchain policy

- `edition = "2024"`, **`rust-version = "1.88"`** (was 1.97). Evidence:
  with the declared version ignored, the library fails on 1.87.0 with
  eight `let` chain errors (stabilized in 1.88) and compiles on 1.88,
  1.94 and 1.97.1; `cargo +1.88 test --lib` passes all 955 tests.
- Development and the five-host CI matrix stay pinned to 1.97.1
  (`rust-toolchain.toml`: rustfmt / clippy output is version-sensitive).
  A separate CI job runs the library tests on 1.88.
- Raising the minimum is a user-visible change: it needs a CHANGELOG entry
  and a reason (a language or std feature the code uses), never a drive-by.

## Packaged crate (`scripts/check-package.py`, run in CI on linux-x64)

| | before | now |
|---|---:|---:|
| files | 568 | 141 |
| compressed `.crate` | 5 141 341 B | 354 988 B |

Before, the package swept in `backlog/`, `lab/`, `corpus/`, scripts,
benches and 4.9 MB of goldens. `include` now ships library sources (no
`*_tests.rs`), README, CHANGELOG and LICENSE. The audit reads the
**packaged** manifest: `[dependencies]` and `[build-dependencies]` empty,
no `build.rs`, no `links`, no target tables — so no native build, link or
download step — and enforces a 512 KiB size budget. `cargo package`
verifies the packaged crate builds. Tests and doc tests need the
repository (goldens are not shipped).

## Minimal consumers (`lab/footprint`, `python3 measure.py [toolchain]`)

Fixed flags: `opt-level 3`, fat LTO, 1 codegen unit, `panic = abort`,
`strip`. Clean target dir; the first row's build time includes compiling
syom itself, the rest are re-links.

rustc 1.97.1:

| consumer | stripped ELF | Δ over base | Δ .text | Δ .rodata | build s |
|---|---:|---:|---:|---:|---:|
| base (no syom call) | 315 008 | — | — | — | 16.7 |
| decode (`decode_with`, every profile and container) | 1 001 608 | 686 600 | 410 561 | 41 760 | 7.2 |
| encode (`encode`, LC ADTS) | 497 552 | 182 544 | 149 154 | 10 528 | 7.4 |
| encode_all (LC M4A, HE v1, HE v2, 5.1) | 699 192 | 384 184 | 319 713 | 24 872 | 12.9 |
| both (decode, re-encode LC) | 1 138 736 | 823 728 | 530 497 | 44 944 | 13.3 |

rustc 1.88.0: decode Δ 691 456, encode Δ 242 464, encode_all Δ 394 016,
both Δ 990 736 (older inlining heuristics; same ordering).

## Budgets (regression guards, re-measure on a toolchain bump)

| cell (1.97.1, Δ over base) | baseline | budget (+15%) |
|---|---:|---:|
| decode | 687 KB | 790 KB |
| encode LC | 183 KB | 210 KB |
| encode_all | 384 KB | 442 KB |
| both | 824 KB | 947 KB |
| packaged crate | 355 KB | 512 KiB (enforced in CI) |

Binary sizes are lab cells, not CI gates: they move with rustc. The crate
size and the dependency audit are gated.

## Feature separation: **no-go**

A consumer that only encodes LC links 183 KB, one that only decodes 687 KB;
link-time dead-code elimination already separates the halves, and
`encode_all` shows HE / PS / surround cost nothing unless called. Cargo
features would add a test matrix (2^n builds) for no measured saving.
Revisit if a `no_std` / embedded target (TASK-105) needs compile-time
exclusion rather than link-time.
