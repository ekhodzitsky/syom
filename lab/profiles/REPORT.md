# LD / ELD / USAC profile-expansion feasibility (TASK-97)

Recorded 2026-09-22. Host: Linux x86_64, gcc 15.2.0, zig 0.14.1 (clang 19
backend, `-target x86_64-linux-gnu`), rustc 1.97.1, ffmpeg 7.0.2-static
(offline oracle only). Pins and rebuild commands: PIN.md. All engines are
lab-only offline oracles; nothing here touches the product crate.

## Method

- Fixtures: deterministic synthetic PCM (`gen_fixtures.py`, seeded): 2 s
  48 kHz speech-like (voiced harmonics + unvoiced noise, 4 Hz syllable AM)
  and music-like (triad + noise bed + castanet impulses), mono and stereo.
  Natural licensed excerpts are not vendored (corpus/manifest.json:
  gap-until-obtained), so these cells are capability/delay/screening
  evidence, not quality qualification.
- Cells: `build/run_cells.sh` logs JSON (`target/tmp/profiles/cells/cells.log`).
- Quality screening: waveform SNR vs source after cross-correlation
  alignment (`score_cells.py`), decoded streams at their output rate.
  Caveats: synthetic fixtures; SNR penalizes legal noise shaping; SNR is
  meaningless for PS cells (parametric stereo cannot reproduce a
  decorrelated second channel waveform-exactly); FDK streams are decoded by
  FDK, syom streams by lavc (FDK 2.0.3 rejects syom encoder ADTS, see
  interop notes), xHE by libxaac where possible. This violates the
  neutral-decoder preference of DOC-3; treat SNR as coarse screening only.
- Delay: measured roundtrip lag (samples at output rate) + FDK-reported
  `nDelay`/`outputDelay`/`frameLength`.

## Capability and provenance matrix (measured)

| engine (pin) | LD enc | LD dec | ELD enc | ELD dec | USAC enc | USAC dec |
|---|---|---|---|---|---|---|
| fdk-aac v2.0.3 | ✓ LOAS (ADTS rejected) | ✓ | ✓ LOAS, auto-SBR | ✓ | — | — |
| libavcodec 7.0.2 | n/a | ✗ "invalid frame was output" on FDK LD LOAS | n/a | ✗ same on FDK ELD LOAS | n/a | ✗ codec not recognized in xHE m4a |
| syom (today) | — | `Unsupported(AudioObjectType(23))` | — | `Unsupported(AudioObjectType(39))` | — | `Unsupported(AudioObjectType(42))` on exhale m4a |
| libxaac v0.1.13 | ✓ (paramfile + ran AOT 23) | ✓ FDK LD LOAS, full length, err 0 | ✓ (paramfile lists 39/39+mps) | ✗ fatal on ELD-noSBR init; ELD+SBR partial: core-rate 24 kHz output, 11 frames, err 0x180a | ✓ ran AOT 42 | ⚠ see USAC gap below |
| exhale v1.2.2 | — | — | — | — | ✓ m4a (measured rates below) | — (encoder only) |

syom rejects all three profiles with clean typed errors — the current
state is safe and explicit, not silent LC fallback.

LD **decode** of 512-sample LOAS/M4A/raw AUs shipped after this spike.
The 2026-09-22 row above is the pre-decode baseline. Tolerances, delay
and the hostile-input run are in `LD_QUALIFY.md` (TASK-131). ELD and
USAC are unchanged.

### USAC decode gap (recorded, not invented)

Both pinned decoder paths were built from source on this host and both
fail at the USAC access-unit level:

- libxaac stock testbench (`xaacdec`): init loop consumes 0 bytes with
  err 0 and never finishes even for a known-good LC ADTS golden
  (`src/goldens/sine48.adts`), under zig-clang and gcc builds, with and
  without `-DDRC_ENABLE -DMULTICHANNEL_ENABLE -DLOUDNESS_LEVELING_SUPPORT`.
- Minimal direct-API harness (`build/xaac_mini_dec.c`, same archive):
  decodes LC ADTS (13 312 samples, matches FDK/lavc/syom) and syom HE LOAS
  correctly; USAC: parses the exhale ASC (AOT 42, 32 kHz, stereo — correct)
  but AU decode fails with 0xffffffff (AU carrying in-band config) /
  0x80000000 (subsequent AUs); a 32-byte-config prefix of a libxaac-encoder
  USAC stream decoded 4 clean frames then errored. Hand-built LOAS wraps
  (audioMuxVersion 0 and 1) fail at init.

