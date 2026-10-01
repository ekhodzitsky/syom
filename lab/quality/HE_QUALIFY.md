# TASK-95 — Qualify HE v1/v2 against low-rate native and Rust leaders

Recorded 2026-09-23. Host: Linux x86_64 (AMD Ryzen AI 9 HX 370), rustc
1.97.1. Engine revision: HEAD `a0d230a` **plus the in-flight TASK-121
`enc_pad` working-tree change** (EXT_FILL stuffing instead of trailing
zero bytes) — see the interop section; the matrix is meaningless without
it. Isolated harness: `lab/quality` bin `syom_he_qualify`
(`src/he_qual.rs`), never run by `cargo test`.

| Piece | Pin |
|---|---|
| Neutral decoder (all engines) | ffmpeg 7.0.2-static CLI → `pcm_f32le` |
| FDK leader | fdk-aac 2.0.3 tarball sha256 e25671…a3a78, zig-c++ build, `lab/fdk` `encode-pcm` AOT 2/5/29 |
| FAAC peer | FAAC 1.31.1 tarball sha256 3191bf…33527, `lab/faac` `encode-pcm`, **input-scale fix 2026-09-23** (see `lab/faac/REPORT.md`) |
| Rust leader | oxideav-aac =0.1.7 crate sha256 35453f…35048, `lab/oxideav` |
| FDK Rust port | fdk-aac-rust 0.2.3 — **context only**: rust-only encoder is LC, same FDK 2.0.3 lineage (`lab/fdk-aac-rust/REPORT.md`) |
| lavc peer | FFmpeg 9.0.1 `avc_driver` native `aac` (LC only) |
| Apple | **no macOS/AudioToolbox host** — cell pending (TASK-10), no claim |
| USAC | **separately labeled context** (`lab/profiles/REPORT.md`, TASK-97, 2026-09-22): exhale 1.2.2 xHE/USAC encode ran on this host (m4a, measured rates); **no working pinned USAC decoder** (libxaac 0.1.13 and ffmpeg 7.0.2 both fail at AU level), so no neutral-decode USAC quality cell exists; profile-expansion evaluation is TASK-97 |

## Method

- Clips: 2 s 48 kHz dev synth — voice-like (stereo speech-like
  harmonic+HF), noise-st (independent channels), mix-st (tone+noise),
  tremolo (8 Hz AM tonal stereo), click-st (transient trains), ambience
  (panned tones + independent HF, the spatial probe) — plus the 0.25 s
  `lecture.m4a` golden as stereo and mono (`lecture-m`). **Holdout
  recordings were not touched** (TASK-108 FREEZE).
- Rates: stereo 24/32/48/64 kbps (HE v2: 24/32/48), mono 24 kbps.
  `actual kbps` = 8·ADTS bytes / source seconds. Ranking only compares
  cells whose **actual** rates match within ~15%; oxideav under-spends
  (its budget is a ceiling: −15…−30% at low rates) and FAAC over-spends
  on some clips (68 kbps at a 24k request on voice-like) — flagged, not
  silently ranked.
- Alignment: declared engine delay (syom 1024/3018 output samples, FDK
  `nDelay` 2048/5057/7106, oxideav 1024/3030 by impulse probe, lavc
  1024) refined by ±128-sample xcorr; FAAC (no declared delay) gets a
  free RMS-envelope search — its lag landed anywhere in 22…9952 across
  clips, so **FAAC waveform scores are indicative only**. Free xcorr
  aliasing on periodic/HE content (the TASK-16 sine-alias problem) was
  verified and avoided by the declared-delay path.
- Metrics per cell: waveform SNR (not PEAQ), spectral LF SNR
  (< 6.75 kHz), HF band-level error over 375 Hz bands in 6.75–15.375 kHz
  (the SBR range), stereo ILD error and ICC error. PEAQ/ViSQOL are not
  installed on this host; no perceptual-model score is claimed.
- Shape checks before scoring: ffprobe profile/rate/channels and decoded
  plane count must match the source (HE → 48000/2, full-band). Mono HE
  streams make ffmpeg's implicit-PS probe return HE-AACv2/2 ch; when the
  two channels are bit-identical the cell is scored on one channel with
  the note `dual-mono upmix` (an ffmpeg presentation quirk, not PS
  content).
- Independent-lineage acceptance: every syom stream is additionally
  decoded in-process by FDK 2.0.3 and FAAD2 2.11.3 (`fdk-dec` /
  `faad-dec` notes).
- CPU/memory: syom in-process median/p95 of 20 reps after warmup; peers
  as processes (5 reps, wall includes launch + PCM read) via
  `peer_run.py` (ru_maxrss = child peak). Process-lane vs in-process is
  never ranked directly (DOC-3).

## Interop record (a TASK-95 finding, root-caused by TASK-121)

During harness bring-up (pre-`enc_pad` tree), **FDK 2.0.3 rejected every
syom-encoded stream — LC included — with `AAC_DEC_PARSE_ERROR` from AU
~2**, while lavc 9.0.1, ffmpeg 7.0.2 and FAAD2 2.11.3 decoded them
fully. Root cause (TASK-121): the ABR rate loop padded undersized frames
with trailing zero bytes after `ID_END`, which FDK's parser rejects.
With the in-flight `enc_pad` fix in the tree, **all 52 syom matrix
streams decode completely in FDK 2.0.3, FAAD2 2.11.3 and ffmpeg**
(`fdk-dec ok samples=98304` / `faad-dec ok samples=96256` on every HE
row). The committed goldens and this matrix are therefore
three-lineage-accepted; keep an FDK decode gate on encoder goldens.

## Full matrix

<!-- MATRIX:BEGIN -->

Raw `syom_he_qualify` stdout (clip table + timing lane):

