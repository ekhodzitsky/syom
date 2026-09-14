# TASK-15 matched-output LC / HE / PS baseline

Recorded 2026-09-14. Host and protocol: [PIN.md](PIN.md).
Commands:

```
taskset -c 0 cargo bench --bench baseline
taskset -c 0 cargo bench --bench mem_iso
```

`perf record` failed (`perf_event_paranoid=4`). CPU sites below are
**cell subtraction + phase split**, not gprof.

## Preflight

Equal-output planar_split (syom 0.6.0 reference shape):

| fixture | syom | rusty_aac 0.5.0 | oxideav-aac 0.1.7 | symphonia 0.6.1 |
|---|---|---|---|---|
| lc_adts | 48 kHz 1 ch 13312 | comparable | comparable | comparable |
| lc_m4a | 44.1 kHz 1 ch 11264 | no ISOBMFF | no ISOBMFF | 12288 samples (non-comparable) |
| he_adts | 48 kHz 2 ch 18432 | 24 kHz 1 ch 9216 | 48 kHz 1 ch 18432 | 24 kHz 1 ch 9216 |
| ps_adts | 48 kHz 2 ch 53248 | core-only | comparable | core-only |
| mc_adts | 48 kHz 6 ch 20480 | (not enrolled) | comparable | decoder not constructed |
| lecture_m4a | 48 kHz 2 ch 12288 | no ISOBMFF | no ISOBMFF | no packets |

Failed candidates would abort; none failed.

## Wall (median ns, 20 reps)

| group | peer | median_ns | p95_ns | bootstrap 95% median |
|---|---|---|---|---|
| lc_adts | syom | 105532 | 119494 | 105237–106394 |
| lc_adts | symphonia | 131101 | 157555 | 127470–142712 |
| lc_adts | rusty_aac | 178658725 | 183419470 | 177669600–180929227 |
| lc_adts | oxideav-aac | 175184482 | 178270027 | 174976392–175947162 |
| lc_m4a | syom | 100518 | 112872 | 98068–106068 |
| he_adts | syom | 1529908 | 1657748 | 1519293–1537032 |
| he_m4a | syom | 1547456 | 2022552 | 1534432–1564534 |
| ps_adts | syom | 7961682 | 8176650 | 7913947–8023908 |
| ps_adts | oxideav-aac | 371926849 | 379247163 | 371001805–375027878 |
| mc_adts | syom | 940243 | 1033638 | 938690–946360 |
| mc_adts | oxideav-aac | 1684913218 | 1701567806 | 1682902774–1687713679 |
| lecture_m4a | syom | 140399 | 148678 | 140223–142202 |
| lc_adts_speech | syom | 104536 | 108834 | 104050–104756 |
| lc_adts_discard | syom | 104156 | 110276 | 103750–104722 |
| enc_lc_mono | syom | 418880 | 481263 | 406587–431424 |
| enc_lc_stereo | syom | 868974 | 951604 | 861822–885720 |

Encode sine is **non-matched-rate** (43 kbps / 65 kbps vs 128k request).
Raw ns: `benches/baseline.rs` stdout in the session log.

## Latency (syom ADTS push, sine48)

| phase | median_ns | p95_ns | bootstrap 95% median |
|---|---|---|---|
| startup `Decoder::new` | 100 | 110 | 100–100 |
| first_output | 8536 | 9367 | 8506–8776 |
| steady remaining feeds | 97438 | 102362 | 96942–99472 |
| finish | 246 | 310 | 226–266 |

Codec ADTS vs container M4A (syom LC): 106 µs vs 101 µs. Cold new is
negligible vs first-output.

## Isolated memory (`mem_iso`, one OS process, forward=reverse)

| case / phase | peak_live | allocs | alloc_bytes | retained |
|---|---|---|---|---|
| lc_adts one_shot | 120737 | 363 | 210102 | 37417 |
| lc_adts stream/8 | 68503 | 363 | 158426 | 37413 |
| he_adts one_shot | 589852 | 1889 | 2305514 | 38953 |
| he_adts stream/8 | 442714 | 1889 | 2158690 | 38949 |
| ps_adts one_shot | 1267061 | 11056 | 14639523 | 40489 |
| ps_adts stream/8 | 841589 | 11055 | 14214358 | 40485 |
| mc_adts one_shot | 703391 | 1483 | 1421083 | 40489 |
| mc_adts stream/8 | 216782 | 1478 | 934402 | 40485 |

Streaming drops collected PCM (`peak_live`) but **alloc count stays**.
Retained ~37–40 KiB independent of LC vs HE vs PS duration of these goldens.

## CPU / alloc sites (no `perf`)

Measured cost rank (syom median wall):

1. PS decode 7.96 ms (~75× LC) and 11k allocs / 14 MiB — PS + SBR
2. HE decode 1.53 ms (~14× LC) and 1.9k allocs / 2.3 MiB — SBR
3. MC 0.94 ms / 1.5k allocs — 6-ch LC
4. LC encode stereo 0.87 ms / mono 0.42 ms — rate loop + MDCT
5. LC decode 0.106 ms / 363 allocs — core LC; discard 0.104 ms so
   collecting PCM is not the LC wall

Source (F21), not sampled stacks: SBR owned f32→f64 and nested output;
QMF scalar; no x86 SIMD (NEON-only transforms).

## Optimization cells (go/no-go)

Protected regression set for all: lc_adts / he_adts / ps_adts medians
above, plus encoder goldens and `speech()` default. Gate: ≥5% median
with CI excluding 0, or ≥20% peak_live / ≥50% allocs; no >3% median
or >5% p95 elsewhere.

| Task | Baseline cell | Rank | Decision |
|---|---|---|---|
| TASK-80 PS workspace | ps_adts allocs 11056 / 8.0 ms | 1 | **go** |
| TASK-79 SBR buffers | he_adts allocs 1889 / 1.53 ms | 2 | **go** |
| TASK-81 QMF | he_adts+ps_adts wall | 3 | **go** (function profile blocked) |
| TASK-78 LC steady allocs | lc_adts stream 363 allocs | 4 | **go** (DOC-3 zero-alloc target; not the LC wall) |
| TASK-82 x86 SIMD | lc_adts 106 µs, this host avx2 | 5 | **go** |
| TASK-83 rate/quant | enc_lc_mono 419 µs | 6 | **go** |
| TASK-77 workspace reuse | Encoder::new 8 allocs / 82 KiB / 19 µs | 7 | **go** (reset 0 allocs / 0.37 µs, −98%; see RESET.md) |
| TASK-84 first-output | 8.5 µs vs 106 µs total | 8 | **no-go** as a speed gate (already small) |

## Missing

- aarch64 wall/memory
- in-process lavc/FDK timings (process-launch drivers are a different lane)
- Apple
- `perf` stacks
- Equal-rate encode wall (sine is content-limited)

## Go / no-go for this spike

| Lane | Decision |
|---|---|
| Replace historical unequal-work decode leaderboard | **go** (this table) |
| Claim “faster than all peers” | **no-go** (qualification is TASK-110; rusty/oxideav 175 ms needs a collect-path check) |
| Start TASK-79/80/81/82/83 from these cells | **go** |
| Start TASK-84 for throughput | **no-go** |
