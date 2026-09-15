# TASK-81 QMF DCT-IV modulation

Parent: `781c39d` (TASK-80). Host: Linux x86_64, `taskset -c 0`, rustc
1.97.1, thin LTO / codegen-units=1 (measure crate). 4 warmup + 20
measured reps, median and p95. Isolated lanes: one process per cell so
HE/PS i-cache does not pollute LC. Fixture: `src/goldens/he48.adts`,
`ps48.adts`, `sine48.adts`. aarch64 not measured on this host.

The candidate is a DCT-IV factorization of the ISO Figure 4.42 / 4.43
kernels (analysis: four length-32 DCT-IV; synthesis: DCT-IV + DST-IV of
length 64). DST-IV is a sign-flip plus reversal of DCT-IV. DCT-IV itself
is a 2N-point radix-2 FFT with pre/post twiddles. Twiddle tables are
interned spec constants via libm at init (decoder path). A future
encoder reuse must rebuild those tables with `det_math` and keep
butterflies FMA-free. The dense GEMV remains the test reference.
Goldens were not reminted.

## Parent (GEMV)

| cell | median_ns | p95_ns |
|---|---|---|
| QMF kernel 4096 slots (same-binary GEMV half) | 17_947_271 | — |
| HE unbounded isolated | **1_349_450** | 1_411_235 |
| LC speech isolated | 106_429 | 114_404 |

Combined-process parent HE (HE then PS then LC) median 1_376_801 ns.

## After (DCT-IV)

| cell | median_ns | vs parent |
|---|---|---|
| QMF kernel 4096 slots | 10_241_908 | **−43%** (gate ≥25%) |
| HE unbounded isolated | **940_493** | **−30.3%** (gate ≥10%) |
| HE p95 isolated | 972_042 | −31.1% |
| LC speech isolated | 102_142 | −4.0% (no regression) |
| LC p95 isolated | 108_854 | −4.9% |

PS combined-process median 7.34 ms → 4.64 ms (−37%); not a named gate.

Numeric: impulse / random / tonal / silence / 256-slot state match the
GEMV reference (analysis 1e-20, synthesis 1e-12). HE/PS goldens unchanged.

## Decision

| Lane | Decision |
|---|---|
| DCT-IV QMF as production modulation | **go** |
| ≥25% QMF CPU | **go** (43%) |
| ≥10% whole-HE throughput | **go** (30%) |
| LC / p95 DOC-3 guards | **go** (isolated LC not slower) |
| Goldens / `speech()` default | unchanged |
| aarch64 native cell | not measured (same host as TASK-15) |
