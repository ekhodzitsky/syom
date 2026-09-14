# Isolated scoring pin (TASK-13)

Offline alignment / objective scoring. **Not** a product API. Not invoked
by `cargo test --workspace`. Independent decode via syom
`decode_with(..., unbounded())` when scoring ADTS.

## Metrics in this lab

| Name | What it is | Certification |
|---|---|---|
| delay | leading-silence pad, else cosine xcorr (smallest \|lag\| tie-break) | none |
| valid_samples | overlap after delay | none |
| actual_bps | `8 * coded_bytes / (valid_samples / rate)` | none |
| snr_db / max_abs | aligned planar error | **not** PEAQ |
| PEAQ BS.1387-2 | [ITU-R BS.1387-2](https://www.itu.int/rec/R-REC-BS.1387/en) | **unavailable** on this host (`gst-inspect peaq` missing; no `peaq` binary) |
| ViSQOL | [google/visqol](https://github.com/google/visqol) | **unavailable** (no binary, not built) |

Do not treat in-tree SNR as a BS.1387 ODG or ViSQOL MOS-LQO.

## Diagnostics (block a quality score)

Silent (RMS < 1e-6), delayed (\|lag\| ≥ 64), truncated (overlap < 50 % of
reference), channel-swap (cross-corr > 1.25 × same-corr), channel/rate
mismatch → `unscorable`.

## Commands

```sh
cargo test --manifest-path lab/score/Cargo.toml
cargo run --release --manifest-path lab/score/Cargo.toml -- controls
cargo run --release --manifest-path lab/score/Cargo.toml -- adts A.adts B.adts
```
