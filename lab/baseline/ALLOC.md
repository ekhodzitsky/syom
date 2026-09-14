# TASK-78 LC borrowed-output allocations

Parent: `befb089` (TASK-77). Probe: counting `GlobalAlloc`, thin LTO,
codegen-units=1 (same as TASK-77 RESET.md). After 2-frame warmup, remaining
ADTS frames fed one-at-a-time with a discard callback (caller PCM not
collected). rustc 1.97.1.

## Parent (before)

| path | steady frames | allocs | per frame |
|---|---|---|---|
| LC speech mono | 11 | 33 | 3.00 |
| LC split mono | 11 | 138 | 12.55 |
| LC stereo | 28 | 271 | 9.68 |
| LC 5.1 | 18 | 1033 | 57.39 |
| encode LC mono | 18 | 234 | 13.00 |

Sites: `Vec<&[f32]>` per split emit; `mem::take(spec)` drop/realloc;
`pending` take; `ids` Vec; `map_planes`/`reorder` Vec; TNS/MS/PNS
`rand_vec`; encoder `BitWriter`/`encode_frame` Vec.

## After

| path | allocs | per frame | vs parent |
|---|---|---|---|
| LC speech mono | **0** | **0** | −100% |
| LC stereo split | 1 / 28 | **0.04** | −99.6% |
| LC 5.1 | 27 | 1.50 | −97% (was PNS `rand_vec`) |
| LC split mono | 94 | 8.55 | −32% |
| encode LC mono | 126 | 7.00 | −46% |

Owned one-shot `decode`/`encode` still allocate collected PCM/payload
(not counted). Input-buffer growth when the whole remainder is one
`feed` is one alloc, not per callback.

## Decision

| Lane | Decision |
|---|---|
| Zero-alloc default speech borrowed path | **go** |
| Zero-alloc stereo split borrowed path | **go** (1 leftover / 28 frames) |
| Zero-alloc 5.1 / split-mono / encode | **not yet** (1.5 / 8.55 / 7 per frame) |
| Claim one-shot owned output is zero-alloc | **no-go** |
| Goldens / `speech()` default | unchanged |
