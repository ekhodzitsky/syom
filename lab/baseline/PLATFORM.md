# TASK-98 native platforms and encoder byte-determinism

Parent: `dc95305` (TASK-82). Ordinary `cargo test` still does not spawn
ffmpeg/FDK, fetch corpora, or mint goldens. Goldens were **not** reminted.

## Qualified native cells (CI matrix)

Pinned toolchain **1.97.1**. Each cell runs fmt, clippy `-D warnings`,
lib+doc tests, file-size, changelog. Host dump uploaded as
`host-<cell>`.

| cell | runner | ISA |
|---|---|---|
| linux-x64 | `ubuntu-latest` | x86_64, SSE2 min, AVX runtime |
| linux-arm64 | `ubuntu-24.04-arm` | aarch64, NEON baseline |
| macos-arm64 | `macos-latest` | aarch64, NEON baseline |
| macos-x64 | `macos-13` | x86_64, SSE2 min |
| windows-x64 | `windows-latest` | x86_64, SSE2 min |

Encoder tripwire on every cell: `enc48.adts` / `enc48m.m4a` /
`enc48t.adts` / `enc48l.adts` re-encoded from the committed `det_math`
fixtures must match the bytes. Scalar FFT and (x86) SSE2-only FFT must
match auto SIMD. A 64 kbps encode has no golden; scalar vs auto still
must match.

## This host (session)

- OS: Linux, `x86_64-unknown-linux-gnu`
- rustc 1.97.1 (`8bab26f4f`), LLVM 22.1.6
- CPU: AVX present (`is_x86_feature_detected`)
- `cargo test --workspace --lib` / `--doc` / clippy / fmt / file-size /
  changelog run here

## Unqualified (not a pass)

| target | why |
|---|---|
| i686 / 32-bit | no runner, no `usize==4` cell; `native_cell_is_64_bit` fails closed |
| wasm32 | TASK-100 |
| Android / iOS | TASK-101 |
| Apple AudioToolbox quality | TASK-10 (no authorized host) |

Minimum ISA: **x86_64 SSE2** (Rust `x86_64-unknown-linux-gnu` baseline)
and **aarch64 NEON**. Scalar `ifft_soa` is the fallback when SSE2 is
absent (`-C target-feature=-sse2`). 32-bit arithmetic of `f32`/`f64` is
IEEE on qualified 64-bit cells; a 32-bit *build* is not claimed.
