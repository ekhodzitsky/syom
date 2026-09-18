# TASK-96 — multichannel AAC-LC encoding: decision and decomposition

Decision: **go, staged**, default configurations 3–7 only, PCE output
no-go. Recorded as a backlog decision; three follow-up tasks created.

## Peer behaviour (measured)

libavcodec's native encoder (ffmpeg 7.0.2) is the only surround encoder
available offline; fdk/faac adapters in `lab/` are mono/stereo cells.
Fixtures `mc51` (5.1) and `mc71` (7.1): one tone per channel, 256 kbps,
decoded by libavcodec, Hann-windowed 8192-point tone analysis.

| fixture | kbps per channel | per-channel isolation (own tone vs strongest foreign tone) |
|---|---:|---|
| mc51 | 43 | 54–102 dB (FL 62, FR 102, FC 66, LFE 63, BL 59, BR 54) |
| mc71 | 32 | 46–65 dB (FL 63, FR 50, FC 65, LFE 65, BL 46, BR 46, SL 57, SR 64) |

Layout and signalling: standard layouts use `channel_configuration` 3–7
with elements in ISO order (SCE C, CPE front, [CPE side], CPE back, LFE);
non-standard layouts get an in-band PCE, and the `7.1(wide)` PCE codes the
LFE as a plain SCE (see `mc71p`). Delay: 1024 priming samples, the same as
stereo. Channel coupling (CCE) is not used by any peer encoder surveyed.

## Specification for syom

- **Layouts**: input planes in the decoder's public order (TASK-62):
  3.0 FL FR FC, 4.0 + BC, 5.0 FL FR FC BL BR, 5.1 + LFE at index 3,
  7.1 FL FR FC LFE BL BR SL SR. The channel count alone selects the
  configuration (3, 4, 5, 6, 8 planes); 7 planes and more than 8 are a
  typed error. No PCE writer: every supported layout has a default
  configuration and a PCE adds parser risk for no capability.
- **Element routing**: one `LcEncoder` per element, bitstream order
  SCE(C), CPE(FL,FR), [CPE(SL,SR)], CPE(BL,BR) or SCE(BC), LFE;
  `element_instance_tag` counts per element type from 0.
- **Common window**: each CPE keeps today's shared window decision; block
  switching is per element (the syntax allows it and it avoids spreading
  one channel's attack to six others).
- **LFE**: `lfe_channel_element` = SCE syntax, long windows only, no TNS,
  no PNS, psy cutoff at 120 Hz (`set_cutoff_hz`), excluded from M/S.
- **Rate allocation**: `bitrate_bps` is whole-stream. Per frame the budget
  splits by element weight (SCE 1, CPE 2, LFE 0.1) and then moves toward
  demand: elements report bits at the common allowed-noise offset and one
  offset is searched for the whole frame, so noise-to-mask stays equal
  across elements. The 6144 bits/channel cap applies per element.
- **Transport**: ADTS `channel_configuration` 3–7 (3 bits suffice), ASC
  the same value for M4A / LATM; M4A `channelcount` = plane count.
  HE multichannel: out of scope.
- **Determinism**: nothing new; elements are encoded sequentially with
  det_math only.

## Cost

One `LcEncoder` per element: 7.1 runs five (about 2.5× the stereo
workspace and CPU per output second at equal per-channel rate). No new
dependency, no new container code beyond the channel-count checks.

## Claims after the three tasks

"AAC-LC surround encode for the MPEG default layouts 3.0–7.1, decodable
by libavcodec with channel isolation on committed goldens." Not claimed:
PCE layouts, HE surround, quality parity with libavcodec surround until a
TASK-95-style qualification runs.

## Follow-up tasks

1. Element and bitstream orchestration for configurations 3–7 (fixed
   weight split, channel-isolation goldens decoded by libavcodec).
2. Interelement rate and psy policy (common allowed-noise offset search,
   LFE policy, per-element cap) measured against task 1's fixed split.
3. Public API and transport integration (`encode*`, `Encoder`, ADTS /
   M4A / LATM signalling, layout semantics, errors, docs).

Revisit trigger for PCE output: a user need for a layout outside 3.0–7.1.
