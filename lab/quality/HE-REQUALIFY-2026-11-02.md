# HE development comparison, 0.9.0, 2026-11-02

This is a refreshed diagnostic slice, not a passed SOTA gate or a blind
quality study. Product codec sources are unchanged from published 0.9.0
(base checkout `67a9c05`; release source `1bb8152`). The scorer changes
and all source hashes are retained in the [run bundle](runs/he-requalify-20261102/report.json).
No holdout recordings were opened or measured.

## Corrections to the historical interpretation

`HF err` is mean absolute band-level error: **lower is better**. LF is
magnitude-spectrum SNR: higher is better. The lecture rows did not show
a syom HF deficit; the speech2 rows did. The original text and trial in
[CORE_TUNE](CORE_TUNE.md) and the [October clarification](HF-GAP-2026-10-16.md)
remain historical evidence, not fresh comparisons. Do not tune toward
larger HF error or use the task's historical lecture-gap title as a metric.

The local FDK executable predated the complete-drain adapter fix. This
run rebuilt it against the pinned FDK 2.0.3 static library before measuring.
The library SHA-256 matches the retained
[native build manifest](../baseline/peer_speed/runs/lc-encode-20261101/native-manifest.json).
The adapter drains to `AACENC_ENCODE_EOF`. Old short-input FDK byte counts
and overlap-only scores must not be copied into a refreshed scorecard.

The speech2 lane now rejects any missing reference prefix/tail. It reports
source, decoded and scored sample counts per channel. Waveform scores cover
all 96,000 speech2 samples or all 12,000 lecture samples; spectral scores
retain the historical complete 2048-sample FFT blocks (46 or 5 blocks),
excluding the final partial block for both engines.

## Observations

Linux x86_64, Ryzen AI 9 HX 370, rustc 1.97.1, release profile, CPU 0;
neutral decoder ffmpeg 7.0.2-static, FDK afterburner off. No timing was
measured. Same requested 24 kbps; `actual` below means complete ADTS bytes
including transport, priming and padding divided by source duration.
It is not elementary-stream bitrate and is not a matched-rate quality gate.

| Development clip | syom / FDK actual kbps | syom / FDK LF SNR ↑ | syom / FDK HF absolute error ↓ |
|---|---:|---:|---:|
| speech2 stereo, 2 s | 26.7 / 27.4 | 15.5 / 5.5 | 22.2 / 17.2 |
| speech2 mean mono, 2 s | 26.6 / 26.1 | 22.0 / 16.9 | 17.5 / 6.6 |
| lecture stereo, 0.25 s | 39.0 / 38.3 | 36.6 / 34.9 | 17.6 / 22.4 |
| lecture mean mono, 0.25 s | 37.9 / 38.6 | 37.8 / 33.3 | 21.5 / 25.7 |

The speech2 HF weakness persists in this diagnostic; lecture has lower HF
error for syom. These are development fixtures, two of them synthetic.
Neither SNR nor band-level error establishes perceived quality. Same-rate
comparison, independent natural inputs and blinded listening remain open.

The four mono ADTS streams are probed as HE-AACv2, 48 kHz, two channels by
ffmpeg's implicit-PS detection. Their decoded channels are identical. The
scorer validates equality before collapsing dual mono to one channel; this
does not establish that the encoded stream contains PS. Original probe
fields remain in the bundle. All eight streams probe at 48 kHz.

## Reproduce locally

Build the pinned prefix as described in [FDK PIN](../fdk/PIN.md), then:

```sh
python3 lab/quality/requalify_he.py --out target/he-requalify-local
CARGO_TARGET_DIR="$PWD/target" cargo test --locked --offline \
  --manifest-path lab/quality/Cargo.toml --bin syom_tune133 complete_alignment
cargo fmt --manifest-path lab/quality/Cargo.toml --check
python3 scripts/verify_corpus.py
python3 scripts/verify_corpus.py --require-holdouts
python3 -m unittest discover -s scripts -p test_verify_corpus.py
```

The runner rebuilds both encoders' lab executables, captures commands and
logs, records source/binary/static-library hashes and stream probes, and
preserves old reports by requiring a new output directory. Run it serially:
the inherited scorer uses a shared temporary directory. Audio remains in
that temporary directory; only hashes/lengths and textual evidence are saved.

The strict holdout command currently **must fail**: eight recording groups
are registered but zero files have acquired, hash-verified bytes. Default
verification checks the inventory and goldens without pretending the natural
corpus exists. Byte readiness alone does not validate licenses, provenance,
PCM format/duration, stimuli rendering or study readiness. Exact source files
and excerpt selection need a documented acquisition specification before the
new candidate is frozen and holdout outputs are opened. The manifest generator
currently resets acquisition fields; preserve acquisition metadata separately.

This slice leaves TASK-110, TASK-112.1, TASK-112.4 and TASK-120 open. Next
measurement work is matched elementary-stream bitrate on longer development
inputs, followed by an independently selected natural corpus. Do not tune on
the registered holdout recordings. Apple and other physical platforms were
not tested here; no new release was published and no hosted CI was dispatched.

## Local validation

All seven AGENTS.md checks passed on Linux x86_64: formatting, source size,
changelog, clippy, 1,168 library tests (seven existing ignores), two integration
tests, 29 doctests and the package check. The 12 Python readiness regressions
and three isolated full-reference alignment tests also passed. See
[check results](runs/he-requalify-20261102/checks.json) and
[actual readiness](runs/he-requalify-20261102/corpus-readiness.json).
The reproduction runner was additionally exercised with an inherited custom
`TMPDIR` and checked for refusal to overwrite an existing report. Its effective
temporary directory and exact eight-stream artifact coverage are recorded.