Conclusion: **no working pinned USAC decoder on this host**. USAC
quality/delay cells are pending. The missing resource is a validated
xHE-AAC decoder build (or ISO reference decoder); the FDK/libxaac/ffmpeg
set does not provide one here. USAC evidence below is therefore
encode-side + documentation only.

## Measured cells

### FDK LD / ELD vs LC anchors (48 kHz; delay in samples @48 k)

| cell | transport | stream B (2 s) | actual kbps | frame | enc nDelay | dec delay | lag (meas.) | SNR dB |
|---|---|---|---|---|---|---|---|---|
| LC 64k mono speech (FDK) | ADTS | 16512 | 66.0 | 1024 | 2048 | 1744 | 3792 (79 ms) | 19.0 |
| LD 64k mono speech | LOAS | 16356 | 65.4 | 512 | 512 | 0 | 512 (10.7 ms) | 18.3 |
| LD 48k mono speech | LOAS | 12324 | 49.3 | 512 | 512 | 0 | 512 | 16.7 |
| LD 32k mono speech | LOAS | 8205 | 32.8 | 512 | 512 | 0 | 512 | 14.3 |
| ELD 64k mono speech (no SBR) | LOAS | 16381 | 65.5 | 512 | 256 | 0 | 256 (5.3 ms) | 18.2 |
| ELD 48k mono speech (SBR auto) | LOAS | 12361 | 49.4 | 1024 | 580 | 64 | 579 (12.1 ms) | 15.2 |
| ELD 32k mono speech (SBR auto) | LOAS | 8277 | 33.1 | 1024 | 580 | 64 | 579 | 13.7 |
| LC 64k stereo speech (FDK) | ADTS | 17123 | 68.5 | 1024 | 2048 | 1744 | 3792 | 8.4 / 8.7 |
| LD 64k stereo speech | LOAS | 16325 | 65.3 | 512 | 512 | 0 | 512 | 13.8 / 14.0 |
| LD 96k stereo speech | LOAS | 24457 | 97.8 | 512 | 512 | 0 | 512 | 17.0 / 17.0 |
| LD 128k stereo speech | LOAS | 32629 | 130.5 | 512 | 512 | 0 | 512 | 18.4 / 18.6 |
| ELD 64k stereo speech (SBR) | LOAS | 16358 | 65.4 | 1024 | 580 | 64 | 579 | 11.5 / 11.9 |
| ELD 48k stereo speech (SBR) | LOAS | 12290 | 49.2 | 1024 | 580 | 64 | 579 | 9.0 / 9.6 |
| LC 64k stereo music (FDK) | ADTS | 17059 | 68.2 | 1024 | 2048 | 1744 | 3792 | 18.8 / 18.9 |
| LD 64k stereo music | LOAS | 16279 | 65.1 | 512 | 512 | 0 | 512 | 19.7 / 19.7 |
| LD 96k stereo music | LOAS | 24513 | 98.1 | 512 | 512 | 0 | 512 | 20.1 / 20.1 |
| ELD 64k stereo music (SBR) | LOAS | 16349 | 65.4 | 1024 | 580 | 64 | 579 | 16.9 / 16.9 |
| ELD 96k stereo music (SBR) | LOAS | 24535 | 98.1 | 1024 | 580 | 64 | 579 | 17.2 / 17.3 |

FDK ELD auto-configurator: SBR off at 64k mono (frame 512, nDelay 256),
SBR on at ≤48k mono and at 48–96k stereo (output frame 1024, nDelay 580).
ASC captured per cell in cells.log (`asc_hex`), e.g. LD mono 48k =
`40017312000000`, ELD+SBR mono 48k = `4001f1d843624138000000`.

FDK bitrate accuracy: actual/target +2.2 % … +3.2 % on all LD/ELD cells.

### syom LC/HE at matched rates (lavc decode, 48 kHz)

| cell | stream B | actual kbps | lag | SNR dB |
|---|---|---|---|---|
| syom LC 64k mono speech | 16889 | 67.6 | 1024 | 17.1 |
| syom LC 48k mono speech | 12866 | 51.5 | 1024 | 10.6 |
| syom LC 32k mono speech | 8830 | 35.3 | 1024 | 10.2 |
| syom HE 32k mono speech | 8530 | 34.1 | 7765 (misaligned; treat as unqualified) | −2.4 |
| syom LC 64k stereo speech | 16955 | 67.8 | 1024 | 8.2 / 9.5 |
| syom LC 48k stereo speech | 12900 | 51.6 | 1024 | 2.6 / 5.2 |
| syom HE 48k stereo speech | 12655 | 50.6 | 3018 | 9.6 / 8.9 |
| syom HE2 32k stereo speech | 8522 | 34.1 | 3018 | 1.3 / 1.1 (PS — SNR n/a) |
| syom LC 64k stereo music | 16968 | 67.9 | 1024 | 13.9 / 14.1 |
| syom LC 96k stereo music | 25077 | 100.3 | 1024 | 15.5 / 15.6 |

