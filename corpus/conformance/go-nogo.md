# Syntax inventory: current status and archived decisions

Status review: **2026-11-02**, unpublished 0.9.0 candidate.
**No conformance certificate** is claimed. ISO/IEC 14496-26 bitstreams
remain unobtained. This page separates the original TASK-17 motivations
from current authored regressions; neither source review nor a passing
regression establishes complete profile coverage.

## Supersession map

The [API guide](../../src/guide.rs) is the current contract. Test names
below identify source evidence reviewed for this update. Local suite reruns
are recorded with the documentation corrections; external oracle comparisons
and normative-vector acquisition were not repeated.

| Original finding | Current implementation evidence | What this does not establish |
|---|---|---|
| TASK-26: explicit HE ASC and rate fields | [asc_tests](../../src/engine/asc_tests.rs): `explicit_sbr_two_rate_24_48`, `explicit_ps_two_rate_24_48`, `one_rate_explicit_sbr_is_not_amd2`; [he_config_tests](../../src/engine/he_config_tests.rs) covers declared 1x/2x rates and cross-container rate agreement | An arbitrary HE/PS stream or downsampled-SBR corpus certificate |
| TASK-27/32: ASC PCE and declaration validation | [asc_tests](../../src/engine/asc_tests.rs): `embedded_pce_rejects_object_type_and_rate_and_dup_tag`; [channel_map_tests](../../src/engine/channel_map_tests.rs): `asc_embedded_pce_seeds_decoder_without_in_band_pce`, `pce_decode_rejects_inconsistent_elements` | Complete PCE/layout conformance or natural PCE content |
| TASK-28: multiple ADTS raw data blocks | [adts_tests](../../src/engine/adts_tests.rs): `two_rdb_sine_delivers_two_core_frames`, `two_rdb_truncated_is_unexpected_end` | Every multi-block transport or CRC layout |
| TASK-29: ADTS CRC | [adts_crc_tests](../../src/engine/adts_crc_tests.rs): `valid_protected_single_block_matches_crc_free_pcm`, `multi_block_header_and_per_block_crc`, corruption/truncation cases | An official protected-stream corpus or independent validation of all normative CRC coverage rules |
| TASK-30/31: latmGetValue and subframes | [latm_tests](../../src/engine/latm_tests.rs): `mux_v1_latmgetvalue_parses_lc_48k`, `mux_cfg_stores_num_sub_frames`; [stream_latm_tests](../../src/stream_latm_tests.rs): `two_subframes_double_samples_and_chunked_agrees`, `two_subframes_missing_second_payload_is_error` | Every LATM multiplex configuration or external broadcast corpus |
| TASK-34/35/36: CCE fence and reconstruction | [cce_tests](../../src/engine/cce_tests.rs): `public_decode_dependent_cce_succeeds`, `short_window_unity_matches_audible`, `independent_second_frame_keeps_cce_overlap`; [skip_tests](../../src/engine/skip_tests.rs): `public_decode_independent_cce_missing_target_is_error` | Independent CCE quality certification across all tool combinations |
| TASK-63 and later LD480 scope | [frame_grid_tests](../../src/frame_grid_tests.rs): `lc960_loas_m4a_and_raw_match_the_lab_geometry`, `aac_ld_480_decodes_one_frame_and_is_not_a_960_error`, `he_over_960_is_not_the_lc_success_path` | Independent LD480 waveform qualification, HE960 support, or 960/480 encode |
| TASK-62: 7.1 layout | [channel_map_71_tests](../../src/engine/channel_map_71_tests.rs): `default_config_7_maps_to_lavc_7_1_order_with_labels`, `pce_bitstream_7_1_decodes_in_declaration_order` | All PCE content or configurations 8–15 (unsupported) |

See [coverage.md](../coverage.md) for implementation versus registered
fixture distinctions and [LD qualification](../../lab/profiles/LD_QUALIFY.md)
for the separate 512-sample FDK/libxaac evidence with PNS exclusions.
The remaining acquisition limits below still apply. Original wording about
“today”, ignored fields, rejected AOT5 and unsupported 960/480 describes
the archived state, not current known defects.

## Archived TASK-17 go / no-go

The original decision inventory follows unchanged. Preserve its rationale
and negative results; use the supersession map above for current status.

No conformance certificate is claimed. ISO/IEC 14496-26:2024 bitstreams
were not obtained.

### Can proceed with established evidence (no ISO purchase)

These S1 parser tasks have independently authored vectors and at least one
non-syom syntax source. They must not treat a self-roundtrip as proof.

