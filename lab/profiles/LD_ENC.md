# AAC-LD encoder architecture (TASK-132)

Recorded 2026-09-26. Host: Linux x86_64, rustc 1.97.1. This is an
architecture spike, not an encoder. Product `encode` stays AAC-LC.
Ordinary `cargo test` does not spawn ffmpeg or FDK.

Peer numbers are the TASK-97 FDK v2.0.3 cells in `REPORT.md` (synthetic
speech and music, 48 kHz, SNR after alignment). They are screening, not
a listening result. Decode qualification of those bitstreams is
`LD_QUALIFY.md`.

## Decision

**Go opt-in AAC-LD encode** on the 512-sample grid, staged, after the
shipped decoder. **No-go as a 0.x default** (`encode` /
`EncodeOptions::default` stay LC 128 kbps causal). **No-go ADTS** (the
header cannot signal AOT 23; FDK rejects it too). **No-go the 480-sample grid** (`frameLengthFlag` stays `Unsupported`). **No-go ELD, LD-SBR and USAC.** **No-go lookahead** on this path: one extra frame would make the
delay 1024 and erase the only measured reason to build it. **No-go a
quality or listening claim.** **No-go copying the FDK encoder.** FDK and
libxaac are oracles.

Cancel the epic if the forward transform cannot round-trip an impulse
through the shipped decoder at a stable 512-sample delay with
deterministic bytes, or if both FDK and libxaac reject the bitstream.
A request for 480-sample frames or ELD is a new spike, not a silent
widening of this one.

## Why (delay, not a better waveform)

FDK LD at 48 kHz reports encoder `nDelay` 512, decoder `outputDelay` 0,
measured roundtrip lag 512 samples (10.7 ms). syom's causal LC encoder
delays one 1024-sample frame. FDK LC on the same host lagged 3792
samples. That gap is the product reason.

FDK LD SNR on the synthetic cells is ordinary, not a win over LC:

| cell | frame | enc delay | dec delay | lag | SNR dB |
|---|---:|---:|---:|---:|---:|
| LD 64k mono speech | 512 | 512 | 0 | 512 | 18.3 |
| LD 48k mono speech | 512 | 512 | 0 | 512 | 16.7 |
| LD 32k mono speech | 512 | 512 | 0 | 512 | 14.3 |
| LD 64k stereo speech | 512 | 512 | 0 | 512 | 13.8 / 14.0 |
| LD 96k stereo speech | 512 | 512 | 0 | 512 | 17.0 / 17.0 |
| LD 128k stereo speech | 512 | 512 | 0 | 512 | 18.4 / 18.6 |
| LC 64k mono speech (anchor) | 1024 | 2048 | 1744 | 3792 | 19.0 |

`REPORT.md` calls LD quality at equal rate about the same as LC on these
fixtures. Do not retune toward beating that SNR. The gate is a legal
bitstream, the 512-sample delay, and decode agreement.

## What already exists

| Piece | Where | Encode still needs |
|---|---|---|
| 512-line SWB tables | `swb.rs` `long_offsets_ld` | point the psy bands at them |
| Sine and low-overlap windows | `filterbank.rs` (decode synthesis) | the matching analysis window |
| Inverse MDCT of 512 lines | `imdct.rs` `PLAN_1024` | a forward plan; `mdct_into_f32` only has fast paths for 2048→1024 and 256→128 |
| TNS band limits for LD | `tns.rs` `MAX_BANDS_LD` | the encoder's LPC span on that table |
| Quantizer, Huffman, rate loop | `enc_quant`, `enc_frame` | frame length 512, not 1024; sequence forced to `ONLY_LONG` |
| LOAS writer | `latm_write.rs` `loas_frame_into` | an AOT 23 ASC; the writer already copies ASC bits |
| M4A writer | `m4a_write.rs` `mux_aac` | `frame_len` 512 and priming 512; the writer already takes both |
| Decoder oracle | TASK-129–131 | FDK and libxaac stay the external pair; lavc cannot decode LD |

## Scope (v1)

| Axis | Supported | Reject |
|---|---|---|
| Profile | ER AAC LD, AOT 23, 512-sample frames | 480-sample grid, ELD, USAC, HE |
| Window | `ONLY_LONG` only. Sine by default. Low-overlap is the attack shape, not an eight-short sequence | LC LongStart / EightShort / LongStop |
| Rates | The rates `long_offsets_ld` already accepts | anything else; no custom 24-bit rate in v1 |
| Channels | 1 or 2 | 3.0–7.1 encode (decode of those configs stays) |
| Tools | TNS on the LD band limit, order ≤ 12. No PNS in v1 | intensity, PNS, SBR, PS |
| Transport | LOAS and M4A, plus raw AU + ASC for a push encoder | ADTS |
| Default | off | `encode()` must not emit LD |
| Lookahead | off, and not offered | a mode that adds a frame of delay |

## Signal path

