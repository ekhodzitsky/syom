# fdk-aac v2.0.3 in-process smoke (TASK-7)

Recorded 2026-09-14, rustc 1.97.1, gcc 15.2.0 (C adapter), zig-c++ clang
19.1.7 GNU target (libfdk-aac 170 TUs, `-fno-exceptions -fno-rtti
-Wno-date-time`), Linux x86_64. Not invoked by `cargo test`.

`aacDecoder_Fill` can swallow the whole ADTS buffer; the adapter now
drains `DecodeFrame` until `NOT_ENOUGH_BITS` (first-frame-only was a
lab bug, not an FDK limit).

## Decode goldens (in-process FDK)

| File | rate | ch | samples | aot | delay | notes |
|---|---|---|---|---|---|---|
| sine48.adts | 48000 | 1 | 13312 | 2 | 1744 | matches syom / lavc length |
| tns48.adts | 48000 | 1 | 1024 | 2 | 1744 | one-frame golden |
| he48.adts | 48000 | 2 | 18432 | 2 | 3730 | 48 kHz PCM = SBR; reported aot stays 2 |
| ps48.adts | 48000 | 2 | 53248 | 2 | 3730 | matches syom HE-v2 length |

## Encode LC ADTS (2 s 440 Hz sine, peak 0.5, 48 kHz, afterburner=0)

| ch | bps | ADTS bytes | FDK decode samples | delay | independent lavc |
|---|---|---|---|---|---|
| 1 | 64000 | 16214 | 97280 | 2048 | — |
| 1 | 128000 | 32426 | 97280 | 2048 | 48000/1/97280 |
| 2 | 128000 | 32426 | 97280 | 2048 | 48000/2/97280 |

Input 96000 samples/ch. FDK/lavc decode of the 128k mono stream is
**97280** (+2304). Encoder `nDelay=2048`.

syom `decode_with(..., unbounded())` of FDK 128k ADTS fails at frame 11
(`extension_payload invalid`). Independent engine for that cell is
**libavcodec 9.0.1**, not syom. Follow-on: syom FIL vs FDK fill.

## Unsupported cells

- Encoder AOT 5/29 (HE) and 23/39 (LD/ELD): **not** this adapter.
- No LD/ELD input fixtures on this host.

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` / ordinary tests | **no-go** (Fraunhofer) |
| LC ADTS decode oracle | **go** (length-matched vs syom/lavc on goldens) |
| LC ADTS encode peer | **go** with actual bytes, FDK delay, independent lavc PCM length |
| syom as decoder of FDK ADTS | **no-go** until FIL (frame 11) is understood |
| HE encode peer | **no-go** |
