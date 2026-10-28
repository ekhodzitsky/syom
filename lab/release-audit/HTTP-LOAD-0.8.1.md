# syom 0.8.1 paired HTTP load and fault qualification

Passed locally on 2026-10-28 UTC (2026-10-29 Moscow). Each of the same
gigastt binaries used in the [HTTP canary](HTTP-CANARY-0.8.1.md) processed
**224 measured successful requests** under concurrent load. All 448 responses
match the saved serial reference's normalized transcript hashes and durations.
Both versions recover from queue timeouts, client disconnects/read timeouts,
and the inference watchdog. No installed service or binary was changed.

## Protocol

The runner is [run_http_load.py](run_http_load.py). It verifies all 32 input
hashes against the passing serial canary before starting. Inputs are 15 speech
recordings as WAV/AAC-LC plus HE v1/v2 derivatives. Model/runtime and binary
hashes are recorded; the four cached model files were verified unchanged.
Consumer Rust sources remain identical between binaries, as documented in
the previous canary. Models load offline; HTTP connections use loopback
directly, without environment proxies. Output retains hashes, not transcripts.

Normal load uses RNN-T INT8 CPU, two pool slots, two encoder threads per slot,
punctuation/ITN off, 30-second checkout and 600-second inference watchdog.
After four warmups per binary, seven waves each submit the same 32 inputs
with concurrency **1, 2, 4, 8, 8, 8, 8**. Baseline/candidate wave order
alternates; neither binary runs an inference wave concurrently with the other.
The experiment samples process RSS every 50 ms and verifies model-ready health
and full pool availability after each wave.

Gates were fixed in the runner before requests: every measured response is
200 with the reference's nonempty normalized text and duration within one
16 kHz sample; candidate p95/p99 in every wave <= baseline x1.5; candidate
sampled peak RSS <= baseline x1.25 + 32 MiB; post-wave RSS growth from the
first to any subsequent concurrency-8 wave <= 64 MiB on each binary.

## Normal load results

| Wave / clients | Baseline p95 / p99, ms | Candidate p95 / p99, ms |
|---|---:|---:|
| 1 / 1 | 278.9 / 292.1 | 319.4 / 348.5 |
| 2 / 2 | 355.7 / 378.5 | 307.4 / 322.7 |
| 3 / 4 | 523.1 / 554.7 | 542.6 / 590.1 |
| 4 / 8 | 925.0 / 936.4 | 865.9 / 920.3 |
| 5 / 8 | 1064.9 / 1096.5 | 1015.0 / 1121.0 |
| 6 / 8 | 1082.9 / 1120.8 | 1112.6 / 1144.4 |
| 7 / 8 | 943.9 / 988.3 | 1009.9 / 1017.2 |

All latency gates pass; the candidate is not consistently faster. These are
32-request waves on a shared Linux x86_64 host, so p99 is the sample maximum,
not a production SLO or a statistically precise tail estimate.

Sampled peak RSS is **746812 KiB baseline / 801756 KiB candidate** (candidate
about 7.4% higher). This includes two loaded model sessions, mapped weights,
startup and transient inference buffers; it is not decoder-only memory.
Post-wave RSS grows **848 KiB / 1912 KiB**, respectively, across the four
concurrency-8 waves. This bounded experiment passes the stated memory gates;
it does not establish absence of leaks in a long soak.

## Faults and recovery

Fault servers use one pool slot and one encoder thread. A 180-second AAC-LC
M4A repeats the original four-second Golos speech recording 45 times, encoded
with FFmpeg 7.0.2-static at 64 kbit/s. The exact recipe and hashes are in
[provenance.json](runs/http-load-0.8.1/provenance.json); media remain ignored.

| Exercise | Baseline | Candidate |
|---|---|---|
| Hold slot with long file; submit eight short requests with 1 s checkout | 8 x 503 `timeout`, Retry-After present | same |
| Held 180 s request completes | 200, 180 s, matching text hash | same |
| Three admitted request disconnects | pool recovered; exact control response each time | same |
| Three admitted requests with 1 ms client read timeout, then socket close | timeout observed; pool recovered; exact control response each time | same |
| Long request with 1 s no-progress watchdog | 504 `inference_timeout` in 1002.7 ms | 504 in 1002.9 ms |
| Pool recovery after watchdog response | 825.9 ms | 702.4 ms |
| Ten consecutive short probes after each fault profile | all exact reference responses | same |
| All six temporary server processes drain and exit | exit 0 | exit 0 |

Admission is established by observing `/ready` report zero available slots
before closing the client socket. A client disconnect has no observed HTTP
response status; recovery is established independently through readiness and
successful inference. The longest cancellation recovery was **2122.2 ms
baseline / 1996.1 ms candidate**, within the predefined 30-second limit.
Cancellation is cooperative: an already running runtime call can finish before
the slot returns. The watchdog is a **no-progress** timer, not a maximum total
file duration. Its expected 504 and the expected checkout 503 responses are
intentional fault results, not unexpected errors in normal load.

Prometheus independently records eight checkout timeouts and one inference
timeout per binary. Final readiness has all pool slots available. The
runner gates these counters, recovery, reference probes and clean shutdown.

## Evidence and scope

[Full result and all gates](runs/http-load-0.8.1/result.json),
[progress](runs/http-load-0.8.1/progress.log), and per-profile metrics/lifecycle
logs are stored under [runs/http-load-0.8.1](runs/http-load-0.8.1/).
An interrupted runner startup diagnostic is also retained: the first runner
incorrectly expected pool counts in `/health`; those counts belong to
`/ready`. It sent no transcription requests, was stopped, and its experimental
server exited cleanly. The final runner checks model health and pool readiness
separately and accepts readiness 503 during intentional pool saturation.

This qualifies the tested local consumer snapshot and CPU REST path. It does
not exercise WS/SSE, diarization, other execution providers, physical mobile,
multi-hour soak or live production traffic. Upstream gigastt main does not
yet contain the working-tree syom integration used by these binaries; the
dependency patch applies to that integration, not to upstream main alone.

## Reproduce

```sh
python3 lab/release-audit/run_http_load.py \
  --baseline target/rollout-canary/gigastt-baseline \
  --candidate target/rollout-canary/gigastt-candidate \
  --model-dir /path/to/models \
  --ort /path/to/libonnxruntime.so.1.28.0 \
  --manifest target/rollout-canary/manifest.json \
  --reference lab/release-audit/runs/http-canary-0.8.1/result.json \
  --long-input target/rollout-canary/inputs/load-180s.m4a \
  --out target/rollout-canary/load-repeat
```

Use a fresh output directory. Rebuild binaries and prepare matching private
inputs as described in the previous canary; neither models nor recordings
are distributed with this repository. All GitHub CI remains skipped.
