# HE-AAC encoder architecture (TASK-85)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, HEAD `ee5170b`.
This is an architecture spike, not an encoder. Product encode remains
AAC-LC. Ordinary `cargo test` does not spawn ffmpeg/FDK.

## Decision

**Go HE v1 encode** as a staged original path (TASK-86 → 90).
**No-go as a 0.x default** (`encode` / `EncodeOptions::default` stay LC
128 kbps). **No-go HE v2 (PS) encode** until a lavc-accepted v1
bitstream exists (TASK-91–93 stay queued, gated). **No-go** copying
oxideav/FDK/libavcodec encoder source. **No-go** downsampled (1×) SBR
encode, 5.1 HE encode, 960-frame, LD/ELD/USAC in this programme.

Revisit: TASK-86 cannot keep `det_math` + FMA-free QMF alignment, or
TASK-88 cannot get two independent decoder lineages to accept the
payload. Then cancel 86–90 and keep the claim “encode is LC only”.

## Why (measured gap, not a peer slogan)

LC at 24 kbps keeps a 48 kHz 440 Hz tone and **darkens** white noise
(HF/LF Goertzel tilt < half the source). A single 8 kHz sine is cheap
for LC and is not the gap. HE exists to carry dense HF parametrically
while the LC core spends bits below `k0`. Waveform SNR vs the original
is **not** a HE quality gate (SBR is not a waveform coder).

Shipped `encode_with` / `decode_with(audio())` on 1 s 48 kHz mono,
peak 0.5, after skipping 1024 priming (see
`src/encode_he_arch_tests.rs`):

| signal | req bps | observation | note |
|---|---:|---|---|
| 440 Hz sine | 24_000 | Goertzel ratio ≥ 0.5 | LC can code LF |
| white noise | 24_000 | HF/LF tilt < 0.8× source (measured ~0.64×) | the HE gap |
| 8 kHz sine | 24_000 | not a gap | one MDCT bin is cheap |

`he48.adts` (832 B) is a short decode golden, not a matched-rate encode
cell. FDK lab adapter: HE **decode** of that golden is 48 kHz / 2 ch /
delay 3730; HE **encode** peer is **no-go** (`lab/fdk/REPORT.md`).
Apple AAC: no AudioToolbox host (TASK-10). oxideav-aac 0.1.7 advertises
`HeAacEncoder` (DOC-2); reference evidence only — not a source to copy.

CPU/memory lower bounds from HE **decode** (same host, isolated):

| cell | value | source |
|---|---|---|
| HE decode median | 940 µs | `lab/baseline/QMF.md` |
| LC speech decode median | 102 µs | same |
| HE decode peak live heap | 313 KiB | `lab/baseline/SBR.md` |
| LC encode leftover allocs/frame | 7 | `lab/baseline/ALLOC.md` |

## Scope (v1)

| Axis | Supported | Reject |
|---|---|---|
| Profile | HE v1 (SBR on LC core) | HE v2/PS until 91–93; Main/SSR/LTP/LD/ELD/USAC |
| SBR ratio | **2× dual-rate only** | 1× downsampled encode (decoder still accepts 1×) |
| Output rates | 16 / 22.05 / 24 / 32 / 44.1 / 48 kHz | others; core must be an ADTS table rate (`output/2`) |
| Core rates | 8 / 11.025 / 12 / 16 / 22.05 / 24 kHz | 960-line (`frameLengthFlag`) |
| Channels | 1 or 2 | 3.0–5.1 HE encode (LC 5.1 decode stays) |
| Transport | ADTS + M4A first; raw AU + HE ASC via push `Encoder` | LATM encode is TASK-94; one-shot Raw stays rejected |
| Default | off — caller opts in at TASK-90 | silent HE from `encode()` |

## Signal path

```
PCM output_rate, 1–2 ch, [-1, 1]
  ├─ 2:1 downsample (det_math; pair-mean or QMF-low inverse) → core PCM
  │    └─ existing LcEncoder @ core_rate, core share of bitrate_bps
  └─ 32-band AnalysisQmf on full-rate PCM (reuse decoder bank, det_math twiddles)
       └─ grid / envelope / noise / optional harmonic (TASK-87)
       └─ SBR FIL writer (TASK-88; inverse of sbr_huffman)
  mux (TASK-89): one AU = LC raw_data_block + SBR fill; finish drains LC MDCT
                 plus QMF history; no profile-header-only HE
```

