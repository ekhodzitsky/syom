# TASK-15 matched-output baseline pin

Not invoked by `cargo test --workspace`. Harness: `cargo bench --bench baseline`
and `cargo bench --bench mem_iso`. Preflight is
`syom::decode_cmp::run_preflight` (same function as `benches/aac.rs`).

## Host (2026-09-14)

- OS: Linux 7.0.0-31-generic x86_64
- CPU: AMD Ryzen AI 9 HX 370, 24 threads, governor=performance
- ISA: sse2, avx, avx2, fma (no NEON on this host)
- rustc 1.97.1 (`8bab26f4f`), `profile.bench` thin LTO, codegen-units=1
- Placement: `taskset -c 0`, single-thread (no Rayon)
- Reps: 4 warmup discarded + 20 measured; median, p95, bootstrap 95% CI
  of the median (1000 resamples, LCG seed `0x9e3779b9`); all raw ns published
- Outliers: none discarded

## Lanes

Primary: planar split, native rate. `lc_adts_speech` and `lc_adts_discard`
are named. Encode cells that miss ±1% bitrate are **non-matched-rate**,
still timed but not equal-rate quality cells.

## Missing / blocked

- Function-level `perf` profile: `perf_event_paranoid=4`
- aarch64 native cell
- In-process lavc/FDK wall (lab adapters exist; not linked into this bench)
- Apple AudioToolbox
