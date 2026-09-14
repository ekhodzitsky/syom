# TASK-77 prepared workspace reuse

Parent: `6734f6d` (TASK-61). Host: [PIN.md](PIN.md). Probe: counting
`GlobalAlloc` in an isolated path-dep binary (not `cargo test`; not
Criterion). rustc 1.97.1, `profile.release` thin LTO, codegen-units=1.

Named TASK-15 cells at this parent (this probe, discard-output push):

| cell | parent | TASK-15 REPORT |
|---|---|---|
| startup `Decoder::new` | 290 ns median / 0 allocs | 100 ns (20 reps) |
| LC session allocs | 79 / 76 KiB | stream 363 (collects PCM) |
| retained | — | 37–40 KiB |
| Encoder::new (attributed setup) | 19.3 µs / 8 allocs / 82 KiB | (not a named wall cell) |

`Decoder::new` is empty (Filterbank on the stack). The encoder setup
hotspot is `LcEncoder::new`: KBD `window` Box, `ShortWindows` (3 boxes),
two `Psy` 51×51 spreading matrices, quant boxes. Parent `Encoder::reset`
called `new_lc` and paid that cost every session. Parent `Decoder::reset`
reconstructed `StreamDecoder`; the next session reallocated the same 79
LC (1605 HE, 1167 5.1) heap blocks.

## After in-place reset

| phase | parent median | after median | parent allocs | after allocs |
|---|---|---|---|---|
| Decoder::new | 290 ns | 210 ns | 0 | 0 |
| LC reset() | 250 ns | 110 ns | 0 | 0 |
| LC 2nd session | 109 µs | 105 µs | 79 / 76 KiB | 66 / 62 KiB |
| HE 2nd session | — | — | 1605 / 2.09 MiB | 1587 / 2.07 MiB |
| MC 2nd session | — | — | 1167 / 851 KiB | 1131 / 627 KiB |
| Encoder::new | 19.3 µs | 19.3 µs | 8 / 82 KiB | 8 / 82 KiB |
| ENC reset() | 18.8 µs | **0.37 µs** | **8 / 82 KiB** | **0 / 0** |
| ENC 2nd session | 237 µs | 243 µs | 64 / 22 KiB | 64 / 22 KiB |

Encoder reset time **−98%** (≥20%) and setup allocs **−100%** (≥50%).
AC#2 **go**. LC 2nd-session allocs −16% (pcm/spec `Vec` capacity kept;
remainder is per-frame — TASK-78). HE 2nd session still rebuilds SBR/PS
instances (`SbrPool::reset` drops slots; TASK-79). No global cache:
two `Decoder`/`Encoder` values are independent.

Protected cells: LC discard session after ≈104 µs matches TASK-15
`lc_adts_discard` 104 µs. `Encoder::new` 19.3 µs unchanged. Encoder
goldens byte-exact (existing `encode_tests`; reset path equals
`encode_with`). `speech()` default unchanged.

## Decision

| Lane | Decision |
|---|---|
| Ship in-place `Decoder::reset` / `Encoder::reset` | **go** |
| Flip any public default | **no-go** |
| Claim LC 2nd-session zero-alloc | **no-go** (TASK-78) |
| Reuse SBR/PS instances across reset | **no-go** here (TASK-79) |
