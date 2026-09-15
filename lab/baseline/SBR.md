# TASK-79 SBR conversion/reconstruction workspace

Parent: `44e2c10` (TASK-78). Probe: counting `GlobalAlloc` with live/peak,
thin LTO, codegen-units=1 (same as ALLOC.md / RESET.md). After 2-frame
warmup, remaining HE ADTS frames fed one-at-a-time with a discard
callback. rustc 1.97.1. Fixture: `src/goldens/he48.adts`.

## Parent (before)

| path | allocs | notes |
|---|---|---|
| HE whole unbounded | 1764 / 2.12 MiB / peak_live **451273** | TASK-15 he_adts was 1889 / 2.3 MiB |
| HE steady unbounded | 7 frames / 1154 / **164.86**/frame | after warm=2 |
| HE whole speech | 1474 | |
| PS whole unbounded | 10598 | left to TASK-80 |
| LC speech whole | 33 | unchanged |

Sites: `sbr_attach` f32→f64 `Vec` every plane; `process_frame`/`upsample`
new `Vec<Vec<f64>>` then f64→f32; `analyze` `x_low`; `generate_hf`
`x_high`; `adjust` nested maps + `y`; `x_cols`; synth pcm; envelope
parse/dequant `Vec`s. QMF algorithm unchanged.

## After

| path | allocs | vs parent |
|---|---|---|
| HE whole unbounded | 480 / 0.34 MiB / peak_live **313425** | −73% allocs, **−30.5% peak** |
| HE steady unbounded | 101 / **14.43**/frame | **−91.2%** (gate ≥90%) |
| HE whole speech | 196 | |
| PS whole unbounded | 6973 | remainder is TASK-80 |
| LC speech whole | 33 | unchanged |

Zero recurring conversion buffers after prepare: core f32 workspace,
in-place planar 1024→2048, recycled `SbrExtensionData` box, stack
XLow/XHigh/env-adjust maps, interned QMF `n_mat`/`m` (spec tables, not
a session cache). Remaining ~14 allocs/frame are PS hybrid/decorr
matrices and a few parse tails (TASK-80).

## Decision

| Lane | Decision |
|---|---|
| Reuse SBR conversion/reconstruction workspace | **go** |
| ≥90% fewer HE steady allocs | **go** (164.86 → 14.43) |
| ≥20% lower peak live heap | **go** (451273 → 313425, −30.5%) |
| Goldens / `speech()` default | unchanged |