| clip | class | engine | req k | actual k | enc ms | profile | out Hz/ch | delay | valid | SNR | LF SNR | HF err | ILD err | ICC err | note |
|---|---|---|---:|---:|---:|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| voice-like | speech-like stereo | syom-lc | 24 | 27.2 | 20.5 | LC | 48000/2 | 1024 | 96000 | 0.8 | 2.6 | 77.2 | -0.4 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-lc | 32 | 35.2 | 20.4 | LC | 48000/2 | 1024 | 96000 | 1.1 | 3.3 | 66.2 | 0.3 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-lc | 48 | 51.5 | 22.0 | LC | 48000/2 | 1024 | 96000 | 1.4 | 3.7 | 45.5 | 0.9 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-lc | 64 | 67.7 | 22.2 | LC | 48000/2 | 1024 | 96000 | 1.9 | 5.1 | 29.9 | 1.6 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he1 | 24 | 26.0 | 13.1 | HE-AAC | 48000/2 | 3020 | 95284 | -0.8 | 3.5 | 9.7 | -3.9 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he1 | 32 | 34.3 | 14.5 | HE-AAC | 48000/2 | 3018 | 95286 | -0.4 | 4.9 | 4.0 | -1.8 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he1 | 48 | 50.6 | 12.9 | HE-AAC | 48000/2 | 3018 | 95286 | 0.6 | 7.4 | 2.0 | -0.3 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he1 | 64 | 66.8 | 19.4 | HE-AAC | 48000/2 | 3018 | 95286 | 1.4 | 9.9 | 2.9 | -0.6 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he2 | 24 | 26.0 | 9.0 | HE-AACv2 | 48000/2 | 3018 | 95286 | -0.9 | 4.1 | 2.9 | -0.1 | 0.2 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he2 | 32 | 34.2 | 10.8 | HE-AACv2 | 48000/2 | 3018 | 95286 | -0.5 | 4.7 | 3.1 | 0.1 | 0.2 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | syom-he2 | 48 | 50.5 | 11.9 | HE-AACv2 | 48000/2 | 3018 | 95286 | -0.2 | 5.3 | 2.0 | -0.1 | 0.2 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| voice-like | speech-like stereo | fdk-lc | 24 | 26.8 | 6.3 | LC | 48000/2 | 2048 | 95232 | 0.5 | 3.8 | 96.6 | 0.3 | 0.0 |  |
| voice-like | speech-like stereo | fdk-lc | 32 | 34.5 | 6.7 | LC | 48000/2 | 2048 | 95232 | 0.9 | 5.1 | 95.0 | 0.2 | -0.0 |  |
| voice-like | speech-like stereo | fdk-lc | 48 | 50.5 | 6.2 | LC | 48000/2 | 2048 | 95232 | 1.4 | 6.1 | 62.4 | 0.1 | 0.0 |  |
| voice-like | speech-like stereo | fdk-lc | 64 | 68.1 | 10.7 | LC | 48000/2 | 2048 | 95232 | 1.3 | 7.7 | 24.4 | 0.1 | 0.0 |  |
| voice-like | speech-like stereo | fdk-he1 | 24 | 26.0 | 14.4 | HE-AAC | 48000/2 | 5057 | 93247 | -0.1 | 5.4 | 20.2 | -0.0 | 0.0 |  |
| voice-like | speech-like stereo | fdk-he1 | 32 | 34.2 | 18.6 | HE-AAC | 48000/2 | 5057 | 93247 | 0.1 | 6.0 | 2.1 | 0.0 | 0.0 |  |
| voice-like | speech-like stereo | fdk-he1 | 48 | 50.0 | 14.9 | HE-AAC | 48000/2 | 5057 | 93247 | 0.7 | 7.4 | 2.0 | -0.0 | 0.0 |  |
| voice-like | speech-like stereo | fdk-he1 | 64 | 66.3 | 14.3 | HE-AAC | 48000/2 | 5057 | 93247 | 0.4 | 8.5 | 1.7 | 0.0 | 0.0 |  |
| voice-like | speech-like stereo | fdk-he2 | 24 | 25.4 | 12.2 | HE-AACv2 | 48000/2 | 7105 | 91199 | -0.7 | 4.3 | 2.8 | -0.2 | 0.2 |  |
| voice-like | speech-like stereo | fdk-he2 | 32 | 33.5 | 8.0 | HE-AACv2 | 48000/2 | 7106 | 91198 | -0.4 | 5.0 | 2.7 | -0.1 | 0.2 |  |
| voice-like | speech-like stereo | fdk-he2 | 48 | 49.8 | 13.4 | HE-AACv2 | 48000/2 | 7106 | 91198 | -0.3 | 5.3 | 2.5 | 0.1 | 0.2 |  |
| voice-like | speech-like stereo | oxideav-lc | 24 | 16.4 | 4026.1 | LC | 48000/2 | 1024 | 96000 | -0.1 | 0.6 | 79.3 | 0.1 | -0.0 |  |
| voice-like | speech-like stereo | oxideav-lc | 32 | 23.2 | 3973.3 | LC | 48000/2 | 1024 | 96000 | -0.2 | 1.1 | 67.4 | 0.3 | -0.0 |  |
| voice-like | speech-like stereo | oxideav-lc | 48 | 39.3 | 4048.2 | LC | 48000/2 | 1024 | 96000 | -0.1 | 2.1 | 44.1 | 0.2 | -0.0 |  |
| voice-like | speech-like stereo | oxideav-lc | 64 | 52.2 | 4046.0 | LC | 48000/2 | 1024 | 96000 | -0.0 | 2.6 | 28.9 | 0.0 | -0.0 |  |
| voice-like | speech-like stereo | oxideav-he1 | 24 | 16.4 | 1908.9 | HE-AAC | 48000/2 | 3042 | 95262 | -0.1 | 0.8 | 86.8 | -1.7 | -0.0 |  |
| voice-like | speech-like stereo | oxideav-he1 | 32 | 25.3 | 1931.5 | HE-AAC | 48000/2 | 3042 | 95262 | -0.0 | 2.1 | 82.4 | -1.2 | -0.0 |  |
| voice-like | speech-like stereo | oxideav-he1 | 48 | 42.9 | 1992.8 | HE-AAC | 48000/2 | 3042 | 95262 | -0.1 | 5.5 | 1.9 | -0.2 | 0.0 |  |
| voice-like | speech-like stereo | oxideav-he1 | 64 | 59.1 | 1993.0 | HE-AAC | 48000/2 | 3042 | 95262 | 0.7 | 9.8 | 1.7 | -0.2 | 0.0 |  |
| voice-like | speech-like stereo | faac-lc | 24 | 68.3 | 9.5 | LC | 48000/2 | 22 | 96000 | -2.2 | 2.7 | 27.4 | 0.2 | -0.0 | free-align lag=22 |
| voice-like | speech-like stereo | faac-lc | 32 | 68.7 | 7.8 | LC | 48000/2 | 55 | 96000 | -2.2 | 2.7 | 27.3 | 0.2 | -0.0 | free-align lag=55 |
| voice-like | speech-like stereo | faac-lc | 48 | 69.8 | 6.8 | LC | 48000/2 | 54 | 96000 | -2.2 | 2.7 | 26.8 | 0.3 | -0.0 | free-align lag=54 |
| voice-like | speech-like stereo | faac-lc | 64 | 76.0 | 7.0 | LC | 48000/2 | 57 | 96000 | -2.3 | 2.8 | 19.2 | 0.2 | -0.0 | free-align lag=57 |
| voice-like | speech-like stereo | lavc9-lc | 24 | 27.2 | 22.8 | LC | 48000/2 | 1024 | 96000 | 0.4 | 2.3 | 101.3 | -0.9 | 0.2 |  |
| voice-like | speech-like stereo | lavc9-lc | 32 | 34.4 | 36.1 | LC | 48000/2 | 1024 | 96000 | 0.9 | 3.5 | 99.7 | -0.7 | 0.1 |  |
| voice-like | speech-like stereo | lavc9-lc | 48 | 51.7 | 33.2 | LC | 48000/2 | 1024 | 96000 | 1.1 | 7.7 | 84.7 | 0.0 | -0.0 |  |
| voice-like | speech-like stereo | lavc9-lc | 64 | 67.6 | 37.8 | LC | 48000/2 | 1024 | 96000 | 1.6 | 9.1 | 34.4 | 0.0 | 0.0 |  |
| noise-st | noise stereo | syom-lc | 24 | 27.2 | 13.6 | LC | 48000/2 | 1024 | 96000 | 0.4 | 1.3 | 73.5 | -0.7 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-lc | 32 | 35.4 | 13.7 | LC | 48000/2 | 1024 | 96000 | 0.6 | 1.6 | 58.0 | -0.9 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-lc | 48 | 51.8 | 15.4 | LC | 48000/2 | 1024 | 96000 | 0.9 | 2.5 | 35.4 | -0.9 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-lc | 64 | 67.9 | 17.2 | LC | 48000/2 | 1024 | 96000 | 1.4 | 3.4 | 21.9 | -0.8 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he1 | 24 | 26.1 | 11.3 | HE-AAC | 48000/2 | 3018 | 95286 | -1.3 | 2.9 | 5.5 | -0.0 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he1 | 32 | 34.2 | 11.7 | HE-AAC | 48000/2 | 3018 | 95286 | -1.2 | 4.8 | 2.0 | 0.0 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he1 | 48 | 50.6 | 13.5 | HE-AAC | 48000/2 | 3018 | 95286 | -0.7 | 8.1 | 1.6 | 0.0 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he1 | 64 | 66.8 | 16.7 | HE-AAC | 48000/2 | 3018 | 95286 | 0.1 | 9.6 | 1.4 | 0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he2 | 24 | 26.0 | 9.5 | HE-AACv2 | 48000/2 | 3018 | 95286 | -1.4 | 3.6 | 1.9 | 0.2 | 0.1 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he2 | 32 | 34.1 | 10.2 | HE-AACv2 | 48000/2 | 3018 | 95286 | -1.2 | 4.2 | 1.7 | 0.0 | 0.1 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | syom-he2 | 48 | 50.6 | 10.6 | HE-AACv2 | 48000/2 | 3018 | 95286 | -0.9 | 5.0 | 1.7 | 0.0 | 0.1 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| noise-st | noise stereo | fdk-lc | 24 | 25.6 | 5.5 | LC | 48000/2 | 2048 | 95232 | 0.2 | 3.4 | 102.7 | -0.1 | -0.0 |  |
| noise-st | noise stereo | fdk-lc | 32 | 34.3 | 7.6 | LC | 48000/2 | 2048 | 95232 | 0.4 | 4.6 | 102.6 | -0.1 | -0.0 |  |
| noise-st | noise stereo | fdk-lc | 48 | 50.3 | 7.0 | LC | 48000/2 | 2048 | 95232 | 0.8 | 5.5 | 63.8 | -0.0 | -0.0 |  |
| noise-st | noise stereo | fdk-lc | 64 | 68.3 | 11.3 | LC | 48000/2 | 2048 | 95232 | 0.2 | 7.0 | 23.7 | 0.0 | -0.0 |  |
| noise-st | noise stereo | fdk-he1 | 24 | 25.5 | 13.2 | HE-AAC | 48000/2 | 5057 | 93247 | -0.7 | 5.2 | 20.6 | -0.0 | 0.1 |  |
| noise-st | noise stereo | fdk-he1 | 32 | 33.9 | 19.2 | HE-AAC | 48000/2 | 5057 | 93247 | -0.7 | 5.9 | 2.0 | 0.0 | 0.1 |  |
| noise-st | noise stereo | fdk-he1 | 48 | 49.8 | 19.0 | HE-AAC | 48000/2 | 5057 | 93247 | -0.4 | 7.0 | 1.8 | -0.0 | 0.0 |  |
| noise-st | noise stereo | fdk-he1 | 64 | 66.2 | 13.7 | HE-AAC | 48000/2 | 5058 | 93246 | -0.8 | 7.3 | 1.6 | -0.0 | 0.1 |  |
| noise-st | noise stereo | fdk-he2 | 24 | 25.5 | 9.0 | HE-AACv2 | 48000/2 | 7106 | 91198 | -0.9 | 4.4 | 2.8 | -0.1 | 0.1 |  |
| noise-st | noise stereo | fdk-he2 | 32 | 33.5 | 12.2 | HE-AACv2 | 48000/2 | 7105 | 91199 | -0.9 | 4.8 | 2.9 | -0.0 | 0.1 |  |
| noise-st | noise stereo | fdk-he2 | 48 | 49.7 | 13.6 | HE-AACv2 | 48000/2 | 7106 | 91198 | -0.9 | 5.1 | 2.5 | -0.0 | 0.1 |  |
| noise-st | noise stereo | oxideav-lc | 24 | 15.3 | 3990.4 | LC | 48000/2 | 1024 | 96000 | -0.1 | 0.1 | 78.4 | -0.1 | 0.0 |  |
| noise-st | noise stereo | oxideav-lc | 32 | 20.2 | 3973.0 | LC | 48000/2 | 1024 | 96000 | -0.1 | 0.1 | 61.0 | -0.1 | 0.0 |  |
| noise-st | noise stereo | oxideav-lc | 48 | 33.7 | 4033.5 | LC | 48000/2 | 1024 | 96000 | -0.2 | 0.5 | 30.3 | 0.1 | -0.0 |  |
| noise-st | noise stereo | oxideav-lc | 64 | 52.4 | 4131.7 | LC | 48000/2 | 1024 | 96000 | -0.4 | 1.0 | 11.3 | -0.0 | -0.0 |  |
| noise-st | noise stereo | oxideav-he1 | 24 | 16.8 | 1967.6 | HE-AAC | 48000/2 | 3042 | 95262 | -0.0 | 0.6 | 94.0 | 0.5 | 0.0 |  |
| noise-st | noise stereo | oxideav-he1 | 32 | 25.1 | 1897.3 | HE-AAC | 48000/2 | 3042 | 95262 | -0.0 | 1.4 | 90.7 | 0.1 | 0.0 |  |
| noise-st | noise stereo | oxideav-he1 | 48 | 43.1 | 1972.2 | HE-AAC | 48000/2 | 3042 | 95262 | -1.3 | 4.7 | 1.6 | 0.0 | 0.0 |  |
| noise-st | noise stereo | oxideav-he1 | 64 | 59.4 | 1967.1 | HE-AAC | 48000/2 | 3042 | 95262 | -0.9 | 8.7 | 1.5 | 0.0 | -0.0 |  |
| noise-st | noise stereo | faac-lc | 24 | 65.2 | 7.0 | LC | 48000/2 | 4787 | 91469 | -1.7 | 1.7 | 18.3 | 0.0 | 0.0 | free-align lag=4787 |
| noise-st | noise stereo | faac-lc | 32 | 65.7 | 6.9 | LC | 48000/2 | 4570 | 91686 | -1.7 | 1.8 | 17.9 | 0.0 | 0.0 | free-align lag=4570 |
| noise-st | noise stereo | faac-lc | 48 | 66.8 | 7.1 | LC | 48000/2 | 9559 | 86697 | -1.7 | 1.7 | 18.7 | 0.0 | 0.0 | free-align lag=9559 |
| noise-st | noise stereo | faac-lc | 64 | 73.0 | 8.2 | LC | 48000/2 | 9576 | 86680 | -1.8 | 1.9 | 14.1 | -0.1 | 0.0 | free-align lag=9576 |
| noise-st | noise stereo | lavc9-lc | 24 | 27.3 | 30.2 | LC | 48000/2 | 1024 | 96000 | 0.1 | 1.7 | 109.5 | 0.0 | 0.1 |  |
| noise-st | noise stereo | lavc9-lc | 32 | 34.0 | 40.4 | LC | 48000/2 | 1024 | 96000 | 0.4 | 2.6 | 107.6 | -0.0 | 0.0 |  |
| noise-st | noise stereo | lavc9-lc | 48 | 50.8 | 36.2 | LC | 48000/2 | 1024 | 96000 | 0.2 | 6.9 | 90.4 | 0.0 | 0.1 |  |
| noise-st | noise stereo | lavc9-lc | 64 | 67.1 | 43.8 | LC | 48000/2 | 1024 | 96000 | -0.3 | 6.7 | 35.7 | 0.1 | 0.1 |  |
| mix-st | tone+noise stereo | syom-lc | 24 | 27.2 | 14.9 | LC | 48000/2 | 1024 | 96000 | 14.4 | 19.1 | 70.7 | -0.1 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-lc | 32 | 35.3 | 14.7 | LC | 48000/2 | 1024 | 96000 | 14.5 | 19.3 | 56.2 | -0.1 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-lc | 48 | 51.7 | 16.0 | LC | 48000/2 | 1024 | 96000 | 14.8 | 19.5 | 34.3 | -0.1 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-lc | 64 | 67.9 | 17.0 | LC | 48000/2 | 1024 | 96000 | 15.0 | 19.7 | 20.6 | -0.1 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he1 | 24 | 26.1 | 11.2 | HE-AAC | 48000/2 | 3018 | 95286 | 13.6 | 18.8 | 5.0 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he1 | 32 | 34.3 | 11.6 | HE-AAC | 48000/2 | 3018 | 95286 | 14.1 | 19.4 | 2.8 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he1 | 48 | 50.6 | 17.4 | HE-AAC | 48000/2 | 3018 | 95286 | 14.2 | 19.8 | 1.7 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he1 | 64 | 66.9 | 17.1 | HE-AAC | 48000/2 | 3018 | 95286 | 14.7 | 20.1 | 1.3 | -0.1 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he2 | 24 | 26.0 | 9.6 | HE-AACv2 | 48000/2 | 3018 | 95286 | 7.4 | 8.4 | 2.3 | -4.8 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he2 | 32 | 34.2 | 10.8 | HE-AACv2 | 48000/2 | 3018 | 95286 | 7.4 | 8.4 | 1.7 | -4.8 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | syom-he2 | 48 | 50.5 | 11.9 | HE-AACv2 | 48000/2 | 3018 | 95286 | 7.5 | 8.4 | 1.7 | -4.8 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| mix-st | tone+noise stereo | fdk-lc | 24 | 26.7 | 5.5 | LC | 48000/2 | 2048 | 95232 | 1.0 | 2.4 | 82.6 | -0.3 | 0.0 |  |
| mix-st | tone+noise stereo | fdk-lc | 32 | 34.7 | 8.0 | LC | 48000/2 | 2048 | 95232 | 5.2 | 6.8 | 82.6 | -0.6 | 0.0 |  |
| mix-st | tone+noise stereo | fdk-lc | 48 | 50.3 | 9.0 | LC | 48000/2 | 2048 | 95232 | 10.3 | 12.1 | 54.9 | 0.8 | -0.0 |  |
| mix-st | tone+noise stereo | fdk-lc | 64 | 68.0 | 10.8 | LC | 48000/2 | 2048 | 95232 | 17.3 | 26.8 | 22.0 | -0.0 | -0.0 |  |
| mix-st | tone+noise stereo | fdk-he1 | 24 | 25.9 | 12.3 | HE-AAC | 48000/2 | 5057 | 93247 | 11.0 | 15.1 | 18.1 | -0.2 | 0.0 |  |
| mix-st | tone+noise stereo | fdk-he1 | 32 | 34.1 | 12.3 | HE-AAC | 48000/2 | 5057 | 93247 | 16.1 | 25.2 | 1.9 | -0.1 | 0.0 |  |
| mix-st | tone+noise stereo | fdk-he1 | 48 | 50.0 | 13.2 | HE-AAC | 48000/2 | 5057 | 93247 | 16.1 | 24.9 | 1.7 | -0.0 | -0.0 |  |
| mix-st | tone+noise stereo | fdk-he1 | 64 | 66.5 | 15.7 | HE-AAC | 48000/2 | 5057 | 93247 | 16.0 | 26.9 | 1.5 | -0.1 | 0.0 |  |
| mix-st | tone+noise stereo | fdk-he2 | 24 | 25.5 | 9.8 | HE-AACv2 | 48000/2 | 7105 | 91199 | 7.0 | 8.0 | 2.8 | -5.8 | -0.1 |  |
| mix-st | tone+noise stereo | fdk-he2 | 32 | 33.6 | 11.7 | HE-AACv2 | 48000/2 | 7105 | 91199 | 7.1 | 8.0 | 2.5 | -5.8 | -0.1 |  |
| mix-st | tone+noise stereo | fdk-he2 | 48 | 49.7 | 9.3 | HE-AACv2 | 48000/2 | 7105 | 91199 | 7.1 | 8.1 | 2.5 | -5.8 | -0.1 |  |
| mix-st | tone+noise stereo | oxideav-lc | 24 | 17.3 | 3719.9 | LC | 48000/2 | 1024 | 96000 | 15.0 | 19.3 | 67.5 | -0.1 | 0.0 |  |
| mix-st | tone+noise stereo | oxideav-lc | 32 | 19.8 | 3716.0 | LC | 48000/2 | 1024 | 96000 | 15.1 | 19.4 | 59.8 | -0.2 | 0.0 |  |
| mix-st | tone+noise stereo | oxideav-lc | 48 | 35.5 | 3758.7 | LC | 48000/2 | 1024 | 96000 | 14.9 | 19.1 | 31.3 | 0.1 | 0.0 |  |
| mix-st | tone+noise stereo | oxideav-lc | 64 | 54.6 | 3826.5 | LC | 48000/2 | 1024 | 96000 | 15.2 | 19.8 | 12.6 | 0.2 | 0.0 |  |
| mix-st | tone+noise stereo | oxideav-he1 | 24 | 16.2 | 1763.1 | HE-AAC | 48000/2 | 3042 | 95262 | 15.5 | 19.8 | 66.5 | 0.1 | 0.0 |  |
| mix-st | tone+noise stereo | oxideav-he1 | 32 | 25.7 | 1768.9 | HE-AAC | 48000/2 | 3042 | 95262 | 16.1 | 21.6 | 66.7 | 0.1 | -0.0 |  |
| mix-st | tone+noise stereo | oxideav-he1 | 48 | 43.1 | 1974.8 | HE-AAC | 48000/2 | 3042 | 95262 | 15.6 | 24.6 | 1.6 | -0.0 | 0.0 |  |
| mix-st | tone+noise stereo | oxideav-he1 | 64 | 59.7 | 1879.0 | HE-AAC | 48000/2 | 3042 | 95262 | 16.3 | 28.0 | 1.5 | -0.0 | 0.0 |  |
| mix-st | tone+noise stereo | faac-lc | 24 | 27.1 | 7.6 | LC | 48000/2 | 85 | 96000 | -2.7 | 22.0 | 74.4 | -0.0 | -0.0 | free-align lag=85 |
| mix-st | tone+noise stereo | faac-lc | 32 | 35.5 | 8.4 | LC | 48000/2 | 85 | 96000 | -2.7 | 22.5 | 63.5 | 0.1 | -0.0 | free-align lag=85 |
| mix-st | tone+noise stereo | faac-lc | 48 | 51.2 | 7.9 | LC | 48000/2 | 85 | 96000 | -2.8 | 23.1 | 38.2 | -0.1 | -0.0 | free-align lag=85 |
| mix-st | tone+noise stereo | faac-lc | 64 | 66.2 | 7.0 | LC | 48000/2 | 85 | 96000 | -2.8 | 23.8 | 19.1 | -0.0 | -0.0 | free-align lag=85 |
| mix-st | tone+noise stereo | lavc9-lc | 24 | 27.8 | 27.2 | LC | 48000/2 | 1024 | 96000 | 9.3 | 13.6 | 81.6 | -0.0 | 0.0 |  |
| mix-st | tone+noise stereo | lavc9-lc | 32 | 35.2 | 31.5 | LC | 48000/2 | 1024 | 96000 | 9.8 | 14.8 | 81.7 | 0.0 | -0.0 |  |
| mix-st | tone+noise stereo | lavc9-lc | 48 | 51.3 | 33.2 | LC | 48000/2 | 1024 | 96000 | 13.8 | 18.5 | 73.4 | -0.2 | -0.0 |  |
| mix-st | tone+noise stereo | lavc9-lc | 64 | 66.8 | 34.6 | LC | 48000/2 | 1024 | 96000 | 16.4 | 25.6 | 31.1 | -0.0 | 0.0 |  |
| tremolo | stereo tonal | syom-lc | 24 | 27.1 | 4.9 | LC | 48000/2 | 1133 | 96000 | 13.2 | 14.2 | 33.6 | -0.4 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 align drift declared=1024 used=1133 |
| tremolo | stereo tonal | syom-lc | 32 | 35.3 | 5.9 | LC | 48000/2 | 1024 | 96000 | 20.4 | 23.8 | 34.0 | -0.1 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-lc | 48 | 51.8 | 5.2 | LC | 48000/2 | 1024 | 96000 | 21.0 | 25.9 | 33.9 | -0.0 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-lc | 64 | 68.0 | 5.4 | LC | 48000/2 | 1024 | 96000 | 27.4 | 31.0 | 34.0 | -0.0 | -0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he1 | 24 | 26.4 | 7.1 | HE-AAC | 48000/2 | 3018 | 95286 | 18.4 | 22.6 | 54.1 | -0.0 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he1 | 32 | 34.6 | 7.5 | HE-AAC | 48000/2 | 3018 | 95286 | 18.2 | 21.7 | 54.1 | -0.0 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he1 | 48 | 51.4 | 7.1 | HE-AAC | 48000/2 | 3018 | 95286 | 25.1 | 25.7 | 59.6 | -0.0 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he1 | 64 | 68.1 | 7.8 | HE-AAC | 48000/2 | 3018 | 95286 | 33.8 | 37.9 | 59.3 | -0.0 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he2 | 24 | 26.4 | 6.6 | HE-AACv2 | 48000/2 | 3018 | 95286 | 25.1 | 25.8 | 59.5 | 0.1 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he2 | 32 | 34.6 | 6.5 | HE-AACv2 | 48000/2 | 3018 | 95286 | 25.1 | 25.8 | 59.3 | 0.1 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | syom-he2 | 48 | 51.4 | 6.1 | HE-AACv2 | 48000/2 | 3018 | 95286 | 41.0 | 42.5 | 59.3 | 0.1 | -0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| tremolo | stereo tonal | fdk-lc | 24 | 27.8 | 7.8 | LC | 48000/2 | 2048 | 95232 | 29.9 | 34.2 | 37.6 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-lc | 32 | 33.6 | 5.5 | LC | 48000/2 | 2048 | 95232 | 37.4 | 40.4 | 37.5 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-lc | 48 | 49.0 | 8.0 | LC | 48000/2 | 2048 | 95232 | 37.5 | 40.5 | 40.5 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-lc | 64 | 65.2 | 8.3 | LC | 48000/2 | 2048 | 95232 | 37.9 | 41.4 | 44.5 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-he1 | 24 | 24.7 | 12.1 | HE-AAC | 48000/2 | 5057 | 93247 | 30.9 | 38.7 | 53.7 | -0.0 | -0.0 |  |
| tremolo | stereo tonal | fdk-he1 | 32 | 32.9 | 14.1 | HE-AAC | 48000/2 | 5057 | 93247 | 30.9 | 38.4 | 55.4 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-he1 | 48 | 49.5 | 11.7 | HE-AAC | 48000/2 | 5057 | 93247 | 31.1 | 38.9 | 60.8 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-he1 | 64 | 65.7 | 18.7 | HE-AAC | 48000/2 | 5057 | 93247 | 31.2 | 39.2 | 63.4 | -0.1 | -0.0 |  |
| tremolo | stereo tonal | fdk-he2 | 24 | 24.6 | 11.6 | HE-AACv2 | 48000/2 | 7160 | 91144 | 27.6 | 29.0 | 55.7 | -0.2 | -0.0 |  |
| tremolo | stereo tonal | fdk-he2 | 32 | 32.8 | 10.9 | HE-AACv2 | 48000/2 | 7160 | 91144 | 27.6 | 29.0 | 62.3 | -0.2 | -0.0 |  |
| tremolo | stereo tonal | fdk-he2 | 48 | 49.2 | 8.1 | HE-AACv2 | 48000/2 | 7160 | 91144 | 27.6 | 29.0 | 62.0 | -0.2 | -0.0 |  |
| tremolo | stereo tonal | oxideav-lc | 24 | 23.8 | 3287.6 | LC | 48000/2 | 1024 | 96000 | 30.9 | 34.7 | 50.7 | 0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-lc | 32 | 31.9 | 3323.5 | LC | 48000/2 | 1024 | 96000 | 38.1 | 42.9 | 50.8 | 0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-lc | 48 | 47.6 | 3289.0 | LC | 48000/2 | 1024 | 96000 | 44.8 | 53.8 | 50.8 | -0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-lc | 64 | 61.6 | 3334.7 | LC | 48000/2 | 1024 | 96000 | 45.3 | 58.5 | 51.4 | 0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-he1 | 24 | 20.0 | 1681.0 | HE-AAC | 48000/2 | 3042 | 95262 | 34.9 | 37.0 | 53.9 | 0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-he1 | 32 | 26.8 | 1707.1 | HE-AAC | 48000/2 | 3042 | 95262 | 43.1 | 43.9 | 53.9 | -0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-he1 | 48 | 35.3 | 1710.7 | HE-AAC | 48000/2 | 3042 | 95262 | 49.5 | 52.9 | 55.6 | -0.0 | -0.0 |  |
| tremolo | stereo tonal | oxideav-he1 | 64 | 40.7 | 1706.5 | HE-AAC | 48000/2 | 3042 | 95262 | 53.0 | 54.7 | 60.6 | -0.0 | -0.0 |  |
| tremolo | stereo tonal | faac-lc | 24 | 25.5 | 5.3 | LC | 48000/2 | 3952 | 92304 | 11.9 | 11.8 | 59.5 | -1.9 | -0.0 | free-align lag=3952 |
| tremolo | stereo tonal | faac-lc | 32 | 34.2 | 5.5 | LC | 48000/2 | 3952 | 92304 | 12.1 | 12.0 | 62.7 | -1.8 | -0.0 | free-align lag=3952 |
| tremolo | stereo tonal | faac-lc | 48 | 54.0 | 6.4 | LC | 48000/2 | 3952 | 92304 | 13.0 | 12.8 | 69.3 | -0.0 | -0.0 | free-align lag=3952 |
| tremolo | stereo tonal | faac-lc | 64 | 66.8 | 5.9 | LC | 48000/2 | 3952 | 92304 | 13.0 | 12.9 | 72.6 | -0.0 | -0.0 | free-align lag=3952 |
| tremolo | stereo tonal | lavc9-lc | 24 | 28.2 | 24.8 | LC | 48000/2 | 969 | 96000 | 8.7 | 12.0 | 41.1 | -1.0 | -0.0 |  |
| tremolo | stereo tonal | lavc9-lc | 32 | 35.0 | 26.8 | LC | 48000/2 | 1024 | 96000 | 9.5 | 12.8 | 40.8 | -1.8 | -0.0 |  |
| tremolo | stereo tonal | lavc9-lc | 48 | 51.0 | 36.5 | LC | 48000/2 | 1024 | 96000 | 11.6 | 14.6 | 39.0 | -0.8 | -0.0 |  |
| tremolo | stereo tonal | lavc9-lc | 64 | 67.8 | 34.4 | LC | 48000/2 | 1024 | 96000 | 14.1 | 16.1 | 39.1 | -0.4 | -0.0 |  |
| click-st | transient stereo | syom-lc | 24 | 27.2 | 14.6 | LC | 48000/2 | 1024 | 96000 | 2.3 | 1.3 | 72.0 | 0.1 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-lc | 32 | 35.3 | 19.5 | LC | 48000/2 | 1024 | 96000 | 2.3 | 1.6 | 56.0 | -0.0 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-lc | 48 | 51.6 | 15.9 | LC | 48000/2 | 1024 | 96000 | 2.3 | 3.4 | 30.9 | -0.0 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-lc | 64 | 67.8 | 18.7 | LC | 48000/2 | 1024 | 96000 | 2.4 | 4.6 | 17.6 | -0.1 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he1 | 24 | 26.1 | 12.3 | HE-AAC | 48000/2 | 3019 | 95285 | -0.1 | 3.5 | 6.9 | -0.2 | 1.1 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he1 | 32 | 34.3 | 13.1 | HE-AAC | 48000/2 | 3019 | 95285 | -0.1 | 5.2 | 2.5 | -0.2 | 1.1 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he1 | 48 | 50.6 | 15.0 | HE-AAC | 48000/2 | 3017 | 95287 | -0.0 | 8.2 | 1.7 | -0.0 | 1.2 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he1 | 64 | 67.1 | 16.8 | HE-AAC | 48000/2 | 3018 | 95286 | -0.0 | 9.2 | 1.5 | 0.0 | 1.2 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he2 | 24 | 26.0 | 9.6 | HE-AACv2 | 48000/2 | 3017 | 95287 | -0.1 | 4.0 | 2.0 | 0.0 | 1.3 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he2 | 32 | 34.2 | 11.1 | HE-AACv2 | 48000/2 | 3018 | 95286 | -0.1 | 4.2 | 1.9 | 0.0 | 1.3 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | syom-he2 | 48 | 50.5 | 11.8 | HE-AACv2 | 48000/2 | 3018 | 95286 | -0.0 | 4.9 | 1.7 | 0.1 | 1.3 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| click-st | transient stereo | fdk-lc | 24 | 25.8 | 7.6 | LC | 48000/2 | 2048 | 95232 | 0.0 | 4.1 | 94.4 | 0.1 | 1.2 |  |
| click-st | transient stereo | fdk-lc | 32 | 34.5 | 7.9 | LC | 48000/2 | 2048 | 95232 | 0.0 | 5.1 | 93.5 | 0.1 | 1.2 |  |
| click-st | transient stereo | fdk-lc | 48 | 50.4 | 7.8 | LC | 48000/2 | 2048 | 95232 | 0.0 | 5.5 | 57.7 | -0.0 | 1.2 |  |
| click-st | transient stereo | fdk-lc | 64 | 68.0 | 8.3 | LC | 48000/2 | 2048 | 95232 | 0.0 | 6.7 | 20.2 | 0.0 | 1.1 |  |
| click-st | transient stereo | fdk-he1 | 24 | 26.0 | 16.0 | HE-AAC | 48000/2 | 5056 | 93248 | -0.0 | 5.8 | 20.6 | -0.0 | 1.3 |  |
| click-st | transient stereo | fdk-he1 | 32 | 34.4 | 14.4 | HE-AAC | 48000/2 | 5057 | 93247 | -0.0 | 6.2 | 2.0 | -0.0 | 1.3 |  |
| click-st | transient stereo | fdk-he1 | 48 | 50.1 | 12.8 | HE-AAC | 48000/2 | 5056 | 93248 | -0.0 | 7.2 | 1.8 | 0.0 | 1.2 |  |
| click-st | transient stereo | fdk-he1 | 64 | 66.3 | 14.2 | HE-AAC | 48000/2 | 5061 | 93243 | -0.2 | 8.0 | 1.6 | 0.1 | 1.2 |  |
| click-st | transient stereo | fdk-he2 | 24 | 25.5 | 8.3 | HE-AACv2 | 48000/2 | 7106 | 91198 | -0.0 | 5.3 | 2.6 | -0.0 | 1.5 |  |
| click-st | transient stereo | fdk-he2 | 32 | 33.6 | 9.3 | HE-AACv2 | 48000/2 | 7104 | 91200 | -0.0 | 5.8 | 2.6 | 0.0 | 1.5 |  |
| click-st | transient stereo | fdk-he2 | 48 | 49.8 | 11.1 | HE-AACv2 | 48000/2 | 7106 | 91198 | -0.1 | 6.1 | 2.4 | 0.0 | 1.5 |  |
| click-st | transient stereo | oxideav-lc | 24 | 14.9 | 3657.9 | LC | 48000/2 | 1024 | 96000 | 6.4 | 0.1 | 78.3 | -0.0 | -0.0 |  |
| click-st | transient stereo | oxideav-lc | 32 | 22.6 | 3682.3 | LC | 48000/2 | 1024 | 96000 | 7.7 | 0.3 | 45.9 | 0.0 | -0.0 |  |
| click-st | transient stereo | oxideav-lc | 48 | 33.5 | 3704.1 | LC | 48000/2 | 1024 | 96000 | 9.2 | 0.5 | 27.3 | 0.1 | -0.0 |  |
| click-st | transient stereo | oxideav-lc | 64 | 53.3 | 3836.7 | LC | 48000/2 | 1024 | 96000 | 10.2 | -0.1 | 9.6 | -0.1 | 0.0 |  |
| click-st | transient stereo | oxideav-he1 | 24 | 15.6 | 1696.9 | HE-AAC | 48000/2 | 3042 | 95262 | -0.0 | 0.9 | 91.7 | -0.1 | 1.0 |  |
| click-st | transient stereo | oxideav-he1 | 32 | 25.4 | 1699.2 | HE-AAC | 48000/2 | 3042 | 95262 | -0.0 | 2.1 | 86.1 | -0.0 | 1.1 |  |
| click-st | transient stereo | oxideav-he1 | 48 | 42.8 | 1744.5 | HE-AAC | 48000/2 | 3041 | 95263 | -0.1 | 5.0 | 1.5 | 0.1 | 1.2 |  |
| click-st | transient stereo | oxideav-he1 | 64 | 59.2 | 1696.3 | HE-AAC | 48000/2 | 3040 | 95264 | -0.0 | 8.9 | 1.5 | -0.1 | 1.2 |  |
| click-st | transient stereo | faac-lc | 24 | 68.0 | 9.5 | LC | 48000/2 | 9952 | 86304 | -0.1 | 1.8 | 18.7 | 0.4 | 1.1 | free-align lag=9952 |
| click-st | transient stereo | faac-lc | 32 | 68.3 | 7.5 | LC | 48000/2 | 9952 | 86304 | -0.1 | 1.8 | 18.7 | 0.4 | 1.1 | free-align lag=9952 |
| click-st | transient stereo | faac-lc | 48 | 71.1 | 8.0 | LC | 48000/2 | 9952 | 86304 | -0.1 | 2.0 | 18.0 | 0.4 | 1.1 | free-align lag=9952 |
| click-st | transient stereo | faac-lc | 64 | 76.7 | 7.0 | LC | 48000/2 | 9952 | 86304 | -0.1 | 2.3 | 15.6 | 0.5 | 1.1 | free-align lag=9952 |
| click-st | transient stereo | lavc9-lc | 24 | 28.0 | 32.7 | LC | 48000/2 | 1024 | 96000 | 0.0 | 1.8 | 101.6 | 0.1 | 1.2 |  |
| click-st | transient stereo | lavc9-lc | 32 | 31.7 | 35.8 | LC | 48000/2 | 1024 | 96000 | 0.0 | 2.4 | 100.8 | 0.1 | 1.4 |  |
| click-st | transient stereo | lavc9-lc | 48 | 51.2 | 35.7 | LC | 48000/2 | 1024 | 96000 | 0.0 | 7.5 | 80.9 | 0.0 | 1.3 |  |
| click-st | transient stereo | lavc9-lc | 64 | 67.3 | 47.3 | LC | 48000/2 | 1024 | 96000 | 0.0 | 7.4 | 30.5 | 0.2 | 1.3 |  |
| ambience | spatial stereo | syom-lc | 24 | 27.2 | 13.8 | LC | 48000/2 | 1024 | 96000 | 12.2 | 16.2 | 74.0 | 0.3 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-lc | 32 | 35.2 | 14.3 | LC | 48000/2 | 1024 | 96000 | 12.3 | 16.4 | 61.3 | 0.2 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-lc | 48 | 51.7 | 15.8 | LC | 48000/2 | 1024 | 96000 | 12.5 | 16.9 | 45.4 | 0.1 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-lc | 64 | 67.9 | 18.5 | LC | 48000/2 | 1024 | 96000 | 13.0 | 17.3 | 35.0 | 0.0 | 0.0 | fdk-dec ok samples=97280 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he1 | 24 | 26.0 | 10.5 | HE-AAC | 48000/2 | 3018 | 95286 | 11.5 | 18.3 | 6.2 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he1 | 32 | 34.3 | 11.4 | HE-AAC | 48000/2 | 3018 | 95286 | 11.9 | 19.0 | 2.9 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he1 | 48 | 50.6 | 13.5 | HE-AAC | 48000/2 | 3018 | 95286 | 12.1 | 19.8 | 2.1 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he1 | 64 | 66.9 | 18.9 | HE-AAC | 48000/2 | 3018 | 95286 | 12.5 | 20.1 | 1.4 | -0.1 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he2 | 24 | 26.1 | 9.4 | HE-AACv2 | 48000/2 | 3018 | 95286 | 11.3 | 17.4 | 2.7 | -0.3 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he2 | 32 | 34.1 | 10.0 | HE-AACv2 | 48000/2 | 3018 | 95286 | 11.3 | 17.5 | 1.8 | -0.4 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | syom-he2 | 48 | 50.6 | 11.4 | HE-AACv2 | 48000/2 | 3018 | 95286 | 11.6 | 17.9 | 1.7 | -0.3 | 0.0 | fdk-dec ok samples=98304 faad-dec ok samples=96256 |
| ambience | spatial stereo | fdk-lc | 24 | 27.3 | 7.6 | LC | 48000/2 | 2048 | 95232 | 1.2 | 2.6 | 89.0 | -2.2 | -0.0 |  |
| ambience | spatial stereo | fdk-lc | 32 | 34.9 | 7.9 | LC | 48000/2 | 2048 | 95232 | 2.6 | 3.9 | 88.3 | -2.0 | -0.0 |  |
| ambience | spatial stereo | fdk-lc | 48 | 50.6 | 6.2 | LC | 48000/2 | 2048 | 95232 | 9.6 | 13.8 | 59.0 | -0.4 | 0.0 |  |
| ambience | spatial stereo | fdk-lc | 64 | 68.6 | 7.9 | LC | 48000/2 | 2048 | 95232 | 13.8 | 23.5 | 23.3 | 0.1 | -0.0 |  |
| ambience | spatial stereo | fdk-he1 | 24 | 26.1 | 16.0 | HE-AAC | 48000/2 | 5056 | 93248 | 6.1 | 8.9 | 18.9 | -0.1 | -0.0 |  |
| ambience | spatial stereo | fdk-he1 | 32 | 34.4 | 12.2 | HE-AAC | 48000/2 | 5057 | 93247 | 12.8 | 21.3 | 2.5 | 0.1 | 0.0 |  |
| ambience | spatial stereo | fdk-he1 | 48 | 50.0 | 15.0 | HE-AAC | 48000/2 | 5057 | 93247 | 13.1 | 22.1 | 2.0 | 0.0 | -0.0 |  |
| ambience | spatial stereo | fdk-he1 | 64 | 66.4 | 15.0 | HE-AAC | 48000/2 | 5057 | 93247 | 12.7 | 24.0 | 1.6 | 0.0 | 0.0 |  |
| ambience | spatial stereo | fdk-he2 | 24 | 25.5 | 10.5 | HE-AACv2 | 48000/2 | 7105 | 91199 | 12.6 | 21.4 | 3.4 | 0.1 | 0.0 |  |
| ambience | spatial stereo | fdk-he2 | 32 | 33.6 | 11.8 | HE-AACv2 | 48000/2 | 7105 | 91199 | 12.6 | 22.1 | 2.9 | 0.2 | 0.0 |  |
| ambience | spatial stereo | fdk-he2 | 48 | 49.8 | 8.5 | HE-AACv2 | 48000/2 | 7105 | 91199 | 12.6 | 22.2 | 2.8 | 0.1 | 0.0 |  |
| ambience | spatial stereo | oxideav-lc | 24 | 18.1 | 3712.5 | LC | 48000/2 | 1024 | 96000 | 12.0 | 15.5 | 62.2 | 0.3 | 0.0 |  |
| ambience | spatial stereo | oxideav-lc | 32 | 22.2 | 3792.2 | LC | 48000/2 | 1024 | 96000 | 12.1 | 15.4 | 54.5 | 0.4 | 0.0 |  |
| ambience | spatial stereo | oxideav-lc | 48 | 34.1 | 3784.6 | LC | 48000/2 | 1024 | 96000 | 12.0 | 15.8 | 35.0 | 0.4 | 0.0 |  |
| ambience | spatial stereo | oxideav-lc | 64 | 52.2 | 3844.1 | LC | 48000/2 | 1024 | 96000 | 12.1 | 16.3 | 14.9 | -0.2 | 0.0 |  |
| ambience | spatial stereo | oxideav-he1 | 24 | 16.8 | 1770.5 | HE-AAC | 48000/2 | 3042 | 95262 | 12.9 | 17.3 | 69.8 | 0.1 | 0.0 |  |
| ambience | spatial stereo | oxideav-he1 | 32 | 25.8 | 1840.2 | HE-AAC | 48000/2 | 3042 | 95262 | 13.0 | 17.6 | 69.4 | 0.4 | 0.0 |  |
| ambience | spatial stereo | oxideav-he1 | 48 | 43.0 | 1823.8 | HE-AAC | 48000/2 | 3042 | 95262 | 12.3 | 21.0 | 1.6 | 0.2 | 0.0 |  |
| ambience | spatial stereo | oxideav-he1 | 64 | 59.4 | 1864.6 | HE-AAC | 48000/2 | 3042 | 95262 | 12.8 | 24.7 | 1.5 | 0.1 | 0.0 |  |
| ambience | spatial stereo | faac-lc | 24 | 29.1 | 9.2 | LC | 48000/2 | 25 | 96000 | 11.4 | 17.8 | 62.6 | 0.2 | 0.0 | free-align lag=25 |
| ambience | spatial stereo | faac-lc | 32 | 37.7 | 6.7 | LC | 48000/2 | 25 | 96000 | 11.2 | 18.3 | 51.6 | 0.2 | 0.0 | free-align lag=25 |
| ambience | spatial stereo | faac-lc | 48 | 53.3 | 9.1 | LC | 48000/2 | 25 | 96000 | 10.9 | 19.0 | 33.3 | 0.1 | 0.0 | free-align lag=25 |
| ambience | spatial stereo | faac-lc | 64 | 69.3 | 7.6 | LC | 48000/2 | 25 | 96000 | 10.6 | 19.7 | 19.4 | 0.0 | 0.0 | free-align lag=25 |
| ambience | spatial stereo | lavc9-lc | 24 | 28.9 | 27.1 | LC | 48000/2 | 1024 | 96000 | 8.6 | 12.7 | 89.2 | 0.4 | 0.0 |  |
| ambience | spatial stereo | lavc9-lc | 32 | 34.0 | 32.0 | LC | 48000/2 | 1024 | 96000 | 8.7 | 13.4 | 89.1 | 0.4 | 0.0 |  |
| ambience | spatial stereo | lavc9-lc | 48 | 50.7 | 36.6 | LC | 48000/2 | 1024 | 96000 | 12.2 | 17.7 | 79.6 | 0.4 | 0.0 |  |
| ambience | spatial stereo | lavc9-lc | 64 | 67.1 | 36.0 | LC | 48000/2 | 1024 | 96000 | 12.9 | 19.8 | 33.5 | 0.3 | 0.0 |  |
| lecture | speech stereo (0.25 s) | syom-lc | 24 | 31.2 | 0.6 | LC | 48000/2 | 1024 | 12000 | 20.8 | 28.5 | 7.6 | 0.0 | 0.0 | fdk-dec ok samples=13312 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-lc | 32 | 40.6 | 0.7 | LC | 48000/2 | 1024 | 12000 | 26.5 | 29.4 | 7.8 | 0.0 | 0.0 | fdk-dec ok samples=13312 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-lc | 48 | 59.2 | 0.6 | LC | 48000/2 | 1024 | 12000 | 34.1 | 38.8 | 7.8 | 0.0 | 0.0 | fdk-dec ok samples=13312 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-lc | 64 | 76.2 | 0.6 | LC | 48000/2 | 1024 | 12000 | 48.1 | 53.1 | 7.8 | 0.0 | 0.0 | fdk-dec ok samples=13312 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he1 | 24 | 32.8 | 1.1 | HE-AAC | 48000/2 | 3018 | 11318 | 15.6 | 15.1 | 16.4 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he1 | 32 | 42.0 | 1.1 | HE-AAC | 48000/2 | 3018 | 11318 | 24.8 | 27.9 | 16.0 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he1 | 48 | 62.5 | 1.1 | HE-AAC | 48000/2 | 3018 | 11318 | 30.4 | 32.9 | 22.0 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he1 | 64 | 83.1 | 1.5 | HE-AAC | 48000/2 | 3018 | 11318 | 37.2 | 45.0 | 19.7 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he2 | 24 | 32.8 | 1.0 | HE-AACv2 | 48000/2 | 3018 | 11318 | 18.5 | 18.9 | 22.1 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he2 | 32 | 42.0 | 1.2 | HE-AACv2 | 48000/2 | 3018 | 11318 | 25.1 | 28.2 | 20.0 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | syom-he2 | 48 | 62.7 | 1.8 | HE-AACv2 | 48000/2 | 3018 | 11318 | 34.0 | 37.8 | 19.8 | 0.0 | 0.0 | fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture | speech stereo (0.25 s) | fdk-lc | 24 | 31.4 | 1.5 | LC | 48000/2 | 2048 | 11264 | 28.5 | 31.5 | 2.5 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-lc | 32 | 39.1 | 2.1 | LC | 48000/2 | 2048 | 11264 | 33.1 | 35.1 | 2.6 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-lc | 48 | 53.9 | 1.7 | LC | 48000/2 | 2048 | 11264 | 33.1 | 35.2 | 7.5 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-lc | 64 | 71.4 | 2.2 | LC | 48000/2 | 2048 | 11264 | 34.7 | 35.2 | 13.8 | -0.0 | -0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he1 | 24 | 32.5 | 3.5 | HE-AAC | 48000/2 | 5057 | 9279 | 30.4 | 34.4 | 21.0 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he1 | 32 | 40.5 | 3.2 | HE-AAC | 48000/2 | 5057 | 9279 | 30.3 | 34.0 | 23.5 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he1 | 48 | 58.4 | 3.8 | HE-AAC | 48000/2 | 5057 | 9279 | 30.5 | 34.8 | 23.2 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he1 | 64 | 77.8 | 3.8 | HE-AAC | 48000/2 | 5057 | 9279 | 30.2 | 34.2 | 22.5 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he2 | 24 | 32.9 | 2.9 | HE-AACv2 | 48000/2 | 7105 | 7231 | 26.6 | 30.5 | 22.3 | 0.0 | -0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he2 | 32 | 42.1 | 2.7 | HE-AACv2 | 48000/2 | 7105 | 7231 | 27.1 | 31.6 | 22.3 | 0.0 | -0.0 |  |
| lecture | speech stereo (0.25 s) | fdk-he2 | 48 | 58.0 | 2.9 | HE-AACv2 | 48000/2 | 7105 | 7231 | 27.2 | 32.0 | 21.2 | 0.0 | -0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-lc | 24 | 26.3 | 456.0 | LC | 48000/2 | 1024 | 12000 | 35.3 | 37.9 | 14.8 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-lc | 32 | 34.0 | 452.9 | LC | 48000/2 | 1024 | 12000 | 38.9 | 43.6 | 14.7 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-lc | 48 | 44.1 | 442.6 | LC | 48000/2 | 1024 | 12000 | 45.5 | 53.4 | 14.3 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-lc | 64 | 51.6 | 460.3 | LC | 48000/2 | 1024 | 12000 | 43.7 | 56.8 | 14.2 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-he1 | 24 | 23.2 | 246.0 | HE-AAC | 48000/2 | 3042 | 11294 | 20.1 | 27.7 | 13.4 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-he1 | 32 | 31.8 | 250.7 | HE-AAC | 48000/2 | 3042 | 11294 | 28.4 | 34.7 | 13.5 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-he1 | 48 | 43.5 | 257.8 | HE-AAC | 48000/2 | 3042 | 11294 | 37.6 | 44.3 | 20.9 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | oxideav-he1 | 64 | 54.6 | 270.7 | HE-AAC | 48000/2 | 3042 | 11294 | 44.0 | 50.5 | 20.7 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | faac-lc | 24 | 31.6 | 2.2 | LC | 48000/2 | 240 | 12000 | 4.6 | 12.4 | 13.6 | 0.0 | -0.0 | free-align lag=240 |
| lecture | speech stereo (0.25 s) | faac-lc | 32 | 37.6 | 2.2 | LC | 48000/2 | 240 | 12000 | 4.6 | 12.4 | 18.8 | -0.0 | -0.0 | free-align lag=240 |
| lecture | speech stereo (0.25 s) | faac-lc | 48 | 46.6 | 2.2 | LC | 48000/2 | 240 | 12000 | 4.6 | 12.4 | 26.5 | 0.0 | 0.0 | free-align lag=240 |
| lecture | speech stereo (0.25 s) | faac-lc | 64 | 50.5 | 2.5 | LC | 48000/2 | 240 | 12000 | 4.6 | 12.3 | 31.4 | 0.0 | -0.0 | free-align lag=240 |
| lecture | speech stereo (0.25 s) | lavc9-lc | 24 | 32.4 | 5.7 | LC | 48000/2 | 1024 | 12000 | 14.7 | 15.4 | 1.2 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | lavc9-lc | 32 | 35.4 | 4.5 | LC | 48000/2 | 1024 | 12000 | 14.9 | 15.4 | 1.2 | 0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | lavc9-lc | 48 | 48.4 | 9.8 | LC | 48000/2 | 1024 | 12000 | 29.7 | 37.8 | 0.2 | -0.0 | 0.0 |  |
| lecture | speech stereo (0.25 s) | lavc9-lc | 64 | 53.8 | 8.1 | LC | 48000/2 | 1024 | 12000 | 31.0 | 36.8 | 0.0 | -0.0 | -0.0 |  |
| lecture-m | speech mono (0.25 s) | syom-lc | 24 | 31.3 | 0.4 | LC | 48000/1 | 1024 | 12000 | 21.0 | 28.1 | 7.6 | — | — | fdk-dec ok samples=13312 faad-dec ok samples=12288 |
| lecture-m | speech mono (0.25 s) | syom-he1 | 24 | 32.4 | 0.8 | HE-AACv2 | 48000/2 | 3018 | 11318 | 21.3 | 22.2 | 22.2 | — | — | ffmpeg dual-mono upmix (implicit PS probe) fdk-dec ok samples=14336 faad-dec ok samples=12288 |
| lecture-m | speech mono (0.25 s) | fdk-lc | 24 | 31.0 | 1.6 | LC | 48000/1 | 2048 | 11264 | 30.9 | 31.9 | 2.8 | — | — |  |
| lecture-m | speech mono (0.25 s) | fdk-he1 | 24 | 32.9 | 2.2 | HE-AACv2 | 48000/2 | 5057 | 9279 | 28.5 | 32.7 | 23.6 | — | — | ffmpeg dual-mono upmix (implicit PS probe) |
| lecture-m | speech mono (0.25 s) | oxideav-lc | 24 | 26.0 | 240.8 | LC | 48000/1 | 1024 | 12000 | 34.8 | 37.6 | 14.6 | — | — |  |
| lecture-m | speech mono (0.25 s) | oxideav-he1 | 24 | 25.6 | 135.8 | HE-AACv2 | 48000/2 | 3042 | 11294 | 28.8 | 32.6 | 21.9 | — | — | ffmpeg dual-mono upmix (implicit PS probe) |
| lecture-m | speech mono (0.25 s) | faac-lc | 24 | 27.4 | 1.7 | LC | 48000/1 | 240 | 12000 | 4.6 | 12.3 | 25.4 | — | — | free-align lag=240 |
| lecture-m | speech mono (0.25 s) | lavc9-lc | 24 | 31.1 | 4.4 | LC | 48000/1 | 1024 | 12000 | 33.5 | 37.4 | 0.1 | — | — |  |

