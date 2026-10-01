# TASK-64 — Bounded fragmented MP4 (fMP4) AAC input: inventory, contract, go/no-go

Spike deliverable. Verdict: **GO (bounded)** — see §5. Decision recorded as
decision-25; follow-up tasks TASK-124/125/126, decoder-strictness audit TASK-127.

## 1. Fixtures and generators (AC#1)

All fixtures regenerable via `gen_fixtures.sh` (pins in PIN.md; `*.mp4`/`*.m4a`
are git-ignored build artifacts, third-party vector kept under
`fixtures/thirdparty/` with sha256 in PIN.md).

| fixture | generator | structure exercised |
|---|---|---|
| `lc_empty_moov.mp4` | ffmpeg 7.0.2 mov muxer, `empty_moov`, 1 s fragments | baseline: tfhd base-data-offset, tfdt, trun data_offset+size |
| `lc_dbmoof.mp4` | `+default_base_moof` | tfhd 0x020000 (default-base-is-moof), no base-data-offset |
| `lc_omit_off.mp4` | `+omit_tfhd_offset` | implicit base = end of previous fragment's data |
| `lc_sidx.mp4` | `+global_sidx` | top-level sidx; **inconsistent tfhd base-data-offset** (§4.2) |
| `lc_negcts.mp4` | `+negative_cts_offsets` | trun **version 1** (signed cto space; no cto field written for audio) |
| `lc_cmaf.mp4` | `+cmaf` | tfhd 0x2003a (sample_description_index + defaults + default-base-is-moof), trun v1 |
| `lc_cenc.mp4` | `+cenc-aes-ctr` encryption | `enca` entry, `sinf`/`schm`/`tenc`, per-traf `saiz`/`saio`/`senc` — unsupported cell |
| `lc_2track.mp4` | two AAC tracks | two `trex`, two `traf` per `moof` sharing one `mdat` — unsupported cell |
| `he_empty_moov.mp4`, `he2_empty_moov.mp4` | syom HE v1/v2 M4A goldens remuxed by ffmpeg mov muxer | explicit two-rate ASC (AOT 5 `2b118800` / AOT 29 `eb098800`), AU stride 2048 at 48 kHz media timescale |
| `dash/init.m4s` + `seg_01..07.m4s` | ffmpeg dash muxer | separate init/media boundary; `styp` (msdh) + per-segment `sidx`; self-contained moofs (default-base-is-moof); **init carries `elst` media_time 1024** |
| `thirdparty/bbb_init.m4a` + `bbb_seg1.m4a` | GPAC-muxed Akamai BBB DASH vector (HE v1) | `mehd`, bare tfhd (0x20000, all fields from trun), trun 0x701 (per-sample duration+size+flags), zero-count FIL elements (§4.1) |

Structural inventory per fixture: `out/inventory_*.json` (produced by
`walk_mp4.py`, a stdlib-only box walker written for this task — independent of
libavformat).

### Observed fragment semantics

- **Init/media boundary**: init segment = `ftyp` + `moov` with **empty sample
  table** (`stts`/`stsc`/`stco` entry count 0, `mdhd.duration` 0) + `mvex`.
  ffmpeg writes only `trex` (defaults all zero); BBB init adds `mehd`
  (duration hint 634354 movie units). Media segments add `styp` + optional
  `sidx`. Nothing in the init sample table is usable for fragments — the
  whole index comes from `moof`s.
- **tfhd flag patterns seen**: 0x39 (base-data-offset + dur/size/flags
  defaults), 0x20038 (defaults, default-base-is-moof), 0x38 (defaults,
  implicit base), 0x2003a (CMAF: + sample_description_index), 0x20000 (BBB:
  no defaults at all). All three base-data-offset modes occur in practice.
- **trun patterns**: v0/v1 with flags 0x1 (data_offset only, tail fragment:
  1 sample, duration from tfhd default 256), 0x201 (data_offset + per-sample
  size; duration/size defaults from tfhd), 0x701 (BBB: per-sample
  duration+size+flags). Sample flags (0x400) carry no audio-relevant
  information; cto absent in every audio fixture.
- **tfdt** present in every ffmpeg/dash/BBB fragment (v0, u32); decode time
  is otherwise an accumulator across fragments. tfdt values are exact
  multiples of 1024 (LC) / 2048 (HE) media units; media timescale is the
  **output rate** (48000) for HE, so one HE AU strides 2048.
