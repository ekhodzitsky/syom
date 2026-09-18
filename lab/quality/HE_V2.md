# TASK-92 / TASK-93 — HE-AAC v2 encode (parametric stereo)

`EncodeOptions::with_he_v2(true)`: stereo input becomes one mono HE v1
stream (LC core at half rate + SBR) plus a `ps_data()` payload per access
unit in the SBR `bs_extended_data` block. Analysis: `PS_EST.md`.

## Payload (`engine/enc_ps_bits.rs`)

- Syntax written: `iid_mode` 1 (20 bands, coarse), `icc_mode` 1 (20
  bands), no extension layer, FIX class with one envelope, or zero
  envelopes ("hold") when no analysis frame is due (tail, drain).
- Each row is frequency- or time-differential, whichever is shorter; a
  frame carrying the configuration header is always frequency-differential
  (a decoder can join there). The header repeats every 8 access units.
- Carriage: `bs_extended_data`, `bs_extension_size` with the 15 + 8-bit
  escape, `bs_extension_id` 2, zero fill to the declared bytes. Mono
  element only; a channel pair with PS is an error.
- Evidence: hand-derived bit vectors (header frame = 55 bits, repeat
  frame, hold frame), 200-frame random walk recovered exactly by the
  decoder's `PsData::parse` + `resolve` (DF and DT both exercised, header
  refresh, holds), exact block sizes across the 14/15/16-byte boundary with
  an independent field read-back, PS inside a real `sbr_extension_data()`
  parsed by the SBR element parser, failure cases write nothing.
- External lineage: ffmpeg 7.0.2 identifies the streams as **HE-AACv2,
  2 channels, 48 kHz** and reproduces the written image (below).

## Integration (`engine/enc_he_ps.rs`)

- `PsHeEncoder` = `PsAnalysis` (downmix + parameters per 2048-sample
  block) in front of the unchanged mono `HeEncoder`; the downmix has no
  delay, so priming (3018 output samples), remainder and one access unit
  per 2048 samples are HE v1's. `bitrate_bps` is whole-stream: the fill
  element (SBR + PS) is counted in the core's rate loop as before.
- Alignment: the decoder reaches a frame's parameters at the END of its PS
  frame and interpolates linearly towards them. Analysis frame `k` covers
  input slots `[32k − 8, 32k + 24)` and rides in access unit `k`. Measured
  on a hard left→right flip: the 0 dB crossing sits about 500 samples
  before the event and the ramp ends about 2.6 k samples after it; sending
  the frame one unit later put the crossing 1.5 k samples late.
- Signalling: ADTS implicit (mono header at the core rate); M4A, LATM and
  `Encoder::asc` explicit AOT 29 (`asc::write_he_ps`), M4A `channelcount`
  2, `EncodeInfo` reports 2 channels / `Mpeg(2)`.
- Errors before any write: mono input, `ps` without `he`, quality VBR,
  rates outside the HE table.

## Measurements

libavcodec decode of the goldens `he2_48e.adts` / `he2_48em.m4a` (flip
programme, 32 kbps): left part +25.0 dB, right part −25.0 dB, 17.4 dB one
unit before the flip, −23.1 dB one unit after; syom's decode of the same
bytes agrees with libavcodec above 60 dB SNR per channel.

HE v2 against HE v1 stereo at the same whole-stream rate (tones panned
6 dB + independent ambience, 4.1 s; source ILD 5.76 dB, L/R correlation
0.952; release build, Ryzen AI 9 HX 370):

| mode | request | ADTS kbps | encode ms | mid-signal SNR | ILD | L/R corr |
|---|---:|---:|---:|---:|---:|---:|
| v1 | 24 | 25.9 | 20.7 | 7.0 dB | 5.73 dB | 0.975 |
| v2 | 24 | 25.9 | 15.5 | 4.7 dB | 6.77 dB | 0.991 |
| v1 | 32 | 34.1 | 21.0 | 6.9 dB | 5.69 dB | 0.967 |
| v2 | 32 | 34.1 | 17.2 | 15.6 dB | 6.77 dB | 0.989 |
| v1 | 48 | 50.4 | 24.2 | 4.3 dB | 5.78 dB | 0.971 |
| v2 | 48 | 50.3 | 18.9 | 15.9 dB | 6.77 dB | 0.990 |

ADTS kbps includes 1.3 kbps of headers. Reading: from 32 kbps the mono
core holds every tone as a waveform (crossover 9 kHz) and the mid signal
gains about 9 dB over v1, whose per-channel crossover stays near 4.5–6.7
kHz; at 24 kbps both modes regenerate the 7 kHz tone by SBR. v2 is 20–25%
cheaper to encode (one core channel). The ILD is 1 dB off because 5.76 dB
falls between the coarse grid points 4 and 7 dB.

## Limits and follow-ups

- Coarse IID grid (up to about ±1.5 dB of level error mid-range); the fine
  grid (`iid_mode` 4) is a measured follow-up.
- No IPD/OPD and a time-domain downmix: exact anti-phase content cancels.
- One envelope per unit: spatial changes are followed within about one
  access unit (43 ms at 48 kHz).
- Evidence is objective and synthetic; no listening, and FDK / FAAD2
  decoders were not available offline (libavcodec and syom only).
