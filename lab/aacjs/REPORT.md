# TASK-20 — AAC.js as a browser/JavaScript integration baseline

Pins: [PIN.md](PIN.md) (aac 0.1.3 = git `2d9bd01`, av 0.4.9 = aurora.js
`v0.4.9`; Node v22.19.0 V8; Firefox 155.0 headless; ffmpeg 7.0.2 oracle).
Host: Linux x86_64, Ryzen-class desktop. All runs 2026-09-22.

Scope of the claim: AAC.js is evaluated as a **contextual cross-runtime peer**
(doc-2: "Historical browser counterpart; compare only supported LC with pinned
runtime and JIT policy"). It is a decoder only (no encoder), LC only (HE is
upstream "future work"), LGPL-3.0.

## AC#1 — Pinned capability verification

LC = AAC-LC. Reference = ffmpeg 7.0.2 decode to interleaved f32
(`compare.py`; diffs in s16 LSB units, ×32768). AAC.js output is Float32
interleaved, internally scaled by 1/32768 (`decoder.js`).

| Case (in-tree golden unless noted) | AAC.js result | Evidence |
|---|---|---|
| LC mono ADTS `sine48.adts` | Decodes, no error. Frames 0–2 **bit-exact** vs lavc; frames 3+ diverge max 0.096 (3154 LSB), RMS 850 LSB ≈ −31 dB | defect D2 |
| LC mono M4A `sine441.m4a` | Decodes; aurora M4A demuxer **ignores `elst`** (priming): best alignment −1024 samples vs lavc; same D2 divergence (max 3077 LSB) | `compare.py` |
| LC mono ADTS w/ TNS `tns48.adts` | **Bit-close**: max 0.059 LSB | TNS path fine |
| LC stereo M4A `lecture.m4a` | Decodes; `elst` offset −2048; max 693 LSB, RMS 192 LSB | D2 |
| LC stereo ADTS `enc48.adts` (syom-encoded, lavc-verified) | **FAILS**: `Too many bands (91 > 49)`, then `Prediction not implemented.`, then stall without `end` | defect D3 |
| LC stereo M4A `enc48m.m4a` (same AUs, exact slicing) | **FAILS** identically after 1 AU: `Too many bands (12 > 7)` | D3 is not ADTS-specific |
| LC w/ PNS `pns48.adts` | **NaN output**: 12 544 / 13 312 floats NaN from sample 0 | defect D1 |
| LC 5.1 ADTS `mc51.adts` | Decodes 6 ch, but 98 432 / 122 880 floats NaN | D1 |
| LC 7.1 PCE ADTS `mc71p.adts` | **FAILS**: `PCE unimplemented` / `TODO: PCE_ELEMENT`, then stall; format event reports 0 channels | PCE unsupported |
| ffmpeg-encoded LC probes (`out/probe_*.adts`, generated, recorded below) | steady sine, `-aac_pns 0 -aac_tns 0`: decodes, max 959 LSB / RMS 296 LSB (D2). ffmpeg **defaults** (`aac_pns` is on by default in 7.0): NaN flood (D1) | `probe_nan.cjs` |
| LATM/LOAS `latm48.latm` | Clean unsupported: `A demuxer for this container was not found.` | no LATM demuxer |
| HE v1 ADTS `he48.adts` (implicit SBR) | **Silent wrong output**: decoded as 24 kHz mono LC core, 9 216 floats, no error, no HE indication; lavc: 48 kHz stereo HE | SBR ignored |
| HE v1 M4A `he48.m4a` (explicit ASC) | Cookie parsed as profile 2 / 24 kHz / mono (LC core); decodes core only, container label stays 48 kHz | SBR ignored |
| HE v2 ADTS `ps48.adts` | **Silent wrong output**: 24 kHz mono LC core; PS lost; no error | SBR+PS ignored |

`decoder.js` carries `// TODO: sbrPresent` / `psPresent`; upstream README
lists HE as future work. The dangerous part is not lack of support — it is
that HE input **does not error**: it returns half-bandwidth core-rate audio
with the container's sample rate attached.

### Root-caused defects (all in pinned 0.1.3 sources)

- **D1 — PNS noise generator degenerates to zero → NaN output.**
  `ics.js:232`: `randomState = (randomState * (1664525 + 1013904223))|0`
  folds the LCG's additive constant into the multiplier. From the fixed seed
  `0x1F2E3D4C` the state hits 0 at **step 11** and stays 0 (`x*(m+c)` has 0 as
  a fixed point). Noise-band energy becomes 0 → `scale = sf / sqrt(0) = Inf`
  → `0 * Inf = NaN` (`ics.js:239–241`). Any stream with PNS (`NOISE_BT`)
  bands is destroyed — including everything ffmpeg's native encoder produces
  by default (`-aac_pns` defaults to true). Repro:
  `node decode.cjs ../../src/goldens/pns48.adts`.
