# syom 0.8.1 consumer qualification candidate

The later [paired HTTP ASR canary](HTTP-CANARY-0.8.1.md) also passes
32 speech-format cases against syom 0.7.1, with model inference,
Prometheus counters and exercised startup/stop handling.

Prepared locally on 2026-10-28, based on `997bf56` plus the recorded
working-tree changes. Version and changelog are 0.8.1; the verified crate
is `target/package/syom-0.8.1.crate`. This is an untagged candidate, not a
published release. [Source hashes and checks](runs/candidate-0.8.1/) pin
the tested sources separately from the original 0.8.0 audit.

**Go for the recorded decode and resample canary.** All 19 local cases
pass independent decoder comparison, and the actual gigastt audio adapter
produces identical flat and streamed output on all of them. The candidate
also fixes a real PNS seeking defect discovered by this qualification.
Full ASR, HTTP rollout, mobile runtime and encoder listening qualification
are separate from these decode results.

## Inputs and independent decoder results

The local kover job directories contain four byte-identical copies of one
408-second AAC-LC stereo recording (48 kHz) inside a 57.7 MB MP4. They count
as **one** recording. Additional inputs are 15 existing Golos speech WAV
fixtures from gigastt, encoded independently with FFmpeg AAC-LC at 64 kbps.
The kover recording also supplies a lossless ADTS remux and 30-second
HE v1/v2 excerpts encoded by FDK 2.0.3. There are **19 cases / 16 source
recordings**, not 19 independent recordings or speakers.

Private media stays in ignored `target/`; the committed evidence contains
only hashes, generation/probe metadata, scores and timings. No recording
was downloaded. FFmpeg 7.0.2-static is the independent decoder; its binary
hash and the FDK adapter identity are in `runs/candidate-0.8.1/sources.json`.

| Cases | Result versus independent decode |
|---|---|
| Original 408 s stereo LC MP4 and lossless ADTS | equal shape; about 131.0 dB SNR; max difference 0.015625 s16 LSB |
| Natural HE v1 excerpt | equal shape; about 130.6 dB; max difference below 0.008 LSB |
| Natural HE v2 excerpt | equal shape; about 97.8 dB; max difference below 1.483 LSB |
| 15 speech LC M4A files | presentation-aligned length; 130.7–131.8 dB; max difference below 0.012 LSB |

The pass gate is equal rate/channels/presentation length and at most
2 s16 LSB peak error. On several M4As FFmpeg emits a padded final AAC
frame beyond the container's presentation duration. The comparator trims
that reference only when syom's length equals the independent ffprobe
`duration_ts * time_base * rate`. Raw and compared reference lengths remain
visible in every row; mismatched lengths are not silently treated as
codec agreement. These are decoder waveform metrics, not a listening
study or a claim about encoder quality.

[All 19 comparisons](runs/candidate-0.8.1/natural-summary.json) and the
per-case raw timing/seek/truncation metadata are preserved under
`runs/candidate-0.8.1/natural/`.

## PNS seek regression and fix

Linear decode of the long recording agreed with FFmpeg, but a seek to
sample 9792000 changed 2048 subsequent sample values. The first mismatch
was at sample 14536512; peak error was about 1236.5 s16 LSB. A small
regression made from the existing `pns48.adts` golden reproduced the
failure without copying the private recording into tests.

Resetting the decoder before a short preroll reset its PNS random seed.
That loses the history needed for later noise-coded bands. The candidate
tracks a constant-size contiguous prefix known to preserve the initial
PNS state. A cold seek, an unverified/noise-coded prefix, implicit SBR,
or a legacy predictive profile replays history from AU 0. Verified
noise-free LC/LD prefixes retain short overlap preroll. No per-frame
checkpoint array or new allocation is introduced.

All five positions in the long recording now reproduce the complete
remaining PCM exactly, as do cold and reused PNS regression sources in
mono and split modes. The tradeoff is visible: on this recording, the
midpoint and near-end seek-to-first-output costs were about 129 and
262 ms including prefix replay. The API docs describe that cost; the
nominal overlap AU count is not a bound on cold/stateful seek work.

## Actual consumer canary

