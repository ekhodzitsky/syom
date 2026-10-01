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

## Rate-loop reuse (2026-09-29)

Same machine and pin (`taskset -c 4`). Harness `syom_speed`, 5 warm-up
runs and 7 measured reps per cell — shorter than the 63-rep protocol
above, so the intervals are wider. Baseline is the tree immediately
before this change; candidate is the tree that contains it.
`speed_ci.py`, 10 000 resamples, seed 83.

What changed, without moving a bit on the locked signals:

1. Scalefactor gain is a 256-entry table of `det_math::exp2`. The
   exponent is a multiple of 1/16, so the table matches that function
   bit for bit (`quant_gain_table_matches_det_math_for_every_scalefactor`).
2. Long-window band peaks are the allocation cache, measured once on
   the spectrum the quantizer sees.
3. An all-zero band returns a closed-form Huffman cost, and a band
   whose peak fits a smaller book does not walk the larger books. The
   one-pass table still matches `spectral_bits`.
4. The ABR search starts at the previous frame's noise offset and
   walks at most eight steps, then bisects the side that remains.
   Bit counts dip by a few bits, so an arbitrary start can stop at a
   different crossing than a cold bisection. The encoder feeds only
   the previous frame's result (the first frame starts at the low
   bound, which is the cold search). That path matches a cold
   bisection on the encoder goldens and on the low-rate noise
   trajectory tests.

| cell | base ms | cand ms | speedup | 95% CI of ratio | bytes/hash |
|---|---:|---:|---:|---|---|
| lecture-st-lc128 | 5.89 | 4.71 | +20.0% | [+18.8%, +23.3%] | identical |
| lecture-st-lc64 | 6.94 | 5.45 | +21.4% | [+15.8%, +28.2%] | identical |
| lecture-st-q5 | 5.73 | 4.55 | +20.6% | [+15.8%, +27.9%] | identical |
| lecture-mono-lc128 | 3.28 | 2.64 | +19.4% | [+14.0%, +29.7%] | identical |
| lecture-mono-lc64 | 4.00 | 3.02 | +24.5% | [+4.9%, +28.0%] | identical |
| lecture-mono-q5 | 3.15 | 2.38 | +24.6% | [+10.4%, +34.0%] | identical |
| music-st-lc128 | 38.43 | 20.28 | +47.2% | [+45.7%, +49.8%] | identical |
| music-st-lc64 | 32.95 | 14.08 | +57.3% | [+56.9%, +59.2%] | identical |
| music-st-q5 | 20.41 | 16.73 | +18.1% | [+14.9%, +23.1%] | identical |
| noise-st-lc128 | 40.97 | 23.16 | +43.5% | [+38.5%, +44.8%] | identical |
| noise-st-lc64 | 32.78 | 15.04 | +54.1% | [+50.7%, +54.7%] | identical |
| noise-st-q5 | 17.01 | 14.98 | +11.9% | [+6.8%, +16.1%] | identical |
| click-mono-lc128 | 9.50 | 9.66 | -1.7% | [-12.2%, +9.0%] | identical |
| click-mono-lc64 | 8.17 | 7.68 | +6.1% | [+3.1%, +17.2%] | identical |
| click-mono-q5 | 4.16 | 3.75 | +9.8% | [+9.2%, +19.4%] | identical |

median gain over cells: +20.6% (15 cells)

Every cell is byte-identical (size + FNV-1a), and the committed encoder
goldens (`enc48*`, `he48e*`, `he2_48e*`, `enc_mc*`, lookahead) still
match. Busy ABR (music and noise at 64 and 128 kbps) is +43% to +57%:
the search settles and stops re-quantizing. Lecture and quality VBR
rarely leave one offset, so their +18% to +25% is the cheaper
quantizer. `click-mono-lc128` is −1.7% and its interval includes zero
(seven reps; the offset jumps on every attack, so the warm start has
little to reuse).

Debug `cargo test` does not assert a wall clock. It counts quantizer
builds: a steady tone at most four per frame after the first, 64 kbps
noise under eight, and the payload scratch stays inside the ADTS
ceiling. The gain table is 256 `f32` values for the life of the
process. The 128 KiB stack test still passes.

The 63-rep table above is the TASK-83 result and is not this baseline.
Its absolute milliseconds are from an older encoder.
