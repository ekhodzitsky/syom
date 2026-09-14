# TASK-65/66: live LC rate accounting (syom encode)

Recorded 2026-09-14. Host: Linux x86_64, rustc 1.97.1.
Encoder: in-tree `Encoder` / `encode_with`, ADTS, default lookahead **off**.
Ordinary `cargo test` drives the same path (`src/encode_rate_tests.rs`);
no ffmpeg/FDK.

Duration: 1.000 s at 48 kHz except `lecture` (presentation 12 000 samples
= 0.250 s after `elst`) and the 10 s ABR gate. **Valid N** is
`EncodeInfo.samples`. **Coded** is `aac_frames * 1024` (content +
zero-pad remainder + overlap drain). **Payload** is ADTS frame length
minus the 7-byte header (`encode_cmp::adts_payload_bytes`).
`pay/valid` = `8 · payload · sr / N`. `adts/valid` includes headers.

Per-frame **budget** = `min(bitrate_bps · 1024 / sr, 6144 · ch)`.
`credit` is at most one frame of leftover (not a bit reservoir) and
still feeds the next frame's rate-loop search. **TASK-66 pad** is unused
bytes after `ID_END` (decoder stops at END). All-zero spectra are not
padded. Frames that spent credit above `budget` accrue `pad_debt` so
later undershoot frames stuff less — average stays on the ABR target.
Every frame has `adts_buffer_fullness = 0x7FF` (VBR / unknown).

## After TASK-66 pad (1 s matrix)

| clip | class | req | payload | ADTS | frames | valid | coded | pay/valid | pay/coded | adts/valid | bits min/mean/max | budget | pay/req |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|---:|
| silence | silence | 128000 | 288 | 624 | 48 | 48000 | 49152 | 2304 | 2250 | 4992 | 48 / 48 / 48 | 2730 | 0.018 |
| sine440 | tonal | 64000 | 8235 | 8571 | 48 | 48000 | 49152 | 65880 | 64336 | 68568 | 1328 / 1372 / 1992 | 1365 | 1.029 |
| sine440 | tonal | 128000 | 16519 | 16855 | 48 | 48000 | 49152 | 132152 | 129055 | 134840 | 2672 / 2753 / 3992 | 2730 | 1.032 |
| noise | noise | 64000 | 8192 | 8528 | 48 | 48000 | 49152 | 65536 | 64000 | 68224 | 1200 / 1365 / 1552 | 1365 | 1.024 |
| noise | noise | 128000 | 16359 | 16695 | 48 | 48000 | 49152 | 130872 | 127805 | 133560 | 2584 / 2726 / 2896 | 2730 | 1.022 |
| click | transient | 64000 | 6146 | 6482 | 48 | 48000 | 49152 | 49168 | 48016 | 51856 | 48 / 1024 / 2008 | 1365 | 0.768 |
| click | transient | 128000 | 11975 | 12311 | 48 | 48000 | 49152 | 95800 | 93555 | 98488 | 48 / 1996 / 4016 | 2730 | 0.748 |
| tremolo | stereo | 128000 | 16522 | 16858 | 48 | 48000 | 49152 | 132176 | 129078 | 134864 | 2728 / 2754 / 3960 | 2730 | 1.033 |
| lecture | speech | 128000 | 4578 | 4669 | 13 | 12000 | 13312 | 146496 | 132058 | 149408 | 2664 / 2817 / 3976 | 2730 | 1.145 |

## 10 s ABR gate (payload/valid ±3%)

| clip | req | pay/valid | ratio | bits mean / budget |
|---|---:|---:|---:|---|
| sine10s | 128000 | 128338 | 1.003 | 2731 / 2730 |
| noise10s | 64000 | 64174 | 1.003 | 1365 / 1365 |
| noise10s | 128000 | 128297 | 1.002 | 2730 / 2730 |
| tremolo10s | 128000 | 128422 | 1.003 | 2732 / 2730 |
| lecture10s | 128000 | 128366 | 1.003 | 2731 / 2730 |

0.25 s lecture is 14.5% high on pay/valid because drain dominates; the
10 s tile meets ±3%. 1 s sine/tremolo sit ~3.2% high for the same
drain reason (documented exception, not the TASK-66 gate).

## Mechanism (measured)

- **Leftover-band fill at TARGET_Q:** hits ±3% but injects residual
  (440 Hz sine SNR ~0 dB). **Rejected.**
- **FIL / DSE before END:** hits ±3% and is PCM-neutral on 440 Hz, but
  inserting an extra element plus counting stuffed bytes as `credit`
  starved the next frame's rate loop (440+2997 stereo SNR 26.3 dB vs
  ≥30). **Rejected as the credit source.**
- **Unused bytes after `ID_END`:** decoder (syom and lavc) stops at END.
  PCM-identical to the unpadded `raw_data_block`. `credit` uses coded
  size before pad. `pad_debt` pays back frames that spent credit above
  `budget` by stuffing less later. **Accepted.**

## Reading

- **Sine / tremolo / lecture** spend the 10 s budget via unused bytes
  after END (mean bits ≈ budget).
- **Noise** was already budget-limited; pad rarely runs.
- **Silence** stays tiny (48 bits/frame of syntax, ratio 0.018). Not
  stuffed.
- **Click** still undershoots on quiet (all-zero) frames (min 48 bits).
- No frame exceeds `6144 · channels`. Fullness stays `0x7FF`.

## Terms (this crate)

| Name | Meaning here |
|---|---|
| `bitrate_bps` | ABR target: per-frame ceiling + unused bytes after `ID_END`. |
| ABR | payload/valid ±3% on ≥10 s non-silent (TASK-66, met). |
| CBR | Bit reservoir + `buffer_fullness ≠ 0x7FF`. **Not implemented.** |
| TVBR / quality VBR | A quality knob instead of a bit target. TASK-67. |
| Reservoir | ISO LC decoder buffer. One-frame `credit` is **not** that. |

## TASK-66 result

Measurement: `8 · payload_bytes · sr / N` (valid duration). Default 128k
/ lookahead-off unchanged. Goldens reminted (ADTS/M4A bytes); lavc s16
PCM is unchanged (stuffing is PCM-neutral).

- **Met:** 10 s sine/noise/tremolo/lecture payload/valid ratio **1.002–1.003**.
- **Exception (held):** silence ratio 0.018 (not stuffed); `N < 2048`
  still encodes; illegal rates still `Encode`; 1 s drain can sit ~3%
  high; click trains undershoot on quiet frames.
- **No-go held:** CBR reservoir / non-`0x7FF`; TVBR; 128k default flip;
  leftover-band fill (quality); FIL/DSE stuffing as the credit source.
- Search bound: existing offset binary search + ≤ bands·ch drops + one
  emit-and-resize. No unbounded loop.
