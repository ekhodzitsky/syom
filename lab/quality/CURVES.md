# Development curves and presets (TASK-108)

Recorded 2026-09-15. Host: Linux x86_64 (AMD Ryzen AI 9 HX 370), rustc
1.97.1, engine at `b5660e9` (TASK-113 allocation). Produced by the isolated
`lab/quality` binary `syom_curves` (`cargo run --release --bin
syom_curves`; never run by `cargo test`). Decoder: syom `decode_with`
unbounded (interchangeable with the ffmpeg lane on our own bitstreams,
see REPORT.md / ATH.md). **Not PEAQ / ViSQOL / listening.** Dev synth
clips + the 0.25 s lecture golden only; the corpus holdout split was not
touched.

Columns: `SNR` = waveform SNR after the declared priming (LC 1024, HE
3018 output samples); `LF SNR` = spectral SNR below 6.75 kHz; `HF err`
= mean |band level error| over 375 Hz bands in 6.75–15.4 kHz (the v1 SBR
range; neither mode codes above 15.4 kHz at these rates). `HF err` is
meaningful for broadband clips (noise, mix, click, voice-like); on tonal
clips it measures leakage of a −60 dB floor and is not ranked.

| clip | class | mode | req kbps | actual kbps | SNR dB | LF SNR dB | HF err dB |
|---|---|---|---:|---:|---:|---:|---:|
| sine440 | tonal | LC | 32 | 35.3 | 30.8 | 39.4 | 42.6 |
| sine440 | tonal | LC | 48 | 51.6 | 42.3 | 56.1 | 42.6 |
| sine440 | tonal | LC | 64 | 67.6 | 45.8 | 55.0 | 42.6 |
| sine440 | tonal | LC | 96 | 100.4 | 57.8 | 61.3 | 42.6 |
| sine440 | tonal | LC | 128 | 132.9 | 59.6 | 62.0 | 42.6 |
| sine440 | tonal | LC | 192 | 198.2 | 60.1 | 62.0 | 42.6 |
| sine440 | tonal | LC+la | 64 | 67.6 | 45.8 | 55.0 | 42.6 |
| sine440 | tonal | LC+la | 128 | 132.9 | 59.6 | 62.0 | 42.6 |
| sine440 | tonal | HE | 24 | 26.1 | 32.1 | 33.5 | 52.1 |
| sine440 | tonal | HE | 32 | 34.3 | 33.7 | 34.4 | 50.1 |
| sine440 | tonal | HE | 48 | 51.0 | 34.4 | 34.9 | 50.1 |
| sine440 | tonal | HE | 64 | 67.5 | 34.5 | 35.1 | 50.1 |
| noise | noise | LC | 32 | 35.4 | 1.3 | 3.9 | 20.4 |
| noise | noise | LC | 48 | 51.5 | 2.3 | 5.6 | 5.2 |
| noise | noise | LC | 64 | 67.6 | 3.8 | 7.9 | 1.7 |
| noise | noise | LC | 96 | 100.0 | 9.0 | 12.0 | 0.7 |
| noise | noise | LC | 128 | 132.3 | 12.5 | 15.1 | 0.5 |
| noise | noise | LC | 192 | 197.3 | 19.1 | 23.5 | 0.3 |
| noise | noise | LC+la | 64 | 67.6 | 3.8 | 7.9 | 1.7 |
| noise | noise | LC+la | 128 | 132.3 | 12.5 | 15.1 | 0.5 |
| noise | noise | HE | 24 | 26.0 | -0.6 | 8.0 | 1.5 |
| noise | noise | HE | 32 | 34.1 | 0.2 | 9.8 | 1.3 |
| noise | noise | HE | 48 | 50.5 | 0.5 | 14.6 | 1.3 |
| noise | noise | HE | 64 | 66.9 | 0.5 | 19.3 | 1.2 |
| mix | tone+noise | LC | 32 | 35.3 | 14.5 | 19.3 | 56.2 |
| mix | tone+noise | LC | 48 | 51.7 | 14.8 | 19.5 | 34.3 |
| mix | tone+noise | LC | 64 | 67.9 | 15.0 | 19.7 | 20.6 |
| mix | tone+noise | LC | 96 | 100.5 | 15.9 | 20.3 | 6.1 |
| mix | tone+noise | LC | 128 | 132.3 | 16.7 | 20.7 | 1.9 |
| mix | tone+noise | LC | 192 | 197.3 | 18.0 | 21.0 | 0.7 |
| mix | tone+noise | LC+la | 64 | 67.9 | 15.0 | 19.7 | 20.6 |
| mix | tone+noise | LC+la | 128 | 132.3 | 16.7 | 20.7 | 1.9 |
| mix | tone+noise | HE | 24 | 26.1 | 13.6 | 18.8 | 5.0 |
| mix | tone+noise | HE | 32 | 34.3 | 14.1 | 19.4 | 2.8 |
| mix | tone+noise | HE | 48 | 50.6 | 14.2 | 19.8 | 1.7 |
| mix | tone+noise | HE | 64 | 66.9 | 14.7 | 20.1 | 1.3 |
| tremolo | stereo tonal | LC | 32 | 35.3 | 20.3 | 23.8 | 34.0 |
| tremolo | stereo tonal | LC | 48 | 51.8 | 20.9 | 25.9 | 33.9 |
| tremolo | stereo tonal | LC | 64 | 68.0 | 27.3 | 31.0 | 34.0 |
| tremolo | stereo tonal | LC | 96 | 100.7 | 42.3 | 50.7 | 34.1 |
| tremolo | stereo tonal | LC | 128 | 132.8 | 47.4 | 57.3 | 34.1 |
| tremolo | stereo tonal | LC | 192 | 198.2 | 56.9 | 61.3 | 34.1 |
| tremolo | stereo tonal | LC+la | 64 | 68.0 | 23.8 | 28.4 | 34.2 |
| tremolo | stereo tonal | LC+la | 128 | 133.3 | 45.3 | 57.3 | 34.3 |
| tremolo | stereo tonal | HE | 24 | 26.4 | 18.3 | 22.6 | 54.1 |
| tremolo | stereo tonal | HE | 32 | 34.6 | 18.1 | 21.7 | 54.1 |
| tremolo | stereo tonal | HE | 48 | 51.4 | 25.1 | 25.7 | 59.6 |
| tremolo | stereo tonal | HE | 64 | 68.1 | 33.7 | 37.9 | 59.3 |
| click | transient | LC | 32 | 35.2 | 2.0 | 4.8 | 16.2 |
| click | transient | LC | 48 | 51.5 | 3.6 | 6.3 | 4.6 |
| click | transient | LC | 64 | 67.6 | 11.1 | 8.0 | 1.9 |
| click | transient | LC | 96 | 100.0 | 16.0 | 11.1 | 0.8 |
| click | transient | LC | 128 | 132.3 | 17.0 | 15.4 | 0.5 |
| click | transient | LC | 192 | 197.2 | 18.8 | 22.2 | 0.3 |
| click | transient | LC+la | 64 | 67.6 | 12.2 | 6.9 | 1.8 |
| click | transient | LC+la | 128 | 132.4 | 17.1 | 15.0 | 0.6 |
| click | transient | HE | 24 | 26.0 | -0.0 | 8.1 | 1.5 |
| click | transient | HE | 32 | 34.2 | -0.0 | 9.8 | 1.4 |
| click | transient | HE | 48 | 50.5 | 0.0 | 14.1 | 1.2 |
| click | transient | HE | 64 | 66.9 | 0.0 | 18.4 | 1.2 |
| voice-like | harmonic+HF | LC | 32 | 35.0 | 0.2 | 1.6 | 30.8 |
| voice-like | harmonic+HF | LC | 48 | 51.3 | 0.1 | 1.7 | 17.3 |
| voice-like | harmonic+HF | LC | 64 | 67.5 | 0.9 | 4.4 | 15.1 |
| voice-like | harmonic+HF | LC | 96 | 99.9 | 3.5 | 10.2 | 6.5 |
| voice-like | harmonic+HF | LC | 128 | 132.6 | 4.5 | 11.8 | 5.6 |
| voice-like | harmonic+HF | LC | 192 | 197.2 | 8.7 | 14.7 | 2.9 |
| voice-like | harmonic+HF | LC+la | 64 | 67.5 | 0.9 | 4.4 | 15.1 |
| voice-like | harmonic+HF | LC+la | 128 | 132.6 | 4.5 | 11.8 | 5.6 |
| voice-like | harmonic+HF | HE | 24 | 26.0 | -0.3 | 4.2 | 2.2 |
| voice-like | harmonic+HF | HE | 32 | 34.2 | 0.2 | 6.3 | 2.0 |
| voice-like | harmonic+HF | HE | 48 | 50.6 | 3.0 | 9.4 | 2.1 |
| voice-like | harmonic+HF | HE | 64 | 66.9 | 4.6 | 12.2 | 2.5 |
| lecture | speech (0.25 s) | LC | 32 | 40.6 | 26.5 | 29.4 | 7.8 |
| lecture | speech (0.25 s) | LC | 48 | 59.2 | 34.1 | 38.8 | 7.8 |
| lecture | speech (0.25 s) | LC | 64 | 76.2 | 48.1 | 53.1 | 7.8 |
| lecture | speech (0.25 s) | LC | 96 | 113.0 | 56.8 | 60.7 | 7.8 |
| lecture | speech (0.25 s) | LC | 128 | 147.0 | 58.6 | 61.3 | 7.8 |
| lecture | speech (0.25 s) | LC | 192 | 215.9 | 59.1 | 61.4 | 7.8 |
| lecture | speech (0.25 s) | LC+la | 64 | 76.2 | 48.1 | 53.1 | 7.8 |
| lecture | speech (0.25 s) | LC+la | 128 | 147.0 | 58.6 | 61.3 | 7.8 |
| lecture | speech (0.25 s) | HE | 24 | 32.8 | 15.6 | 15.1 | 16.4 |
| lecture | speech (0.25 s) | HE | 32 | 42.0 | 24.8 | 27.9 | 16.0 |
| lecture | speech (0.25 s) | HE | 48 | 62.5 | 30.4 | 32.9 | 22.0 |
| lecture | speech (0.25 s) | HE | 64 | 83.1 | 37.2 | 45.0 | 19.7 |

