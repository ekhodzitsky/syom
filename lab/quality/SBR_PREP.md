# HE v1 core-rate preparation (TASK-86)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, HEAD `bfe2684`
(+ this task). Crate-internal `engine/enc_sbr_prep.rs` (`SbrPrep`) and
`engine/enc_sbr_qmf.rs` (`EncAnalysisQmf`). `encode()` stays LC; no
`EncodeOptions` field; no SBR parameters or payload (TASK-87/88).

## What it is

- **2:1 downsample**: 17-tap Hann-windowed sinc halfband (cutoff π/2,
  DC gain 1), one core sample per two output samples, group delay 8
  output = **4 core** samples. Pair-mean was rejected (18 kHz |H| ≈ 0.38).
- **64-band analysis QMF** on the full-rate PCM — the decoder's `X`
  grid (64 subbands × 32 slots per 2048-sample frame; `fTableHigh`
  borders index this grid, so a 32-band full-rate bank cannot express
  odd borders). ISO Figure 4.42 windowing generalised to `M = 64` with
  all 640 `QMF_WINDOW` taps (Table 4.A.89, in-tree); modulation
  `W[k] = Σ_n u[n]·exp(iπ/128·(k+½)(2n−½))` folded into one 128-point
  complex FFT (pre/post twiddles) built from `det_math::sincos` /
  `det_math::twiddle_table`; butterflies are separate mul/add (no FMA
  contraction), no libm on the path. Kernel gain 1 so `|W|²` sits on
  the decoder's `XLow` energy scale (its 32-band core bank uses 2× over
  half the taps).
- **Incremental**: `push` any chunk size; `finish` pads `FIR_DELAY +
  QMF_DRAIN` (8 + 640) zeros so the delayed tail appears; `reset` keeps
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

Encoder 64-band bank vs a direct O(N²) f64 evaluation of the kernel on
the same history (2 kHz + 17.3 kHz mix): max |Δ| < 2e-5 of the peak
magnitude (`fft_modulation_matches_direct_kernel`).

Encoder 64-band bank on full-rate PCM vs the decoder's 32-band
`AnalysisQmf` (f64, libm tables) on the halfband core — same band index
and same energy scale (`encoder_bank_matches_decoder_core_bank_grid_and_scale`):

| tone | band (fs/128) | enc/dec energy ratio |
|---:|---:|---:|
| 2 kHz | 5 | 1.00 |
| 5 kHz | 13 | 1.00 |
| 8 kHz | 21 | 1.06 (halfband droop −0.27 dB on the decoder side) |

Tone selectivity of the 64-band bank (share of total energy in the
nominal band, steady state): 2 kHz → band 5, 0.976; 16 kHz → band 42,
0.976; 23 kHz → band 61, 0.977; 9 kHz sits exactly on the 23/24 border
and splits 0.50. White-noise band-energy spread across the 64 bands:
1.77 dB max/min (no band dropout or alias pile-up).

CPU / workspace (mono, 10 × 2048 output samples, best of 20 after warmup):

| cell | value |
|---|---:|
| `SbrPrep` push + finish | 340 µs |
| LC `encode_with` 48 kHz mono, same PCM | 467 µs |
| ratio | **0.73×** |
| `size_of::<SbrPrep>()` | 2920 B (no heap; QMF plan is a shared `OnceLock`) |

HE_ENC.md budget is **whole HE encode ≤ 4× LC**. Prep alone is 0.73×;
the core LC then runs at half the frame count (≈ 0.5×), leaving ≈ 2.8×
for envelope/noise/writer (TASK-87–89). The debug-mode unit test asserts
the 4× bound only; it is not a benchmark. (A first cut with a 32-band
bank and four DCT-IVs per slot measured 1.67×; superseded.)

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
- Slot phase convention is the decoder's Figure 4.42 form; only `|W|²`
  is asserted against the decoder bank (envelopes need energies).
