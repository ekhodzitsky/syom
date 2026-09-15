# HE v1 access units (TASK-89)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, on top of
`e957621` (TASK-88). Crate-internal `engine/enc_he.rs` (`HeEncoder`),
LC hooks in `enc_frame_he.rs` (`set_fill`, `set_cutoff_hz`), header
policy in `enc_sbr_header.rs`. `encode()` / push `Encoder` still emit
LC only; options and ADTS/M4A/ASC signalling are TASK-90.

## Pipeline per access unit (2048 output samples)

```
PCM out_rate ─ SbrPrep ─┬─ halfband 2:1 (delay 4 core) → core frame n → LcEncoder(core_rate, bitrate_bps)
                        │                                   psy cutoff at k0 (core codes only below the crossover)
                        └─ 64-band slots ─ window [32(n−1)−6, 32(n−1)+26) → SbrEstimator per channel
                                            → sbr_extension_payload (header in every AU) → LcEncoder::set_fill
AU n = SCE/CPE raw_data_block + FIL(EXT_SBR_DATA) + END   (fill bits counted in build(), fit_budget pads the whole AU)
```

- The SBR window for AU `n` covers core frame `n − 1` (the LC decoder's
  one-frame priming) shifted 6 analysis slots early (`tHFGen − tHFAdj`);
  slots before the stream or after the flushed banks are exact zeros.
- Lookahead: the LC emits frame `n − 1` when frame `n` is pushed, so the
  fill is delayed one frame with it; `finish` flushes the held frame.
- `finish`: `SbrPrep::finish` pads the halfband/QMF drain, the content
  tail is zero-padded to a core frame, then one silent drain frame;
  `core_content = ceil((N + 8) / 2)`, AUs = `ceil(core_content / 1024) + 1`.
- Crossover per core kbps per channel (`he_header`): 12 → 4.5 kHz,
  24 → 6.75 kHz, ≥ 32 → 9 kHz at 48 kHz, clamped to `[4.5 kHz, 0.2·fs_sbr]`
  (k0 under ≈ 12 at 44.1/48 kHz has no valid patch set). The LC psy
  never codes bands above it (long and short).
- No CRC; `bs_coupling = 0`; header transmitted in every AU (16 bits ≈
  0.4 kbps at 48 kHz) so every AU is a sync point.

## Evidence (`cargo test --lib enc_he_tests -- --nocapture`, 8 tests)

| check | result |
|---|---|
| ADTS with the core `sampling_frequency_index`, `probe` says LC | decodes at 48 kHz / core 24 kHz / 2 ch, `aus · 2048` samples (`audio()` and `speech()`) |
| chunking 1 / 333 / 2048 / 100000 samples, reset, lookahead | byte-identical AUs and tallies |
| rate accounting, 10 s stereo material | 24 000 → 23 997 bps, 48 000 → 48 010 bps; largest AU 1232 / 2360 bits (cap 12288) |
| HF energy 6.75–15.4 kHz vs source (48 kHz stereo) | HE −0.4 / −0.2 / −0.1 dB at 24 / 32 / 48 kbps; LC at those rates decodes to silence (see finding) — gate HE ≥ LC + 6 dB met trivially |
| LF energy < 6.75 kHz vs source | −1.1 / +2.7 / +3.3 dB (core quantization inflation, see finding) |
| delay: 300 Hz Hann burst, cross-correlation lag | **3018** output samples = `HE_PRIMING_OUT` (2048 LC + 8 halfband + 962 QMF chain) |
| final 200 source samples | present in the decoded tail (drain frame carries them); `remainder_out` < 3 frames |
| silence | every AU still carries an SBR header + element (≥ 8 payload bytes); no header-only AU |
| libavcodec | `ffmpeg -i src/goldens/he48e.adts` → `aac (HE-AAC), 48000 Hz, stereo`; our decode vs `he48e.lavc.s16` within the LC golden tolerance (≤ 2 LSB s16 / ≥ 55 dB per channel, same length ⇒ same delay) |
| determinism | fresh encode byte-exact with the committed golden (tripwire, like `enc48.adts`) |

Per-band levels (decoded / source, 64-band re-analysis, 2 s stereo
material): the SBR range 18..40 sits within ±1 dB at every rate; bands
above k2 (41+) are not coded (−14 dB then −73 dB), by design.

CPU / memory (release, 10 s stereo 48 kHz noise + tones, best of 5):

| cell | value |
|---|---:|
| HE 48 kbps (`HeEncoder`, 4096-sample chunks) | 77.7 ms (60 409 B) |
| HE 48 kbps + lookahead | 76.7 ms |
| LC 48 kbps (`encode_with`) | 113.1 ms (63 450 B) |
| LC 128 kbps default | 171.6 ms |
| HE / LC same rate | **0.69×** (budget ≤ 4×) |
| `size_of::<HeEncoder>()` | 18 088 B + core/slot buffers (bounded: slots older than the next window are dropped) |

## Findings

- **LC core at low rates** (not HE-specific, pre-existing): with ≈ 1024
  bits per channel-frame the LC quantizer inflates noise-like band
  energy by +3–4 dB (visible in the HE core bands 0..17 at 24 kbps/ch and
  in LC at 128 kbps on the same material); at ≤ 48 kbps stereo 48 kHz
  (≤ 512 bits/channel-frame) LC drops every band and decodes to silence.
  Backlogged as a separate task; the HE gate above is therefore not a
  comparison against a working LC at those rates.
- The decoder's SBR chain delay is half-integral (prototype filters);
  962 is the rounded cross-correlation lag and libavcodec's output has
  the same length and matches sample-for-sample within tolerance.
- The empty-patch-source effect (SBR_EST.md) is real with an LC core:
  the crossover must track the core's affordable bandwidth; the kbps
  rule above is the v1 answer, TASK-95 retunes it.

## Not claimed

- No public API, no M4A/ASC signalling, no LATM (TASK-90/94).
- Quality vs FDK/Apple/oxideav (TASK-95); no listening (TASK-109).
- No aarch64 run this session; the golden tripwire covers it in CI once
  TASK-90 lands it in the encoder golden matrix.
