# TASK-72 short-window TNS ablation

Recorded 2026-09-14. Decoder: syom `decode_with` unbounded. Grouping is
still 8 groups of 1 (TASK-71 not landed). Long-window TNS unchanged.

## Syntax (independent of encode)

`EncTnsShort` emit is parsed by shipped `tns::TnsData::parse` (1-bit
`n_filt`, 4-bit length, 3-bit order, max order 7). Analysis filter
inverts through `tns::apply` (err < 1e-4 of signal energy). Tests:
`enc_tns_short_tests`.

## Quality (named click, pos 700 of frame 5 — short-windowed)

Predeclared gate: ≥3 dB transient-error vs TNS-off at matched rate.

| | bytes | click-region error | Δ |
|---|---:|---:|---:|
| short_tns off (default) | 685 | 0.0005 | — |
| short_tns on | 689 | 0.0003 | **+2.3 dB** |

Side-information: +4 bytes on this clip. Prediction-gain gate is the
same 2 dB (`PG_MIN`) as long TNS; windows that miss it emit `n_filt=0`.
M/S still runs after TNS (decoder inverts TNS after M/S undo). Steady
sine: short_tns on == default (no EightShort).

## Go / no-go

**No-go as 0.x default** (+2.3 dB < 3 dB; not PEAQ). Keep
`EncodeOptions::with_short_tns` opt-in. Do not enable just because
native encoders expose short TNS. Goldens (`enc48t`) unchanged.

Reproduce: `cargo test --lib encode_short_tns -- --nocapture`.
