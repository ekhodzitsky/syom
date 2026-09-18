# TASK-84 — first-output and chunk-buffering latency

Decision: **no-go** for an adapter change. The push, reader and sink
adapters already hand a frame over on the byte (decode) or sample
(encode) that completes it; there is no avoidable buffering cell to cut
by 20%. The result is pinned by `src/latency_tests.rs`.

## Delay components

| component | decode | encode |
|---|---|---|
| codec algorithmic delay | LC 1024 priming samples; HE adds the SBR delay (in the PCM, not in time-to-first-frame) | LC causal: none beyond the frame; opt-in lookahead: one frame (1024); HE v1: one AU = 2048 output samples |
| input availability | a frame needs all of its bytes (`aac_frame_length` / LOAS length) | a frame needs 1024 samples (HE: 2048) |
| adapter buffering | **0 bytes** past the frame boundary (ADTS LC / HE / PS / 7.1, LOAS), one byte at a time | **0 samples**: frames leave on samples 1024, 2048, …; lookahead on 2048, 3072, …; HE on 2047, 4095, … |
| reader adapter | never reads past the packet that completes a frame (1 B, 188 B, 1500 B packets) | n/a |
| callback wall latency | 8 µs to the first frame of a 4.4 KB ADTS feed | 75 µs to the first frame (one LC frame encode) |

Wall numbers: AMD Ryzen AI 9 HX 370, release build,
`cargo test --release --lib -- --ignored latency_report --nocapture`,
best of 7.

## The one chunk-size effect

`Encoder::feed` validates the whole chunk (finite, |x| ≤ 1) before the
first frame so a bad chunk fails before any callback:

| feed chunk (samples) | first callback |
|---:|---:|
| 1 024 | 75 µs |
| 4 096 | 76 µs |
| 48 000 | 97 µs |
| 480 000 | 198 µs |

Validating frame by frame would cut the 10 s-chunk cell by about 60%,
but it would let frames out before a later bad sample is rejected, which
changes the documented error lifecycle. A caller that cares about 0.1 ms
feeds small chunks and already pays nothing. Not changed.

`encode_write` feeds 4096-sample slices, so its first write follows the
first 1024 samples' encode; `encode_write_m4a` must finish before the
file is playable (moov after mdat) — container semantics, not buffering.

## Guards

No product code changed: throughput, p95, allocation count and workspace
are those of the previous commit. Revisit if a networked reader adapter or
fragmented MP4 output (TASK-64) lands.
