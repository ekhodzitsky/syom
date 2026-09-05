# syom benches

`cargo bench --bench aac -- --quick` then `cargo bench --bench mem`.
Machine: macOS aarch64, `profile.bench` thin LTO, 2026-09-05.
Peers: rusty_aac 0.5.0, symphonia 0.6.1, oxideav-aac 0.1.7 (all
dev-deps; product `[dependencies]` stays empty). C lavc/libfdk are not
linked (`c-peers-unavailable`).

oxideav-aac 0.1.7 is a real decode peer: the published tarball ships
SBR + PS (its crates.io "parser" description is stale). ADTS only — no
ISOBMFF demux — so it joins the ADTS groups. Verdict below.

Sample counts (planar samples, ch0-equivalent), `syom::decode` (`speech()`):

| fixture | syom | rusty_aac | oxideav | symphonia |
|---|---|---|---|---|
| LC ADTS `sine48.adts` | 13312 | 13312 | 13312 | 13312 |
| LC M4A `sine441.m4a` | 11264 (elst 1024) | 0 | n/a (no ISOBMFF) | 12288 (no elst) |
| HE ADTS `he48.adts` | 18432 @ 48 kHz | 9216 core | 18432 @ 48 kHz (SBR) | 9216 core |
| HE M4A `he48.m4a` | 16320 @ 48 kHz | 0 | n/a | 9216 core |
| PS ADTS `ps48.adts` | 53248 @ 48 kHz | 26624 core | 106496 = 2×53248 (SBR+PS, L≠R) | 26624 core |
| 5.1 ADTS `mc51.adts` | 20480 (speech mono) | 122880 (6 ch) | 122880 (6 ch) | 0 (no decode) |

Wall (criterion median):

| group | syom | rusty_aac | oxideav | symphonia |
|---|---|---|---|---|
| lc_adts | 224.78 µs | 135.84 ms | 136.53 ms | 223.18 µs |
| lc_m4a | **212.77 µs** | — | — | 218.52 µs |
| he_adts | **3.046 ms** (SBR+PS) | 89.70 ms (core) | 92.82 ms (SBR) | 211.00 µs (core) |
| he_m4a | **3.038 ms** (SBR+PS) | — | — | 214.81 µs (core) |
| ps_adts | **16.05 ms** (SBR+PS) | 276.90 ms (core) | 292.84 ms (SBR+PS) | 286.82 µs (core) |
| mc_adts | **1.796 ms** | (decodes; ~1.3 s/iter, not wall-benched) | 1.283 s | 3.39 µs (no decode) |

Memory (`cargo bench --bench mem`, 200 iters; allocs/bytes are
cumulative ÷ 200. Peak RSS is process-lifetime; LC ADTS syom is the
first row and the only fair RSS cell):

| group / peer | allocs/iter | alloc bytes/iter | peak RSS |
|---|---|---|---|
| lc_adts syom | 32 | **146 KiB** | **2.59 MiB** |
| lc_adts rusty_aac | 178 | 444 KiB | 2.89 MiB |
| lc_adts oxideav | 2826 | 1.77 MiB | 3.72 MiB |
| lc_adts symphonia | **30** | 156 KiB | 4.37 MiB |
| lc_m4a syom | **36** | **142 KiB** | **4.39 MiB** |
| lc_m4a symphonia | 57 | 160 KiB | 4.63 MiB |
| he_adts syom (SBR) | **1548** | **2.13 MiB** | — |
| he_adts oxideav (SBR) | 2251 | 2.82 MiB | — |
| ps_adts syom (SBR+PS) | **10599** | **13.88 MiB** | — |
| ps_adts oxideav (SBR+PS) | 12614 | 15.85 MiB | — |
| mc_adts syom | **1112** | **1.03 MiB** | — |
| mc_adts rusty_aac | 1315 | 3.83 MiB | — |
| mc_adts oxideav | 10708 | 15.58 MiB | — |

- **LC ADTS wall: statistical tie with symphonia 0.6.** 224.78 vs
  223.18 µs in the full run (0.7% apart, CIs overlap); an isolated
  re-run flipped the order (221.38 vs 222.23 µs). symphonia 0.5 lost
  this cell by ~7%; 0.6 closed the gap. A/B vs the pre-streaming
  refactor commit (96521eb): syom went 218 → 224 µs on this fixture
  (~3% from layering one-shot decode on the streaming core; not yet
  fixed). Everything measured ~25-35% slower than the 2026-09-02 entry
  on this machine today — cross-day numbers are not comparable;
  within-run comparisons are.
- **LC M4A wall:** syom 212.77 µs vs symphonia 218.52 µs. syom honours
  `elst` (11264 samples); symphonia does not (12288).
- **LC allocs:** symphonia 0.6 wins the count narrowly (30 vs syom 32);
  syom wins bytes/iter (146 vs 156 KiB) and the fair peak-RSS row
  (2.59 MiB, first row). Both beat rusty_aac and oxideav by 5-90×.
- **HE completeness: oxideav joins syom.** oxideav-aac 0.1.7 decodes
  `he48.adts` to full 2×-rate PCM (18432 samples) and `ps48.adts` to
  real stereo (106496 interleaved, L≠R verified). syom remains ~30×
  faster on HE ADTS and ~18× faster on PS ADTS, and is the only linked
  peer that does HE from M4A (oxideav has no ISOBMFF demux: probe of
  `sine441.m4a` returns `AdtsSyncNotFound`). rusty_aac and symphonia
  decode the LC core only (symphonia 0.6 newly decodes the HE-ADTS
  core; 0.5 did not decode HE ADTS at all).
- **5.1:** syom decodes `mc51.adts` 714× faster than oxideav; symphonia
  0.6 does not decode it (0 samples — its ADTS reader rejects the 5.1
  config); rusty_aac decodes but at ~1.3 s/iter is not a wall peer.
- **oxideav verdict (evidence):** `StreamDecoder::decode_all` on
  committed goldens — `sine48.adts` 13312 s16 mono 48 kHz ✓,
  `he48.adts` 18432 s16 @ 48 kHz (SBR active) ✓, `ps48.adts` 106496 s16
  stereo with L≠R frames ✓, `mc51.adts` 122880 s16 6 ch ✓,
  `sine441.m4a` → `Err(AdtsSyncNotFound)` (parser/ADTS only).
- **Correctness** on committed fixtures via `syom::decode` /
  `decode_with`: vs lavc native s16, max abs ≤ 1 LSB, SNR ≥ 70 dB,
  peak ≥ 1000 (lecture, 44.1 M4A, ADTS, TNS, PNS, HE ADTS/M4A, PS,
  3.0-5.1 + PCE, LATM).
- **Product `[dependencies]`:** empty. Caps: `speech()` / `unbounded()`.
  `speech()` is 1 plane; `unbounded()` Split is 2 planes (lavc implicit
  HE-AACv2 PS, dual-mono on `he48`). benches call `decode()`
  (`speech()`).
