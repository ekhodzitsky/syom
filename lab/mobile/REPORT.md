# TASK-101 — Mobile interoperability and runtime-cost validation

Recorded 2026-09-22, host per PIN.md (Linux x86_64, no devices). Reproduce:
`python3 lab/mobile/capture.py`. Scope discipline per DOC-3: mobile is a
separate deployment lane; nothing below extrapolates desktop numbers to a
phone.

## AC#1 — Targets, toolchain, CPU: compile-only vs executed

| Target | `cargo check --lib --release` | `cargo build --lib --release` (rlib) | Executed? |
|---|---|---|---|
| aarch64-linux-android | ok | ok, rlib 5 444 896 B | **no** — no NDK linker, no device/emulator |
| armv7-linux-androideabi | ok | ok, rlib 4 924 992 B | **no** (note: armv7 has no SIMD path; scalar only) |
| i686-linux-android | ok | ok, rlib 5 177 554 B | **no** |
| x86_64-linux-android | ok | ok, rlib 5 538 284 B | **no** |
| aarch64-apple-ios | ok | ok, rlib 4 847 168 B | **no** — no Xcode/macOS |
| aarch64-apple-ios-sim | ok | ok, rlib 4 844 432 B | **no** |
| x86_64-apple-ios | ok | ok, rlib 4 863 384 B | **no** |
| aarch64-unknown-linux-musl (control) | ok | ok, rlib 5 444 992 B | **no** — no qemu-user on host |

All cells are **compile-only**: full codegen to rlib, zero product
dependencies, no build script, no linker invoked. Nothing here was executed
on Android/iOS. Executed cross-lane evidence that already exists and was
not re-run: native CI Linux/macOS/Windows × x86_64/aarch64 with byte-exact
encoder goldens (TASK-98), WASM under Node 22 (TASK-100, `lab/wasm`).

Device/OS/toolchain/CPU for executed cells: **none available** — recorded
as a gap, not implied.

## AC#2 — Platform-decoder captures (ADTS goldens, host builds)

Engines: fdk-aac 2.0.3 (= the AOSP platform software AAC decoder
codebase), FAAD2 2.11.3 (historic OEM alternative), libavcodec 9.0.1
(control; in-tree golden oracle). `samples` = planar samples per channel as
reported; `delay` = FDK `outputDelay`. Full JSON per case: `capture.py`.

| Golden | FDK | FAAD2 | lavc |
|---|---|---|---|
| sine48 (LC mono fixture) | 1ch/13312 d1744 | 2ch(!)/12288 | 1ch/13312 |
| tns48 (LC fixture) | 1ch/1024 | FAIL empty pcm (1-frame file, EOF hold) | 1ch/1024 |
| tns_gain (LC fixture) | **FAIL AAC_DEC_UNKNOWN** | 2ch/5120 | 2ch/6144 |
| pns48 (LC fixture) | 1ch/13312 | 2ch(!)/12288 | 1ch/13312 |
| enc48 (syom LC) | **FAIL AAC_DEC_UNKNOWN** | 2ch/29696 | 2ch/30720 |
| enc48t / enc48l (syom LC) | 1 frame then stop | 2ch/29696 | 2ch/30720 |
| he48 (HE v1 fixture) | 2ch/18432 d3730 | 2ch/16384 | 2ch/18432 |
| ps48 (HE v2 fixture) | 2ch/53248 d3730 | 2ch/51200 | 2ch/53248 |
| he48e (syom HE v1) | 2ch/26624 (short) | 2ch/30720 | 2ch/32768 |
| he2_48e (syom HE v2) | 2ch/4096 (short) | 2ch/63488 | 2ch/65536 |
| mc30/40/50/51 (fixtures) | 3/4/5/6ch, full length | full length −1 frame | full length |
| mc71 (cfg-7 fixture) | **6ch(!)**/16384 | 8ch/15360 | 8ch/16384 |
| mc71p (PCE fixture) | **FAIL AAC_DEC_UNKNOWN** | 8ch/15360 | 8ch/16384 |
| enc_mc51 / enc_mc71 (syom) | 1 frame then stop | **FAIL ADTS syncword** | full length |

### Findings