- **Defaults chain**: trun field → tfhd default → `trex` default. ffmpeg
  sets per-fragment tfhd `default_size` to the fragment's first AU size and
  per-sample sizes in trun anyway (redundant but legal); BBB uses trun-only.
- **Sample description**: every fixture has exactly one `stsd` entry; no
  mid-stream sample-description switch observed. ASC may carry trailing
  sync-extension bytes (`119056e500` = LC 48 kHz stereo + 3 extension bytes).
- **Edit/priming**: mov-muxer fMP4 carries **no `elst`** — encoder priming
  (1024 samples) is presented; measured: lavf decode of `lc_empty_moov.mp4`
  = flat-m4a decode + exactly one priming frame prefix (289792 vs 288768
  frames, zero residual at offset 1024). The dash init segment carries a
  priming-only `elst` (one entry, seg_dur 0, media_time 1024, rate 1.0).
  The BBB HE vector has no elst either (HE priming untrimmed). lavf's dash
  demuxer synthesizes a `pts=-1024` skip marker instead.
- **Trailer**: ffmpeg writes `mfra`/`tfra`/`mfro` unless `skip_trailer`;
  random-access metadata only, not needed for linear decode.
- **Last fragment**: one short-tail AU (7 bytes, declared duration 256) —
  declared trun durations are *timeline* metadata; lavf still decodes full
  1024-sample frames (total 289792 = 283×1024).

### Cross-validation against an independent demuxer

`crosscheck.py` compares every walker's resolved sample (absolute offset,
size, dts, cts, duration) against `ffprobe -show_packets` (libavformat mov
demuxer — independent of `walk_mp4.py`):

```
lc_empty_moov OK (283 samples)   lc_dbmoof OK   lc_omit_off OK
lc_sidx OK (declared arithmetic; see §4.2)   lc_negcts OK   lc_cmaf OK
he_empty_moov OK (16)   he2_empty_moov OK (32)   bbb_cat OK (94)
```

PCM-level proof that the resolved AUs are the real payloads: the lab bridge
(§3) decodes through syom's existing raw-AU API and matches the lavf decode
of the same file: **max 1 LSB, rms 0.83 LSB** (LC), max 2 LSB (HE v1),
max 1 LSB (HE v2), max 1 LSB (dash concat vs single-file lavf decode).
All tfhd addressing variants decode to byte-identical PCM.

## 2. Bounded contract (AC#2)

Proposed support envelope — one AAC track in unencrypted fMP4:

**Supported boxes**: top level `ftyp`, `moov`, `moof`, `mdat`, `free`,
`skip`, `sidx` (parsed, index ignored), `mfra` (skipped); init:
`mvhd`, `trak`/`tkhd`, `mdia`/`mdhd`/`hdlr`, `minf`/`stbl`/`stsd`(`mp4a`→
`esds` ASC via existing `isomp4.rs` machinery), `edts`/`elst` (existing
single-entry rule), `mvex`/`trex`, `mehd` (hint only); fragment:
`mfhd`, `traf`/`tfhd`/`tfdt`/`trun`, `styp` (skipped). Ignored: `emsg`,
`prft`, `meta`/`udta`.

**Timeline rules**:
- decode order = fragment arrival order; `mfhd` sequence numbers must be
  continuous (gap → `Malformed`), matching every generator observed.
- sample dts: `tfdt` when present, else accumulated; per-sample duration =
  trun field → tfhd default → `trex` default; zero resolved duration without
  `duration_is_empty` → `Malformed`.
- base data offset: explicit `base_data_offset` → `default_base_is_moof` →
  implicit (end of previous fragment; first fragment = its moof position).
  Resolved ranges **must land inside an `mdat` payload**; violation →
  `Malformed` (this fence catches the §4.2 inconsistency).
- trun v0/v1 accepted; composition-time-offset present and nonzero →
  `Unsupported` (audio cts == dts in every observed fixture).
- `stsd` entry count must be 1; `sample_description_index` switches →
  `Unsupported` (no multi-ASC stream observed; ASC change semantics would
  duplicate the existing `AscChange` contract).
- edit: init `elst` reuses the current single-entry contract
  (`media_time` priming, rate 1, exact conversion); absent → present from
  tfdt 0 including priming (mirrors lavf on mov-muxer files).

**Explicit unsupported outcomes** (typed, matchable — never silent):
- encrypted: `enca` entry, or `saiz`/`saio`/`senc` in a traf →
  `Unsupported` (`enca` already rejected today; the traf-level fence is new).