## Timing / memory lane (10 s stereo noise 48 kHz)

| engine | mode | req k | lane | reps | median ms | p95 ms | peak RSS KiB |
|---|---|---:|---|---:|---:|---:|---:|
| syom-lc | — | 48 | in-process | 20 | 72.1 | 75.6 | — |
| syom-lc | — | 48 | process | 5 | 78.2 | 80.8 | 12760 |
| syom-he1 | — | 48 | in-process | 20 | 80.4 | 86.6 | — |
| syom-he1 | — | 48 | process | 5 | 87.0 | 93.8 | 21172 |
| syom-he2 | — | 32 | in-process | 20 | 50.7 | 53.5 | — |
| syom-he2 | — | 32 | process | 5 | 57.5 | 74.3 | 12256 |
| fdk-lc | — | 48 | process | 5 | 25.3 | 34.2 | 12232 |
| fdk-he1 | — | 48 | process | 5 | 65.3 | 68.2 | 12692 |
| fdk-he2 | — | 32 | process | 5 | 37.7 | 47.3 | 12820 |
| oxideav-lc | — | 48 | process | 5 | 20016.2 | 20100.7 | 12780 |
| oxideav-he1 | — | 48 | process | 5 | 9631.2 | 9657.3 | 13348 |
| faac-lc | — | 48 | process | 5 | 27.2 | 28.1 | 12528 |
| lavc9-lc | — | 48 | process | 5 | 142.2 | 145.5 | 12252 |

