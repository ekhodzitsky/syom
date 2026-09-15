# HE v1 core-rate preparation (TASK-86)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, HEAD `bfe2684`
(+ this task). Crate-internal `engine/enc_sbr_prep.rs` (`SbrPrep`) and
`engine/enc_sbr_qmf.rs` (`EncAnalysisQmf`). `encode()` stays LC; no
`EncodeOptions` field; no SBR parameters or payload (TASK-87/88).

## What it is

- **2:1 downsample**: 17-tap Hann-windowed sinc halfband (cutoff π/2,
  DC gain 1), one core sample per two output samples, group delay 8
  output = **4 core** samples. Pair-mean was rejected (18 kHz |H| ≈ 0.38).
- **32-band analysis QMF** on the full-rate PCM: ISO Figure 4.42
  windowing (`QMF_WINDOW` Table 4.A.89, in-tree) + the same DCT-IV /
  DST-IV factorization as the decoder bank (`sbr_qmf_dct.rs`), rebuilt
  in `f32` with `det_math::sincos` twiddles; butterflies are written
  as separate mul/add (no FMA contraction), no libm on the path.
- **Incremental**: `push` any chunk size; `finish` pads `FIR_DELAY +
  QMF_DRAIN` (8 + 320) zeros so the delayed tail appears; `reset` keeps
  storage. Exact `source` / `core` / `slots` counters.

## Measurements (release, `cargo test --release`, one-off scratch test)

Downsampler, 0.5-amplitude tones at 48 kHz, Goertzel of the core
(24 kHz) at `f` (or its alias `24k − f`) relative to the source:

| f (Hz) | core level rel. source (dB) |
|---:|---:|
| 1000 | −0.02 |
| 4000 | −0.02 |
| 8000 | −0.27 |
| 10000 | −1.91 |
| 11000 | −3.59 |
| 12000 | −71.4 (band edge, alias of itself) |
| 13000 | −9.5 |
| 14000 | −14.2 |
| 16000 | −30.7 |
| 18000 | −43.7 |
| 20000 | −63.2 |
| 22000 | −58.8 |

Impulse at output index 100 → core peak index 54 = (100 + 8) / 2, value
0.4992 (centre tap). White noise RMS core / source = −3.48 dB
(ideal halfband −3.01 dB; the rest is the 10–13 kHz transition).

Encoder QMF vs decoder `AnalysisQmf` (f64, libm tables), tone slots:

| tone | peak band | Σ|ΔE| / Σmax(E) | max |Δre/Δim| |
|---:|---:|---:|---:|
| 2 kHz | 2 | 2.4e-7 | 6.1e-6 |
| 9 kHz | 11 (band edge 12) | 1.7e-7 | 5.5e-6 |
| 16 kHz | 21 | 1.5e-7 | 8.5e-6 |
| 23 kHz | 30 | 1.8e-7 | 8.3e-6 |

White-noise band-energy spread across the 32 bands: 1.39 dB max/min
(no band dropout or alias pile-up).

CPU / workspace (mono, 10 × 2048 output samples, best of 20 after warmup):

| cell | value |
|---|---:|
| `SbrPrep` push + finish | 792 µs |
| LC `encode_with` 48 kHz mono, same PCM | 474 µs |
| ratio | **1.67×** |
| `size_of::<SbrPrep>()` | 1512 B (no heap; QMF plan is a shared `OnceLock`) |

HE_ENC.md budget is **whole HE encode ≤ 4× LC**. Prep alone is 1.67×;
the core LC then runs at half the frame count (≈ 0.5×), leaving ≈ 1.8×
for envelope/noise/writer (TASK-87–89). The debug-mode unit test asserts
the 4× bound only; it is not a benchmark. A cheaper modulation
(N/2-point complex FFT per DCT-IV instead of the 2N-point one shared with
the decoder) is a follow-up if TASK-89 misses the budget.

## Determinism

`enc_sbr_*` contain no `powf` / `.sin()` / `.cos()` / `f32::sin`
(`encoder_files_have_no_libm_decision_path`); twiddles and phases come
from `det_math::sincos`. Chunking `[1 | 31 | rest]` vs one push and
push→reset→push give bit-identical core PCM and slots
(`chunking_and_reset_are_identity`). Cross-platform byte hashes of the
prep output are **not yet in CI**: the native matrix (TASK-98) asserts
encoder goldens, and SBR output only reaches a golden at TASK-90.

## Not claimed

- No SBR encode, no HE bitstream, no quality number.
- No aarch64 run in this session (the CI matrix will cover it once a
  golden exists).
- 44.1/22.05 kHz families reuse the same taps (cutoff is relative).