| mode | req kbps | encode ms / 10 s stereo | ×realtime | declared priming (output samples) | extra internal latency |
|---|---:|---:|---:|---:|---|
| LC | 128 | 179.0 | 56× | 1024 | none (causal) |
| LC | 48 | 113.0 | 88× | 1024 | none (causal) |
| LC+la | 128 | 178.6 | 56× | 1024 | +1024 samples (held frame) |
| HE | 48 | 109.1 | 92× | 3018 | none beyond the SBR window |
| HE | 24 | 77.6 | 129× | 3018 | none beyond the SBR window |

## Reading the curves

- **LC** climbs monotonically with rate on every clip; 128 kbps
  (default) sits at noise 12.5 dB / click 17 dB / tremolo 47 dB / tone
  60 dB; 192 kbps adds 2–10 dB on broadband clips and is the top of every
  curve → the `high_quality()` preset.
- **HE v1 at 48 kbps** beats LC 48 kbps on every broadband clip in the
  low band (LF SNR: noise 14.6 vs 5.6, mix 19.8 vs 19.5, click 14.1 vs
  6.3, voice-like 9.4 vs 1.7) and reproduces the 6.75–15.4 kHz band
  levels within 1.2–2.1 dB where LC 48 kbps drops them (5–34 dB) → the
  `low_rate()` preset. On **speech-only** material LC wins at the same
  rate (lecture 48 kbps: LC 34.1 dB vs HE 30.4 dB; the SBR noise floor
  costs 22 dB of HF level error on quiet HF) and on pure tones HE's core
  caps at ≈ 34 dB; HE above 64 kbps is not a use case.
