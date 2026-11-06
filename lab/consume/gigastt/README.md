# Current-model gigastt HTTP qualification

The combined candidate completed local qualification: native PCM parity,
consumer checks, two preregistered six-case timing confirmations on CPU 1,
and the current-model HTTP/load/fault policy. The original CPU 0 scan failure
and raw HTTP equality failure remain in the [audit](../../release-audit/TASK145.md).
This is Linux x86_64 evidence for an unpublished syom 0.10.1 candidate.

The [post-qualification review](../../release-audit/TASK145-REVIEW.md) records
the subsequent tooling refactor. `http_contract.py` holds exact duration
comparisons and all four model/32 input identities. `http_prerequisites.py`
recomputes timing summaries from individual requests and checks recorded RSS,
health and negative responses. Gate flags alone cannot qualify a run. The
maintained load matcher uses the same exact one-sample comparison as reference
qualification. The measured decoder, adapter patch and archived runs are
unchanged; offline replay reproduces every saved policy field and all 448 load
response verdicts.

The 305 MiB model canary recognized all 32 inputs per backend. Baseline/candidate
p95 was 135.08/152.87 ms, p99 171.27/174.60 ms, and peak HWM
1159.82/1161.03 MiB. The separate presentation reference qualified; the raw
equality canary remains false. Load testing completed 224 recognized requests
per backend across seven bursts, exercised 76 admission retries per backend,
and passed all 12 gates. Peak load RSS was 1022.86/1066.72 MiB; steady growth
was 2.05/0.98 MiB. Queue timeout, cancellation/disconnect and watchdog recovery
passed, and all six owned load/fault servers exited cleanly.

The first reference attempt rejected two exact one-sample differences because
of binary subtraction rounding. Its failure is retained. Exact rational
comparison of recorded decimal durations now implements the same inclusive
1/16000-second bound; the original 17 offline policy/parser controls include the actual
boundaries and adjacent floats outside them. No numeric limit was increased.

These offline tools use owned loopback processes and local source/model files.
No model or recording is downloaded or committed. Run them after isolated decoder
latency campaigns finish, using fresh output directories and stable binary copies.

Provenance: the canary is a byte-for-byte copy of the retained 2026-11-04
305 MiB runner (`~/.cache/gigastt/aac-0100/run_http_canary.py`, SHA-256
`3ea6ed22ee203075c1085419cfe55e3f054d8be1825e98df8b80a2b84e078b6f`).
The load runner derives from the adjacent `run_http_load.py`; only reference
validation and backend-specific response lookup changed. Its request schedules,
admission/fault checks and numeric limits are preserved. The two new evidence
tools and offline policy tests are maintained here.

`run_http_canary.py` is the unchanged current-model canary retained from the
2026-11-04 campaign. It still reports ordinary text/duration equality failures;
never edit its result or turn its `pass` flag into a reference admission bypass.

`presentation_evidence.py` measures presentation for all WAV/M4A cases using
source WAV headers, ffprobe, FFmpeg and the actual production probes. It records
both an unbounded FFmpeg decode and a separate decode bounded with `-t` to the
container duration. Counts and PCM hashes identify both outputs, the applied
duration and any full-decode surplus. The bounded PCM must be a byte-identical
prefix of the full decode, which must cover the complete presentation span.
The forced `-t` count is not treated as independent evidence of duration:
source WAV, container and candidate counts must agree separately.
It recognizes the FFmpeg-presented golos_04 through the same candidate binary and
305 MiB model. Raw reference media and recognized text remain in the chosen local
output folder; only reviewed textual/hash evidence belongs in Git.

`http_reference.py` permits load testing only after the independent policy
qualifies all paired responses. Unknown transcript or duration differences fail.
The known golos_04 correction must agree with the human reference, original WAV
and fresh FFmpeg presentation recognition. Other M4A duration differences are
labeled only as excess over independently validated presentation, without
assuming a specific priming/padding mechanism. Native baseline/candidate counts
must explain their observed HTTP durations. All four model files, static ORT,
binaries, corpus inputs and evidence files are bound before load starts.

