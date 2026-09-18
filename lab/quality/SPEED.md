# TASK-83 — LC rate/quantization hot path

Machine: AMD Ryzen AI 9 HX 370, Linux, `--release`, pinned with
`taskset -c 4`. Harness: `syom_speed` (5 warm + 21 reps per round, three
interleaved rounds of baseline and candidate, 63 reps per cell);
comparison: `speed_ci.py` (bootstrap 95% CI of the ratio of medians,
10 000 resamples). `perf` is unavailable here (`perf_event_paranoid = 4`),
so attribution came from temporary `Instant` stage timers that were not
committed.

## Profile after TASK-113 (baseline, 2 s stereo "music", LC 128k)

| stage | share of encode |
|---|---:|
| rate control (all `build` calls) | 89% |
| of which `quantize` | 65–70% |
| of which Huffman cost table (`fill_bits`) | about 44% |
| `plan_books` | 10% |
| MDCT + psy + TNS + M/S | about 10% |

The ABR search runs about 11 `build` calls per frame (2 bounds + 9
bisection steps over offsets −60..240). Every call recomputed
`|x|^0.75` for all 1024 lines although the spectrum is fixed for the
frame, and walked every band once per book (11 walks, each recomputing
the tuple ordinal and the range check).

## Change

1. `|x|^0.75` is cached once per frame per channel (`cache_mags`, long
   and short paths); `quantize_cached` only multiplies by the band gain.
2. `fill_bits` is one pass: the band's largest magnitude selects the
   representable books, and book pairs sharing a tuple ordinal (1–2, 3–4,
   5–6, 7–8, 9–10) are costed from one index. A test pins equality with
   the per-book `spectral_bits` reference for every magnitude class.

No heuristic, threshold, search bound or bitstream decision changed.

## Result (candidate vs baseline, same harness binary source)

| cell | base ms | cand ms | speedup | 95% CI of ratio | bytes/hash |
|---|---:|---:|---:|---|---|
| lecture-st-lc128 | 3.77 | 3.83 | -1.6% | [-1.9%, -1.3%] | identical |
| lecture-st-lc64 | 4.54 | 4.45 | +2.1% | [+1.9%, +2.3%] | identical |
| lecture-st-q5 | 3.79 | 3.77 | +0.4% | [+0.2%, +0.7%] | identical |
| lecture-mono-lc128 | 2.16 | 2.13 | +1.4% | [+1.1%, +1.6%] | identical |
| lecture-mono-lc64 | 2.80 | 2.62 | +6.3% | [+6.2%, +6.5%] | identical |
| lecture-mono-q5 | 2.18 | 2.08 | +4.6% | [+4.5%, +4.8%] | identical |
| music-st-lc128 | 44.39 | 26.63 | +40.0% | [+39.8%, +40.3%] | identical |
| music-st-lc64 | 37.08 | 23.64 | +36.2% | [+36.0%, +36.4%] | identical |
| music-st-q5 | 20.28 | 14.26 | +29.7% | [+29.6%, +29.8%] | identical |
| noise-st-lc128 | 51.28 | 29.58 | +42.3% | [+42.2%, +42.4%] | identical |
| noise-st-lc64 | 35.99 | 22.14 | +38.5% | [+38.3%, +38.7%] | identical |
| noise-st-q5 | 18.50 | 12.67 | +31.5% | [+31.2%, +31.7%] | identical |
| click-mono-lc128 | 11.04 | 7.37 | +33.2% | [+33.0%, +33.5%] | identical |
| click-mono-lc64 | 12.09 | 7.50 | +38.0% | [+37.7%, +38.2%] | identical |
| click-mono-q5 | 3.81 | 3.00 | +21.3% | [+21.1%, +21.9%] | identical |

median gain over cells: +29.7% (15 cells)

Every cell's elementary stream is byte-identical (size + FNV-1a), so
requested/achieved rate, quality and delay are unchanged by construction;
the byte-exact encoder goldens in `cargo test` pass unchanged.

- Affected busy material (music, noise, clicks; mono and stereo): +21% to
  +42%, all intervals exclude zero. Target ≥ 10%: met.
- Near-silent natural speech (`lecture`, MDCT-bound, few coded bands):
  −1.6% to +6.3%; the worst cell stays inside the 3% guard.
- Work per frame stays bounded: the same number of `build` calls, each
  strictly cheaper. Workspace: +4 KiB per quantizer channel (16 KiB per
  encoder), no new allocations.

Limitation: the only natural clip in the tree is the 0.25 s lecture golden
(tiled to 2 s and normalized to 0.5 peak); "music" is a deterministic
synthetic (harmonics with vibrato over a noise floor).

Reproduce: `cd lab/quality && cargo run --release --bin syom_speed -- 21 > new.txt`,
the same on the baseline checkout, then `python3 speed_ci.py base.txt new.txt`.
