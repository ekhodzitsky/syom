# TASK-145: gigastt AAC adapter performance

This investigation targets the optional gigastt AAC adapter and its native
decoder cost. The starting codec was published **syom 0.10.0**, source
`eef30caa6b1596ee4ba15db81492bd087747426e`, registry SHA-256
`864eacba963d7f6db2815289d4a8572f62a8784ee61f71a013bf9a9b935491ce`.
The measured FFT improvement is now an **unpublished syom 0.10.1 candidate**.
The downstream PR remains draft; this work neither merges it, changes the
default consumer backend nor publishes a crate. Combined consumer qualification
is still pending; the failed adapter-only attempts remain below.

## Reproduced gap and attribution

Accepted gigastt main is `adf4db0d7b06a150534734b69b925e3136f0a631`;
the optional backend starts at `49faef1375dd4bef7f0d4ac9602b4dd68bc51dbf`.
The unchanged six-case harness runs seven alternating process pairs, with two
warmups and seven measured decodes per process, pinned to CPU 0. Every case
must have median paired candidate/base latency at most **1.05**. No samples
or failed campaigns are removed. These comparisons do not demonstrate a
syom 0.10.0 regression: the earlier 0.9.0/0.10.0 comparison passed with identical
PCM, whereas the problem here compares two different decoder integrations.

| Consumer case | Original cached adapter | Stereo specialization alone |
|---|---:|---:|
| Short M4A, file | 1.113 | 1.084 |
| Short M4A, bytes | 1.088 | 1.076 |
| ADTS, file | 1.077 | 1.019 |
| Long stereo M4A, mono | 1.134 | 1.100 |
| Long stereo M4A, split | 1.010 | 1.020 |
| Long stereo M4A, scan | 1.031 | 1.024 |

Both campaigns failed. Specializing stereo downmix alone was insufficient.
The first adapter changes follow the measured overhead:

- The mono consumer spawned a thread and handed each logical 24,576-frame
  block across a bounded synchronous channel. Split and scan were synchronous.
- Three long-file decodes through the original candidate produced 11,297 read,
  11,285 seek, 1,997 futex and three clone calls under strace. Standard decoding
  produced 122 reads, 20 seeks, two futex calls and no clones. Syscall profiling
  changes timing and is attribution evidence, not the latency gate.
- A native `M4aSeek` diagnostic counted 3,756 reads and 3,758 seeks per long
  decode. Ordinary `BufReader` lost its cache on absolute AU seeks and amplified
  physical reads from about 1.25 MB to 30.78 MB. A bounded buffer retaining
  in-buffer absolute seeks reduced this to 21 reads and four seeks in that
  diagnostic, with identical native sample bits.
- The same cold, instrumented long-file open/decode counted 3,897 allocations
  and 290 reallocations. These include lazy table/workspace initialization;
  requested bytes are cumulative, not peak live memory. The adapter change
  does not claim to eliminate those codec allocations.
- Slice decode, allocation counting, native downmix and the exact consumer
  resampler were measured separately. Slice decode is not the production
  `M4aSeek` path. Native Mono and Split-plus-average had identical bits on the
  three timing fixtures and no useful speed advantage. Changing resampler
  cadence was therefore rejected; the adapter preserves the original logical
  cuts and floating-point operation order.

The [isolated profile drivers](../consume/gigastt/profile/README.md) retain the
diagnostics, their limitations and the licensed original resampler source.
Instrumentation and shared-host exploratory timings are not qualifying runs.

The pull/buffer variant (**candidate 2**) passed correctness and local consumer
checks but still failed both unchanged qualifying campaigns:

| Case | Campaign 1 | Separate confirmation |
|---|---:|---:|
| Short M4A, file | 1.0495 | 1.0481 |
| Short M4A, bytes | 1.0408 | 1.0457 |
| ADTS, file | **1.0735** | **1.0699** |
| Long stereo M4A, mono | **1.0783** | **1.0959** |
| Long stereo M4A, split | 1.0165 | 1.0260 |
| Long stereo M4A, scan | 1.0159 | 1.0088 |

The [raw summary](runs/task145-20261105/candidate2/timing-summary.json)
retains both failures. This is **not a qualified adapter release**. Its actual
long-file syscall counts fell to 91 reads, 26 seeks, one futex and no threads
across three decodes; residual cost is userspace CPU. Controlled 30-repetition
diagnostics still measured about 4.7–5.1 ms beyond the standard consumer.
Additional native-decoder/resampler attribution is required before promotion.

Recycling resampler staging capacity was tested independently and rejected as
a latency fix: allocations fell from 602 to 523 and reallocations from 86 to
nine, but the long-input median paired time ratio was **1.0035**, with all
float bits unchanged. Lower cumulative allocation is not a measured speedup.
The [negative result](runs/task145-20261105/resample-recycle/timing.json) stays
in the record; the shared production resampler was not changed.

Hardware sampling was unavailable (`perf_event_paranoid=4`). An isolated
Linux SIGPROF sampler, preloaded only into an owned native benchmark, collected
30,755 instruction-pointer samples without changing system security settings.
The early scalar butterflies inside the AVX-512 FFT accounted for **13.41%**
of all native samples and **55.59%** of FFT samples. Later wide-vector stages
were much smaller. This identifies the first two radix-2 stages as a focused
optimization target; helper inlining alone was already present in assembly.
[Region counts](runs/task145-20261105/sigprof/ifft-regions.json) and
[disassembly](runs/task145-20261105/sigprof/ifft-hot-disassembly.txt) preserve
the finding. This statistical diagnostic is not a qualifying latency run.

