# Frozen candidate for qualification (TASK-108 AC #3)

Frozen 2026-09-15 before any held-out listening or peer comparison.

| Item | Value |
|---|---|
| Engine revision | `b5660e9` (TASK-113 allocation) + the TASK-108 preset commit on top (presets only add `EncodeOptions` constructors; encoder bytes unchanged — the goldens `enc48{,m,t,l}` / `he48e{,m}` are the byte tripwire) |
| Toolchain | rustc 1.97.1, cargo release profile, one thread |
| Oracle | ffmpeg 7.0.2-static (johnvansickle) for lavc decodes; `corpus/oracles/provenance.json` |
| LC default | ADTS, 128 kbps ABR, causal block switching, KBD, TNS on long frames, per-band M/S, noise-to-mask allocation (18 dB SMR, water level), no ATH / tonality / PNS / IS / grouping / refine / lookahead |
| `high_quality()` | same, 192 kbps |
| `low_rate()` | HE v1, 48 kbps whole-stream, crossover 6.75 kHz at 24 kbps/ch, header every AU, no PS |
| Corpus split | development = `lab/quality` synth clips + `src/goldens/lecture.m4a`; holdout = `corpus/manifest.json` `split = holdout` rows and the TASK-14 listening items — **not encoded, not measured, not tuned against** |
| Gates carried into TASK-95 / TASK-109 | LC lavc tolerance ≤ 2 LSB s16 / ≥ 55 dB on goldens; HE HF energy ≥ LC + 6 dB and within 3 dB of source (6.75–15.4 kHz) at 24/32/48 kbps; ABR payload/valid ±3 % on ≥ 10 s; listening protocol and statistics preregistered in `doc-5` (MUSHRA / BS.1116, no post-hoc metric choice) |
| Unmet targets | white noise at ≤ 48 kbps stereo LC loses level (water level drops quiet bands; PNS opt-in is the tool); HE on speech-only material trails LC at the same rate; click at 128 kbps trails libavcodec (17.0 vs 33.0 dB, REPORT.md) |

Changing any row re-opens this freeze.
