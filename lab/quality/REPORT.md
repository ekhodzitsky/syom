# Same-bitrate LC encoder quality baseline (TASK-16)

Recorded 2026-09-14. Host: Linux x86_64, rustc 1.97.1, AMD Ryzen AI 9 HX 370.
Independent decode: ffmpeg 7.0.2-static `pcm_f32le` for every ADTS file.
SNR is waveform error after alignment (**not** PEAQ / ViSQOL).

Encoders in the **same-clip** matrix (synth **dev** only, 48 kHz, 1.000 s,
planar f32 in `[-1, 1]`):

| Engine | Build |
|---|---|
| syom LC | in-tree `encode_with` ADTS |
| lavc9 | FFmpeg **9.0.1** `avc_driver encode-lc-adts` (`ffmpeg-9.0.1-native-aac threads=1`) |
| ffmpeg-cli | host ffmpeg **7.0.2-static** native `aac` |

Held-out qualification recordings were **not** used.

## Same-clip matrix

Independent decode: ffmpeg pcm_f32le. SNR after alignment (not PEAQ).

| clip | class | engine | req bps | bytes | actual bps | delay | valid | dec_len | SNR dB | max_abs | note |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| sine440 | tonal | syom | 64000 | 2799 | 22392 | 1024 | 48000 | 49152 | 45.4 | 4.029e-2 |  |
| sine440 | tonal | lavc9 | 64000 | 8750 | 70000 | 1024 | 48000 | 49152 | 22.3 | 2.729e-1 |  |
| sine440 | tonal | ffmpeg-cli | 64000 | 8740 | 69920 | 1024 | 48000 | 49152 | 19.8 | 3.965e-1 |  |
| sine440 | tonal | syom | 128000 | 3217 | 25736 | 478 | 48000 | 49152 | — | 5.001e-1 | sine xcorr alias delay=478 (not priming); SNR not ranked |
| sine440 | tonal | lavc9 | 128000 | 15909 | 127272 | 104 | 48000 | 49152 | — | 1.113e0 | sine xcorr alias delay=104 (not priming); SNR not ranked |
| sine440 | tonal | ffmpeg-cli | 128000 | 15928 | 127424 | 104 | 48000 | 49152 | — | 1.113e0 | sine xcorr alias delay=104 (not priming); SNR not ranked |
| noise | noise | syom | 64000 | 8499 | 67992 | 1024 | 48000 | 49152 | 1.5 | 8.287e-1 |  |
| noise | noise | lavc9 | 64000 | 8631 | 69048 | 1024 | 48000 | 49152 | 1.1 | 8.902e-1 |  |
| noise | noise | ffmpeg-cli | 64000 | 8631 | 69048 | 1024 | 48000 | 49152 | 1.1 | 8.902e-1 |  |
| noise | noise | syom | 128000 | 16695 | 133560 | 1024 | 48000 | 49152 | 11.2 | 2.669e-1 |  |
| noise | noise | lavc9 | 128000 | 16021 | 128168 | 1024 | 48000 | 49152 | 5.0 | 8.606e-1 |  |
| noise | noise | ffmpeg-cli | 128000 | 16021 | 128168 | 1024 | 48000 | 49152 | 5.0 | 8.606e-1 |  |
| click | transient | syom | 64000 | 6482 | 52841 | 2047 | 47105 | 49152 | -2.6 | 9.090e-1 | impulse xcorr unstable |
| click | transient | lavc9 | 64000 | 6651 | 60832 | 7168 | 41984 | 49152 | 6.0 | 2.306e-1 | impulse xcorr unstable |
| click | transient | ffmpeg-cli | 64000 | 6651 | 60832 | 7168 | 41984 | 49152 | 6.0 | 2.306e-1 | impulse xcorr unstable |
| click | transient | syom | 128000 | 12208 | 111659 | 7168 | 41984 | 49152 | 23.7 | 3.947e-2 | impulse xcorr unstable |
| click | transient | lavc9 | 128000 | 9107 | 79422 | 5120 | 44032 | 49152 | 33.0 | 4.416e-3 | impulse xcorr unstable |
| click | transient | ffmpeg-cli | 128000 | 9107 | 79422 | 5120 | 44032 | 49152 | 33.0 | 4.416e-3 | impulse xcorr unstable |
| tremolo | stereo | syom | 64000 | 4712 | 37696 | 1024 | 48000 | 49152 | 24.5 | 3.639e-1 |  |
| tremolo | stereo | lavc9 | 64000 | 8429 | 78248 | 7787 | 41365 | 49152 | 14.9 | 3.995e-1 | delay ≠ priming; treat SNR with caution |
| tremolo | stereo | ffmpeg-cli | 64000 | 8353 | 78161 | 8114 | 41038 | 49152 | 14.7 | 3.953e-1 | delay ≠ priming; treat SNR with caution |
| tremolo | stereo | syom | 128000 | 5123 | 40984 | 1024 | 48000 | 49152 | 44.1 | 3.593e-2 |  |
| tremolo | stereo | lavc9 | 128000 | 16601 | 132808 | 1024 | 48000 | 49152 | 25.7 | 3.275e-1 |  |
| tremolo | stereo | ffmpeg-cli | 128000 | 16679 | 133432 | 1024 | 48000 | 49152 | 25.1 | 3.275e-1 |  |

