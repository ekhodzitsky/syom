# DOC-3 coverage vs this manifest

Cells are **covered**, **synth-only**, **identified-not-obtained**, or **gap**.

## Categories

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

## Rates (Hz)

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

## Layouts

| Layout | Status |
|---|---|
| Mono | in-tree + synth + identified speech |
| Stereo | in-tree HE/PS/lecture/encoder + synth antiphase |
| LC 3.0–5.1 | in-tree `lc-mc*` |
| PCE | **gap** (no dedicated PCE natural) |
| 7.1 | **gap** (TASK-62) |

## Profiles / bitrates

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