1. **FDK rejects syom's ABR stuffing (release-blocking interop bug).**
   syom pads undersized ABR frames with zero bytes after `ID_END`
   (`enc_frame_rate.rs::fit_budget`). FDK answers the first stuffed frame
   with `AAC_DEC_UNKNOWN` and the stream dies; lavc and FAAD2 accept the
   same bytes. Probe (`capture.py` second table): stripping only the
   trailing zero bytes and fixing the ADTS frame length makes FDK decode
   **every** syom stream to full length — enc48 30/30 frames, enc48t/l
   30/30, he48e 16/16 (26624→32768), he2_48e 32/32 (4096→65536),
   enc_mc51/enc_mc71 16/16. The payload before `ID_END` is never the
   problem. Follow-up task filed.
2. **FAAD2 loses ADTS sync on stuffed *multichannel* syom streams**
   (`enc_mc51`, `enc_mc71`); stereo/mono/HE stuffed streams pass. The
   stripped variants decode fully. Same root cause class as finding 1.
3. **FDK reports 6 channels for `channel_configuration` 7** (mc71,
   enc_mc71) where lavc/FAAD2 report 8 — FDK's cfg-7 mapping on this pin
   does not expose 7.1. Channel *order* on 5.1 matches the lavc-layout
   goldens (`enc_mc51.lavc.s16` is asserted in-tree); FAAD2 reports
   positions `[FC FL FR BL BR LFE]` (its own order), FDK does not print
   positions in this adapter.
4. **Trim / priming.** ADTS carries no trim metadata: FDK passes priming
   through (`outputDelay` 1744 LC / 3730 HE is informational), lavc AU
   decode returns full padded length, FAAD2 holds one frame at EOF (and
   reports 2ch on mono fixtures — its output stage). These are recorded
   disagreement classes, not syom failures. Container trim (`elst`,
   priming/remainder) is validated in-tree against lavc
   (`enc48m`/`he48em`/`he2_48em` M4A goldens); on-device
   MediaCodec/AudioToolbox trim is a qualification gap below.
5. FDK also rejects two *fixtures* (`tns_gain`, `mc71p` PCE) with
   `AAC_DEC_UNKNOWN` — pinned-platform limitations, recorded for the
   competitor ledger, not syom bugs (lavc/FAAD2 decode both).
6. LATM and M4A captures: the FDK/FAAD2 adapters are ADTS-only
   (`TT_MP4_ADTS`); those transports stay lavc-covered in-tree and are
   on-device gaps below.

### syom-decode direction

Already recorded in `lab/fdk/REPORT.md`: syom rejects FDK-encoded 128k
ADTS at frame 11 (`extension_payload invalid` — FDK fill elements).
Unchanged; it is the reverse lane of the same interoperability story.

## AC#3 — Startup/steady CPU, memory, latency on mobile

**Not measured — no device, no emulator, no qemu-user, no NDK.** DOC-3
forbids desktop-ratio extrapolation, so no numbers are invented here.
Existing *desktop-only* facts that transfer as architecture facts (not
performance claims): release decode/encode stacks ≤ 96 KiB (TASK-118
probe), zero steady-state heap allocation after warm-up for borrowed
streaming, 8 MiB workspace cap, single-threaded engine, no runtime
threads/JIT. Power/thermal policy, big.LITTLE placement, and per-SoC
real-time factors all remain open; see follow-up task.

## AC#4 — Integration guidance and gaps

`INTEGRATION.md` (this directory) covers Rust linkage and buffer
ownership. Qualification gaps, each with its concrete missing resource:

| Gap | Missing resource | Follow-up requirement |
|---|---|---|
| On-device execution (CPU/mem/latency, thermal) | Android device or emulator image + NDK; Apple hardware + Xcode | Follow-up task: build `staticlib` per INTEGRATION.md, run the smoke set, measure per DOC-3 (20 reps, median/p95, startup vs steady) |
| MediaCodec / AudioToolbox decode captures | same devices | Decode `src/goldens` syom outputs on-device; verify channel order, HE v1/v2 reconstruction, `elst` trim |
| Final link for Android/iOS | NDK clang / Xcode clang | Link check only; no product change expected |
| FDK rejects ABR stuffing (finding 1) | none — repo-local | Follow-up task: replace zero-byte stuffing with a valid `fill_element()` (or stop stuffing); add FDK-decode golden |
| armv7 SIMD | none — repo-local, optional | Scalar-only today; add NEON path only if a measured cell demands it |

## Product invariants

No product code changed. No dependency added. External codecs stayed
offline oracles; nothing here is spawned by `cargo test`.