<!-- MATRIX:END -->

## Same-achieved-rate summary (48 kbps stereo request; SNR / LF SNR / HF err dB)

| clip | syom-lc | syom-he1 | syom-he2 | fdk-lc | fdk-he1 | fdk-he2 | oxideav-lc (act.) | oxideav-he1 (act.) | lavc9-lc |
|---|---|---|---|---|---|---|---|---|---|
| voice-like | 1.4 / 3.7 / 45.5 | 0.6 / 7.4 / 2.0 | −0.2 / 5.3 / 2.0 | 1.4 / 6.1 / 62.4 | 0.7 / 7.4 / 2.0 | −0.3 / 5.3 / 2.5 | −0.1 / 2.1 / 44.1 (39.3k) | −0.1 / 5.5 / 1.9 (42.9k) | 1.1 / 7.7 / 84.7 |
| noise-st | 0.9 / 2.5 / 35.4 | −0.7 / 8.1 / 1.6 | −0.9 / 5.0 / 1.7 | 0.8 / 5.5 / 63.8 | −0.4 / 7.0 / 1.8 | −0.9 / 5.1 / 2.5 | −0.2 / 0.5 / 30.3 (33.7k) | −1.3 / 4.7 / 1.6 (43.1k) | 0.2 / 6.9 / 90.4 |
| mix-st | 14.8 / 19.5 / 34.3 | 14.2 / 19.8 / 1.7 | 7.5 / 8.4 / 1.7 | 10.3 / 12.1 / 54.9 | 16.1 / 24.9 / 1.7 | 7.1 / 8.1 / 2.5 | 14.9 / 19.1 / 31.3 (35.5k) | 15.6 / 24.6 / 1.6 (43.1k) | 13.8 / 18.5 / 73.4 |
| tremolo | 21.0 / 25.9 / 33.9 | 25.1 / 25.7 / 59.6 | **41.0 / 42.5 / 59.3** | 37.5 / 40.5 / 40.5 | 31.1 / 38.9 / 60.8 | 27.6 / 29.0 / 62.0 | 44.8 / 53.8 / 50.8 (47.6k) | 49.5 / 52.9 / 55.6 (35.3k) | 11.6 / 14.6 / 39.0 |
| click-st | 2.3 / 3.4 / 30.9 | −0.0 / 8.2 / 1.7 | −0.0 / 4.9 / 1.7 | 0.0 / 5.5 / 57.7 | −0.0 / 7.2 / 1.8 | −0.1 / 6.1 / 2.4 | 9.2 / 0.5 / 27.3 (33.5k) | −0.1 / 5.0 / 1.5 (42.8k) | 0.0 / 7.5 / 80.9 |
| ambience | 12.5 / 16.9 / 45.4 | 12.1 / 19.8 / 2.1 | 11.6 / 17.9 / 1.7 | 9.6 / 13.8 / 59.0 | 13.1 / 22.1 / 2.0 | 12.6 / 22.2 / 2.8 | 12.0 / 15.8 / 35.0 (34.1k) | 12.3 / 21.0 / 1.6 (43.0k) | 12.2 / 17.7 / 79.6 |
| lecture 0.25 s | 34.1 / 38.8 / 7.8 | 30.4 / 32.9 / 22.0 | 34.0 / 37.8 / 19.8 | 33.1 / 35.2 / 7.5 | 30.5 / 34.8 / 23.2 | 27.2 / 32.0 / 21.2 | 45.5 / 53.4 / 14.3 (44.1k) | 37.6 / 44.3 / 20.9 (43.5k) | 29.7 / 37.8 / 0.2 |
| lecture-m 24k mono | 21.0 / 28.1 / 7.6 | 21.3 / 22.2 / 22.2 | — | 30.9 / 31.9 / 2.8 | 28.5 / 32.7 / 23.6 | — | 34.8 / 37.6 / 14.6 (26.0k) | 28.8 / 32.6 / 21.9 (25.6k) | 33.5 / 37.4 / 0.1 |

