# HE v1 SBR bitstream writer (TASK-88)

Recorded 2026-09-15. Host: Linux x86_64, rustc 1.97.1, on top of
`b575e98` (TASK-87). Crate-internal `engine/enc_sbr_bits.rs`.
`encode()` stays LC; nothing here reaches an AU yet (TASK-89).

## What is written

| Syntax | Writer | Notes |
|---|---|---|
| `sbr_header()` Table 4.63 | `write_header` | extra-1/extra-2 blocks only when their flag is set; clear flag + non-default field = error (unrepresentable, never silently dropped) |
| `sbr_grid()` Table 4.69 | `write_grid` | FIXFIX (1/2/4 envelopes, one shared `bs_freq_res`), FIXVAR (reversed `freq_res`), VARFIX, VARVAR; `bs_pointer` width `ceil(log2(LE+1))` from the parser's `ptr_bits`; `LE ≤ 5` |
| `sbr_dtdf()` / `sbr_invf()` | `write_dtdf` / `write_invf` | lengths checked against the grid / `NQ` |
| `sbr_envelope()` / `sbr_noise()` | `write_row` | frequency direction: start value (7/6 bits env, 5 noise) + `f_huffman`; time direction: `t_huffman` for every band; codewords are the inverse lookup `(len, code)[delta + LAV]` of the in-tree ISO tables; out-of-table delta = error |
| `sbr_single_channel_element()` / uncoupled `sbr_channel_pair_element()` | `write_sbr_data` | Table 4.66 order (grids, dtdfs, invfs, envelopes, noises, harmonics); `bs_data_extra = 0`, `bs_coupling = 0`, `bs_extended_data = 0`; coupled pairs are not written |
| `sbr_extension_data()` | `write_sbr_extension` | `bs_header_flag`, optional header, element; returns `num_sbr_bits`; **no** `EXT_SBR_DATA_CRC` (the in-tree decoder and libavcodec only skip `bs_sbr_crc_bits`) |
| `extension_payload(cnt)` | `sbr_extension_payload` | type nibble `0b1101`, extension, zero `bs_fill_bits` to the byte; `cnt` = byte length ≤ 269 |
| `fill_element()` | `write_fill_element` | `ID_FIL`, 4-bit count, `esc_count` when ≥ 15 (`cnt = 15 + esc − 1`), payload bytes at any bit offset |

## Evidence (`cargo test --lib enc_sbr_bits`, 13 tests)

Hand-authored bit patterns (not reader round trips):

- v1 header at 48 kHz = 16 bits `1101010000000000` (`D4 00`); a
  header with both extra blocks = 27 bits in Table 4.63 order.
- FIXFIX grids = 5 bits (`00001`, `00010`, `00100`); FIXVAR / VARFIX /
  VARVAR vectors (the same values as the `sbr_grid` parser tests)
  assemble to 15 / 12 / 19 bits and parse back equal.
- Minimal SCE (NHigh 12, NQ 2, start 33 / 10, zero deltas) = 49 bits
  `0 00001 0 0 0000 0100001 00×11 01010 0 0 0` using
  `f_huffman_env_1_5dB(0) = "00"` and `f_huffman_env_3_0dB(0) = "0"`.
- Non-zero deltas use the transcribed codes: `t_huffman_env_3_0dB`
  −1 `10`, +1 `110`, +2 `11110`; `f_huffman_env_3_0dB` −2 `1110`;
  `t_huffman_noise_3_0dB` −1 `110`, +2 `11110`.
- `fill_element` with 9 bytes after a 3-bit prefix = `110 1001 …`; with
  40 bytes = `110 1111 00011010` (`esc = 26`); the parser's
  `fill_count` returns 9 / 40; 0 and 270 bytes are errors.

Accounting and round trips:

- Payload for the minimal SCE with header = 4 + 66 bits → 9 bytes, 2
  zero fill bits; `SbrExtensionData::parse(.., cnt = 9, ..)` lands on
  bit 72 and reports `num_sbr_bits = 66`.
- Header reuse (`bs_header_flag = 0`) parses only with a prior header.
- Estimator frames (6 frames, noise + 700 Hz + late 9.1 kHz tone): the
  writer's `num_sbr_bits` equals `1 + header + est_bits` on every frame,
  the in-tree parser returns the identical grid/dtdf/invf/envelope/
  noise, and a second write is byte-identical.
- Uncoupled CPE parses back both channels, including a sinusoidal
  block on one channel only.

Errors: delta 61 at 1.5 dB (LAV 60), start value 128 (7-bit), band
count mismatch, `invf_mode` 4, sinusoidal length ≠ NHigh, 0 or 3
channels, header fields out of range or non-default under a clear
extra flag.

## Second decoder lineage

Only the in-tree parser has consumed these bits. libavcodec needs a
complete AU (LC `raw_data_block` + FIL) and ADTS/M4A framing, which is
TASK-89; its lavc-decoded goldens are where AC #2 of this task is
closed. No claim of external acceptance is made here.

## Not claimed

- No `EXT_SBR_DATA_CRC`, no coupled CPE, no `bs_extended_data` (PS),
  no VARVAR use by the estimator yet (writer supports it).
- Cross-platform determinism is structural (integer bit packing only);
  no aarch64 run this session.
