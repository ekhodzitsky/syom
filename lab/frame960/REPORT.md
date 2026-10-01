# TASK-63 — 960-sample AAC frame support: decision and decomposition

Date: 2026-09-22. Verdict: **NO-GO for the 0.x product** (decode and encode),
with the exact limitation documented and explicit revisit triggers below.
Reproduce: `lab/frame960/run.sh` (offline oracles; never spawned by cargo
test). Pins: [PIN.md](PIN.md).

## 1. What `frameLengthFlag=1` is

In `GASpecificConfig` (ISO/IEC 14496-3), the GA profiles (AAC Main, LC, SSR,
LTP) carry a `frameLengthFlag` bit: `0` = 1024/128-sample IMDCT geometry,
`1` = **960/120**. The bit exists only in the ASC, so 960-frame streams can
appear in **LATM/LOAS**, **MP4/M4A (`esds`)** and **raw AU + out-of-band
ASC** transports. **ADTS cannot carry it** (the ADTS fixed header has no such
bit; MPEG-4 ADTS GA is 1024-only) — measured below: nothing in a 960 vector
ever parses as ADTS.

Deployed demand is broadcast radio: **DAB+ (ETSI TS 102 563 V1.1.1): "For
AAC, two transforms are specified. For DAB, only the 960 transform is
permitted"** at 48/32/24/16 kHz — and DAB+ audio is HE-AAC **v2**, i.e. the
960 core typically carries **SBR+PS** (core at half rate). DRM (ETSI ES
201 980) likewise. FFmpeg's long-standing gap for exactly this
combination is tracked as
[trac #1407 "HE-AAC (v2): 960/120 MDCT window is not implemented"](https://trac.ffmpeg.org/ticket/1407).

## 2. Independent vectors (AC#1)

No encoder on this host can emit 960 frames (measured: FDK encoder rejects
`AACENC_GRANULE_LENGTH=960` for **every** AOT with
`AACENC_UNSUPPORTED_PARAMETER`; its v2.0.3 header documents only
1024/512/480/256/240/128/120; FFmpeg `aacenc` and FAAC have no such option).
So the vectors are **hand-built from ISO/IEC 14496-3 syntax** by
[gen960.py](gen960.py) and their conformity is established by four
independent decoders (§3), not by the table sources. This is a recorded
limitation: encoder-produced 960 vectors would be stronger.

| Vector | Content | Transport | SHA-256 |
|---|---|---|---|
| `vectors/lc960-m48.loas` | 8 × SCE, ONLY_LONG, book-1 quads in bands 0–15, sf = global_gain | LATM/LOAS, ASC `118c` (LC, 48 kHz, mono, flag=1) | `a473c329…8826c` |
| `vectors/lc960-s48.loas` | 8 × CPE, sequences LONG, LONG, **LONG_START, EIGHT_SHORT, LONG_STOP**, LONG×3 | LATM/LOAS, ASC `1194` (stereo) | `782016da…e368` |
| `vectors/lc960-s48.m4a` | same AUs as above | minimal M4A v0 (`stts` delta 960, `esds` ASC) | `207ac55f…0458` |
| `vectors/lc960-{m48,s48}.au` + `.asc` | raw access units + ASC for Init2/ConfigRaw lanes | raw | see manifest |

The stereo vector deliberately exercises the full 960/120 window state
machine (long→start→eight-short→stop→long), not just steady long frames.
Deterministic content: book-1 quads of ±1/0, all scale factors equal to
global_gain 172 (measured lavc decode gain ≈ 2^-20.5 → PCM peak ≈ 0.21;
the earlier gg=84 variant decoded to peak ≈ 4e-8 and is not used).
LOAS framing mirrors syom's lavc-validated `engine/latm_write.rs`.

## 3. Measured behavior matrix (AC#1)

Sample counts are per channel unless noted; every vector is 8 frames →
7680 core samples/channel expected.

| Decoder | lc960-m48.loas | lc960-s48.loas | lc960-s48.m4a | raw AU + ASC |
|---|---|---|---|---|
| FFmpeg 7.0.2 CLI (lavc) | **7680**, peak 0.2076 | **15360 total (7680/ch)**, incl. all window transitions | 15360; ffprobe: aac LC 48 kHz 2ch, `duration_ts=7680` | — |
| `avc_driver` FFmpeg 9.0.1 (lavc) | — | — | ok, 7680/ch, checksum `0xb5ee19c1ece878b1` | — |
| FDK 2.0.2 decoder (ctypes) | — | — | — | 8/8 frames rc=0, StreamInfo: rate 48000, **frameSize 960**, channels 1 (mono) / 2 (stereo) |
| FAAD2 2.11.2 (ctypes, `NeAACDecInit2`) | — | — | — | 8/8 frames err=0, 1920 interleaved samples/frame = 960/ch |
| **syom (current)** | `probe`/`decode`/push all fail `Unsupported(FrameLength960)`; `sniff_is_latm` still true | same | `probe`/`decode` fail `Unsupported(FrameLength960)`; push fails `Unsupported(M4aPush)` (M4A push is off by design) | — |

FAAD2 lane quirk (calibrated): it reports `channels=2`/`samples=1920` even
for the mono ASC — the same harness reports ch=2/2048 for a genuine
ffmpeg-encoded mono 1024-frame ADTS stream, so this is a faad2-runtime
output-configuration behavior, not a vector defect. Per-channel geometry
(960) is correct in both.

Two authoring traps hit and fixed while building the vectors (documented
because they are exactly what a future implementation task must get right):
ZERO_HCB sections **do** carry `sect_len_incr` (Table 4.5 run-length
applies to every section), and `scale_factor_grouping` bit **1** means
"window joins the current group" (all-ones = one group of 8 windows).

## 4. Geometry and subsystem impact (AC#2, source-backed)

Transform. 960-line long windows and 120-line short windows (8 per frame),
KBD or sine shapes. FFmpeg 7.0.2 `aacdec_template.c` runs a **separate
960-point MDCT context** (`MDCT_INIT(ac->mdct960, …, 960)`, l. 1233) with
dedicated `aac_kbd_long_960`/`sine_960` window tables (l. 70–71, 1129–1133)
and a separate `imdct_and_windowing_960` path (l. 2710). 960 = 2^6·3·5, so
the transform needs **radix-3/5 stages**; syom's IMDCT
(`src/engine/imdct.rs`) is a DCT-IV factorization over a **radix-2-only**
FFT with fast plans only for 2048/256 (`PLAN_256`/`PLAN_2048`,
`imdct_into_f32` falls back to an O(n²) naive path otherwise). A 960 plan
therefore means new radix-3/5 FFT machinery plus 1920/240 twiddles, and
per the byte-exactness rule the twiddles must come from
`engine/det_math` (no libm) with SIMD kept scalar-identical.

Bands. The 960/120 scale-factor band tables are **not** the 1024/128
tables scaled: separate ISO tables (in FFmpeg n7.0.2 `aactab.c`:
`ff_swb_offset_960`/`ff_swb_offset_120`, `ff_aac_num_swb_960`/`_120`; in
faad2 `specrec.c`: `num_swb_960_window` etc.). At 48 kHz: **49 long bands**
ending at 960, **14 short bands** ending at 120 (the 120 offset table
carries one extra entry beyond `num_swb` — max_sfb limit is 14). Note an
interoperability wart: FFmpeg's `num_swb_960[44.1k]=46` vs faad2's
`45` (different table vintages; vectors here use 48 kHz where all
implementations agree on 49). syom's `src/engine/swb.rs` ships only the
1024/128 tables.

Window transitions. LONG_START/LONG_STOP keep the long transform length
(960) with asymmetric 120-sized short flanks; grouping rules and
`max_sfb` widths differ (6-bit long / 4-bit short). The stereo vector
proves lavc accepts the full 960 transition cycle.

SBR implications. This is the decisive one for real demand: DAB+ is
HE-AAC v2 over a 960 core. The SBR time grid changes from 16 to **15
slots** per core frame (faad2 `sbr_dec.c:96-99`:
`framelength == 960 → numTimeSlots = NO_TIME_SLOTS_960`), i.e. 30 QMF
slots / 1920 output samples per frame dual-rate. lavc **does not support
it**: `aacdec_template.c:846` and `:2438` — `avpriv_report_missing_feature(
"SBR with 960 frame length")`, and explicit-config SBR/PS is **disabled**
when `frame_length_short` is set (`m4ac->sbr = 0; m4ac->ps = 0`), so FFmpeg
decodes a DAB+ AAC core band-limited and silent about PS. syom's SBR
decoder hardcodes the 1024-core geometry (`sbr_decoder.rs`: "2048 output
samples per 1024-sample core frame", 16-slot grid in `sbr_time_grid.rs`).
So decode-only 960 **LC** support would still not decode the actual
deployed 960 content (HE v2) correctly — the feature only has value as
LC+SBR+PS together.

Stream timing. Per-frame samples become 960/120; `elst`/duration math,
priming, push-decoder cadence, and `stts` deltas all key off the frame
length. syom currently assumes 1024 in these paths (`engine/decode.rs`,
M4A timing). FAAD2/FDK both report per-frame 960 from the same ASC, so
timing derivation from `frameLengthFlag` is well-defined.

Deterministic-transform validation (for a future go). The same contract
as existing goldens: mint 960/120 goldens offline (`MINT_GOLDENS=1`),
assert syom-decode vs lavc-decode ≤ 2 LSB s16 / ≥ 55 dB, keep the decision
path on `det_math` so encoder byte-exactness across platforms is
preserved, and cross-check FDK/faad2 on the same vectors (this lab's
lanes are reusable).

## 5. Decision (AC#3): NO-GO for 0.x

Measured demand: the only identified real-world 960 content is DAB+/DRM
broadcast (HE-AAC v2 in LOAS superframes with RS coding and virtual
interleaving — a receiver pipeline far outside syom's media-file/stream
scope). The registered corpus has **zero** 960 cells (`corpus/coverage.md`
lists the gap explicitly against this task). No consumer of this crate
(kover/sluh) is a broadcast receiver. Competitor coverage
is partial at the ecosystem's two most common decoders (FFmpeg drops
SBR/PS over 960; FDK **encoder** cannot emit 960 at all).

Measured/estimated cost: new radix-3/5 transform (decode + encode),
second SFB table family, window tables, transition state machine,
per-frame timing everywhere, plus the 15-slot SBR grid and PS for the
feature to mean anything — each is a separate PR per this task's own
scope rule, and none is justified without a consumer. Benefit to the
stated product (speech/media decode + LC encode): none measurable today.

**Explicit public claims after this decision (unchanged product behavior):**
syom supports AAC-LC/HE 1024-line frames from ADTS, M4A and LATM/LOAS.
`frameLengthFlag=1` (960/120) is **unsupported on every transport** and
fails early at ASC parse with matchable `AacError::Unsupported(
UnsupportedFeature::FrameLength960)` — measured on `probe`, `decode` and
push `Decoder` for LOAS and on `probe`/`decode` for M4A (`src/guide.rs`
already states this). ADTS never carries the flag.

**Exact limitation:** any GA stream whose ASC sets `frameLengthFlag=1`
(LC/Main/SSR/LTP core, with or without SBR/PS) is rejected with
`Unsupported(FrameLength960)`; ADTS is unaffected by construction.

**Revisit triggers** (any one reopens this decision):
1. A concrete consumer request for DAB+/DRM (or other 960) decode —
   especially a request covering HE v2 over 960, which is the deployed
   shape.
2. ISO/IEC 14496-26 conformance bitstreams obtained (they contain
   960-frame LC cases) — then 960-LC decode becomes a conformance cell.
3. An encoder-side requirement for broadcast/contribution output
   (would start from the transform task below).

**Decomposition sketch for a future GO** (per AC#4 the no-go path creates
no backlog tasks; these are the ready-task outlines to create if a
trigger fires — in this order, one session each):
1. *Transform validation*: radix-3/5 IMDCT/MDCT plans 1920/240 (+120/960
   windows) on `det_math` twiddles, scalar-first, validated against lavc
   `imdct_and_windowing_960` outputs; golden minting.
2. *LC reconstruction/timing*: 960/120 SFB tables, ICS/section path,
   per-element timing (960/frame), overlap pool geometry; decodes this
   lab's `lc960-*` vectors bit-plausibly vs lavc/FDK/faad2.
3. *Transport integration*: ASC flag plumbed through probe/LATM/M4A
   (already isolated at `engine/asc.rs:220-223`), `stts`/`elst` math at
   960, streaming cadence.
4. *SBR/PS over 960* (the DAB+ shape): 15-slot grid, 1920-output path —
   prerequisite for claiming real-world 960 coverage.
5. *Encode support*, only if a broadcast-output requirement exists
   (psy model band tables, block switching and rate loop at 960).

## 6. Study limitations

- Vectors are hand-built (no 960-capable encoder exists on this host —
  measured); conformity rests on four independent decoder implementations
  agreeing on geometry and err=0 decode.
- ISO/IEC 14496-26 conformance vectors not obtained (same recorded gap as
  the TASK-103 campaign); FFmpeg trac #1407 is bot-walled, cited from
  search snippet plus direct source findings in n7.0.2.
- FAAD2 lane channel-count reporting quirk calibrated but not root-caused
  (runtime without headers; per-channel 960 geometry unaffected).
- HE-960 (SBR/PS) decode behavior of FDK/faad2 not measured end-to-end:
  no way to mint an HE-960 stream here. lavc's refusal is source-verified.
