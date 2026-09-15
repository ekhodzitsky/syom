# TASK-82 x86 SIMD transforms

Parent: `7910f48` (TASK-81). Host: Linux x86_64 (AMD, SSE2 compile-time
baseline; AVX/AVX2/FMA present at runtime). rustc 1.97.1. Isolated
whole-codec: thin LTO, codegen-units=1, `taskset -c 0`, 4 warmup + 20
reps. Transform probe: same-binary scalar vs dispatched SIMD, 512-point
SoA FFT (N=2048 IMDCT inner size), 8000 reps, `copy_from_slice` reset.
aarch64 already had NEON 4-wide; this task adds the x86 family.

Kernels: 4-wide SSE2 and 8-wide AVX FFT butterflies, **mul/add/sub
only** (no FMA). Runtime `is_x86_feature_detected!("sse2"|"avx")` before
`#[target_feature]`. Declared minimum x86_64 target is **SSE2** (Rust
`x86_64-unknown-linux-gnu`); scalar `ifft_soa_scalar` is the fallback
and the bit-exact oracle. Encoder MDCT shares `ifft_soa`, so bytes stay
exact iff SIMD matches scalar IEEE — tests assert `assert_eq!` on f32
bits. Goldens not reminted.

## Parent (scalar x86)

| cell | median_ns | p95_ns |
|---|---|---|
| FFT n=512 (scalar half of same-binary probe) | 28_410_722 | — |
| LC speech isolated | **104_055** | 121_568 |
| HE unbounded isolated | 947_236 | 971_070 |
| LC encode isolated | 498_595 | 521_217 |

## After (SSE2 + AVX)

| cell | median_ns | vs parent |
|---|---|---|
| FFT n=512 SIMD | 21_578_684 | **−24.0%** (gate ≥15%) |
| LC speech isolated | **96_631** | **−7.1%** (gate ≥5% whole-codec also met) |
| LC p95 | 111_028 | −8.7% |
| HE unbounded isolated | 935_243 | −1.3% |
| HE p95 | 1_016_847 | +4.7% (< 5% DOC-3 p95 guard) |
| LC encode isolated | 479_018 | −3.9% |

Code size: `butterfly4` 115 B, `butterfly8` 126 B `.text`. Workspace
rlib +124 KiB (metadata-heavy; not a .text delta).

Numeric: random / impulse / silence / Nyquist, n ∈ {8,16,64,512},
SIMD bits == scalar. Encoder `enc48` / `enc48m` / `enc48t` / `enc48l`
ADTS goldens byte-exact.

## Decision

| Lane | Decision |
|---|---|
| x86 SSE2/AVX FFT as production | **go** |
| ≥15% transform | **go** (24%) |
| ≥5% whole-codec | **go** on LC decode (7%) |
| Encoder bytes exact | **go** |
| LC/HE p95 DOC-3 | **go** (HE p95 +4.7%) |
| Goldens / `speech()` | unchanged |
| aarch64 | existing NEON, not re-measured |
