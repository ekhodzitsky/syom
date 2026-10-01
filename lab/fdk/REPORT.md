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

## syom-encoded ADTS through FDK (TASK-121, fixed 2026-09-23)

Before: syom's ABR stuffing (zero bytes after `ID_END`) made FDK answer
the first stuffed frame with `AAC_DEC_UNKNOWN` — enc48 and tns_gain
failed, enc48t/l stopped after one frame, he48e/he2_48e decoded short
(lab/mobile/REPORT.md finding 1). syom now pads undersized frames with
EXT_FILL `fill_element()`s *before* `ID_END` (`src/engine/enc_pad.rs`);
transport bytes change, decoded PCM is bit-identical (the committed
lavc s16 oracles did not change on re-mint).

`smoke.py` (this directory) is the regression gate — every syom-encoded
ADTS golden must decode to full length, exit 1 otherwise:

| Golden | FDK samples/ch | libavcodec length |
|---|---|---|
| enc48.adts / enc48t / enc48l | 30720 | 30720 |
| he48e.adts | 32768 | 32768 |
| he2_48e.adts | 65536 | 65536 |
| enc_mc51.adts / enc_mc71.adts | 16384 | 16384 |

FAAD2's ADTS-sync loss on stuffed multichannel streams (finding 2) is
gone with the same change. FDK still reports 6 channels for cfg-7 and
rejects the `tns_gain` / `mc71p` fixtures — pinned-platform limitations,
not syom bugs (lab/mobile/REPORT.md findings 3, 5).

## Unsupported cells

- Encoder AOT 23/39 (LD/ELD): **not** this adapter.
- No LD/ELD input fixtures on this host.

## HE v1/v2 encode (TASK-95, 2026-09-23)

`encode-pcm RATE CH BITRATE_BPS AOT IN.f32 OUT.adts` encodes planar f32le
with `AACENC_AOT` 2 (LC) / 5 (HE-AAC v1) / 29 (HE-AAC v2, stereo only).
Smoke (2 s stereo tones+HF, 48 kHz):

| AOT | bps | ADTS bytes | nDelay | ffprobe profile |
|---|---:|---:|---:|---|
| 2 | 48000 | 12214 | 2048 | LC |
| 5 | 48000 | 12289 | 5058 | HE-AAC |
| 29 | 32000 | 8193 | 7106 | HE-AACv2 |

Matrix results live in `lab/quality/HE_QUALIFY.md`.

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` / ordinary tests | **no-go** (Fraunhofer) |
| LC ADTS decode oracle | **go** (length-matched vs syom/lavc on goldens) |
| LC ADTS encode peer | **go** with actual bytes, FDK delay, independent lavc PCM length |
| syom as decoder of FDK ADTS | **no-go** until FIL (frame 11) is understood |
| HE v1/v2 encode peer | **go** (TASK-95 `encode-pcm` AOT 5/29; ffprobe-confirmed profiles) |
