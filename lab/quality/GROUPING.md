# Short-window grouping (TASK-71)

Recorded 2026-09-14. Isolated; ordinary `cargo test` never runs a lab
binary. PEAQ is unavailable — click-region MSE is the predeclared
substitute (same as TASK-72).

## Method

`EncodeOptions::with_short_group` (default **off**). Consecutive short
windows merge when max energy ≤ 4× min (≈ 6 dB) or both sit below
`1e-6`. CPE uses the per-window max across channels (`common_window`).
Independent parse-back: `enc_group_tests` (all 128 ICS bit patterns vs
`ics::grouping_of`) and `paired_grouping_parses_back_through_shipped_ics`
(4×2 emit → `IcsInfo` / `parse_quant_into`).

Fixture: TASK-72 click (14 frames, burst at 5×1024+700), 48 kHz mono,
128 kbps ADTS, rustc 1.97.1 Linux x86_64.

## Click

| | syntax bits (grouping+section+sf) | ADTS bytes | click-region Δ |
|---|---:|---:|---|
| 8×1 (default) | 281 | 685 | — |
| grouped | 229 | 679 | 0.0 dB |
| saving | **18.5%** | 0.9% | |

Short frames on this clip grouped as **4** windows-merged groups (not 8×1).

## Go / no-go

| Lane | Decision |
|---|---|
| Syntax overhead ≥ 5% (AC2) | **GO** (18.5% on this click) |
| Click-region quality | 0.0 dB — not a quality win |
| Default 0.x | **NO-GO** — keep 8×1 so `enc48t` goldens and 8-group MS tests stay; opt-in `with_short_group` |
| CPU | grouping is 8 energy sums, no extra MDCT (under 3%) |
| Long / transition overlap | unchanged (grouping is EightShort-only) |

PEAQ ODG was not measured (unavailable). Do not treat 18.5% syntax
savings as a total-rate or listening win.
