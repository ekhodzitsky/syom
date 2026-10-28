# syom 0.8.1 paired HTTP ASR canary

The local HTTP canary passed on 2026-10-28: **32/32 normalized transcripts
and durations match** between syom 0.7.1 and the 0.8.1 candidate. Both
servers reject the two malformed inputs with 422, remain healthy, and
exit cleanly. Latency and RSS pass the recorded relative regression
gates. This extends the [decode-only qualification](CANDIDATE-0.8.1.md)
to actual model inference through `/v1/transcribe`.

## Controlled comparison

Two temporary gigastt 2.21.0 server binaries were built from identical
consumer Rust sources (217 files checked). Only syom selection differs:
registry 0.7.1 versus the local 0.8.1 path. Consumer dependency lock entries,
binary hashes, source hashes and model hashes are in
[runs/http-canary-0.8.1/provenance.json](runs/http-canary-0.8.1/provenance.json).
The snapshot includes existing working-tree changes in the sibling
consumer; it is not identical to its published release or HEAD alone.

Both servers use the same existing RNN-T INT8 model, ORT 1.28.0 CPU runtime,
one pool slot, two encoder threads, and punctuation/ITN disabled. They are
built without optional diarization. ORT binary-download/TLS defaults are
disabled in the lab snapshot and dynamic loading is enabled. Models are
loaded offline from the existing cache; optimized graphs are written into
separate experiment directories. No installed binary or service was
replaced. Both the user and system gigastt services were inactive.

Inputs are 15 existing Russian speech recordings, each as WAV and
independently encoded AAC-LC M4A, plus HE v1/v2 ADTS derivatives of one
of those recordings: 32 format cases, **15 source recordings**. Payloads
stay local; urllib proxies are explicitly disabled. Requests alternate
baseline/candidate order. One recorded warmup per server is excluded from
the timing sample. Response text is normalized for case, punctuation and
ё/е, then stored only as SHA256 plus word count.

## Acceptance gates and results

Before paired requests, the protocol defined the following limits:

| Gate | Result |
|---|---|
| Every positive request returns 200 and the same nonempty normalized transcript | 32/32 passed |
| Reported durations agree within one 16 kHz sample | all equal exactly |
| Garbage and truncated M4A produce matching 4xx responses | both 422 on both servers |
| Both servers remain ready after the requests | passed |
| Candidate p95 and p99 each <= baseline x1.5 | passed |
| Candidate peak RSS <= baseline x1.25 + 32 MiB | passed |

The final paired pass measured:

| Metric | Baseline syom 0.7.1 | Candidate syom 0.8.1 |
|---|---:|---:|
| Median HTTP ASR latency | 189.7 ms | 204.6 ms |
| p95 HTTP ASR latency | 256.1 ms | 276.4 ms |
| p99 HTTP ASR latency | 291.9 ms | 283.3 ms |
| Process peak RSS | 291784 KiB | 294436 KiB |
| Successful transcription requests, including warmup | 33 | 33 |
| Expected invalid-input responses | 2 | 2 |
| HTTP 5xx responses | 0 | 0 |

These are 32 different short speech payloads on a shared host, not a
fixed-duration kernel benchmark or a production load test. p99 is near
the sample maximum. The candidate is not uniformly faster; this pass's
p95 is about 8% higher and peak RSS about 0.9% higher. An earlier full
pass also passed, with different timing order. Both distributions are
preserved; no outliers were removed and no speed-superiority claim is
made. Prometheus snapshots provide independent status counters.

[Final results](runs/http-canary-0.8.1/result.json),
[first full pass](runs/http-canary-0.8.1/first-full-pass.json),
[baseline metrics](runs/http-canary-0.8.1/baseline.prom),
[candidate metrics](runs/http-canary-0.8.1/candidate.prom).

## Readiness and rollback behavior

The startup probe found that `/health` returns 200 with `model=loading`
while models are still loading. That is liveness, not readiness. The
initial runner advanced too early, received 503 on warmup and stopped
both experimental servers. The corrected runner waits for a nonempty,
non-loading model before starting requests and checks readiness again
afterward. The [startup diagnostic](runs/http-canary-0.8.1/startup-probe.json)
is retained separately from the passing tests.

The runner returns nonzero on request, transcript, duration, readiness,
latency or RSS gate failures. Its `finally` block sends SIGTERM to its own
processes, waits for graceful drain, and kills only its own process group
if that drain exceeds 30 seconds. Passing and failing runs both cleaned
up. All successful-run server exit codes are 0, and lifecycle logs record
`Drain complete`. This is an exercised local stop/rollback path, not an
untested production deployment script.

## Reproduce and promote

`run_http_canary.py` takes explicit baseline/candidate binaries, model and
ORT paths, and a manifest of local test files. It chooses unused loopback
ports and records commands, health, timings, RSS, responses and metrics:

```sh
python3 lab/release-audit/run_http_canary.py \
  --baseline target/rollout-canary/gigastt-baseline \
  --candidate target/rollout-canary/gigastt-candidate \
  --model-dir /home/ekhodzitsky/.gigastt/models \
  --ort /home/ekhodzitsky/.cache/gigastt-alternative/venv312/lib/python3.12/site-packages/onnxruntime/capi/libonnxruntime.so.1.28.0 \
  --manifest target/rollout-canary/manifest.json \
  --out target/rollout-canary/repeat
```

The binaries are ignored build artifacts, not installed releases. To
repeat on another machine, build the same consumer snapshot twice with
syom 0.7.1 / 0.8.1, or supply its actual baseline and proposed production
build. Use the intended deployment feature flags, ORT runtime, pool/thread
settings and local corpus. The defaults above are an explicit lab
configuration, not a replacement for a service's production profile.

The [reviewable consumer version patch](gigastt-version.patch) and the
verified `target/package/syom-0.8.1.crate` are ready. The production gate
sequence is:

1. Preserve the current consumer binary, dependency lockfile, configuration
   and model/runtime hashes as the rollback reference.
2. Build the upgrade with the target's normal feature set and pin the new
   dependency. Run this paired canary using its intended runtime settings.
3. Start a separate instance, wait for model-ready health rather than status
   200 alone, and validate saved/internal AAC requests before routing traffic.
4. Use the deployment system's normal traffic switch. Stop promotion on a
   response/text/duration mismatch, new 5xx, or a latency/RSS gate failure.
5. For rollback, route back to the preserved instance/binary first, then
   gracefully drain the candidate. Do not overwrite the only working binary.

No production endpoint, traffic router or active service was provided,
so this work completes the local canary and prepares promotion; it does
not claim a live production deployment. WS/SSE, concurrent load,
diarization, other execution providers and mobile targets remain outside
this HTTP RNN-T canary. The original blind encoder/listening qualification
also remains separate.
