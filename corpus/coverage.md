# Corpus coverage and implementation status

Status review: **2026-11-02**, 0.9.0 release. Implementation
support, registration in [manifest.json](manifest.json), and independent
qualification are separate claims. The [API guide](../src/guide.rs) defines
current supported profiles and containers; a registered fixture alone does
not establish a complete profile or natural-content corpus.

## Current status of superseded gap rows

| Feature | Current implementation | Registered fixtures | Qualification and remaining gaps |
|---|---|---|---|
| LC 960-sample frames | Decode via LATM/LOAS, M4A and raw AU + ASC; HE on a 960 core remains unsupported | The 82-case corpus manifest has no dedicated 960 entry. Authored bytes are separately committed under [lab/frame960/vectors](../lab/frame960/vectors/) | [frame_grid_tests](../src/frame_grid_tests.rs), `lc960_loas_m4a_and_raw_match_the_lab_geometry`, checks mono/stereo counts, finite output and nonzero peak; it is not independent waveform equality or ISO certification. The [original TASK-63 study](../lab/frame960/REPORT.md) documents historical external geometry checks and a superseded no-go decision. |
| AAC-LD 480-sample frames | Decode; encode remains 512 only | No dedicated LD480 corpus entry; one synthetic AU is generated in the regression | [frame_grid_tests](../src/frame_grid_tests.rs), `aac_ld_480_decodes_one_frame_and_is_not_a_960_error`, checks one non-silent 480-sample mono frame. The independent [LD qualification](../lab/profiles/LD_QUALIFY.md) covers 512-sample fixtures, not a new LD480 waveform qualification. |
| LC 7.1 / PCE | Eight-plane default configuration 7 and PCE decode; seven-plane encode has no supported layout | `lc-mc71-adts`, `lc-mc71-m4a`, `lc-mc71p-adts`; encoded `enc-mc71-adts`, `enc-mc71-latm` | [channel_map_71_tests](../src/engine/channel_map_71_tests.rs) checks default order, PCE declaration order, LFE handling and typed rejection of configurations 8–15. Dedicated natural PCE content remains a separate corpus gap. |
| AAC-LD 512-sample frames | Decode and opt-in encode | No dedicated LD entry in the 82-case manifest; `ld64m`, `ld64mus`, `ld48` goldens are committed separately under `src/goldens/` | [LD qualification](../lab/profiles/LD_QUALIFY.md) records FDK/libxaac waveform comparisons and PNS exclusions; it is not ISO 14496-26 certification. |
| Natural corpus excerpts | Codec support does not establish acquisition | Registered natural rows remain identified sources; recordings are not vendored | Later external [consumer-recording qualification](../lab/release-audit/CANDIDATE-0.8.1.md) is separate from presence in this corpus manifest. |

No ISO 14496-26 vectors have been obtained. No new external-oracle comparison or additional-platform qualification
was performed. Local suite reruns are recorded with the documentation corrections. See
[usage and validation](../docs/usage-and-validation.md) for release evidence.

## Archived DOC-3 baseline

The inventory below preserves the earlier coverage assessment and its
original task decisions. Its unsupported/no-go statements for LC960,
LD480 and 7.1 are **superseded by the current-status table above**. Other
cells describe the registered corpus assessment, not a current decoder
support matrix or a claim that later external recordings never existed.
Historical measurements and original gap rationale are retained verbatim.

Cells are **covered**, **synth-only**, **identified-not-obtained**, or **gap**.

### Categories

| Cell | Status | Cases |
|---|---|---|
| Speech voiced | identified-not-obtained + in-tree lecture | `nat-librispeech-*` (dev/holdout), `lc-lecture-m4a` |
| Speech unvoiced | identified-not-obtained | `nat-librispeech-1221`, `nat-cv-ja` |
| Speech sibilants | identified-not-obtained / synth stand-in | `nat-librispeech-1284`, `synth-sibilant-hf` |
| Languages other than English | identified-not-obtained | he (`nat-openslr-yesno`), de/fr/zh/ar/ja Common Voice |
| Tonal music | identified-not-obtained + synth | Musopen rows, `lc-sine48-adts`, `synth-sweep` |
| Dense music | gap | no choir/orchestra excerpt on disk |
| Percussion / transients | identified-not-obtained + encoder golden | `nat-nasa-saturnv`, `nat-fma-percussion`, `enc-enc48t-adts` |
| Difficult stereo | synth | `synth-antiphase-stereo`, `synth-ambience-noise` |
| Silence / near-silence | synth | `synth-silence-1024`, `synth-near-silence` |
| Multichannel impulses | in-tree + synth | `lc-mc30`…`lc-mc51`, `synth-mc51-ch-impulses` |
| Window-boundary / first / last impulses | synth | `synth-impulse-*` |
| DC, full-scale, tiny, clipped | synth | `synth-dc`, `synth-fullscale`, `synth-near-silence`, `synth-clipped` |
| Very short 0/1/1023/1024/1025 | synth | `synth-short-*`, `synth-silence-1024` |
| Long stream | synth | `synth-long-stream-tone` (8 s) |
| Anti-phase stereo | synth | `synth-antiphase-stereo` |

### Rates (Hz)

| Rate | Status |
|---|---|
| 8000 | synth `synth-8k-tone`; identified OpenSLR yesno |
| 11025 | identified NASA Apollo (not obtained) |
| 12000 | **gap** |
| 16000 | identified LibriSpeech (not obtained) |
| 22050 | identified LibriVox holdout (not obtained) |
| 24000 | HE core rate only (in-tree HE goldens output 48 kHz) |
| 32000 | **gap** |
| 44100 | in-tree `lc-sine441-m4a`; identified Musopen |
| 48000 | in-tree majority + synth |
| 64000 | **gap** |
| 88200 | **gap** |
| 96000 | synth `synth-96k-impulse` |

### Layouts

| Layout | Status |
|---|---|
| Mono | in-tree + synth + identified speech |
| Stereo | in-tree HE/PS/lecture/encoder + synth antiphase |
| LC 3.0–5.1 | in-tree `lc-mc*` |
| PCE | **gap** (no dedicated PCE natural) |
| 7.1 | **gap** (TASK-62) |

### Profiles / bitrates

| Cell | Status |
|---|---|
| LC | in-tree goldens |
| HE v1 / v2 | in-tree `he-*`, `he-ps*` |
| 960-sample frames | **gap** — no-go 0.x (TASK-63, decision-27, `lab/frame960/`); clean `Unsupported(FrameLength960)` measured |
| Encoder 24–256 kbps ladder | **gap** (identified as needed; only 128 kbps encoder goldens) |
| ISO 14496-26 vectors | **gap** (normative text not obtained) |
| AAC-LD 512-sample | in-tree FDK v2.0.3 / libxaac 0.1.13 oracles `ld64m`, `ld64mus`, `ld48` (`lab/profiles/LD_QUALIFY.md`). Not a 14496-26 certificate. lavc cannot decode the fixtures. |
| AAC-LD 480-sample | **gap** — `frameLengthFlag` stays `Unsupported` |
| Independently authored ASC/ADTS/LATM/PCE headers | in-tree `corpus/conformance/vectors.json` (not 14496-26) |

Natural audio bytes are **not** in git. Until they are obtained offline,
those cells stay `identified-not-obtained`, not measured.
