# Encoder quality baseline pin (TASK-16)

Isolated from the `syom` package. **Not invoked by `cargo test`.**
TASK-68 ATH A/B is `syom_ath` (`make ath` / `ATH.md`). TASK-69 tonality
A/B is `syom_tonality` (`make tonality` / `TONALITY.md`). TASK-70 pre-echo
sweep is ordinary `encode_preecho_tests` plus `PREECHO.md`. TASK-72 short
TNS is `encode_short_tns_tests` plus `SHORT_TNS.md`. Same isolation.

## Independent decoder

FFmpeg CLI (`pcm_f32le`) for every coded ADTS file. This is the common
PCM lane so FDK-incompatible bitstreams can still score when those
encoders are present.

Pinned in-process native encoder: `lab/libavcodec/avc_driver` FFmpeg **9.0.1**
native `aac`. Host CLI ffmpeg may be a different build (recorded in REPORT).

## Encoders in the same-clip matrix

| Engine | How | Notes |
|---|---|---|
| syom LC | in-process `encode_with` | product crate, 64/128 kbps |
| FFmpeg 9.0.1 native aac | `avc_driver encode-lc-adts` | pinned TASK-6 adapter |
| ffmpeg CLI aac | process spawn | host build; not the 9.0.1 pin |
| glint-audio 0.11.0 | `glint_smoke encode-pcm` quality=1 | i16 interleaved |

## Not same-clip (do not rank with the matrix)

- FAAC 1.31.1 / FDK 2.0.3: prefixes not installed on this host; TASK-9/7
  sine-only smokes used internally generated PCM, not these clips.
- Apple AAC: no AudioToolbox host (TASK-10).
- HE encode: none of the matrix engines emit SBR/PS here.

## Corpus

Synth **dev** clips only (tonal / noise / transient / stereo). The
held-out qualification split is **not** used.
