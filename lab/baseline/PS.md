# TASK-80 PS analysis / decorrelation / output workspace

Parent: `9bccde9` (TASK-79). Probe: counting `GlobalAlloc` with live/peak,
thin LTO, codegen-units=1. After 2-frame warmup, remaining PS ADTS
frames fed one-at-a-time with a discard callback. rustc 1.97.1.
Fixture: `src/goldens/ps48.adts` (real stereo HE-AACv2).

## Parent (before)

| path | allocs | notes |
|---|---|---|
| PS whole unbounded | 7257 / 7.06 MiB / peak_live **568293** | isolated process, first decode |
| PS steady unbounded | 24 frames / 6210 / **258.75**/frame | after warm=2 |
| PS whole speech | 6975 | |
| HE whole unbounded | 196 | interned QMF tables already live |
| LC speech whole | 33 | unchanged |

Sites: `PsHybrid::analyze` 32×`nb` `Vec<Complex>`; `PsDecorr::process`
same + per-slot `p`/`g_ratio`; `PsStereo` `h_slots` + hybrid L/R;
`synthesize` new QMF `Vec`s; `PsData::parse` / `resolve` / `map_indices`.
PS audio algorithm unchanged.

## After

| path | allocs | vs parent |
|---|---|---|
| PS whole unbounded | 1241 / 0.47 MiB / peak_live **429671** | −83% allocs, **−24.4% peak** |
| PS steady unbounded | 594 / **24.75**/frame | **−90.4%** (gate ≥90%) |
| PS whole speech | 959 | |
| HE whole unbounded | 196 | |
| LC speech whole | 33 | unchanged |

Zero recurring DSP scratch after prepare: fitted hybrid/decorr rows,
stack H-slot matrix (`32×34×H4`) and stack QMF planes, mix-to-QMF
without hybrid L/R matrices. Remaining **~25 allocs/frame** are
`ps_data` parse/resolve/`map_indices` (bitstream reconstruction, not
DSP scratch). Named follow-up if a later task wants parse-into.

## Decision

| Lane | Decision |
|---|---|
| Reuse PS hybrid/decorr/QMF workspace | **go** |
| ≥90% fewer PS steady allocs | **go** (258.75 → 24.75) |
| ≥20% lower peak live heap | **go** (568293 → 429671, −24.4%) |
| Goldens / `speech()` default | unchanged |
