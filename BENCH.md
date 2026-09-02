# syom benches

`cargo bench --bench aac` — same bytes → PCM, in-process. Machine: macOS aarch64,
criterion `--quick`, `profile.bench` thin LTO.

Peers that **produce PCM** on the fixture are compared. oxideav-aac 0.1.7
links but has no decode API. C lavc/libfdk are not linked
(`c-peers-unavailable`).

Sample counts (planar samples, ch0-equivalent):

| fixture | syom | rusty_aac | symphonia |
|---|---|---|---|
| LC ADTS `sine48.adts` | 13312 | 13312 | 13312 |
| LC M4A `sine441.m4a` | 11264 (elst 1024) | 0 | 12288 (no elst) |
| HE ADTS `he48.adts` | 18432 @ 48 kHz | 9216 core | 0 |
| HE M4A `he48.m4a` | 16320 @ 48 kHz | 0 | 9216 core |

Wall time (`--quick`):

| group | syom | rusty_aac | symphonia |
|---|---|---|---|
| lc_adts | **410 µs** | 136 ms | 223 µs |
| lc_m4a | 373 µs | — | 226 µs |
| he_adts | **3.17 ms** (SBR+PS) | 90 ms (core) | 4.7 µs (no decode) |
| he_m4a | **3.21 ms** (SBR+PS) | — | 227 µs (core) |

- **rusty_aac**: syom is ~330× faster on LC ADTS; rusty does not reconstruct SBR.
- **symphonia-codec-aac**: faster on LC (~1.8×). It does not reconstruct HE
  (0 samples on HE ADTS; core-only on HE M4A). One-call `syom::decode` vs
  probe + FormatReader + CodecParams.
- **oxideav-aac**: parser-only on crates.io — not a decode peer.
- **Correctness** on these fixtures: syom vs lavc native s16, max abs ≤ 1 LSB,
  SNR ≥ 70 dB (including PNS and HE).
- **Memory**: product `[dependencies]` empty. IMDCT FFT buffer is
  thread-local (no per-frame `Vec` for the N/4 IFFT). Caps:
  `DecodeOptions::speech()` / `unbounded()`.

HE is the fair “same bytes → full-band planar f32” comparison; only syom
finishes that job.