Spatial (48k, stereo clips; ILD err / ICC err dB, 0 = perfect):

| clip | syom-he1 | syom-he2 | fdk-he1 | fdk-he2 |
|---|---|---|---|---|
| voice-like | −0.3 / 0.0 | −0.1 / +0.2 | −0.0 / 0.0 | +0.1 / +0.2 |
| mix-st | −0.1 / 0.0 | **−4.8 / 0.0** | −0.0 / 0.0 | **−5.8 / −0.1** |
| ambience | −0.1 / 0.0 | −0.3 / 0.0 | −0.0 / 0.0 | +0.1 / 0.0 |
| click-st | −0.0 / **+1.2** | +0.1 / **+1.3** | +0.0 / **+1.2** | +0.0 / **+1.5** |

## What the numbers say (per category, worst cases first)

1. **Tonal-modulated stereo (tremolo) is syom's worst class.** At 48k,
   syom-lc SNR 21.0 vs fdk-lc 37.5 and oxideav-lc 44.8; syom-he1 LF
   25.7 vs fdk-he1 38.9. The deficit is in the **LC core** (psy/rate
   loop on AM tonal content), inherited by HE. oxideav is ~10% under
   rate here; FDK is rate-matched, so the gap is real. Exception:
   **syom-he2 41.0 dB beats fdk-he2 27.6** — PS frees enough core bits
   that the mono core codes the tone pair properly.