The human reference and the recording-specific priming/padding diagnosis are
documented in the [pinned consumer report](https://github.com/ekhodzitsky/gigastt/blob/49faef1375dd4bef7f0d4ac9602b4dd68bc51dbf/docs/measurements/aac-20261104/README.md#follow-up-the-changed-transcript-is-correct)
and its [golos_04 evidence](https://github.com/ekhodzitsky/gigastt/blob/49faef1375dd4bef7f0d4ac9602b4dd68bc51dbf/docs/measurements/aac-20261104/golos04-presentation.json).
The present run still requires fresh WAV/FFmpeg recognition; the historical
report supplies provenance, not a replacement for that check.

Two separate, hash-pinned inputs have a movie-clock rounding difference:
`golos_09-aac` contains 53,492 source samples but declares 3.344 seconds
(53,504 samples); `golos_10-aac` contains 62,499 but declares 3.907 seconds
(62,512 samples). The generator parses their actual MP4 boxes. Qualification
requires `mdhd == stts == WAV frames + 1024`, unit-rate edit priming of 1024,
and movie/track/edit durations equal to the exact ceiling of WAV duration in
the 1000 Hz movie clock. It records the 12/13-sample surplus explicitly.
These exceptions require the exact input/WAV hashes and recorded integer
clocks; other cases retain the original source-duration relation. Candidate,
container and bounded FFmpeg PCM still agree within one 16 kHz sample, and the
full FFmpeg decode must cover that span without synthetic padding. This clock
rounding evidence is separate from golos_04's diagnosed baseline priming/tail.

`run_http_load.py` uses each backend's independently qualified response rather
than forcing correct presentation to reproduce an incorrect baseline tail.
It recomputes the policy from the hash-bound evidence: changing a boolean cannot
make a failed reference pass. Timing/RSS limits, admission backoff, pool/watchdog
faults, cancellation and recovery checks retain the original definitions.

Command order (each script has `--help`):

1. Run paired `run_http_canary.py` with the unchanged 32-case manifest.
2. Run `presentation_evidence.py` with those probes/binaries/model and raw canary.
3. Run `http_reference.py --canary CANARY/result.json --independent INDEPENDENT/independent.json --out REFERENCE.json`.
4. Run `run_http_load.py` with the same binaries/model/manifest, `--reference REFERENCE.json`, the existing long fixture and `--upload-admission`.

Before starting, record SHA-256 identities of these scripts, both server/probe
pairs, the manifest, model files and static ORT archive. The model encoder must
be `1d5a6f580b692e38ba35ed98d0f54648808f27b0a147dc11bd8978c15dc1a18f`.
Keep command arguments, exit codes, stdout/stderr and all result files in the
new output folder. A canary exit code of 1 retains the failed equality result;
proceed to load only when the separate reference command succeeds. Unexpected
differences or failed timing/RSS prerequisites must stop qualification.

The original failing canary stays unchanged, even when the separate policy
qualifies expectations for load. Record both outcomes and every unexplained
failure. Offline policy regressions require no model or external programs:

```sh
python3 -m unittest discover -s lab/consume/gigastt -p 'test_*.py'
```

Replay the committed raw canary and independent evidence without models,
servers or recordings. Use a fresh output file; the new reference binds paths
in the current checkout rather than the original machine's absolute paths:

```sh
python3 lab/consume/gigastt/http_reference.py \
  --canary lab/release-audit/runs/task145-20261105/http-candidate5/canary/result.json \
  --independent lab/release-audit/runs/task145-20261105/http-candidate5/independent/independent.json \
  --out target/task145-review/replayed-reference.json
```

## Consumer adapter handoff

The complete [adapter.patch](adapter.patch) targets gigastt commit
`49faef1375dd4bef7f0d4ac9602b4dd68bc51dbf`, pinning unpublished syom **0.10.1**
to Git revision `3e540231c9d71e3534be1ef856a91cf419efaead`.
It includes nine files: the consumer manifest/lockfile, `aac.rs`, `aac/input.rs`,
`aac/probe.rs`, and the new `aac/pull.rs`, `aac/buffered.rs`, `aac/adts_pull.rs`
and `aac/adts_pull_tests.rs` modules. The
[adapter manifest](adapter-manifest.json) records the patch/file hashes,
successful apply check and byte-identical applied files. See the
[TASK-145 audit](../../release-audit/TASK145.md) for qualification and limits.
The current nine-file patch SHA-256 is
`82467a7a14cd41cd8b416e53e94fc49b133b94f09a4f266d49650e3daee24968`;
it was independently reapplied to the exact base and all nine files matched.

Verify the complete patch against that exact consumer base. The default command
reads immutable base blobs and applies the patch in a temporary directory;
the consumer's working files are untouched. It checks patch SHA/size, the full
changed-file set and all resulting file hashes, rather than trusting historical
`apply_check` flags in the manifest:

```sh
python3 lab/consume/gigastt/check_handoff.py --consumer /path/to/gigastt-at-49faef1
```

Add `--apply` to apply the verified bytes to the consumer. That mode also
requires a clean tracked and untracked working tree and rechecks its base.

This handoff does not publish a crate, switch the default backend, or merge the
downstream PR. Those actions remain separate from local qualification.
