# syom benches

`cargo bench --bench aac -- --quick` then `cargo bench --bench mem`.
Machine: macOS aarch64, `profile.bench` thin LTO, 2026-09-02.

oxideav-aac 0.1.7 links but has no decode API. C lavc/libfdk are not
linked (`c-peers-unavailable`).

Sample counts (planar samples, ch0-equivalent), `syom::decode` (`speech()`):

| fixture | syom | rusty_aac | symphonia |
|---|---|---|---|
| LC ADTS `sine48.adts` | 13312 | 13312 | 13312 |
| LC M4A `sine441.m4a` | 11264 (elst 1024) | 0 | 12288 (no elst) |
| HE ADTS `he48.adts` | 18432 @ 48 kHz | 9216 core | 0 |
| HE M4A `he48.m4a` | 16320 @ 48 kHz | 0 | 9216 core |

Wall (criterion median):

| group | syom | rusty_aac | symphonia |
|---|---|---|---|
| lc_adts | **165.30 µs** | 109.73 ms | 176.74 µs |
| lc_m4a | **168.22 µs** | — | 173.05 µs |
| he_adts | **2.413 ms** (SBR+PS) | 72.51 ms (core) | 3.588 µs (no decode) |
| he_m4a | **2.364 ms** (SBR+PS) | — | 172.48 µs (core) |

Memory (`cargo bench --bench mem`, 200 iters; allocs/bytes are
cumulative ÷ 200. Peak RSS is process-lifetime; LC ADTS is the first
group and the fair RSS row):

| group / peer | allocs/iter | alloc bytes/iter | peak RSS |
|---|---|---|---|
| lc_adts syom | **26** | **153 KiB** | **2.42 MiB** |
| lc_adts rusty_aac | 178 | 444 KiB | 2.77 MiB |
| lc_adts symphonia | 41 | 160 KiB | 2.77 MiB |
| lc_m4a syom | **36** | **138 KiB** | **2.91 MiB** |
| lc_m4a symphonia | 56 | 165 KiB | 3.08 MiB |

- **LC ADTS wall:** syom 165.30 µs vs Symphonia 176.74 µs (~7% faster).
  syom is ~664× rusty_aac.
- **LC M4A wall:** syom 168.22 µs vs Symphonia 173.05 µs. syom honours
  `elst` (11264 samples); Symphonia does not (12288).
- **LC allocs + peak RSS:** syom ≤ both linked decode peers.
- **HE:** only syom reconstructs SBR/PS to full-band PCM. `speech()` is
  1 plane; `unbounded()` Split is 2 planes (lavc implicit HE-AACv2 PS,
  dual-mono on these fixtures). rusty/symphonia times are core-only or
  no-decode — not the same job. benches call `decode()` (`speech()`).
- **oxideav-aac:** parser-only on crates.io — not a decode peer.
- **Correctness** on committed fixtures via `syom::decode` / `decode_with`:
  vs lavc native s16, max abs ≤ 1 LSB, SNR ≥ 70 dB, peak ≥ 1000
  (lecture, 44.1 M4A, ADTS, TNS, PNS, HE ADTS/M4A, LATM).
- **Product `[dependencies]`:** empty. Caps: `speech()` / `unbounded()`.