| Task | Go? | Evidence | Still not a certificate |
|---|---|---|---|
| TASK-26 HE ASC | **go** | Amd 2/9 public Table 1.13 + FFmpeg n7 `mpeg4audio.c`: explicit AOT 5/29 has **two** rate fields; implicit `0x2b7` then `extensionSamplingFrequencyIndex`; `0x548` PS. syom one-rate explicit form and 2× core output are the bugs. | Full 14496-3 semantics (downsampled SBR corner cases) still unpaid |
| TASK-27 ASC PCE | **go** | FAAD2 `GASpecificConfig` reads `program_config_element` when `channelConfiguration==0`. Vector `asc-lc-ch0-embedded-pce`. | Official PCE conformance files unpaid |
| TASK-28 ADTS multi-block | **go** | Public ADTS `number_of_raw_data_blocks_in_frame` (N−1). Header parse already stores the count; decode ignores it (`_num_raw_data_blocks`). | Payload split inside a multi-RDB frame may need 13818-7 for `raw_data_block_position` |
| TASK-29 ADTS CRC | **go for single-block identity** | 16 bits after the 7-byte header when `protection_absent=0`; CRC-16 poly from 11172-3 §2.4.3.1. | **no-go for multi-block CRC coverage** until 13818-7 `adts_header_error_check` is in hand or a second independent ADTS CRC implementation is compared |
| TASK-30 LATM lengths | **go** | FFmpeg n7 `latm_get_value`: 2-bit `bytesForValue` then `(n+1)*8` bits. FDK v2.0.3 and FAAD2 match that, not syom’s 8-bit+escape `latm_value`. | Full §1.7 unpaid; the 2-bit form is enough to fail today’s parser |
| TASK-31 LATM subframes | **go** | Same StreamMuxConfig sources: `numSubFrames` is 6 bits and is ignored after the read. | Delivery of N payloads still needs a constructed multi-subframe frame |
| TASK-32 PCE vs elements | **go** | `parse_pce` already returns lists; object/rate fields are dropped. Authored PCE body is enough to start mismatch tests. | |
| TASK-34 CCE fence | **go for parse/fence** | FAAD2 Table 4.4.8 field order disagrees with `skip_cce`. Fence (error or documented skip of a complete element) can start. | Reconstruction is TASK-35/36, not this inventory |
| TASK-46 bit limits | **go for a documented LC 6144 bits/channel cap** | Public decoder comments (FAAD2 ER path, common AAC literature). | Exact per-profile tables need full 14496-3; do not invent other numbers |
| TASK-63 960-frame | **go as keep-unsupported** unless a later product decision | `frameLengthFlag==1` already `UnsupportedFrameLength`. | Implementing 960 needs SWB/IMDCT work, not more ISO text for the reject path |

### Need a narrower experiment or unpaid ISO

| Cell | Why |
|---|---|
| ISO 14496-26 pass/fail | Paid bitstreams with redistribution rights. TASK-103 stays unqualified without them. |
| ER AAC LD (AOT 23) 14496-26 | **Not obtained.** Decode qualification is the committed FDK v2.0.3 and libxaac 0.1.13 LOAS oracles in `lab/profiles/LD_QUALIFY.md` (TASK-131), not a conformance certificate. lavc 7.0.2 rejects these streams. The 480-sample grid stays unsupported. |
| ADTS multi-block CRC positions | Need 13818-7 or a measured FFmpeg/FAAD2 CRC over known multi-RDB frames. |
| CCE reconstruction quality | Syntax fence first; coupling math is a separate experiment. |
| Encoder 6144 vs 8184-byte ADTS cap | Measure current encoder against the 6144-bit rule before changing the rate loop. |
| M4A presentation end / multiple `elst` | ISOBMFF, not 14496-26. TASK-43 can use authored `elst` boxes. |
| F18 error/lifecycle | Product contract, not a syntax clause. |
| HE/PS **decode quality** vs syntax | Goldens vs lavc remain; syntax fixes in TASK-26 must not remint goldens to hide PCM drift. |

### Other DOC-1 findings (not syntax-certificate cells)

| Finding | Disposition |
|---|---|
| F14 HE/PS per-element state | Follow TASK-38/39 after TASK-26. This inventory does not prove multi-element SBR. |
| F15 encoder priming/tail | Follow TASK-40/41. Impulse/short-input experiment, not 14496-26. |
| F17 M4A u32 sizes/offsets | Follow TASK-44 using ISOBMFF box-size rules, not AAC payload clauses. |
| F18 error/lifecycle | Product contract (TASK-45/51), not a bitstream clause. |

### Explicit non-claims

- Committed HE goldens use **implicit** SBR (`AOT=2` + `0x2b7`, core 24 kHz, extension 48 kHz). That is why they decode today even though explicit two-rate AOT 5 is rejected (`UnsupportedAot` on the Amd 2 vector).
- `ics_tests::asc_heaac_aot5_unwraps_lc_core` is a **local** one-rate layout, not an independent conformance vector.
- LATM goldens that parse today are consistent with `audioMuxVersion=0` (no `latmGetValue`).
- Decoder consensus is not a substitute for 14496-26. Disagreement is resolved toward the Amd 2/9 field order and the FFmpeg/FAAD2/FDK `latmGetValue`, not a majority of syom tests.
