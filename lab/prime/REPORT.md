# TASK-40 encoder priming / valid length / tail

Pre-fix measurement (TASK-41 added the overlap drain). Recorded 2026-09-14.
Causal LC, 48 kHz mono impulses, peak 0.9.
Decoders: syom `decode_with(..., unbounded())`, oxideav-aac 0.1.7
(`decode_all` ADTS), lavc 9.0.1 `avc_driver` (lab; not `cargo test`).

No encoder repair in this task.

## Lengths (ADTS)

`decoded_len = ceil(N/1024)*1024` on **all three** engines. Empty N=0 is
`AacError::Encode`.

| N | at | syom/oxideav/lavc samples | peak index | peak \|a\| | meaning |
|---|---|---|---|---|---|
| 1 | 0 | 1024 | 1022 | 0.0985 | first sample only as priming-frame leak |
| 1023 | 0 | 1024 | 1022 | 0.0985 | same |
| 1023 | 1022 | 1024 | — | 0 | last sample omitted |
| 1024 | 0 | 1024 | 1022 | 0.0985 | start incomplete (no 2nd window) |
| 1024 | 1023 | 1024 | — | 0 | **last sample omitted** |
| 1025 | 0 | 2048 | 1024 | 0.6048 | start reconstructed (`decoded[i]≈input[i-1024]`) |
| 1025 | 1024 | 2048 | 2047 | 0.0365 | extra pad frame: window leak, not 0.9 |
| 2048 | 0 | 2048 | 1024 | 0.6048 | start ok |
| 2048 | 2047 | 2048 | — | 0 | **last 1024 source samples omitted** |
| 2049 | 2048 | 3072 | 3071 | 0.0365 | leak, not content |
| 3072/4096 last | N-1 | N | — | 0 | omitted whenever N ≡ 0 (mod 1024) |

Lookahead (`with_lookahead(true)`): same frame count `ceil(N/1024)`; last
impulse at 2047 still omitted. Lookahead flush is **not** an overlap drain.

Push `Encoder` bytes match one-shot; omission is identical.

## M4A

`elst.media_time = 1024` always. Presentation length = `n_frames*1024 − 1024`.

| case | syom | lavc |
|---|---|---|
| N=2048 first impulse | 1024 samples, peak at 0, a=0.60 | 1024 |
| N=2048 last impulse | 1024 samples, silent | 1024 |
| N=1024 last impulse | `Unsupported audio format` | `empty pcm` |

A one-frame M4A is entirely priming; both engines have nothing to play.

## EncodeInfo / ADTS metadata

- `EncodeInfo.samples` = source N (not decoded length).
- `EncodeInfo.aac_frames` = `ceil(N/1024)` (padded last **block**, no drain).
- ADTS has no priming/remainder/valid-duration fields.
- Do **not** treat padded decoded length as valid duration (Apple QA1636).

## Ringing vs omitted content

Window leak on N=1025 last sample is ~0.036 at the extra frame’s tail.
Aligned last impulses (N=k×1024) decode to **numerical zeros** across
syom, oxideav and lavc — that is omitted overlap, not lossy ringing.

Disproved: “lookahead flush emits a drain frame”; “padded decode length
is valid duration”; “count-only padding tests would have caught this”
(existing SNR tests skip the last 1024 of the source).

## Go / no-go

| Follow-up | Decision |
|---|---|
| TASK-41 drain: emit one extra MDCT of zeros so the last 1024 source samples reconstruct | **go** (minimal repro: last impulse, N=2048 ADTS, peak=0) |
| TASK-42 write true remainder + valid duration into M4A (`elst` duration, `mdhd`) | **go** after drain (one-frame M4A is empty today) |
| Infer validity from `decoded.len()` | **no-go** |
| Treat current EncodeInfo as a timeline | **no-go** |