- **Lookahead** (`with_lookahead`) helps the click at 64 kbps (+1.1 dB)
  and costs the tremolo 3.5 dB at 64 kbps / 2.1 dB at 128 kbps (more
  short frames on tonal content): it stays opt-in, not in any preset.
- **ATH / tonality** were re-run under the new allocation (`syom_ath`,
  `syom_tonality`, 2026-09-15): ATH still costs noise 128k 7.5 dB and
  lecture 4 dB; tonality costs 0.7–3.5 dB on every clip. Both stay
  opt-in (decisions 9 / 10 hold).
- **Speed / delay**: LC 128 kbps 55× realtime, HE 48 kbps 89×, HE 24 kbps
  122× (10 s stereo, one thread). Priming: LC 1024, HE 3018 output
  samples; lookahead adds 1024 samples of internal latency; the push
  encoder adds no buffering beyond one frame (LC) / the SBR window (HE).
- **Memory**: unchanged from `lab/baseline/ALLOC.md` (LC) and
  `HE_AU.md` (HE `HeEncoder` 18 KiB + bounded slot/core buffers).

## Presets (decision-20)

| preset | settings | when | measured |
|---|---|---|---|
| `EncodeOptions::default()` | LC ADTS 128 kbps, causal | general use, lowest latency | noise 12.5 dB, click 17.0 dB, tremolo 47.4 dB, lecture 58.6 dB |
| `EncodeOptions::high_quality()` | LC ADTS 192 kbps, causal | when bytes are cheap | noise 19.1 dB, click 18.8 dB, tremolo 56.9 dB, voice-like 8.7 dB; ≈ 1.5× default bytes |
| `EncodeOptions::low_rate()` | HE v1 ADTS 48 kbps | full-band content under 64 kbps | LF SNR noise 14.6 / voice-like 9.4 dB, HF within 2 dB; priming 3018 |

No preset is called best or transparent; every number above is a dev
synth SNR, and TASK-109 listening has not run.
