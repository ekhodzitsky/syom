# TASK-68 Terhardt ATH ablation

Recorded 2026-09-14. Host: Linux x86_64, rustc 1.97.1, AMD Ryzen AI 9 HX 370.
Decoder: syom `decode_with` unbounded (A/B of two syom bitstreams). Default
ranked cells match the TASK-16 ffmpeg-decoded SNR (noise 64k 1.5 dB, noise
128k 11.2 dB, sine 64k 45.4 dB, tremolo 64/128k 24.5/44.1 dB), so syom
decode of syom ADTS is interchangeable with that ffmpeg lane here.

**Not PEAQ / ViSQOL** (unavailable). Predeclared equivalent: priming-delay
SNR (TASK-40 delay = 1024). xcorr SNR is reported but unranked when delay
is not priming (sine period alias, TASK-13/16).

## Model

- Production default: Bark spreading + flat 18 dB SMR + −60 dB relative
  floor (`max_e * 1e-6`). No ATH.
- Candidate: Terhardt (1979) simplified ATH in dB SPL via `det_math`
  (no libm). 0 dBFS peak PCM = 96 dB SPL. Band floor =
  `max(mask, rel −60 dB, ATH energy at band center)`.
- Silence: energy 0, still uncoded either way (624-byte ADTS syntax).
- Public `EncodeOptions::with_ath`, **default off**.

## TASK-16 baseline cells (default, post-TASK-66 rate)

| clip | req | TASK-16 ffmpeg SNR | this A/B default xcorr | actual bps now |
|---|---:|---:|---:|---:|
| sine440 | 64k | 45.4 | 45.4 | 68.6k (was 22.4k) |
| noise | 64k | 1.5 | 1.5 | 68.2k |
| noise | 128k | 11.2 | 11.2 | 133.6k |
| tremolo | 64k | 24.5 | 24.5 | 69.1k (was starved) |
| tremolo | 128k | 44.1 | 44.1 | 134.9k (was 41.0k) |

Sine/tremolo now spend the TASK-66 budget. Ranked SNR is unchanged vs
TASK-16 on the cells that already matched rate.

## A/B (priming-delay SNR is the gate)

Reproduce: `make -C lab/quality ath` (not invoked by `cargo test`).

| clip | class | 64k Δpriming | 128k Δpriming | note |
|---|---|---:|---:|---|
| sine440 | tonal | 0.0 | 0.0 | identical bytes |
| noise | noise | **+1.8** | **−6.2** | HF drop helps 64k, starves 128k |
| tremolo | stereo | 0.0 | 0.0 | identical |
| silence | silence | — | — | both Silent, 624 B |
| quiet_sine | level | −0.2 | −0.2 | xcorr delay unstable on ATH; priming ~63.8 both |
| hf_quiet | hf | −31.3 | −31.3 | expected: −40 dB 16 kHz dropped (behavior cell) |
| lecture | speech | **−6.1** | **−6.7** | 0.25 s golden; speech loss |

CPU (10 s noise, one-shot, this process): ath-off ~93 ms, ath-on ~68 ms.
Opt-in path; default is byte-identical to ath-off (0% production
regression). Single-run, not a p95.

## Go / no-go

Predeclared go: ≥0.3 dB aggregate priming-SNR on relevant categories
(speech, tonal, noise, tremolo) at matched actual bitrate, no category
loss >0.5 dB except HF-below-ATH behavior cells.

**No-go as 0.x default.** noise 128k −6.2 dB and lecture −6 dB fail the
per-category guard. hf_quiet SNR drop is the intended inaudible-HF
discard (not a PEAQ claim). Default `ath=false` leaves production and
goldens unchanged.

Opt-in `with_ath(true)` stays for ablation / TASK-108; do not flip the
default on this candidate. TASK-69 tonality is a separate one-change
ablation on the default psy.

Full table: `task-68/ath-ab.log` from `syom_ath`.
