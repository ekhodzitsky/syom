# LC allocation and low-bitrate behaviour (TASK-113)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, ffmpeg 7.0.2-static
(offline oracle only), on top of `3431025` (TASK-90).

## What was wrong

The HE cells (HE_AU.md) exposed two LC defects that predate SBR:

1. **Silence at ≤ 48 kbps stereo 48 kHz.** A coded band whose quantized
   values were all zero still cost a codebook-1 section plus its
   scalefactor: an all-zero frame cost ≈ 1167 bits for two channels,
   above the 1024-bit budget, so `drop_bands_until` removed every band.
2. **Peak-normalized precision.** Every coded band was quantized so its
   peak mapped to the same `target_q` (2048), and the rate loop shifted
   one global scalefactor offset. Masked thresholds only decided coded /
   uncoded; they never shaped the quantization noise. On tone + white
   noise the tone was degraded together with the noise (mix at 64 kbps
   stereo: 2.0 dB SNR), and coarse `round()` steps inflated noise-like
   band energy by +3 dB.

## What changed

| Piece | Now |
|---|---|
| all-zero bands (`enc_section::plan_books`, short twin) | ZERO_HCB (no scalefactor, no spectral bits, merged sections) when the remaining sf chain stays DPCM-valid, else the old books |
| quantizer (`enc_quant`) | `floor(v + 0.4054)` (ISO), `QUANT_SAFE` floor on `sf` keeps peaks under the 8191 clip |
| psy (`enc_psy::analyze`) | outputs per-band allowed noise `T` (masked threshold with −60 dB / ATH floors), energy and `Σ√|x|`; `target_q` is only the precision cap; tonality (opt-in) divides `T` by the Johnston factor (noise-like bands up to 4× more noise) |
| targets (`enc_alloc::noise_targets`) | `G² = 4·Σ√|x| / (27·T)`, `target_q = peak^0.75·G`; offset ≤ 0 scales `T` by `2^(offset/4)` (uniform refinement), offset > 0 raises a per-coefficient water level from the quietest coded band; a band whose allowed noise reaches its energy drops out |
| rate loop (`enc_frame_rate`) | binary search of that offset in `[−60, 240]` (1.5 dB steps); bits fall monotonically |
| per-frame cache (`enc_frame_alloc.rs`) | psy once per frame on the pre-TNS L/R spectra; the cache is finished on the coded spectra: M/S bands take `min(T_L, T_R)`, a TNS channel scales `T` by residual/original energy; every `build` offset only re-derives targets |
| short frames (`enc_short::channel_build`) | same targets per (group, band) with energy / noise / `Σ√|x|` / width folded over the group's windows |

## Measurements (release, 2 s stereo 48 kHz, channel 0, steady state)

Tone = 440/540 Hz at 0.3; noise = white at rms 0.087; mix = tone + white
at rms 0.029 (−17 dB). SNR against the source after the 1024-sample
priming; the split columns re-analyse both signals with the 64-band QMF
(≤ 750 Hz holds the tones).

| signal | kbps | before: level / SNR | after: level / SNR (≤ 750 Hz / > 750 Hz) |
|---|---:|---|---|
| tone | 48–128 | 0.00 dB / 63.7 dB | 0.00 dB / 60.0 dB (60.1 / 34.9) |
| noise | 48 | −9.9 dB / −0.1 dB | −4.4 dB / 0.8 dB (2.0 / 0.8) |
| noise | 64 | +0.2 dB / −0.6 dB | −2.7 dB / 1.2 dB (2.4 / 1.2) |
| noise | 128 | +3.1 dB / 1.5 dB | +0.5 dB / 3.6 dB (5.5 / 3.6) |
| mix | 48 | **−∞ (silence)** / 0.0 dB | −0.4 dB / 15.3 dB (18.3 / 0.7) |
| mix | 64 | +4.2 dB / −1.4 dB | −0.3 dB / 15.5 dB (18.3 / 1.1) |
| mix | 128 | +2.5 dB / 5.6 dB | −0.3 dB / 16.5 dB (18.3 / 3.5) |

Also mix at 32 kbps stereo: −0.4 dB / 15.1 dB (was silence).

The tone region sits at the 18 dB SMR target and the noise region
absorbs the shortage; the tone alone loses 3.7 dB of SNR against the old
loop (it no longer spends every spare bit on one band). White noise
loses level under shortage because bands under the water level drop
out (−4.4 dB at 48 kbps); PNS (`with_pns`) is the tool for that hole and
stays opt-in (TASK-108).

CPU (release, 10 s stereo 48 kHz noise + tones, best of 5):

| cell | before | after |
|---|---:|---:|
| LC 128 kbps | 171.6 ms | 166.4 ms |
| LC 48 kbps | 113.1 ms | 121.1 ms |

The wider offset search costs two more `build` passes; the per-frame psy
cache removes eight psy passes, net −3 % at 128 kbps, +7 % at 48 kbps.

## Behavioural tests retuned

- `tns_improves_speech_lowrate_snr` → at 32 kbps stereo TNS must not
  cost more than 1.5 dB SNR (measured −1.2 dB: side info + force-coded
  span; the +1 dB "win" was an artefact of the peak-normalized loop).
- `tns_never_hurts_tremolo`: tolerance 1.5 dB (measured −1.3 dB, ch1).
- `tonality_keeps_tone_noise_floor_and_raises_noise_like_bands`: the
  option's new semantics.
- `per_band_never_worse_than_whole_pair` passes with the M/S threshold
  rule (per-band 12.5/17.4 dB vs whole 12.5/16.8 dB).

## Goldens

All encoder goldens changed (`enc48.adts`, `enc48m.m4a`, `enc48t.adts`,
`enc48l.adts`, `he48e.adts`, `he48em.m4a`) and were re-minted with
`MINT_GOLDENS=1` + ffmpeg 7.0.2-static; the lavc tolerance tests (≤ 2 LSB
s16 / ≥ 55 dB) pass on every one, and the corpus / oracle manifests were
rebuilt (the HE goldens are now registered too).

## Not done

- The TASK-16 same-bitrate matrix (`lab/quality`, external adapters) was
  not re-run; the ATH / tonality / PNS / IS / grouping no-go decisions
  were measured under the old allocation and are re-evaluated at
  TASK-108.
- No listening; SMR (18 dB) and the water-level rule are untuned.
