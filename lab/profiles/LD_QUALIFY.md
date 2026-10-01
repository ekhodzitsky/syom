# TASK-131 — AAC-LD decode qualification

Recorded 2026-09-26. Host: Linux x86_64, rustc 1.97.1. Oracles are offline
and are not spawned by `cargo test`. Pins: FDK v2.0.3 and libxaac 0.1.13
(`PIN.md`). libavcodec 7.0.2 rejects these FDK LOAS streams
(`REPORT.md`); it is not an oracle here.

DOC-3 gate for a deterministic fixture: ≤ 2 LSB s16 and ≥ 55 dB against
the reference PCM, same length and rate. PNS is non-normative. syom uses
the libavcodec LCG; FDK and libxaac use a different fixed-point scale.
A noise frame and the following overlap frame are excluded from the
2-LSB gate. They are not a waveform-identity failure.

## Oracle matrix

189 frames of 512 samples, 48 kHz, decoder `outputDelay` 0. Measured by
`decode_ld_tests` on the committed goldens.

| fixture | what | vs FDK v2.0.3 | vs libxaac 0.1.13 |
|---|---|---|---|
| `ld64m.loas` | mono music, 64 kbps | max 2 LSB, 86.0 dB, whole file | max 81 LSB, 68.7 dB (fixed-point peak; SNR on the gate) |
| `ld64mus.loas` | stereo music, low-overlap + TNS + M/S | both planes max 1 LSB, 84.5 dB | no committed PCM |
| `ld48.loas` | mono, PNS on many frames | whole file 28.9 dB / 2352 LSB (noise sequence). 59 frames that are not `NOISE_HCB` and not the next overlap frame: 30208 samples, max 2 LSB, 85.5 dB | no committed PCM |

One-shot and chunked LOAS decode of `ld64mus` are the same PCM
(`ld_loas_chunked_feed_matches_one_shot`). M4A trim is TASK-130.

## Delay

TASK-97 FDK cells: encoder `nDelay` 512, decoder `outputDelay` 0, measured
roundtrip lag 512 samples at 48 kHz. This qualification measured the
decoder against FDK bitstreams, not a syom encoder lag. The decoder check
is alignment with the FDK decode of the same bitstream:

- All three fixtures have the same sample count as the FDK PCM, a multiple
  of 512. No frame is dropped or inserted.
- The first 512 samples of `ld64m` match FDK within the 2-LSB gate.
- Shifting syom's `ld64m` PCM by one frame does not.

That is `outputDelay` 0. A 512-sample decoder delay would fail the first
frame and the shift check.

## Hostile input

Ordinary tests (`hostile_campaign_tests`): truncated LD LOAS prefixes
fail without panic; 64 bit-flips of `ld64m.loas` under the speech cap
either error or return finite PCM of at most 200 000 samples per plane;
a 480-sample `frameLengthFlag` is `Unsupported(FrameLength960)`; an
eight-short window in an LD access unit is `Malformed`, and the instance
stays failed.

`lab/fuzz` seeds now include `ld48.loas`, `ld64m.loas` and `ld64mus.loas`
(parser campaign and the stateful campaign). Parser run, release, seed
`0xC0FFEE42`, 24 seeds: 46817 iterations in 20.0 s, 0 panics
(`cargo run --release --manifest-path lab/fuzz/Cargo.toml --bin syom_fuzz -- --seconds 20 --iters 2000000`).
Stateful campaign with `ld64m.loas` in the seed list: 498170 iterations,
15.0 s, 0 panics (`syom_fuzz_state --seconds 15`).

## Not claimed

- No ISO/IEC 14496-26 ER AAC LD conformance bitstream is in the tree
  (`corpus/conformance/go-nogo.md`). This matrix is not a certificate.
- libavcodec cannot decode the fixtures. Do not cite it as an LD oracle.
- 480-sample LD, ELD and USAC stay unsupported.
- This file qualifies the decoder, not the encoder. Encoder
  acceptance is `LD_ENC.md` stage 4 (`EncodeOptions::with_ld`).
