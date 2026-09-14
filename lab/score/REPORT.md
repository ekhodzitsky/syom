# Neutral-decoder scoring smoke (TASK-13)

Recorded 2026-09-14, rustc 1.97.1, Linux x86_64. Isolated `lab/score`.
PEAQ and ViSQOL binaries are **not** on this host.

## Controls (must not look like high quality)

| Control | Outcome |
|---|---|
| identical 440 Hz | scored delay=0 snr=inf |
| silent zeros | diagnostic Silent |
| delayed +256 samples | diagnostic Delayed |
| truncated 128 of 2048 | diagnostic Truncated |
| L/R swap (440 vs 660) | diagnostic ChannelSwap |
| 1 ch vs 2 ch | unscorable channel mismatch |

## ADTS independent decode

`sine48.adts` vs itself: scored delay=0 valid=13312 remainder=0 snr=inf
actual_bps=128423 (8*4452/(13312/48000)). peaq=unavailable
visqol=unavailable.

## Calibration / claims

In-tree SNR is a waveform error after alignment. It is **not** ITU-R
BS.1387 PEAQ and **not** ViSQOL. No transparency or certification claim.
Rate conversion is not performed (mismatch is unscorable). Stereo swap is
a diagnostic, not a mono MOS.

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` | **no-go** |
| Alignment + bitrate + SNR lab | **go** for encoder screening with the diagnostics above |
| PEAQ ODG / ViSQOL MOS-LQO cells | **no-go** until pinned tools exist |
| Treat inf SNR as transparent | **no-go** |