2. **Low-rate speech (lecture 24k).** syom-he1 LF 15.1 vs fdk-he1 34.4
   (stereo); mono lecture-m syom-lc 28.1 vs fdk-lc 31.9, oxideav-lc
   37.6 LF. The 0.25 s clip penalizes HE priming (3018 of 12000
   samples), but FDK carries the same handicap. syom's LC/HE core is
   behind on low-rate speech — same prime suspect as (1): the core psy
   model at low rates (18 dB SMR + water level are untuned, LOWRATE.md).
3. **Broadband noise / speech-like / transient clips: HE parity.**
   voice-like 48k syom-he1 = fdk-he1 exactly (LF 7.4, HF 2.0); noise-st
   syom-he1 LF 8.1 vs fdk 7.0; click-st par. On these classes syom HE
   v1/v2 sit within ±1 dB of FDK at matched rate, and ahead of oxideav
   once rates are matched (oxideav under-spends).
4. **Dense tone+noise under PS collapses identically for both PS
   encoders** (mix-st he2 LF 8.4 syom / 8.1 FDK vs he1 ~19.8/24.9; ILD
   err −4.8 / −5.8): the mono downmix forces the core to carry both
   channels' content. v2 is the wrong mode above ~32 kbps on such
   content — mode-selection guidance, not a syom bug.