syom's causal LC encoder already runs lower pipeline delay than FDK's LC
(1024 vs 3792 samples measured roundtrip); LD halves it again (512) and
ELD-noSBR quarters it (256).

### xHE-AAC (USAC) encode-side cells (exhale v1.2.2, m4a incl. container)

| cell | preset | file B (2 s) | exhale-reported actual |
|---|---|---|---|
| speech stereo | 0 (nom. 48k) | 14607 | 54.9 kbps |
| speech stereo | 1 (nom. 64k) | 17758 | 67.5 kbps |
| speech stereo | 3 (nom. 96k) | 28470 | 110.3 kbps |
| speech stereo | a (eSBR, nom. 36k) | 12715 | ≈50.8 kbps from bytes |
| music stereo | 0 | 16264 | 61.5 kbps |
| music stereo | 3 | 25937 | 100.1 kbps |
| speech mono | 0 | 8036 | 28.6 kbps |
| speech mono | 3 | 14186 | 53.1 kbps |

Decode/quality/delay cells for xHE: **pending** (decoder gap above).

### Interop notes

- FDK 2.0.3 rejects ADTS carriage for AOT 23/39 (encoder init rejects;
  LD/ELD are LATM/LOAS or raw+ASC profiles).
- lavc 7.0.2 fails on FDK LD and ELD LOAS streams ("An invalid frame was
  output by a decoder" / "Invalid data") — mainstream decoder support for
  LD/ELD on Linux desktops is weaker than the spec footprint suggests.
- FDK 2.0.3 fails to decode syom encoder ADTS output (AAC_DEC_UNKNOWN at
  frame 0 / PARSE_ERROR after 1–2 frames, both LC and HE), while lavc
  decodes the same streams within golden tolerances. Mirror of the known
  syom-decodes-FDK FIL failure (lab/fdk/REPORT.md). Worth a dedicated
  interop task; neither direction blocks TASK-97 conclusions.

## Per-profile evaluation (AC#2)

### AAC-LD (AOT 23)

- User benefit: pipeline delay 512 samples @48k (10.7 ms measured
  roundtrip lag) vs 1024 (syom causal LC) / 3792 (FDK LC) — the MPEG AAC
  family answer for VoIP/telepresence where Opus currently wins.
  Quality at equal rate ≈ LC on these fixtures (FDK LD 64k stereo speech
  13.8/14.0 dB vs FDK LC 64k 18.8–19.0 mono/stereo-music anchors; LD
  96k stereo 17.0 dB closes the gap).
- Missing coding tools vs syom today: 512/480-sample frames with
  low-delay window sequence, 480-sample-rate family (44.1 k grid),
  LD-specific TNS limits, AOT 23 signalling. LC Huffman/quantizer/IMDCT
  machinery is shared; the MDCT/FFT already has the needed sizes for HE.
- Algorithmic delay: 512 (enc) + 0 (dec) + 512 frame period; measured.
- Estimated scope: decode = config/parser (small) + transform/window
  tables (medium; new 512/960/480/240 IMDCT paths and LD windows) +
  LOAS/M4A transport (small, exists) + qualification (medium: no licensed
  ER conformance vectors — TASK-17 inventory; FDK + libxaac are two
  independent oracle lineages). Encode = same plus psy/rate-loop retune at
  512 grid (medium-large).
- Zero-dependency/determinism: no new numeric machinery beyond det_math
  patterns already proven for HE; byte-exact cross-platform encode is
  achievable the same way. No ROM tables beyond window/scalefactor data
  of LC/SBR scale.

### AAC-ELD (AOT 39)

- User benefit: 256-sample encoder delay without SBR (5.3 ms measured
  lag), 579 with SBR (12.1 ms) — the lowest-delay MPEG AAC; used by
  Apple (FaceTime) telephony.
- Missing tools: everything LD needs plus LD-SBR (low-delay SBR variant:
  different QMF delay chain, ELD-specific envelope grid),
  ELDSpecificConfig inside ASC, optional ELDv2 = MPS (SAOC-class
  machinery, large). FDK auto-configurator flips SBR on below ~64k mono /
  ~96k stereo, so real-world ELD streams usually carry LD-SBR.
- Decoder support measured: FDK ✓; lavc ✗; libxaac ✗/partial (core-rate
  only); syom clean Unsupported. Ecosystem reach on Linux is poor.
- Estimated scope: LD scope + LD-SBR decoder/encoder + ELD config
  (large); ELDv2 MPS out of scope entirely.
- Determinism: same toolbox; LD-SBR is integer/QMF-friendly.

### USAC / xHE-AAC (AOT 42)

- User benefit: state of the art at 12–64 kbps (speech and music),
  mandatory in some broadcast/streaming profiles; exhale measured stereo
  speech at actual 54.9 kbps (preset 0) — the quality leader in that
  range by published listening tests (not measured here).
- Missing tools: effectively a different codec — ACELP/TCX LPD core with
  FD core switching, eSBR, MPS, MPEG-D DRC/loudness, preroll semantics,
  new config framework. Shared with syom: FD core quantizer concepts
  only.
- Algorithmic delay: not measured (decode gap); published xHE figures are
  ≈42 ms+ at 48 kHz output (frame + SBR + resampling), i.e. not a
  low-delay competitor.
- Decoder reach: Android 9+ and Apple ship xHE; Linux desktop (ffmpeg/
  FDK) does not — measured above.
- Estimated scope: decoder well above the whole current crate; encoder
  (FD-only subset à la exhale ≈12k LOC C++) still a multi-quarter effort
  plus ISO ROM-table licensing review; conformance corpus for USAC is
  license-gated (ISO/IEC 14496-26 USAC modules).

## Accept / reject (AC#3)

| profile | verdict | evidence | claim scope |
|---|---|---|---|
| AAC-LD | **GO** — decode first, then encode, staged epic | measured capability/delay/quality tables above; two independent decoder lineages on host (FDK, libxaac) | Until shipped, syom claims LC/HE v1/v2 only. After LD: "LD decode/encode qualified against FDK+libxaac oracles on synthetic + licensed vectors"; never "all AAC use cases" while ELD/USAC open. lavc LD interop failure limits the mainstream-playback claim. |
| AAC-ELD | **NO-GO (now)** | needs LD-SBR + ELDSpecificConfig on top of LD; measured decoder reach poor (lavc ✗, libxaac partial); no conformance vectors | Revisit when LD decode+encode has shipped and at least one independent ELD decoder path validates on this host (lavc ELD support or a fixed libxaac build), or when a concrete ELD consumer (Apple telephony interop) is demanded. |
| USAC / xHE | **NO-GO (0.x)** | scope is a new codec, not a profile flag; no working pinned decoder on this host; Linux decoder ecosystem absent | Revisit post-1.0 if xHE becomes a stated product requirement; prerequisites: validated xHE decoder oracle on host, ISO conformance access decision (TASK-17 lineage), staffing estimate. Any "best AAC library" claim must explicitly exclude xHE/USAC until then. |

## Follow-up task graph (AC#4)

LD epic (created via backlog CLI, ordered by dependencies):
1. LD configuration/parser (AOT 23 ASC + LOAS acceptance, clean error
   contract) — **ready** (depends only on this task).
2. LD signal core: 512/480 transform + LD windows + LD TNS limits.
3. LD transport: LOAS/LATM + M4A ASC/elst semantics for LD.
4. LD qualification: oracle matrix vs FDK+libxaac, delay verification,
   hostile-input campaign extension.
Encode spike after that qualification: **go, opt-in only**
(`LD_ENC.md`, TASK-132). Not a default, not ADTS, not the 480-sample
grid, and not built yet.

## Reproduce

```sh
# staging dir with the pinned sources unpacked (see PIN.md)
lab/profiles/build/build_fdk.sh <fdk-aac-2.0.3>          # static lib + fdk-prefix
FDK_PREFIX=<fdk-prefix> lab/profiles/build_driver.sh     # profiles_driver
lab/profiles/build/build_exhale.sh <exhale-v1.2.2>
lab/profiles/build/build_xaac_dec.sh <libxaac-0.1.13>    # + xaac_mini_dec.c
python3 lab/profiles/gen_fixtures.py <cells-dir>
lab/profiles/build/run_cells.sh <cells-dir>              # FDK/syom cells, cells.log
lab/profiles/build/score_all.sh <cells-dir>              # SNR + lag table
```

syom probe: `cargo build --release --manifest-path lab/profiles/syom_probe/Cargo.toml`
(not a workspace member; never in ordinary tests).

## Not done / limitations

- No natural licensed excerpts (corpus gap), no PEAQ/ViSQOL, no listening
  evidence — SNR screening only.
- USAC decode-side cells pending (recorded gap above).
- ELD downscaled (granule 256/240) and 480-sample LD variants documented
  from FDK headers, not measured.
- FDK↔syom encoder-output interop failure observed in both directions
  (out of scope here; recorded for follow-up).
