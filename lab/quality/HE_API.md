# HE v1 public surface (TASK-90)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, ffmpeg
`~/.local/bin/ffmpeg` (offline oracle only), on top of `e4bad7b`.

## Surface

| Item | Behaviour |
|---|---|
| `EncodeOptions::with_he(true)` / `he: bool` | default `false`; `encode()` / `EncodeOptions::default()` stay LC 128 kbps |
| input rate | 16 / 22.05 / 24 / 32 / 44.1 / 48 kHz (half must be an ADTS rate), else `AacError::Unsupported(UnsupportedFeature::EncodeHeRate(rate))` |
| channels | 1 or 2 (`AacError::Encode` otherwise, as LC) |
| `bitrate_bps` | whole-stream budget (core + SBR); must fit `6144 · channels · core_rate / 1024` (`AacError::Encode`) |
| ADTS | LC profile, `sampling_frequency_index` = core rate, SBR in `EXT_SBR_DATA` FIL (implicit); `probe` reports LC / core rate, `decode` reports 2× |
| M4A | `esds` explicit two-rate AOT 5 ASC; `mdhd`/`mvhd` timescale = output rate; `stts` 2048 per AU; `elst.media_time` = 3018, duration = N |
| push `Encoder` | ADTS or raw AUs (M4A rejected as for LC); `asc()` = 4-byte HE ASC; `EncodedFrame::samples` attributes ≤ 2048 source samples per AU; `EncodeInfo` priming 3018 / remainder / `coded_samples = aus · 2048` at the output rate; `reset` restarts |
| raw AUs | `wrap_adts_au(au, core_rate, ch)` reproduces ADTS; `mux_raw_he_m4a(aus, output_rate, ch, N)` reproduces one-shot M4A |
| lookahead / ATH / tonality … | `with_lookahead` honoured (one core frame); the LC psy flags apply to the core |

## Evidence (`cargo test --lib encode_he_tests`, 6 tests)

- ASC: `write_he(6, 3, 1)` = `2b 09 88 00`, the independently authored
  ISO Amd 2 conformance vector (`asc-explicit-sbr-two-rate-24-48`);
  stereo `2b 11 88 00`; the in-tree parser reads AOT 2 / 24 000 /
  48 000 / 2 ch / SBR present.
- Push ADTS (1000-sample chunks) == one-shot ADTS == raw AUs wrapped at
  the core rate; raw AUs muxed == one-shot M4A; lookahead parity holds;
  every source sample is attributed exactly once.
- M4A: `probe` → HeAac, core 24 000 / output 48 000; `elst` start 3018,
  presentation N; decode presents exactly N samples at 48 kHz and equals
  the ADTS decode shifted by 3018.
- libavcodec: `he48e.adts` → `aac (HE-AAC), 48000 Hz, stereo`; `he48em.m4a`
  → `Duration: 00:00:00.60` (= N = 28 800 samples exactly), `aac (HE-AAC)
  (mp4a), 48000 Hz, stereo`; our decodes match lavc within ≤ 2 LSB s16 /
  ≥ 55 dB per channel (ADTS full length; M4A over the first N samples —
  ffmpeg applies the edit list and dumps 29 750 samples, coded 32 768 −
  priming 3018).
- Public one-shot bytes == the TASK-89 internal golden (`he48e.adts`).
- Typed errors: 96 kHz / 8 kHz input, 150 kbps mono (core cap 144 kbps),
  3 channels, one-shot Raw, push M4A.
- LC: `encode` default unchanged; all LC goldens byte-exact.

## Not claimed

- No LATM HE output (TASK-94), no HE v2 / PS (91–93), no 5.1 HE.
- Quality vs FDK / Apple / oxideav (TASK-95), no listening (TASK-109);
  the lab encode matrix still marks the HE cell unavailable until
  TASK-95 wires the candidate.
- `EncodeOptions::he` with `EncodeContainer::Raw` in one-shot stays
  rejected like LC (per-AU sizes).
- No aarch64 run this session (CI golden tripwire covers `he48e` /
  `he48em` once the matrix picks them up: they are ordinary `#[test]`s).