5. **Anti-correlated transient stereo reconverges under SBR/PS**
   (click-st ICC err +1.1…+1.5 for syom **and** FDK): regenerated HF is
   correlated across channels. Shared artifact class; nobody wins.
6. **White-noise LC level loss at ≤48k** (syom-lc HF err 35 dB at 48k;
   known water-level hole, `with_pns` is the opt-in tool, TASK-108
   freeze). lavc9 is worse (90 dB), FDK LC similar (63.8). HE modes fix
   this exact hole (HF err 1.6).
7. **oxideav-aac 0.1.7** is a real Rust HE v1 leader on tonal material
   but its encoder is ~100–250× slower than syom (20 s for 10 s LC,
   sub-realtime) and its ABR ceiling lands 15–30% under request.
   **FAAC 1.31.1** cannot hold a 24k request on harmonic content (68k
   actual) and shows no stable encoder delay over ADTS; it is not a
   low-rate quality leader. **lavc9** native `aac` trails the field at
   low rates except on transients.

## Delay / CPU / memory (verified before ranking)

| engine | declared→measured delay (out samples) | encode, 10 s stereo noise 48k | peak RSS (process lane) |
|---|---|---|---|
| syom-lc 48k | 1024 → 1024 | 72.1 ms in-proc (20 reps, p95 75.6) / 78.2 proc | 12.8 MiB |
| syom-he1 48k | 3018 → 3018 | 80.4 ms in-proc (p95 86.6) / 87.0 proc | 21.2 MiB |
| syom-he2 32k | 3018 → 3018 | 50.7 ms in-proc (p95 53.5) / 57.5 proc | 12.3 MiB |
| fdk-lc 48k | 2048 → 2048 | 25.3 ms proc | 12.2 MiB |
| fdk-he1 48k | 5058 → 5057 | 65.3 ms proc | 12.7 MiB |
| fdk-he2 32k | 7106 → 7105 | 37.7 ms proc | 12.8 MiB |
| oxideav-lc 48k | 1024 → 1024 | 20 016 ms proc | 12.8 MiB |
| oxideav-he1 48k | ≈3030 (probe) → 3042 | 9 631 ms proc | 13.3 MiB |
| faac-lc 48k | none declared; envelope lag unstable | 27.2 ms proc | 12.5 MiB |
| lavc9-lc 48k | 1024 → 1024 | 142.2 ms proc | 12.3 MiB |

