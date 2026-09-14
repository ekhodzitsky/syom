# Section-partition DP (TASK-73)

Recorded 2026-09-14. Isolated notes; ordinary `cargo test` runs the
oracle/greedy comparison in `enc_section_tests`, not a lab binary.

## Baseline

Production `plan_books_into` (and short `plan_books_short`) assigns the
cheapest book per coded band, then greedily merges adjacent sections
when saved headers outweigh extra spectral bits. ZERO_HCB stays on
uncoded bands.

## Exact planner

`plan_books_dp` (`enc_section_dp.rs`) is DP on each maximal coded
stretch: `dp[i] = min_j dp[j] + header(i−j) + best_book_spectral(j..i)`.
Worst-case work O(L² · 11) with L ≤ 51 (long) or ≤ 15 (short group).

Independent oracle in tests (same recurrence, separate code) matches DP
on n = 8 LCG tables for long (5-bit) and short (3-bit) headers. DP cost
is never above greedy. Escape book 11 and ZERO holes parse back through
`SectionData`.

## Measurement

| Set | greedy section+spectral | DP | saving |
|---|---:|---:|---:|
| 64 LCG 49-band channels | 36504 | 36371 | **0.36%** |
| `sample_channel(49)` | 3342 | 3342 | 0% |

Gate was ≥ 2% on affected development frames. Enabling DP as the
production planner **changed committed encode goldens** (Huffman
partition only) without a 2% bit win.

## Go / no-go

| Lane | Decision |
|---|---|
| Exact DP correctness vs oracle | **GO** |
| DP ≤ greedy | **GO** (asserted) |
| ≥ 2% section+spectral bits | **NO-GO** (0.36%) |
| Production default | **greedy retained**; goldens byte-identical |
| CPU | DP is off the encode path (0% encoder regression) |

Do not treat 0.36% as a rate or quality win. TASK-108 may still pick DP
if a later corpus shows ≥ 2%.