The current gigastt checkout is already modified and still requests
syom 0.7.1. A separate snapshot in `target/consumer-audit/gigastt-source`
uses syom 0.8.1 through a local path override. The shared checkout was
not changed. Audio source hashes and the snapshot's original commit are
in [gigastt-snapshot.json](runs/candidate-0.8.1/gigastt-snapshot.json).

The snapshot compiles the actual `gigastt-core` file-decode adapter and
rubato resampler, not a reproduction of that algorithm. The decode-only
build disables default ORT binary downloading/TLS to avoid unavailable
OpenSSL development headers; std/ndarray/tracing/API features and dynamic
loading remain enabled. No model or inference session is loaded.

The example drives `decode_audio_bytes_shared` and `AudioChunks` with
1-second mono 16 kHz chunks. For all 19 cases, both paths have identical
sample counts and PCM hashes. On the 408-second video both emit 6527999
16 kHz samples; the one-sample difference from 408*16000 is shared by the
consumer's flat and streamed resampler, not a loss in syom's native-rate
output (which has exactly 19584000 samples per channel).

Observed single-core wall times on this shared development host:

| Consumer operation | Median | p95 | p99 |
|---|---:|---:|---:|
| Complete decode plus resample, 101 reps after 4 warmup | 553.4 ms | 603.7 ms | 682.9 ms |
| Pull next 1-second chunk, 408 chunks | 2.26 ms | 4.56 ms | 5.27 ms |

First streamed output took 7.37 ms including setup. Process peak RSS was
64156 KiB for streamed decode and 89420 KiB for flat decode, including
all 57704049 compressed input bytes. The chunk distribution includes
buffered pulls and worker waits; it is not a pure codec kernel timer.
The host also runs unrelated workloads, so these are observations, not
production latency limits or a cross-codec leaderboard. The original
ADTS pass had p99 4664.8 ms during heavy concurrent work; a recorded rerun
after our core checks finished had median 324.5 / p95 464.2 / p99 481.1 ms.
Both distributions remain in the evidence. Host load was still high, so
neither pass establishes an ADTS latency limit. Raw samples are
in [gigastt-stream.json](runs/candidate-0.8.1/gigastt-stream.json) and
[gigastt-flat.json](runs/candidate-0.8.1/gigastt-flat.json).

## Candidate checks and remaining rollout work

- 1125 library tests and 1 integration test passed; 7 lab diagnostics ignored.
- 28 documentation examples, Clippy, formatting, file-size and changelog checks passed.
- Rust 1.89 and nightly ASan with leak detection passed the same library/integration suite.
- The packaged consumer passes all cells, including zero allocations after warmup for tagged/untagged LC ABR and CBR.
- The package builds independently and retains zero runtime dependencies.

The [consumer version patch](gigastt-version.patch) is ready for review;
its application also needs the normal consumer lockfile update once the
candidate is published or bound with a local path. The lab build's ORT
configuration is not part of that production patch. Kover currently uses
a separate legacy syom CLI helper, while this crate ships a library;
upgrading that helper is a separate integration change.

The local decode canary passed. The subsequent HTTP canary uses temporary
local servers and cached models; no production rollout configuration or
endpoint was supplied. Existing default upload limits also reject a 57.7 MB MP4 in
HTTP configurations capped at 50 MiB, even though local file decode works.
A production rollout must use the intended input size limits and monitor
decode failures, length mismatches, latency and RSS in that environment.
Truncated ADTS tails can still decode successfully with fewer samples;
strict upload completeness requires explicit length/framing validation.

## Reproduce the local canary

The ignored `target/consumer-audit/manifest.json` records local input
paths and hashes. From the repository root:

```sh
cargo build --release --locked --manifest-path lab/release-audit/Cargo.toml --bin natural --bin compare_pcm
python3 lab/release-audit/run_natural.py target/consumer-audit/manifest.json target/consumer-audit/rerun --reps 101
# Actual consumer example, compiled in the isolated snapshot:
taskset -c 0 target/consumer-audit/gigastt-build/release/examples/syom_candidate stream INPUT.mp4 OUT.json
taskset -c 0 target/consumer-audit/gigastt-build/release/examples/syom_candidate flat INPUT.mp4 OUT.json 101
```

`gigastt_candidate.rs` contains the decode-only example. The source media
is not bundled, so another checkout must supply its own licensed inputs.