- **D2 — `window_shape` history is lost every frame.**
  `decoder.js:145/153` constructs a fresh `ICStream` (hence fresh `ICSInfo`
  with `windowShape = [0,0]`) per frame (`ics.js:271`). The previous-window
  KBD/sine bit is always read as 0, so any frame following a KBD-signalled
  frame gets the wrong overlap window. Encoders default to KBD, so ordinary
  content diverges at ≈ −30 dB from the first KBD-after-KBD transition
  (sine48: frames 0–2 exact, frame 3 onward diverges at the exact frame the
  shape bit flips to 1). **Causal proof**: persisting `windowShape` across
  frames (one-line diagnostic patch, reverted) takes the 1 kHz probe from
  max 959 LSB / RMS 296 LSB to **max 0.011 LSB / RMS 0.003 LSB**.
- **D3 — container frame/AU lengths are ignored; trailing bytes desync the
  stream.** `adts_demuxer.js` `readChunk` emits the *entire remaining stream*
  as one buffer and never slices by `frame_length`; `decoder.js` `readChunk`
  parses elements until `ID_END` + byte-align and stops. Any AU whose coded
  elements don't fill its container frame (legal; syom's ABR leaves unused
  tail bytes after `ID_END` by design, decision-8) shifts every subsequent
  frame. Minimal repro: `python3 pad_adts.py out/probe_nopns.adts
  out/probe_padded.adts 5` pads each ADTS frame with 5 bytes; lavc decodes
  the padded stream identically, AAC.js dies after 2 frames
  (`Too many bands (98 > 60)`). This also explains `enc48*` (341-byte AUs
  with ABR leftover tails) failing in **both** ADTS and M4A.
- **Lifecycle defect**: after a decoder error, Aurora emits `error` and stops
  but never emits `end`; `decodeToBuffer` never calls back. Every failing
  case above hangs silently (our harness exits via timeout). Unimplemented
  paths *throw* rather than negotiate: PCE, pulse data
  (`ics.js` `TODO: add pulse data`), AAC Main/LTP prediction, 960-sample
  frames (`frameLengthFlag not supported`).

## AC#2 — Startup / JIT / throughput / memory / payload (JS lanes only)

Equal-output basis: `out/bench_10s.adts` = ffmpeg-encoded 10 s 48 kHz mono LC
96 kbps, `-aac_pns 0 -aac_tns 0` (123 888 B, 470 frames, 481 280 floats).
Decoded output is **bit-identical between Node 22 (V8) and Firefox 155
(SpiderMonkey)**: FNV-1a over float32 bits = `c8619e7a` in both.
These lanes are not mixed with native numbers; the only native context is
cited at the bottom and uses different fixtures.

| Measurement | Node v22.19.0 (V8) | Firefox 155 headless (SpiderMonkey) |
|---|---|---|
| Module load (`require('av')+require('aac')`) | 16–27 ms (5 fresh processes) | n/a (bundle eval inside page load) |
| First decode, cold JIT | 70–105 ms | 72–88 ms (first iter) |
| Warm-up curve (iter 1→12, ms) | 99, 73, 41, 42, 40, 41, 46, 44, 46, 44, 42, 47 | 72, 66, 63, 64, 76, 69, 68, 66, 76, 90, 76, 80 |
| Steady median (iters 11–60 Node / 5–12 Firefox) | **43.2 ms** (p90 61.0) | **76 ms** |
| Steady throughput | **231× realtime** | **132–139× realtime** |
| JIT speedup first→steady | 2.3× | ~1.1× (starts warmer) |
| Memory | heapUsed 5.1 → 5.3 MB after 60 decodes + gc (no leak); heapTotal reserved 39.8 MB; RSS 134.5 MB | `performance.memory` unavailable in Firefox — **gap recorded** |
| Shipped payload (browser) | — | aurora.js 127 357 B + aac.js 153 765 B = **281 122 B raw / 63 357 B gzip-9** |
| Shipped payload (Node require set) | 48 files, 284 754 B raw / ~83 KB gzip-sum; `npm install` footprint 6.5 MB (incl. unused native `speaker`) | — |

Reproduce: `npm run bench` (add `--expose-gc` for gc-separated heap),
`node bench.cjs --startup` ×5, `node browser/driver.cjs` (geckodriver).

