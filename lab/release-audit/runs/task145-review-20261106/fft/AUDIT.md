# TASK145 upstream FFT review and test-only refinement

Read AGENTS.md. No runtime correctness/safety defect found in the specialized
first two FFT stages under their power-of-two/table/ISA contract. Twiddle offsets
0, then1/2, then3 match the original radix-2 ordering; no arithmetic shortcut or
new unsafe operation was introduced.

## Prioritized findings

- P2, corrected: `src/engine/imdct_tests.rs` named a test `bits` but used f32
  equality. That conflates +0/-0. Both matrices now use exact per-lane to_bits
  assertions with backend mode, plane, index, size, case and hex diagnostics.
  Input domain remains the existing finite values, signed zeros and subnormals;
  this does not add a general NaN-payload compatibility claim.
- P2, corrected: production FFT256 (Plan1024 divided by4) was absent from both
  test matrices. Added256 alongside the existing64/512 production sizes and
  tiny early-return cases. No new test that merely copies implementation logic.
- P2, deferred and PRE-EXISTING: process-global FORCE_SCALAR/FORCE_NO_AVX flags
  can affect unguarded automatic-dispatch tests during parallel full-suite runs.
  `src/platform_tests.rs:55,99` compute auto outputs before acquiring the mode
  guard; `src/engine/imdct_tests.rs` also calls dispatched mode without that guard.
  The new direct AVX512 test bypasses these flags and genuinely exercises the
  optimized path. Future test-only explicit backend selection would improve
  determinism; no dispatch/global state changed in this refinement.
- P3, deferred: the scalar tail in `src/engine/imdct_avx512.rs:174` is unreachable
  for legal inputs after the first two stages were specialized: remaining half
  sizes are powers of two >=4 and fully consumed by16/8/4-wide loops. Removing
  it could reduce duplicated unsafe audit surface, but no speed claim is made
  and the measured runtime kernel remains frozen.

Only src/engine/imdct_tests.rs was edited. Root owns other Python changes. Test
commands and compiler versions are in results.json; source hashes and runtime
non-modification guards are in source-hashes.json. Focused commands select FFT
unit/golden cases and retain the ignored manual CPU probe. They are not the full
mandatory pre-commit suite. No consumer builds, models or benchmarks were run.
