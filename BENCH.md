# syom benches

`cargo bench --bench aac -- --quick` then `cargo bench --bench mem`.
Machine: macOS aarch64, `profile.bench` thin LTO, 2026-09-05 (rev 2:
one-shot mono fast path; rev-1 numbers, where changed, are in the notes).
Encode section added 2026-09-06; rev 2 same day after block
switching landed (numbers below are the block-switching encoder).
Peers: rusty_aac 0.5.0, symphonia 0.6.1, oxideav-aac 0.1.7 (all
dev-deps; product `[dependencies]` stays empty). C lavc/libfdk are not
linked (`c-peers-unavailable`).

## Encode (2026-09-06)

Same planar f32 input both sides (the `sine48.adts` golden decoded to
PCM; stereo adds a deterministic 0.8× shadow channel), ADTS bytes out at
128 kbps (`syom::encode` vs rusty_aac `AacEncoder` +
`write_adts_header`). Throughput is input PCM bytes/s.

Wall (criterion median):

| group | syom | rusty_aac |
|---|---|---|
| enc_lc_mono (13 312 samples) | **446.66 µs** (113.7 MiB/s) | 1.645 ms (30.9 MiB/s) |
| enc_lc_stereo (2 × 13 312) | **1.008 ms** (100.7 MiB/s) | 3.902 ms (26.0 MiB/s) |

Memory (`cargo bench --bench mem`, 200 iters; cumulative ÷ 200; the rows
run last in the process, so there is no fair peak-RSS cell):

| group / peer | allocs/iter | alloc bytes/iter |
|---|---|---|
| enc_lc_st syom | **189** | **193 KiB** |
| enc_lc_st rusty_aac | 1067 | 1.53 MiB |

- syom encodes LC ~3.7-3.9× faster than rusty_aac 0.5 at the same 128
  kbps target (mono and stereo), with ~5.6× fewer allocs and ~8× fewer
  alloc bytes. (Rev-1, long-windows-only encoder, same day: 4.3-4.5× /
  156 allocs / 145 KiB — block switching costs per-frame short-path
  allocations and buys 19.3 dB of pre-echo reduction.) syom's per-frame cost is one forward MDCT per channel
  plus ~7 rate-loop trials of quantize+plan; the psy model is a
  precomputed 51×51 matrix multiply per channel.
- Output size on this tonal fixture: syom 2 336 B (≈ 67 kbps —
  content-limited; ADTS carries the VBR fullness marker) vs rusty_aac
  4 580 B (≈ 131 kbps, on target). On budget-limited content syom tracks
  the target to 1.02-1.08× (test `bitrate_accuracy_on_noise`).
- Quality gates for the encoded output live in the test suite, not here:
  sine roundtrip 61-70 dB SNR, band-limited noise 28.8 dB at 128k
  stereo, achieved bitrate 1.02-1.08× of target on white noise, and the
  committed lavc goldens (`src/goldens/enc48{,m}.*`) match ffmpeg's
  decode within 1 LSB s16 / ~80 dB.

## Decode

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
| lc_adts | **218.28 µs** | 135.54 ms | 136.01 ms | 222.55 µs |
| lc_m4a | **211.25 µs** | — | — | 217.75 µs |
| he_adts | **3.010 ms** (SBR+PS) | 89.54 ms (core) | 92.43 ms (SBR) | 210.50 µs (core) |
| he_m4a | **3.026 ms** (SBR+PS) | — | — | 213.96 µs (core) |
| ps_adts | **15.85 ms** (SBR+PS) | 275.63 ms (core) | 291.95 ms (SBR+PS) | 284.65 µs (core) |
| mc_adts | **1.770 ms** | (decodes; ~1.3 s/iter, not wall-benched) | 1.280 s | 3.41 µs (no decode) |

Memory (`cargo bench --bench mem`, 200 iters; allocs/bytes are
cumulative ÷ 200. Peak RSS is process-lifetime; LC ADTS syom is the
first row and the only fair RSS cell):

| group / peer | allocs/iter | alloc bytes/iter | peak RSS |
|---|---|---|---|
| lc_adts syom | **27** | **70 KiB** | **2.58 MiB** |
| lc_adts rusty_aac | 178 | 444 KiB | 2.89 MiB |
| lc_adts oxideav | 2826 | 1.77 MiB | 3.72 MiB |
| lc_adts symphonia | 30 | 156 KiB | 4.30 MiB |
| lc_m4a syom | **36** | **142 KiB** | **4.38 MiB** |
| lc_m4a symphonia | 57 | 160 KiB | 4.63 MiB |
| he_adts syom (SBR) | **1543** | **1.96 MiB** | — |
| he_adts oxideav (SBR) | 2251 | 2.82 MiB | — |
| ps_adts syom (SBR+PS) | **10593** | **13.58 MiB** | — |
| ps_adts oxideav (SBR+PS) | 12614 | 15.85 MiB | — |
| mc_adts syom | **1106** | **0.85 MiB** | — |
| mc_adts rusty_aac | 1315 | 3.83 MiB | — |
| mc_adts oxideav | 10708 | 15.58 MiB | — |

- **LC ADTS wall: clear win over symphonia 0.6 (rev 2).** The first
  2026-09-05 run was a statistical tie (224.78 vs 223.18 µs) — a ~3%
  regression from layering one-shot decode on the streaming core. This
  revision removes that overhead: one-shot mono frames decode straight
  into the output plane (no per-frame scratch + callback copy),
  `decode_streaming` iterates the input slice zero-copy, and ADTS output
  is pre-sized from a frame-header walk. syom 218.28 vs symphonia 222.55
  µs, non-overlapping CIs. Same-session A/B: pre-refactor 96521eb floors
  at 214.10 µs, streaming HEAD was 219.52 µs; the remaining ~2% over the
  floor is one-shot/streaming bookkeeping, not copies.
- **LC M4A wall:** syom 211.25 µs vs symphonia 217.75 µs. syom honours
  `elst` (11264 samples); symphonia does not (12288).
- **LC allocs (rev 2):** syom now wins the count too (27 vs symphonia 30)
  and widens the bytes/iter lead (70 vs 156 KiB); fair peak-RSS row
  2.58 MiB (first row). Pre-revision syom was 32 allocs / 146 KiB — the
  six extra allocs were the streaming-layer input copy plus geometric
  output growth, both now gone. All beat rusty_aac and oxideav by 5-90×.
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