- more than one usable AAC `soun` track, or a `traf` whose `tfhd.track_ID`
  names a different track → `Unsupported` (multi-track fMP4 interleaves
  trafs inside one mdat — out of scope).
- `sinf`-wrapped `esds` stays unreachable (existing CENC behavior).
- fMP4 needs a **distinct matchable error**: today the one-shot path
  collapses fMP4 rejection to `NotAac` ("Unsupported audio format") even
  though `isomp4.rs` has a specific "fragmented MP4 is not supported"
  `Format` error. Contract: `AacError::Unsupported(...)`-class variant.

**Incremental input and buffering budgets** (push model, layered on the
existing pump/budgets):
- init `moov` ≤ existing M4A metadata budget (16 MiB / 2^20 entries).
- per-`moof` tables fenced by the metadata budget; resolved sample table is
  per-fragment and freed after use — resident index is **one fragment**, not
  the stream lifetime (unlike flat M4A's whole-file `stsz`/`stco` index).
- resident compressed buffer = current box payload only; `mdat` is consumed
  per AU as soon as it completes — cap = one AU (existing declared-AU fence)
  plus box headers; the feed cap applies to the resident buffer, never to
  lifetime compressed bytes (existing streaming rule).
- steady state must stay zero-alloc after warmup, same as ADTS/LOAS push.

## 3. Complexity/footprint measurement (the bridge prototype)

`bridge/` (lab-only crate, `syom` path dep): a 402-line standalone
whole-file fMP4→AU extractor feeding `Decoder::from_asc`/`decode_au`.
Line accounting: ~150 lines duplicate what `src/isomp4.rs` already owns
(box walk, esds/ASC, elst, mdhd/mvhd); **genuinely new semantics ≈ 180–250
lines** (trex defaults, tfhd/tfdt/trun resolution, base modes, fences).
Stripped bridge binary 1 036 552 B vs the lab/footprint decode consumer cell
1 001 608 B (different baseline/flags; indicative **≈ +35 KB .text**, against
a 790 KB decode budget with 103 KB headroom — fits).

The same bridge doubles as the working proof of the **external-demux +
raw-AU alternative**: it functions today with zero product changes. That
alternative pushes tfhd/tfdt/trun resolution — including the §4 traps — into
every consumer (kover/sluh, WASM players), with no shared correctness bar.

## 4. Findings (real-world traps the contract must own)

### 4.1 Zero-count FIL elements rejected by syom (decoder strictness, not fMP4)

BBB HE AUs carry `fill_element`s whose count resolves to 0. lavc decodes the
segment fully; syom rejects at `extension_payload.rs` (`parse_with_sbr`,
`cnt == 0` → `ExtensionPayloadInvalid`, surfaced as `Malformed(Sbr)`).
FAAD2 rejects the same AUs for a different reason ("Channel coupling not yet
implemented" — its own known limitation). With a temporary local patch
tolerating `cnt == 0`, all 94 AUs decode and PCM matches lavf at **max 1 LSB
/ rms 0.70**. Patch reverted; evidence: `bridge/src/bin/bbb_au_probe.rs`
reproduces the failure on pristine syom. **Follow-up: TASK-127 (audit).**
This also validates the spike's premise that fMP4 qualification needs
third-party fixtures: the existing corpus never exercises this.

**Resolved by TASK-127 (2026-09-23):** per ISO/IEC 14496-3
`fill_element()`'s `while (cnt > 0)` loop (and lavc's identical `TYPE_FIL`
arm), a zero-count FIL is a legal empty element carrying no
`extension_payload`; `sbr_attach::ingest_fil` now skips it. All 94 AUs
decode through the bridge; PCM matches lavf at max 1 LSB / rms 0.697.
Regression: `stream_au_tests::bbb_*` (goldens `src/goldens/bbb_fil.*`),
`sbr_attach_tests::zero_count_fil_is_an_empty_extension`.

### 4.2 ffmpeg 7.0.2 `global_sidx` writes an inconsistent tfhd base-data-offset

In `lc_sidx.mp4`, tfhd declares `base_data_offset = 732` (the sidx position —
the value the moof would have had without the inserted sidx) while the moof
actually sits at 856 and the data at 1160. Declared arithmetic (732 + trun
data_offset 304 = 1036) lands inside the moof box; ffprobe's reported packet
`pos` reproduces the same wrong number, while lavf actually reads
moof-relative (strace: seek to 856) and decodes correctly. A spec-faithful
resolver rejects the file (bridge: `Malformed(Section)`). Contract
consequence: resolved ranges are fenced against the mdat payload (§2), so
this file is `Malformed`, not silently misparsed. CMAF's
`default_base_is_moof` mandate exists precisely to avoid this class.

### 4.3 Priming presentation differs by muxer path

mov-muxer fMP4 presents encoder priming (no elst); the dash muxer writes a
priming-only elst in the init; lavf's dash demuxer additionally synthesizes
a negative-pts skip marker. The contract keeps elst handling optional and
delegates to the existing trim machinery (§2), so both shapes are exact.

## 5. Go/no-go (AC#3): **GO (bounded)**

- **Integration demand**: fMP4 is the carriage for AAC in DASH/HLS/CMAF —
  the dominant delivery for browser and WASM targets, where syom already
  ships a WASM lane and where **no zero-dependency external demuxer exists**
  to pair with the raw-AU API. kover/sluh spawn this crate; without in-tree
  support each grows a private trun parser (the exact trap surface of §4).
- **Footprint/complexity**: measured ≈ 180–250 new product lines on top of
  existing isomp4 machinery and ≈ +35 KB indicative .text, inside budget
  headroom. The bounded envelope (§2) keeps this a parser, not a framework:
  no DRM, no multi-track, no sidx random access, no network anything.
- **Versus external demux + raw-AU**: proven workable today (§3), but it
  externalizes the correctness-critical part and cannot serve WASM/embedded
  consumers. The raw-AU API remains the internal seam the fMP4 lane feeds.

No-go risks that shaped the envelope: scope creep into a media framework
(refused via §2's explicit unsupported list), and silent acceptance of
inconsistent inputs (refused via mdat fencing and typed errors).

## 6. Follow-up tasks (AC#4)

- **TASK-124** — parser/index: bounded fMP4 init + fragment parsing to raw
  AUs per §2 (one-shot/seekable slice path first).
- **TASK-125** — incremental delivery: push feed of init + moof/mdat with
  the §2 buffering budgets (depends on TASK-124).
- **TASK-126** — timeline/conformance qualification: independent fixtures
  (all addressing modes, dash init+segments, HE v1/v2, truncation and
  mid-fragment cuts, mfhd gaps, encrypted/multitrack rejections, the BBB
  vector once TASK-127 lands).
- **TASK-127** — audit (out of fMP4 scope, surfaced by it): syom rejects
  zero-count FIL fill elements that lavc tolerates (§4.1).

## 7. Verification evidence index

| evidence | artifact / command |
|---|---|
| fixture regeneration | `gen_fixtures.sh` (deterministic lavfi source + pinned ffmpeg) |
| structure inventory | `walk_mp4.py` → `out/inventory_*.json` |
| walker vs ffprobe | `crosscheck.py` (exit 0 on all single-file fixtures + bbb concat) |
| raw-AU bridge decode | `bridge/` (cargo bin); PCM cells in §1 |
| lavf PCM references | `fixtures/*.lavf.s16`, `out/bbb_cat.lavf.s16` |
| priming measurement | §1: offset-1024 zero-residual alignment, frame counts |
| sidx inconsistency | §4.2: byte probes + strace; corruption tests `/tmp` (transient) |
| zero-count FIL repro | `bridge/src/bin/bbb_au_probe.rs` on pristine syom |
| committed golden pins (TASK-126) | `GOLDENS.md` + `verify_pins.py` (sha256 per `src/goldens/fmp4_*`) |
| ffprobe timeline oracle (TASK-126) | `gen_timeline_oracle.py` → `src/goldens/fmp4_timeline.txt`, replayed by `isomp4_frag_timeline_tests` |
| tfdt-less accumulation fixture (TASK-126) | `strip_tfdt.py` → `src/goldens/fmp4_lc_notfdt.mp4` |
| third-party BBB vector in-tree (TASK-126) | `src/goldens/fmp4_bbb_{init,seg1}.m4a` (pins = PIN.md), end-to-end in `stream_fmp4_qualify_tests` |
| truncation/mid-fragment typing (TASK-126) | `stream_fmp4_qualify_tests::mid_fragment_cuts_are_typed_truncated`, seed `corpus/fuzz/fmp4-truncated.bin` |

Product code unchanged (temporary instrumentation reverted; `git diff src/`
empty). Ordinary checks therefore unaffected; bridge/lab crates are not
workspace members and never spawn ffmpeg from tests.
