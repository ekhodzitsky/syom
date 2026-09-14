# FAAC 1.31.1 encode smoke (TASK-9)

Recorded 2026-09-14, gcc 15.2.0, rustc 1.97.1, Linux x86_64.
In-process `libfaac` ADTS, `FAAC_INPUT_FLOAT`, MPEG-4 AAC-LC (`LOW`),
TNS off, `JOINT_MS`. Independent decode: syom `decode_with(..., unbounded())`.
2 s 440 Hz sine, peak 0.5, 48 kHz.

`bitRate` is **per channel**. `faacEncOpen` returns `input_samples=1024*ch`.
Flush: 5 zero-input calls. `quantqual` became **100** at 64k/128k/192k-per-ch
for this host (1.31.1 maps `bitRate` into the quality loop).

## Rate points

| Case | requested bitRate/ch | ADTS bytes | syom rate/ch/samples | finite | actual bps (8*bytes/decoded_dur) |
|---|---|---|---|---|---|
| mono | 64000 | 10931 | 48000 / 1 / 97280 | yes | 43149 |
| mono | 128000 | 11110 | 48000 / 1 / 97280 | yes | 43855 |
| mono | 192000 | 11144 | 48000 / 1 / 97280 | yes | 43989 |
| stereo | 64000 (128 k stream) | 18205 | 48000 / 2 / 97280 | yes | 71862 |

Input PCM was 96000 samples/ch (2.000 s). Independent decode is **97280**
samples/ch (+1280 = 26.67 ms vs source). That is the measured
priming/flush remainder for this 1.31.1 run, not a historical 1024-sample
assumption. Valid output is the full decoded length (finite planar f32).

Requested 64/128/192 kbps **do not** produce proportional ADTS size on this
tonal sine; actual rate sits ~43–44 kbps mono. Do not treat FAAC `bitRate`
as CBR for comparisons.

## Unsupported cells

- HE-AAC v1/v2 encode: **unavailable** in 1.31.1 `libfaac` (no SBR encoder
  sources). Current GitHub README HE-v1 claim is **not** this release.
- LATM/LOAS, M4A mux: **not** this adapter (ADTS only).
- MAIN/SSR/LTP object types: not smoked (LC `LOW` only).

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` / ordinary tests | **no-go** (LGPL + MPEG-4 reference notice) |
| Encoder quality/speed cell vs syom | **go** only with **actual** ADTS bitrate and independent decode length |
| Assume FAAC is CBR at the requested number | **no-go** |
| HE encode peer | **no-go** on 1.31.1 |
