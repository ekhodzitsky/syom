# Parser-fuzz smoke corpus (TASK-48)

Fixed seeds replayed by ordinary `cargo test --lib` (`src/fuzz_smoke_tests.rs`,
`src/fuzz_parse_tests.rs`). The mutational campaign in isolated `lab/fuzz`
loads these plus committed goldens. Ordinary tests never spawn the campaign.

## Valid (already committed goldens)

| id | path | transport |
|---|---|---|
| sine48 | `src/goldens/sine48.adts` | ADTS LC |
| latm48 | `src/goldens/latm48.latm` | LOAS/LATM |
| sine441 | `src/goldens/sine441.m4a` | M4A |
| he48 | `src/goldens/he48.adts` | ADTS HE |

## Malformed (this directory)

| file | intent | parser that must see it |
|---|---|---|
| `empty.bin` | zero bytes | sniff / decode `NotAac` |
| `adts-truncated.bin` | 3-byte ADTS prefix | `AdtsHeader` `UnexpectedEnd` |
| `adts-len-too-small.bin` | `aac_frame_length = 5` | `AdtsFrameLengthTooSmall` |
| `adts-reserved-srate.bin` | `sampling_frequency_index = 13` | `AdtsReservedSampleRateIndex` |
| `asc-main-aot.bin` | ASC AOT 1 (Main) | `AudioSpecificConfig` unsupported AOT |
| `latm-truncated.bin` | LOAS sync only | LATM / decode error |
| `m4a-truncated.bin` | `ftyp` size 24, 8 bytes present | ISOBMFF / decode error |

## Lifecycle (TASK-49)

| file | intent |
|---|---|
| `lifecycle.txt` | Replayable Decoder/Encoder op script (chunked feed, finish, reset, HE after reset, callback fail, short-tail encode, NaN/`|x|>1`) |

Authored 2026-09-14. Not a coverage claim.
