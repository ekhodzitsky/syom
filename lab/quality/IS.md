# Intensity stereo (TASK-76)

Recorded 2026-09-14. Isolated notes; ordinary tests never spawn oracles.
PEAQ unavailable. Declared substitutes: independent `INTENSITY_HCB` /
`is_pos` parse-back, ILD (`rms_r/rms_l`), inter-channel correlation,
priming-aligned SNR. Rate gate **≥ 5%** actual ADTS bytes, or **≥ 0.3 dB**
priming-SNR on a predeclared low-rate stereo cell.

## Baseline

Encoder never emits intensity books. Decoder `apply_intensity` already
matches ISO (sign, `0.5^(is_pos/4)`, PerBand `ms` invert).

## Candidate

`EncodeOptions::with_intensity` (default **off**). Long stereo only.
After TNS, before M/S: HF bands (center ≥ 6 kHz, energy ≥ −40 dB vs
loudest, `|ρ| ≥ 0.85`) stay L/R. Right channel emits `INTENSITY_HCB` or
`HCB2` (sign of ρ) with `is_pos = round(-4 log2 rms_r/rms_l)` via
`det_math`. Skip PNS bands and short windows. Goldens unchanged when off.

## Measurement

48 kHz, rustc 1.97.1 Linux x86_64, priming skip 1024.

| clip | rate | HCB | bytes off/on | Δ bytes | spatial | SNR Δ |
|---|---:|---:|---:|---:|---|---:|
| panned 8 kHz 0.5/0.15 | 64k | 87 | 8614 / 8566 | **−0.6%** | ILD 0.300 → 0.297 | **+0.50 dB** L |
| anti-phase 8 kHz | 64k | 69 | — | — | corr **−1.000** | — |
| uncorrelated noise | 64k | 0 | — | — | corr 0.018 | — |
| panned 440 Hz | 64k | 0 | — | — | LF floor holds | — |

## Go / no-go

| Lane | Decision |
|---|---|
| ≥ 5% actual bitrate | **NO** (−0.6%; ABR pad) |
| ≥ 0.3 dB on declared 64k panned-HF cell | **YES** (+0.50 dB, ILD kept) |
| Anti-phase / ambience / LF panned | pass (no hidden downmix) |
| Default 0.x | **NO-GO** — would remint `enc48m`; only HF panned at low rate |
| CPU | default path unchanged |
| Scope | long stereo, ≥ 6 kHz, `|ρ| ≥ 0.85`, −40 dB energy floor |

Keep opt-in `with_intensity` for TASK-108 (64k HF-panned). Do not flip
the 128k stereo default.
