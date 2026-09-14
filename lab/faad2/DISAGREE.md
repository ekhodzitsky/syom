# FAAD2 2.11.3 vs libavcodec 9.0.1 (TASK-8)

Measured 2026-09-14, gcc 15.2.0, Linux x86_64. Same committed goldens.
FAAD2 adapter: `FAAD_FMT_FLOAT`, planar FNV-1a. libavcodec: FFmpeg 9.0.1
native `aac`, `in_process_au`. This is **not** a vote for either engine.

ADTS `channel_configuration` (parsed from the first header): sine48=1,
he48=1, ps48=1, mc51=6.

| Fixture | Class | FAAD2 | libavcodec | Notes |
|---|---|---|---|---|
| sine48.adts | **syntax / channel labels** | rate 48 kHz, **2 ch**, 12288 samples/ch, ot=2 sbr=0 ps=0, pos [L,R] | rate 48 kHz, **1 ch**, 13312 samples | Bitstream `channel_configuration=1` (mono). FAAD2 emits stereo (ids 2,3). syom/lavc keep one plane. **Do not pick a PCM winner.** |
| sine48.adts | **alignment / priming** | 12288 samples | 13312 samples (Δ +1024) | One LC frame. Consumed bytes both 4452. |
| he48.adts | rate/layout | 48 kHz, 2 ch, ot=5 sbr=1 ps=0 | 48 kHz, 2 ch | Rate and stereo HE agree despite ADTS cfg 1. |
| he48.adts | **alignment / priming** | 16384 samples | 18432 samples (Δ +2048) | One HE frame (2048). Consumed both 832. Checksums differ (expected). |
| ps48.adts | PS flag | ot=5 sbr=1 **ps=1**, 2 ch | 2 ch (no PS flag in adapter) | FAAD2 reports parametric stereo on. |
| ps48.adts | **alignment / priming** | 51200 samples | 53248 samples (Δ +2048) | Consumed both 2440. |
| mc51.adts | channel **count** | 6 | 6 | Agree. |
| mc51.adts | **channel labels / order** | pos [C,L,R,BL,BR,LFE] ids 1,2,3,6,7,9 | lavc FL FR FC LFE BL BR | MPEG speaker ids vs lavc plane order. **Syntax of PCE/default cfg, not a float error.** |
| mc51.adts | **alignment / priming** | 19456 samples | 20480 samples (Δ +1024) | Consumed both 13294. |
| he48.latm | unsupported | `Init Gain control not yet implemented` | lavc `decode-au` also unsupported for LATM | Adapter is ADTS-only. |

**Precision:** both oracles use float PCM. Checksums are not comparable
across engines (different delay, channel count, and plane order). s16
would be `round(f32 * 32768)` with clip; this lab does not emit s16.

**Stochastic noise:** none of these goldens are PNS-identity cells.

**Policy:** disagreements are classified; syom is not required to match
FAAD2's mono→stereo emit or MPEG speaker order. Sample-count deltas are
priming/delay, not proof that either length is “the” valid media time
(TASK-40).