One SBR frame matches one LC frame: 1024 core samples → 2048 output
samples (`numTimeSlots·RATE·64 = 2048`, `sbr_decoder.rs`). Analysis
QMF: 32 samples/slot, 320-sample history (`sbr_qmf.rs`). Encoder
analysis runs on the **original** full-rate PCM (32-band slots);
decoder analysis runs on the **core**. Alignment of those two grids is
TASK-86 AC #1.

## Delay / drain (names; exact counts are TASK-89)

Keep four clocks separate (DOC-3):

1. **LC priming** — 1024 core samples = 2048 at output rate.
2. **SBR QMF analysis delay** — 320-sample analysis history plus
   `tHFAdj = 2` slots at the decoder. FDK HE decode delay on `he48`
   was **3730** samples @ 48 kHz vs LC **1744**.
3. **Lookahead** — existing opt-in one core frame (1024), still off by
   default; HE does not add a second lookahead in v1.
4. **Container trim** — M4A `elst` must cover LC+SBR priming so
   presentation length is source N (same contract as LC TASK-40/42).

ADTS has no trim: decoded length includes delay, as today. Push
`Encoder::finish` must flush held LC frame (lookahead) and QMF tail.
Chunk partitioning must not change SBR decisions (TASK-86 AC #2).

## Envelope / noise / grid (TASK-87)

v1 starts **simple and deterministic**:

- Header: Table 4.63 defaults (`bs_freq_scale=2`, `bs_alter_scale=1`,
  `bs_noise_bands=2`, extra-2 defaults from `sbr_header.rs`).
- Time grid: `FIXFIX`, 1 or 2 envelopes per frame; `VARVAR` only if
  the attack detector (already in `LcEncoder`) fires — not a second
  psychoacoustic.
- Envelope: per `fTableLow`/`fTableHigh` band, energy in the analysis
  QMF; 1.5 dB or 3.0 dB `bs_amp_res` as in the header.
- Noise floor: one or two noise bands; no inverse-filter search in v1
  unless it beats the screening target (D4).
- `bs_add_harmonic`: off (zeros) in v1; optional later.
- CPE: `bs_coupling=0` (independent) in v1; coupling is a TASK-87
  extra, not a default.

Search work is bounded: no per-band DP over the whole frame beyond the
existing LC rate loop. SBR side-info target **1.5–3 kbps**; the rest of
the HE share is envelope/noise.

## Bit budget

`bitrate_bps` stays the **whole-stream ABR ceiling** (decision-8). Split
inside the AU:

- Core LC: `core_bps = max(lc_floor, bitrate_bps − sbr_side − sbr_env_cap)`.
  Starting split: **~75% core / ~25% SBR** on speech-like; TASK-89
  measures and may retune without flipping the public default.
- Per-frame LC still ≤ 6144 bits/channel at the **core** rate.
- SBR payload lives in `fill_element` of the same AU; ADTS
  `aac_frame_length` includes it (13-bit cap 8191 still applies).
- No ISO bit reservoir. ADTS `adts_buffer_fullness = 0x7FF`.
- Header-only HE (ASC/SBR header, silent core, no envelopes) is not a
  successful encode (TASK-89 AC #1).

## Profile signalling (TASK-90)

| Container | Wire | Why |
|---|---|---|
| ADTS | LC profile, `sampling_frequency_index` = **core** rate, SBR in FIL (implicit HE) | Matches shipped `he48.adts` / `probe` (header stays Lc) |
| M4A `esds` | Explicit AOT 5 two-rate ASC (Amd 2), e.g. authored `2b098800` = 24k/48k | `probe` already reports HeAac + core/output; `conformance_tests` pins this vector |
| Push raw | `Encoder::asc()` emits that HE ASC; AUs are LC+SBR `raw_data_block` | Muxers (TASK-53) must not wrap LC-only ASC around HE AUs |

Unsupported rate/channel/profile → typed `Unsupported` / `Encode`, not
silent LC. `speech()` decode default unchanged (decision-7).

## Quality / CPU / memory targets

Same-bitrate, independent decode (syom + lavc), native rate, split
planes. PEAQ/ViSQOL still unavailable; Apple still blocked. Screening
substitutes (DOC-3 engineering tolerances, not listening):

| Cell | Gate | Not a gate |
|---|---|---|
| 24 / 32 / 48 kbps stereo 48 kHz, synth speech + music + 8 kHz tone | HE HF energy above `k0` ≥ LC same-rate + **6 dB**; lavc decodes our bits | all-band waveform SNR vs original |
| 24 kbps mono 48 kHz speech-like | same HF-energy gate vs LC 24k | PEAQ ODG (tool missing) |
| lavc vs syom decode of **our** HE | ≤ 2 LSB s16 / ≥ 55 dB (LC golden rule) | identity with FDK/oxideav PCM |
| oxideav / FDK HE encode | TASK-95 matched-rate cell **after** a lab HE-encode adapter exists | this spike |
| Apple | TASK-10 host | this spike |
| Low-rate stereo ambience / anti-phase | document loss; do not collapse to mono by default | PS (v2) |

CPU (isolated, median, 20 reps after warmup, same as QMF.md):

- HE encode ≤ **4×** LC encode at 48 kHz stereo 48 kbps, same N.
- No unexplained >3% median LC encode regression.

Memory:

- Steady HE encode borrowed callback: no extra alloc per frame beyond
  LC encode leftover after warmup.
- Peak live heap ≤ HE decode peak (313 KiB) + LC encode workspace.

Delay: declare QMF+LC priming in `EncodeInfo` / M4A `elst`; no extra
frame beyond existing lookahead.

## Tables: reuse vs original

**Permitted in-tree ISO transcriptions** (already original to this
crate, not FDK dumps):

- `sbr_qmf.rs` `QMF_WINDOW` Table 4.A.89 (640 taps)
- `sbr_qmf_dct.rs` DCT-IV kernels; encoder twiddles via `det_math`,
  FMA-free butterflies (QMF.md)
- `sbr_freq_bands.rs` `k0` / `k2` / `fMaster` / HiLo
- `sbr_huffman.rs` Tables 4.A.79–4.A.88 (encoder = inverse lookup)
- `sbr_header.rs` default header fields

**Original work (do not translate peers):** downsample, envelope/noise
estimator, grid chooser, bit split, FIL writer, finish/priming math,
public options.

**Forbidden sources:** FDK `libSBRenc`, oxideav `HeAacEncoder`,
libavcodec `aacsbrenc`, FAAC HE (2.1 not the 1.31.1 lab pin).

## Conformance vectors

Independently authored (already in-tree, not self-roundtrip):

- ASC explicit two-rate AOT 5: `asc-explicit-sbr-two-rate-24-48` =
  `2b098800` (`src/conformance_tests.rs`)
- Implicit SBR ASC in `he48.m4a` esds: `130856e598`
- Decode goldens `he48.adts` / `he48.m4a` / `ps48.*` (oracle-encoded;
  decode-only)

Encoder (TASK-88/90): new parameter→bits vectors with expected field
lengths; **two lineages** must parse (syom + lavc). ISO 14496-26 still
**not obtained** (`corpus/conformance/go-nogo.md`); do not call a
syom-only roundtrip a conformance certificate.

## Staged interfaces (already queued; one subsystem per task)

| Task | Subsystem | Public surface |
|---|---|---|
| 86 | Downsample + analysis QMF | crate-internal; reference impulses |
| 87 | Grid/env/noise/harmonic | crate-internal parameters |
| 88 | SBR FIL bits | crate-internal writer; independent bit patterns |
| 89 | LC+SBR AU + finish | still internal; delay/rate accounting |
| 90 | Options + ADTS/M4A/ASC | `EncodeOptions` opt-in; goldens; changelog |
| 91–93 | PS analysis/writer/v2 | only after 89 produces lavc-accepted v1 |
| 94 | LATM encode | after 90 |
| 95 | Qualify vs FDK/oxideav/LC | after 90; Apple still TASK-10 |

`EncodeOptions` stays `#[non_exhaustive]`. TASK-90 adds an explicit
HE opt-in; it does not flip `encode()`.

## Claim that stays unavailable until follow-ons land

- “syom encodes HE-AAC” — **unavailable** (LC only on the wire today).
- “HE encode quality vs FDK/Apple/oxideav” — **unavailable** (no HE
  encode; FDK adapter HE-encode no-go; Apple no host).
- “HE v2 encode” — **unavailable**.
- Listening noninferiority (TASK-109) — **blocked** on humans.

After TASK-90, the claim becomes “opt-in HE v1 encode, 2× dual-rate,
mono/stereo, ADTS/M4A”, still not a default and not a SOTA quality win
until TASK-95 + listening.