syom HE v1 encode is ~1.1× its LC cost, ~2.9× slower than FDK LC,
roughly par with FDK HE v1, ~120× faster than oxideav. Delay: syom HE
priming 3018 output samples vs FDK 5057/7106 — syom HE starts sound
earlier. Exact valid length: decoded coverage after declared priming is
95286/96000 samples on 2 s cells (the ffmpeg ADTS drain keeps < 1 AU of
tail beyond; M4A `elst` exactness is product-tested by TASK-42). The
syom-he1 21.2 MiB process RSS vs 12.3–12.8 for peers/syom-he2 is
dominated by one-shot owned output + stereo SBR analysis buffers;
in-process heap figures stay with `lab/baseline/ALLOC.md` / `HE_AU.md`.

## Blinded screening status (AC#3)

- Registered protocol mechanics re-verified: `python3
  scripts/listen_protocol.py dry-run` passes (8 valid / 1 excluded
  synthetic listener, noninferiority CI computed, output still marked
  `SYNTHETIC-NOT-LISTENING-EVIDENCE`; DOC-5/protocol unchanged).
- The protocol's condition table still lists HE v1/v2 encode as
  "deferred — no HE encoder"; now stale, belongs to TASK-109 (which owns
  the live study and is blocked on humans + TASK-120 holdout + Apple).
- **No mono metric was used to qualify PS stereo**: PS cells carry
  per-channel SNR/LF plus ILD/ICC error against the source image
  (tables above). Human blinded screening of HE modes remains open and
  is **not** claimed here.

## Conclusions (AC#4) — per mode, evidence-bound

| Mode | Verdict | Basis |
|---|---|---|
| HE v1 (`low_rate()` 48k, opt-in) | **release-ready as opt-in for broadband/noise/speech-like content at 32–64 kbps**; **no-go as a quality claim on tonal-modulated stereo or ≤32k speech** | parity with FDK on 4 of 6 classes at matched rate; FDK+FAAD2+lavc three-lineage decode; deficits (1)(2) measured |
| HE v2 (`with_he_v2`, opt-in) | **release-ready as opt-in for stereo ≤32–48k**; no-go on dense tone+noise (shared PS collapse) and anti-phase (documented, PS_EST.md) | ILD/ICC at par or better than FDK HE v2; tremolo +13 dB over FDK HE v2; mix-st collapse matches FDK |
| LC at 24–48k | unchanged: known noise-floor hole (PNS opt-in), now also measured **behind FDK on tonal/speech low-rate** | matrix LC rows |
| Any "beats FDK/Apple" or listening claim | **no-go** | Apple host absent (TASK-10); humans absent (TASK-109); PEAQ/ViSQOL absent |

Material deficits → bounded follow-up tasks (created this session):

1. **TASK-133 (high): LC core underperforms on tonal-modulated stereo
   and low-rate speech** (tremolo 48k: 21.0 vs FDK 37.5 dB; lecture 24k
   HE LF 15.1 vs 34.4). Bound: psy/rate-loop tuning measured on those
   two cells, gate = reach FDK LF SNR at matched actual rate without
   >1 dB loss on noise/mix.
2. **TASK-134 (medium): PS fine IID grid / IPD-OPD evaluation** —
   coarse `iid_mode 1` costs up to ~1.5 dB ILD mid-range (HE_V2.md) and
   −4.8 dB image shift on mix-st (FDK: −5.8); bound: measure
   `iid_mode` 4 on ambience/mix-st, adopt only if ILD err halves at
   ≤ +0.3 kbps.

Already tracked elsewhere, not duplicated: FDK stuffing interop
(TASK-121, in-flight — this matrix is its independent confirmation:
FAAD2/lavc accepted the old bytes, FDK did not), Apple host (TASK-10),
held-out listening (TASK-109/TASK-120), USAC/xHE evaluation (TASK-97),
stale protocol HE row (TASK-109 scope).

## Reproduce

```sh
# one-time oracle builds (pins: lab/fdk/PIN.md, lab/faac/PIN.md, lab/oxideav/PIN.md)
sh target/fdk-build/build.sh            # or per lab/fdk/PIN.md
FDK_PREFIX=$PWD/target/fdk-build/prefix sh lab/fdk/build_driver.sh
# faac prefix per lab/faac/PIN.md; oxideav: cargo build --release --manifest-path lab/oxideav/Cargo.toml
cargo run --release --manifest-path lab/quality/Cargo.toml --bin syom_he_qualify
# or: make -C lab/quality hequal
```

Requires `lab/fdk/fdk_driver`, `lab/faac/faac_driver`,
`lab/oxideav/target/release/oxideav_driver`, `lab/libavcodec/avc_driver`,
`lab/faad2/faad_driver`, `ffmpeg`/`ffprobe` and `python3` on PATH.
Missing peers degrade to `—` rows; syom rows still print.
