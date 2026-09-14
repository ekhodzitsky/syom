# Bandwise leftover-bit scalefactor refine (TASK-74)

Recorded 2026-09-14. Isolated notes; ordinary tests never spawn oracles.
PEAQ unavailable: declared equivalent is priming-aligned SNR (TASK-68/69),
gate **≥ 0.3 dB** at matched payload (~0.1 ODG).

## Baseline

One global sf offset (binary search), then drop-bands / pad. Per-band sf
is `sf_for_peak(peak, target_q) + offset` with ±60 DPCM. Psy mask and
greedy section plan stay fixed.

## Candidate

`EncodeOptions::with_band_refine` (default **off**). Long frames only.
After the global offset, up to 16 successful `sf[b] -= 1` on the coded
band with max `energy/(qmax+1)` among bands with `qmax < TARGET_Q`
(2048), ties by lowest (ch, band). Re-quantize that band, replan
sections, keep iff emit bits ≤ spend, DPCM ±60, sf ∈ [0,255], no
QUANT_MAX clip. Max 48 tries. Short frames skip.

## Measurement

1 s 48 kHz mono, 128 kbps ADTS, rustc 1.97.1 Linux x86_64, priming skip
1024. An earlier score that did not skip `qmax ≥ TARGET_Q` **collapsed
sine SNR 63.8 → 22.9 dB** (over-quantizing the tone into clip); that
variant is rejected.

| clip | SNR off | SNR on | Δ | bytes off | bytes on |
|---|---:|---:|---:|---:|---:|
| noise 0.5 | 11.21 | 11.27 | **+0.07 dB** | 16710 | 16716 |
| sine 440 Hz 0.5 | 63.79 | 63.79 | 0.00 dB | 16855 | 16860 |

Two-band unit: louder band scores higher (`enc_quant_tests`).

## Go / no-go

| Lane | Decision |
|---|---|
| ≥ 0.3 dB priming-SNR at matched rate | **NO-GO** (+0.07 dB noise) |
| Sine / DOC-3 worst-case | 0.00 dB with the TARGET_Q skip |
| Default 0.x | **NO-GO** — keep global offset only; goldens unchanged |
| CPU | default path unchanged; opt-in ≤16 extra `emit`s per long frame |
| Search bound | 16 keeps / 48 tries (explicit) |

Do not treat +0.07 dB as PEAQ. TASK-108 may still pick the knob if a
later corpus shows ≥ 0.3 dB.