Context only, **different engine and fixtures — not a timing cell**: syom
compiled to wasm decodes its goldens at ~120–540× realtime under the same
Node 22 (lab/wasm/REPORT.md), and syom native LC is 0.106 ms/frame
(lab/baseline/REPORT.md). AAC.js's 231× realtime is the same order as syom's
wasm lane for plain LC decode; it is not a slow outlier. The problem is
correctness and maintenance, not speed.

## AC#3 — Buffer/layout/error ergonomics and packaging

- **API shape** (`decode.cjs` is the worked example): `AV.Asset.fromFile/
  fromBuffer`, evented — `format`, `data` (one Float32Array per frame,
  **interleaved only**, no planar option), `end`, `error`. Output is
  normalized by ÷32768 regardless of source. No seeking API per frame;
  `duration` unsupported for ADTS (null) but present for M4A.
- **Errors are strings over an event**, no codes/typed taxonomy; decoder
  throws are turned into `error` events, then the pipeline stalls without
  `end` (see lifecycle defect). `decodeToBuffer` has no error callback at
  all. Callers must race `error` against `end` and impose their own timeout.
- **Metadata is unreliable at the edges**: `format` reports the *container's*
  rate/channels while the decoder emits the LC core (HE inputs mislabelled,
  e.g. 24 kHz mono PCM labelled 48 kHz); PCE streams report 0 channels;
  M4A `elst` priming is ignored (−1024/−2048 sample offsets measured).
- **Browser packaging** (PIN.md has commands): npm packages are 2016-era
  CommonJS shipping compiled-from-CoffeeScript JS; no ESM, no TypeScript
  types. Upstream's browser build (browserify 4 + coffeeify) **no longer runs
  on modern Node** — reproducible failure recorded in PIN.md; the lab's
  metadata-only workaround produces working bundles. Browser consumption
  requires bundling (`window.AV` global shim), or the prebuilt 2014 release
  assets, which are **older than the pinned versions** (aac v0.1.0 vs pinned
  0.1.3; aurora v0.4.4 vs pinned 0.4.9) — there is no official current
  browser artifact.
- **Node packaging**: `av` depends on native `speaker` (playback); install
  needs `--ignore-scripts` without ALSA headers. Runtime is safe (try/catch).
  Install emits deprecation warnings (`coffee-script@1.7.1`, `mkdirp@0.3.5`).
- **License**: aac is **LGPL-3.0** (av is MIT) — a shipped browser bundle is
  a combined work; proprietary web products must honour LGPL terms (notice,
  source offer / dynamic-relinking provisions). syom is MIT.

## AC#4 — Go/no-go

**No-go** for AAC.js as an ongoing peer or integration baseline:

1. Equal-output cells cannot be established even for plain LC without
   patching upstream: D1 NaNs the default ffmpeg encoder's output, D2 puts
   ordinary KBD content ~30 dB off, D3 fails legal streams with stuffed AUs
   (including all syom ABR output). Three independent root-caused correctness
   defects in the pinned revision.
2. HE is not merely unsupported — it is silently misdecoded (core-only audio,
   container rate label, no error). No HE evidence can be collected.
3. Unmaintained since 2016; its own browser toolchain no longer runs; LGPL-3.0
   is incompatible with "drop-in MIT browser asset" positioning.
4. Speed is adequate (231× realtime LC under V8), so nothing is lost on the
   performance axis by dropping it.

**Contextual alternative**: platform decoders — WebCodecs `AudioDecoder` in
browsers (AAC support is ubiquitous via OS codecs) and OS frameworks — are
the meaningful browser-context lane. AAC.js remains useful only as a
historical pure-JS LC decoder reference. Revisit only if a maintained JS/wasm
LC decoder appears; do not invest in patching AAC.js.

## Reproduce

```sh
cd lab/aacjs
npm install --ignore-scripts --no-audit --no-fund
node decode.cjs ../../src/goldens/sine48.adts out/a.f32     # LC decode + JSON meta
python3 compare.py out/a.f32 out/ref_sine48.f32 64          # vs ffmpeg oracle f32
node probe_nan.cjs <file.adts>                              # NaN birth / window dump
python3 pad_adts.py out/probe_nopns.adts out/p.adts 5       # D3 minimal repro
node --expose-gc bench.cjs                                  # throughput/memory
node bench.cjs --startup                                    # cold start
node browser/driver.cjs                                     # Firefox headless lane
```

Probe fixtures (`out/probe_*.adts`, `out/bench_10s.adts`) are generated by the
ffmpeg commands quoted above and in bench.cjs's header comment; they are
lab-local and gitignored.

Not run / gaps: browser memory numbers (Firefox exposes no JS heap API);
Chrome/Edge and Safari lanes (no such browsers on this host); mobile browsers;
AAC.js patched-fork performance (out of scope — no-go).