The resulting kernel prototype groups the first two radix-2 stages into fixed
two/four-point groups. LLVM can vectorize across independent butterflies;
every original multiply, add and subtract retains its order and actual twiddle
value. No algebraic zero shortcuts, FMA, new unsafe operation or filter change
was introduced. The existing scalar/SSE/AVX fallbacks remain unchanged.

All 21 alternating native pairs favored the prototype. Long M4A fell from
33.382 to 28.524 ms (paired ratio **0.8568**); short M4A/ADTS ratios were
**0.8488/0.8469**. Callback geometry and full float-bit hashes matched on all
three fixtures, and six existing IMDCT tests passed (one manual probe ignored).
The [prototype measurements](runs/task145-20261105/fft-native/native-timing.json)
retain every sample and the unrelated shared-host workload snapshot. These
are native attribution results, not the six-case consumer promotion gate.

The integrated 0.10.1 candidate adds a direct AVX-512 regression against the
scalar oracle at sizes 1/2/4/8/16/64/512, including signed zero and subnormals.
Version/active path lockfiles are synchronized; historical measurements and
archive lockfiles retain their measured 0.10.0 identity.

## Adapter behavior

Seekable M4A uses synchronous bounded pull through the existing `M4aSeek`
callback-pause contract. An explicit local flag distinguishes an intentional
batch pause from a real error. Real errors terminate the source and discard
the incomplete batch; reaching EOF still validates presentation completion.
Raw ADTS/LOAS retains the bounded worker. Early drop of an M4A source releases
its reader and decoder without leaving a worker running.
The existing window consumer checks cancellation before and after receiving
each bounded block; synchronous pull does not add producer prefetch.

File pull adds a 64 KiB buffer that preserves cached bytes across nearby
absolute seeks. Read/seek failures invalidate its position. File clones share
an OS offset, so falling back after an unsupported open performs an absolute
rewind. Non-AAC MP4 still falls through to the existing standard decoder.
Stereo averaging uses one output pass while retaining add-then-divide bits;
the mono, multichannel and unequal-plane behavior is preserved.

The initial budget test incorrectly required the internal opener to reject a
long file immediately. The public decoder enforces its budget while consuming
source frames. The corrected regression exercises both public bytes/file
bounded APIs and requires typed `AudioTooLong`, with no returned PCM vector.
The failed run is retained. No production code was changed by that correction.

An additional isolated cold-stack proof includes the exact frozen pull, reader
and buffer modules. Four HE/PS M4A goldens (`he48`, `he48em`, `he2_48em` and `ps48`)
each run in a fresh process with the first decode on a **128 KiB optimized
thread stack**, once from bytes and once through the file clone/buffer path.
All **eight cells pass**, matching warm decode in logical batch geometry and
every native sample bit. This proves the native pull path, not the stack use
of the full ASR application or resampler. The [generator and instructions](
../consume/gigastt/profile/README.md) and [raw results](
runs/task145-20261105/stack-proof/results.json) retain the exact scope.

## Presentation and HTTP policy

PCM parity is against the original syom adapter, including its correct M4A
priming and end trimming. Standard decoder output is not a trimming oracle.
For HTTP, the unchanged equality canary is retained separately from the
[independent qualification policy](../consume/gigastt/README.md). Numeric
latency/RSS thresholds and admission/fault definitions remain unchanged.

The known golos_04 AAC transcript must agree with the human reference, original
WAV and freshly recognized FFmpeg presentation: `ищи телеканал про бизнес на тиви`.
The baseline AAC result says `…на тв`. Other duration differences are accepted
only when source WAV, container presentation, explicitly bounded FFmpeg
presentation, candidate PCM count and HTTP duration agree. Unbounded FFmpeg
decode is recorded separately and must cover the complete presentation span;
its padding is not silently relabeled as presented audio. A general duration difference does not
establish a particular priming/padding mechanism.

## Scope of verification

Host qualification is Linux x86_64 only. Compiler/profile, CPU affinity,
source/fixture/binary hashes, static ORT and the active 305 MiB RNNT model are
recorded with each campaign. No Windows/macOS/ARM parity is inferred.
GitHub Actions remains paused; local checks use no hosted CI.

The integrated upstream candidate passed the following local checks before
its source revision was handed to the combined gigastt build:

| Upstream check | Result |
|---|---|
| Seven required repository checks | pass; 1,186 library tests, seven existing ignores, five integration tests, 29 doctests |
| Rust 1.89.0 library/integration/docs | same counts, pass |
| Rust 1.89.0 optimized cold stack | six fresh profile subprocesses, 128 KiB, pass |
| Rust 1.89.0 optimized IMDCT | seven pass, one existing manual probe ignored; exact IEEE-bit regression passes |
| Packaged Rust 1.89.0 consumers | flows, consumers, exact seek, checkpoint and strict completion pass |
| Parser/state/checkpoint fuzz | 1,216 / 37,870 / 418,980 iterations, zero panics; seeds 145/146/147, 30 seconds each |
| HTTP reference policy | 12 offline tests pass |

[Repository checks](runs/task145-20261105/upstream-checks/checks.json),
[MSRV summary](runs/task145-20261105/upstream-msrv/summary.json),
[fuzz results](runs/task145-20261105/upstream-fuzz/result.json) and
[packaged-consumer provenance](runs/task145-20261105/upstream-consumer/manifest.json)
retain commands, hashes and logs. The locally packaged dirty-tree candidate is
469,226 bytes, SHA-256
`4254f6c31f9ba3fc5ca34baf9de55df49641f27f506d337eb8cad2038277c0df`.
It is not the published 0.10.0 archive and has not been uploaded.
