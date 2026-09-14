# TASK-65: live LC rate accounting (syom encode)

Recorded 2026-09-14. Host: Linux x86_64, rustc 1.97.1.
Encoder: in-tree `Encoder` / `encode_with`, ADTS, default lookahead **off**.
Ordinary `cargo test` drives the same path (`src/encode_rate_tests.rs`);
no ffmpeg/FDK.

Duration: 1.000 s at 48 kHz except `lecture` (presentation 12 000 samples
= 0.250 s after `elst`) and the 4 s sine check. **Valid N** is
`EncodeInfo.samples`. **Coded** is `aac_frames * 1024` (content +
zero-pad remainder + overlap drain). **Payload** is ADTS frame length
minus the 7-byte header (`encode_cmp::adts_payload_bytes`).
`pay/valid` = `8 · payload · sr / N`. `adts/valid` includes headers.

Per-frame **budget** = `min(bitrate_bps · 1024 / sr, 6144 · ch)`.
`credit` is at most one frame of leftover (not a bit reservoir).
Every frame has `adts_buffer_fullness = 0x7FF` (VBR / unknown).

## 1 s matrix (and lecture)

| clip | class | req | payload | ADTS | frames | valid | coded | pay/valid | pay/coded | adts/valid | bits min/mean/max | budget | pay/req |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|
| silence | silence | 128000 | 288 | 624 | 48 | 48000 | 49152 | 2304 | 2250 | 4992 | 48 / 48 / 48 | 2730 | 0.018 |
| sine440 | tonal | 64000 | 2463 | 2799 | 48 | 48000 | 49152 | 19704 | 19242 | 22392 | 312 / 410 / 1992 | 1365 | 0.308 |
| sine440 | tonal | 128000 | 2881 | 3217 | 48 | 48000 | 49152 | 23048 | 22508 | 25736 | 312 / 480 / 3992 | 2730 | 0.180 |
| noise | noise | 64000 | 8163 | 8499 | 48 | 48000 | 49152 | 65304 | 63773 | 67992 | 1128 / 1360 / 1552 | 1365 | 1.020 |
| noise | noise | 128000 | 16359 | 16695 | 48 | 48000 | 49152 | 130872 | 127805 | 133560 | 2584 / 2726 / 2896 | 2730 | 1.022 |
| click | transient | 64000 | 6146 | 6482 | 48 | 48000 | 49152 | 49168 | 48016 | 51856 | 48 / 1024 / 2008 | 1365 | 0.768 |
| click | transient | 128000 | 11872 | 12208 | 48 | 48000 | 49152 | 94976 | 92750 | 97664 | 48 / 1979 / 4016 | 2730 | 0.742 |
| tremolo | stereo | 128000 | 4787 | 5123 | 48 | 48000 | 49152 | 38296 | 37398 | 40984 | 608 / 798 / 3960 | 2730 | 0.299 |
| lecture | speech | 128000 | 1922 | 2013 | 13 | 12000 | 13312 | 61504 | 55442 | 64416 | 232 / 1183 / 3976 | 2730 | 0.480 |

`adts/valid` on noise 128k (133 560) matches the TASK-16 syom cell
(133.6 kbps). Headers are always `7 · frames` (336 B on 1 s). Drain
makes `pay/coded` ~2% below `pay/valid` on 1 s (`48/46.875` frames).

4 s sine 128k still undershoots (`pay/req < 0.40`, mean frame bits
`< 0.30 · budget`): this is the rate loop sitting at the finest sf
offset with leftover budget, not a short-clip drain artefact.

## Reading

- **Noise is budget-limited.** Mean frame bits ≈ budget (2726 vs 2730
  at 128k). `pay/req` ≈ 1.02 (integer sf + one-frame `credit`).
- **Sine / tremolo / lecture are quality-limited.** Finest offset
  (`OFFSET_LO = -8`) still fits in far fewer bits than the ceiling.
  Raising `bitrate_bps` from 64k to 128k barely moves sine (19.7 →
  23.0 kbps payload/valid).
- **Silence** spends syntax (~48 bits/frame). Hitting 128k would be
  stuffing, not signal.
- **Click** is mixed: impulse frames spend, quiet frames do not
  (min 48, max 4016 at 128k).
- No frame exceeds `6144 · channels`.

## Terms (this crate)

| Name | Meaning here |
|---|---|
| `bitrate_bps` | Per-frame **ceiling** (plus at most one frame of `credit`). |
| Capped VBR | Current mode. ADTS `0x7FF`. Easy content undershoots. |
| ABR | Average payload/valid toward the request on **budget-limited** content; TASK-66. |
| CBR | Bit reservoir + `buffer_fullness ≠ 0x7FF`. **Not implemented.** |
| TVBR / quality VBR | A quality knob instead of a bit ceiling. TASK-67, not 0.x default. |
| Reservoir | ISO LC decoder buffer (typically 6144 bits/ch). `credit` is **not** that. |

## TASK-66 gates (set before any rate-loop change)

Measurement: `8 · payload_bytes · sr / N` (valid duration). Report
headers and drain separately. Default 128k / lookahead-off unchanged.

- **Go:** spend leftover budget on non-silent content by coding more
  bands / finer residual until `pay/valid` is within **±3%** of
  `bitrate_bps` on **≥10 s budget-limited or mixed** tracks (noise,
  speech with residual, music), without exceeding 6144 bits/ch.
- **Exception (not a fail):** silence; `N < 2048`; rates rejected at
  construction (`bitrate_bps == 0` or `> 6144·ch·sr/1024`).
- **Infeasible without stuffing:** pure silence. A 440 Hz sine is
  **not** exempt — lavc spends ~127 kbps on the same clip; undershoot
  here is a rate-loop defect, not a law of nature.
- **No-go now:** CBR reservoir / non-`0x7FF` fullness (follow-on after
  ABR). Quality-only TVBR (TASK-67). Changing the 128k default.
- Syntax/quality regression: goldens stay byte-identical until TASK-66
  deliberately remints with independent lavc s16; no panic; LC cap
  holds; `det_math` / no-libm contract unchanged.