```
PCM, 1–2 ch, [-1, 1], a rate long_offsets_ld accepts
  └─ one 1024-sample analysis window (sine, or low-overlap on the right
     half when the existing attack detector fires)
       └─ forward MDCT → 512 coefficients
            └─ LC psy / quantizer / Huffman / TNS on the LD scale-factor bands
                 └─ ONLY_LONG ics_info, no eight-short groups
  mux: one raw_data_block per 512 samples
       LOAS via loas_frame_into, or M4A via mux_aac(frame_len=512, priming=512)
```

The LC block-switch state machine is not reused as a sequence. It can
only vote for the window-shape bit. Overlap on the analysis side is one
frame, matching the decoder.

## Delay

1. Encoder delay is one 512-sample frame. An impulse at sample 0 of the
   source comes out at sample 512 of a decode, same as the FDK cell.
2. Decoder delay stays 0 (`LD_QUALIFY.md`).
3. M4A `elst.media_time` is 512, not the LC 1024. Presentation length is
   the source length. LOAS has no trim; the decoded length includes the
   512-sample priming, and the caller is told.
4. `Encoder::finish` zero-pads the last partial frame of 512, as LC does
   for 1024.

## Bit budget

`bitrate_bps` stays the whole-stream ABR target (decision-8). The per-frame
budget is `bitrate_bps * 512 / sample_rate`, not `* 1024`. Keep the
existing 6144 bits/channel ceiling as a hard stop. Do not invent a second
LD cap. No bit reservoir. Silence is not stuffed. LOAS `frameLengthType` 0,
the same writer as LC.

## Profile signalling

| Container | Wire | Why |
|---|---|---|
| LOAS | ASC AOT 23, 512-sample GA config, `epConfig` 0, config in every frame | ADTS cannot carry AOT 23 |
| M4A | `mp4a` + that ASC, `stts` delta 512, `elst` priming 512 | `mux_aac` already takes the frame length |
| Push raw | `Encoder::asc()` is the LD ASC; AUs are LD `raw_data_block`s | same seam as LC raw |

A wrong rate, channel count, or an ADTS request is `AacError::Encode` or
`Unsupported` before any byte is written.

## Quality targets

Not waveform superiority. Screening only, on the synthetic cells already
used for the FDK table, decoded by FDK and by libxaac, then by syom:

| Check | Gate |
|---|---|
| Both external decoders accept every AU | no decode error |
| syom decode vs FDK decode, frames without PNS | ≤ 2 LSB and ≥ 55 dB (the TASK-131 rule) |
| Impulse lag at 48 kHz | 512 samples, not 1024 and not 0 |
| Bytes | identical across the scalar and SIMD forward-MDCT paths |

PNS stays off, so the non-normative noise generator is not in the
comparison. There is no PEAQ, ViSQOL, or listening gate in this epic.

## Staged interfaces

No public API in any stage before the last. Each stage is one task.

1. **Transform.** Done. `mdct.rs` `PLAN_1024` (1024 windowed samples →
   512 coefficients, `det_math` twiddles, no libm). The sine analysis
   window is `filterbank::ld_analysis_window`, test-gated until stage 2
   calls it. An impulse at source sample 0 comes out at decode sample 512
   through the shipped LD filterbank (`mdct_tests`). No quantizer and no
   public API.
2. **Mono bitstream.** Done for sine windows. `enc_ld::LdEncoder`
   writes one ER access unit per 512 samples (no element id, no
   `ID_END`), using the LC quantizer, Huffman, psy and TNS on
   `long_offsets_ld`. A 440 Hz sine decodes in syom at ≥ 20 dB SNR
   after the 512-sample delay. LOAS (`push_loas`) and M4A (`mux_aac`,
   priming 512) carry that AU. Still no public API, no rate search,
   no stereo, no low-overlap shape.
3. **Rate, stereo, attack shape.** Done. The frame budget is
   `bitrate_bps * 512 / sample_rate`, never above 6144 bits/channel;
   a frame that still exceeds that ceiling is an error. A zero bitrate
   keeps the psy target under the same ceiling. Low-overlap
   (`window_shape` 1) when the attack detector fires. Stereo is one CPE
   with `common_window` and `ms_mask_present` 0 (no M/S). LOAS and M4A
   (priming 512) carry both layouts. Still not a public API and still
   not the default `encode()`.
4. **Qualification.** Done. FDK v2.0.3 and libxaac 0.1.13 accept the
   LOAS from this encoder (mono 64 kbps sine, stereo 128 kbps, 32 kbps
   noise, a low-overlap frame). syom's decode matches both within 2 LSB
   and 55 dB on the committed tones `src/goldens/ldenc64.loas` and
   `ldenc128s.loas` (and their `.fdk.s16` / `.xaac.s16`). A 32-sample
   burst peaks one frame later; a one-sample impulse is below the
   quantizer and is not the lag measurement. Sine correlation against
   the source peaks at lag 512 for both syom and FDK. Public opt-in is
   `EncodeOptions::with_ld` (LOAS or M4A). `encode()` stays LC. ADTS
   is rejected.

## Forbidden

Copying FDK, libxaac, or libavcodec encoder source. ADTS. 480-sample
frames. ELD. A default change. A listening or superiority sentence.
