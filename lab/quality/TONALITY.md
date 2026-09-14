# TASK-69 Johnston SFM tonality ablation

Recorded 2026-09-14. Host: Linux x86_64, rustc 1.97.1, AMD Ryzen AI 9 HX 370.
Decoder: syom `decode_with` unbounded. Default cells match TASK-16/68
(noise 64k 1.5 dB, noise 128k 11.2 dB, sine 64k 45.4 / priming 63.8 dB).

**Not PEAQ / ViSQOL.** Predeclared equivalent: priming-delay SNR (1024).
ATH stays **off** (decision-9). M/S and rate loop unchanged.

## Model

- Default: uniform `target_q = max_q` on coded bands.
- Candidate: per-band Johnston SFM. α = clamp(SFM_dB / −60, 0, 1).
  `target_q = max_q · (0.25 + 0.75·α)`. Coded mask unchanged (TASK-68
  dropping noisy HF starved 128k noise).
- 1-bin bands: α = 1 (no shape).
- `EncodeOptions::with_tonality`, **default off**.
- Rejected variant (not shipped): SFM-based band dropping.

## A/B (priming-delay SNR)

Reproduce: `make -C lab/quality tonality` (not invoked by `cargo test`).

| clip | class | 64k Δpriming | 128k Δpriming | actual bps on/off |
|---|---|---:|---:|---|
| sine440 | tonal | −0.5 | −0.5 | ~68.5 / 134.9k both |
| noise | noise | 0.0 | +0.1 | ~68.3 / 133.9k |
| tremolo | stereo | −0.6 | −0.6 | ~68.7 / 134.9k |
| silence | silence | — | — | 624 B both |
| click | transient | +0.3 | +0.4 | xcorr delay not priming |
| mix | mixed | **+0.5** | **+0.6** | ~68.4 / 133.8k |
| lecture | speech | **+1.4** | −0.1 | 0.25 s golden |

CPU (10 s noise, one-shot): ton-off 91.1 ms, ton-on 121.6 ms (**+33.4%**).
Opt-in path; default is byte-identical to ton-off (0% production).

Rate: payload/valid still ~68/134 kbps. Benefit on `mix` is not a bitrate
change. M/S path untouched.

## Go / no-go

Predeclared go: ≥0.3 dB aggregate priming-SNR on mixed/speech/**tonal**
at matched actual bitrate, no noise loss >0.5 dB.

Noise is safe (0.0 / +0.1). Mix +0.5/+0.6 is the intended mixed-band
win. Aggregate including sine (−0.5) and tremolo (−0.6) is **~0.03 dB**,
below 0.3 dB. Tremolo −0.6 dB is a stereo/tonal loss. CPU +33% on the
opt-in path.

**No-go as 0.x default.** Do not re-scope the aggregate to mix-only after
seeing results. Production `tonality=false`. Goldens not reminted.

Keep opt-in `with_tonality(true)` for TASK-108. TASK-70 temporal masking
ablates the default psy, not this path.

Full table: `task-69/tonality-ab.log` from `syom_tonality`.