`actual_bps` uses aligned valid duration (8·bytes/valid_seconds), not
source duration. `dec_len` is ffmpeg dump length (priming + content +
drain), not valid N.

## Matched actual-rate cells

Only compare SNR when actual rates are within ~15% and delay is priming
(1024 ± 64).

| Cell | syom actual | lavc9 actual | syom SNR | lavc9 SNR |
|---|---:|---:|---:|---:|
| noise 64k | 68.0 kbps | 69.0 kbps | 1.5 dB | 1.1 dB |
| noise 128k | 133.6 kbps | 128.2 kbps | 11.2 dB | 5.0 dB |
| sine 64k | **22.4 kbps** | 70.0 kbps | 45.4 dB | 22.3 dB |
| tremolo 128k | **41.0 kbps** | 132.8 kbps | 44.1 dB | 25.7 dB |

Sine 64k and tremolo 128k are **not** matched-rate. Higher SNR at a much
lower spent rate is expected for a cheap tonal residual; it is not a
quality win at the requested 64/128 kbps.

## Ranked weaknesses (inspected, not inferred from tool lists)

1. **Rate control (tonal / correlated stereo).** Requested 64/128 kbps
   lands ~22–41 kbps on sine/tremolo. Noise spends the budget. This is
   the primary LC encoder defect for TASK-65/66.
2. **Pre-echo / transients.** Click-train xcorr delay is not priming
   (2k–7k). SNR on that alignment is not a regression budget. Need a
   better transient fixture after rate is honest (TASK-70).
3. **Sine period alias.** 440 Hz period ≈ 109 samples; 128k sine delay
   104/478 is xcorr alias, not encoder delay. SNR unranked (TASK-13).
4. **Stereo.** High SNR at ~40 kbps on correlated tremolo shows M/S
   coding a cheap pair, not that 128 kbps was delivered.

Do not infer TNS/PNS/intensity benefit from their presence in lavc.

## Not in the same-clip ranking

| Engine | Why |
|---|---|
| FAAC 1.31.1 | Prefix not installed; TASK-9 sine-only, requested 64/128/192k → ~43 kbps actual. Not this PCM. |
| FDK 2.0.3 | Prefix not installed; TASK-7 sine 128k → 32 426 B / 97 280 decoded. Not this PCM. syom cannot decode that FDK ADTS (FIL). |
| glint 0.11.0 | `encode-pcm` not in the installed `glint_smoke` binary (rebuild needs c++). TASK-11 sine CBR was near target at quality=normal. |
| Apple AAC | No AudioToolbox host (TASK-10). Ranking **unresolved**. |
| HE encode | syom / lavc native aac / FAAC 1.31.1 / this FDK adapter: **no SBR/PS encode**. HE remains a decode-only frontier. |

## HE / low-rate frontier (separate from LC)

- syom HE **decode** goldens exist (he48/ps48); HE **encode** is out of
  scope until TASK-85+.
- Low-rate LC 64k on noise is usable as a frontier cell (both engines
  ~1 dB SNR at ~68–69 kbps). Tonal 64k is not a low-rate quality cell
  until syom spends the bits.

## Regression budgets (dev synth only)

| Guard | Budget |
|---|---|
| noise 64k syom actual | 60–75 kbps |
| noise 64k syom SNR | ≥ 0 dB (measured 1.5) |
| noise 128k syom actual | 120–145 kbps |
| noise 128k syom SNR | ≥ 8 dB (measured 11.2) |
| sine/tremolo actual vs requested | **no SNR budget** until TASK-66 lands |

Do not use click SNR or aliased sine SNR as a gate.

## Go / no-go for quality experiments

| Follow-up | Decision |
|---|---|
| TASK-65 truthful ABR/CBR/reservoir semantics | **go** (undershoot is measured) |
| TASK-66 meet documented LC average bitrate | **go** |
| TASK-68/69 psy thresholds / tonality | TASK-68 ATH **no-go as default** (`ATH.md`). TASK-69 SFM tonality **no-go as default** (`TONALITY.md`) |
| TASK-70 transient pre-echo | **go** after a fixture whose alignment is priming |
| TASK-75/76 PNS / intensity | **no-go** until rate is honest; do not copy lavc tools |
| Treat this SNR as PEAQ/transparency | **no-go** |
| HE encode baseline | **no-go** (no encoder) |
| Consume held-out naturals | **no-go** |

## Reproduce

```sh
# not invoked by cargo test
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_quality
# TASK-68 ATH A/B (default vs with_ath)
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_ath
```

Requires `lab/libavcodec/avc_driver` and `ffmpeg` on PATH for the native
lanes. syom-only rows still print if those tools are missing.
