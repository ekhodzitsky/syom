# TASK-103 conformance / hostile-input campaign

Parent: `28df2fe`. Revision of this report: see git after land.
Ordinary `cargo test` does not spawn ffmpeg/FDK. Goldens not reminted.
ISO/IEC 14496-26 bitstreams **not obtained** — no conformance certificate.

## Advertised feature × vector (AC#1)

Status: **ok** = independent pos/neg vector executed on shipped APIs;
**unqualified** = missing ISO-26 or product-fenced (not a silent pass).

| row | advertised | pos | neg | executed | qualify? |
|---|---|---|---|---|---|
| LC ADTS | yes | `sine48.adts` lavc golden | truncated / len-too-small / reserved srate | decode + hostile campaign | **ok** |
| LC LATM/LOAS | yes | `latm48.latm` | truncated LOAS | stream_latm + fuzz seed | **ok** |
| LC M4A | yes | `sine441.m4a` | truncated ftyp | isomp4 + fuzz seed | **ok** (complex elst/fMP4 still fenced) |
| HE v1 SBR | yes | `he48.adts/m4a/latm` | truncated prefix; explicit AOT5 two-rate authored | decode + vectors.json | **ok** for implicit; ISO-26 **unqualified** |
| HE v2 PS | yes | `ps48.adts/m4a` | truncated prefix | decode + hostile | **ok** for in-tree; ISO-26 **unqualified** |
| MC 3.0–5.1 | yes | `mc30`…`mc51` | truncated prefix | decode_mc + hostile | **ok** |
| LC encode ADTS/M4A | yes | `enc48` / `enc48m` byte-exact | NaN / `|x|>1` / empty / M4A push | encode + pcm + platform | **ok** |
| PCE / CCE | consumed | authored PCE vectors | Main AOT / skip CCE | channel_map + fuzz ASC | CCE apply **unsupported** (documented) |
| ADTS CRC | implemented | crc tests | mismatch | adts_crc_tests | **ok** single-block; multi-RDB CRC **unqualified** |
| 960-frame | no | — | `frameLengthFlag` | UnsupportedFrameLength | **ok** reject |
| LD/ELD/USAC / HE encode | no | — | AOT Main seed | UnsupportedAot | **ok** reject |
| fMP4 | no | — | — | TASK-64 | **unsupported** |

## Fuzz campaign (AC#2)

Tool: in-tree std mutator (`lab/fuzz`), rustc 1.97.1, no libFuzzer.
AddressSanitizer **not** used (stable 1.97.1 pin — recorded limitation).
Unique `--seed` per worker. 16 parser + 8 stateful × **3600 s** =
**24.0 CPU-hours** (wall 3600 s, 24 processes).

Seeds: LC/HE/PS/MC/TNS/PNS/encode ADTS, LATM, M4A goldens + TASK-48
malformed corpus.

| family | workers | wall | iters | panics |
|---|---:|---:|---:|---:|
| parser `syom_fuzz` | 16 | 3600 s | ~42.7 M | **0** |
| stateful `syom_fuzz_state` | 8 | 3600 s | ~357.1 M | **0** |
| **total** | **24** | **3600 s** | **399_813_056** | **0** |

No crash files. No hang (speech/64 KiB caps). No new minimized finding
for ordinary tests beyond the existing `corpus/fuzz` seeds.

## Ordinary-test replay (AC#3)

`src/hostile_campaign_tests.rs` drives `decode` / `decode_with` /
`encode` / push `Decoder`/`Encoder` on truncated, reserved-rate, Main
AOT, PCM domain, duration cap, lifecycle, M4A streaming reject, HE/PS/MC
prefixes. Minimized corpus stays in `corpus/fuzz/*.bin`.

## Coverage (AC#4)

See [COVERAGE.md](COVERAGE.md): **93.55% library lines** (tests excluded).
