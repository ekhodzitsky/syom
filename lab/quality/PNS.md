# Perceptual noise substitution (TASK-75)

Recorded 2026-09-14. Isolated notes; ordinary tests never spawn oracles.
PEAQ unavailable. Waveform priming-SNR is **not** a PNS quality metric
(DOC-3: different valid RNGs). Declared substitutes: independent
`NOISE_HCB` / `noise_nrg` parse-back, reconstructed band L2 = `2^(nrg/4)`,
PCM RMS, stereo correlation. Rate gate **≥ 5%** actual ADTS bytes, or
**≥ 0.3 dB** priming-SNR on a non-PNS control (sine/speech).

## Baseline

Encoder never emits `NOISE_HCB`. Decoder PNS (lavc LCG seed `0x1f2e3d4c`)
already matches `pns48` lavc s16.

## Candidate

`EncodeOptions::with_pns` (default **off**). Long frames only. After
quantize, coded HF bands with Johnston SFM < 0.25, center ≥ 4 kHz, outside
the TNS span, cheapest spectral book ≥ 16 bits become `NOISE_HCB` with
`noise_nrg = round(2·log2 Σx²)` via `det_math`. Stereo `ms_used` ⇒
both-or-neither. Short windows skip. Production global offset / goldens
unchanged when the knob is off.

## Measurement

1 s (or 0.5 s stereo) 48 kHz, 128 kbps ADTS, rustc 1.97.1 Linux x86_64,
priming skip 1024. ABR pad still fills unused bytes after `ID_END`.

| clip | NOISE_HCB | bytes off | bytes on | Δ bytes | notes |
|---|---:|---:|---:|---:|---|
| noise 0.5 mono | 1214 | 16708 | 16726 | **+0.1%** | RMS 0.2892 vs 0.2896; priming-SNR 11.16 → −2.18 dB |
| sine 440 Hz | 0 | — | — | 0.00 dB SNR | tonal guard |
| 120/240/360 Hz | 0 | — | — | 0.00 dB SNR | harmonic/speech-like guard |
| identical L/R noise | 0 | — | — | corr 1.000 | M/S side silent; no mixed PNS |
| independent L/R noise | 1232 | — | — | corr −0.008 | does not force a mid image |

## Go / no-go

| Lane | Decision |
|---|---|
| ≥ 5% actual bitrate saving | **NO-GO** (+0.1% ADTS; ABR pad) |
| ≥ 0.3 dB quality at matched rate | **NO-GO** (waveform SNR −13 dB; RMS preserved) |
| Sine / harmonic guards | 0.00 dB |
| Stereo correlation | independent stays uncorrelated; identical L/R stays 1.0 |
| Default 0.x | **NO-GO** — keep Huffman-only; goldens unchanged |
| CPU | default path unchanged; opt-in one SFM pass + re-plan |
| Cross-oracle RNG | encoder does not generate the noise; decoder LCG matches lavc |

Do not treat −13 dB priming-SNR as a PNS failure — DOC-3 forbids waveform
identity across RNGs. The gates that matter (rate, noninferior control
quality) are not met. TASK-108 may still pick the knob for a later corpus.
