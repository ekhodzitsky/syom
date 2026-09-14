# TASK-14 go / no-go

Recorded 2026-09-14. Host: Linux x86_64. Procedure:
`python3 scripts/listen_protocol.py dry-run --seed task-14`.
**This file is not listening evidence.**

## Dry-run (SYNTHETIC scores)

| Check | Result |
|---|---|
| Holdout recordings | 8/8 TASK-2 identities |
| Method split | 7× MUSHRA 64 kbps speech/transient; 1× BS.1116 128 kbps Chopin |
| Packet filenames | `itemNN/cKK.meta.json` — no codec tokens |
| Duration pad | raw lengths differ; `n_samples` identical per trial (576320 @ 48 kHz on item 0) |
| UI vs file order | item0 `[1,0,3,2,4]` ≠ `0..4` |
| Screening | L00 excluded (hidden-ref fail frac 1.0); L01–L08 valid |
| Planted (syom − lavc) | mean −1.135, sd 0.719, 95% CI [−1.737, −0.534] |
| Noninferior vs margin 3 | true on **synthetic** grades |
| Superior | false |
| Qualification n (σ=10) | 69 |
| Exploratory n | 20 |
| Session burden | 17.5 min ≤ 45 |

Injecting `syom` into a packet filename fails `verify`. Stripping the
SYNTHETIC label from scores fails `verify`.

The planted CI is a **procedure check** (analysis recovers a known
slightly-worse-but-within-margin difference and drops an inattentive
listener). It is **not** a quality claim.

## Missing requirements

| Gap | Status |
|---|---|
| Human participants | none; TASK-109 needs consented experts |
| Holdout natural bytes | all 8 `gap-until-obtained` (TASK-2) |
| Apple AAC host | no AudioToolbox (TASK-10) |
| FDK / FAAC / glint same-PCM | not in TASK-16 matched-rate matrix on this host |
| Encoder defaults frozen | TASK-108 still To Do |
| PEAQ / ViSQOL | unavailable (TASK-13); listening does not require them |
| Dense-music holdout | coverage gap; one tonal-music item only |
| HE encode cells | no encoder (TASK-90/93) |

## Go / no-go

| Lane | Decision |
|---|---|
| Protocol (method, anchors, level/trim, randomization, inclusion, stopping) | **GO** — locked in `PROTOCOL.md` + `scripts/listen_protocol.py` |
| Dry-run blinding and analysis | **GO** — seeded `task-14` |
| Present synthetic grades as listening evidence | **NO-GO** |
| Execute TASK-109 now | **NO-GO** until humans, holdout bytes, and strongest-peer coverage (Apple or an explicit exclusion) exist |
| Change `speech()` / 128k / encoder defaults | **NO-GO** (not this task) |
