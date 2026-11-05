# TASK-145: gigastt AAC adapter performance

This investigation targets the optional gigastt AAC adapter and its native
decoder cost. The starting codec was published **syom 0.10.0**, source
`eef30caa6b1596ee4ba15db81492bd087747426e`, registry SHA-256
`864eacba963d7f6db2815289d4a8572f62a8784ee61f71a013bf9a9b935491ce`.
The measured FFT improvement is now an **unpublished syom 0.10.1 candidate**.
The downstream PR remains draft; this work neither merges it, changes the
default consumer backend nor publishes a crate.

The final combined candidate passes two preregistered six-case latency
campaigns on CPU 1, all 29 exact PCM cases, full consumer checks and the
current-305-MiB-model HTTP recognition/admission/load/fault qualification.
Every failed attempt remains below, including an unexplained CPU 0 scan
failure. Qualification covers the pinned Linux x86_64 setup and requires the
complete downstream adapter patch, not just a library version update.
The [machine-readable result](runs/task145-20261105/RESULT.json) and
[SHA-256 evidence index](runs/task145-20261105/evidence-index.json) identify
the qualified artifacts and retained failures.

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

The first combined consumer (**candidate 3**) passed all correctness checks
and five latency cases, but ADTS still failed twice: **1.05160/1.05268**.
Long mono now passed at **1.0242/1.0281**; split/scan ratios were below one.
The unchanged 1.05 limit was not rounded or waived; see
[both raw campaigns](runs/task145-20261105/candidate3/timing-summary.json).

Controlled queue-capacity tests did not support increasing prefetch: two/four
packets reduced context switches but paired latency ratios were 1.0033/1.0147
against one packet. A 16 KiB input-read experiment saved about 22 microseconds
in its median but regressed in two of seven pairs and added a new feeding-loop
surface; 64 KiB was less consistent. Neither change was adopted. The original
one-packet queue and 4 KiB read quantum remain, with
[queue](runs/task145-20261105/worker-capacities/) and
[input-quantum evidence](runs/task145-20261105/worker-quanta/) retained.

The next targeted changes remove repeated transport detection and the
single-plane normalization pass. ADTS was being classified three times before
decoding; carrying its classification removes two reads and two rewinds while
preserving tagged/push/unsupported-M4A fallback behavior. A single plane is
already mono: copying it directly avoids a dynamic divide-by-one pass. The
[helper experiment](runs/task145-20261105/mono-append/result.json) preserved
all 193,536 finite sample bits, signed zero/subnormal edges and quiet NaN
payloads. It measured roughly 22 microseconds of potential savings for this
ADTS input, which is not itself an end-to-end gate result. Six real
classification/I/O/fallback regressions and the repeated eight-cell native
HE/PS cold-stack proof pass before the next full consumer campaign.

Candidate 4 still failed its preliminary ADTS gate at **1.05589**, so expensive
workspace checks were stopped; its other five cells and 29 PCM cases passed.
That [preliminary failure](runs/task145-20261105/candidate4/early-diagnostic/decoder-timing.json)
is retained. Helper-only microbench improvements were not treated as an
end-to-end pass.

Candidate 5 removes the worker for **bare ADTS**. A small framing layer feeds
declared AUs through the existing public `Decoder::feed`, stopping between
calls once the original logical block is ready. It forwards junk and partial
headers too; the codec remains the sole syntax/CRC/payload validator. The
separate framing/fed offsets retain at most five already-forwarded bytes across
reads, avoiding replay and preserving header-error-before-I/O precedence.
EOF always calls `finish`, retaining the old partial-tail behavior.

One ADTS AU contains at most four raw data blocks and two-times SBR expansion:
at most 8,192 native samples, below the 24,576-frame logical cut. The new path
therefore needs one ready block and a partial batch, with defensive overflow
refusal; its compressed stash is at most 4,101 bytes plus a 4 KiB read scratch.
Tagged ADTS/LOAS and unsupported-M4A push retain their worker. Existing ISO-BMFF
priority is retained even for an ambiguous ADTS/`ftyp` prefix.

The native guard passes CRC, LC/HE/PS four-RDB, File/Bytes geometry/full-bit,
junk, short-read, EOF and error-order cases. The additional fresh-process
[ADTS cold-stack proof](runs/task145-20261105/adts-cold-stack/results.json)
passes eight File/Bytes LC/HE1/PS/implicit-PS cells at 128 KiB, without encoder
warming. Its preliminary unchanged six-case timing gate passes, including
ADTS **1.03704**. Full workspace checks and two post-validation campaigns
were still required after that preliminary result.

The first full candidate-5 workspace run passed **1,305 tests** (49 existing
ignores), including all 27 AAC guards and the public output-budget and
cancellation cases. Extended Clippy then rejected an unchecked `read` byte
count in the classification regression. The test now asserts all 64 prefix
bytes were read; production source before `#[cfg(test)]` is byte-identical.
The failed attempt, source snapshots and correction proof are retained. The
corrected source also passes all 1,305 extended tests and both Clippy profiles;
the default workspace passes 1,269 tests (49 existing ignores in each run).
Both qualifying campaigns use the corrected test source and final binary hashes.

The first post-validation campaign failed **scan at 1.06908**, while its other
five cells passed. The separate campaign passed all six, including scan at
**0.97663**. The candidate scan samples in the failed run are bimodal, around
100/123 ms; this observation alone does not establish its cause or waive the
failure. Both complete campaigns remain in the record. HTTP was held while
host scheduling and CPU/SMT activity were investigated independently of timing
results; these first two campaigns did not satisfy the two-pass criterion.

Two passive five-second `/proc/stat` windows and static CPU capacity/frequency
metadata were collected before any further decoder run. The primary repeat
CPU was preregistered as **CPU 1, SMT sibling 13**, the least busy observed
physical pair within CPU 0's hardware class (capacity 1,024, maximum frequency
5,157,895 kHz, AMD highest-performance value 196). A quieter CPU 8 pair was
excluded from primary qualification because its maximum frequency/performance
class differs. This selection uses load evidence, not new decoder outcomes.
These short observations cannot prove the cause of the earlier scan failure.
Exactly two confirmation campaigns were registered on CPU 1; the original CPU 0
failure and success are retained separately and are not averaged away.

Both preregistered CPU 1 confirmations passed all six unchanged latency gates:

| Consumer case | Confirmation 1 | Separate confirmation 2 |
|---|---:|---:|
| Short M4A, file | 1.01605 | 1.01974 |
| Short M4A, bytes | 1.01942 | 1.01981 |
| ADTS, file | 1.02601 | 1.02782 |
| Long stereo M4A, mono | 1.02023 | 1.01907 |
| Long stereo M4A, split | 0.98132 | 0.98729 |
| Long stereo M4A, scan | 0.97882 | 0.96747 |

The passive observer ran on another physical core, CPU 8, consuming
0.164/0.172 CPU-seconds (0.455%/0.477% of one core). It recorded CPU 1/13
occupation, reported frequency and owned probe scheduling without changing
governors or unrelated processes. These passes qualify this pinned Linux
setup; they do not erase the CPU 0 failure or establish its cause.

The [paired results and observer summaries](
runs/task145-20261105/candidate5/preregistered-confirmation-summary.json),
[original CPU 0 scan audit](runs/task145-20261105/candidate5/scan-audit.json)
and [preregistered hardware/load selection](
runs/task145-20261105/quiet-core-selection/same-class-preregistered-selection.json)
retain the scope. This is a latency qualification, not a universal memory win:
the split probe peaks near **53.0/53.1 MiB**, versus **36.2/36.0 MiB** for
the standard decoder in these confirmations. Its higher RSS is retained.

The final probe is SHA-256
`6fd6ca682a88d17b76725ecfe57f5ead7f7464b50ed44390051a959b67534894`;
the final server is
`66f32ffb098c368f212670aa36f801686773424bb0c5334a818a9260522107b6`.
They are byte-identical to the initial candidate-5 production binaries that
passed all **29 exact PCM cases** against the original syom 0.10.0 adapter.
The test-only correction changed neither production artifact.

## Adapter behavior

Seekable M4A uses synchronous bounded pull through the existing `M4aSeek`
callback-pause contract. An explicit local flag distinguishes an intentional
batch pause from a real error. Real errors terminate the source and discard
the incomplete batch; reaching EOF still validates presentation completion.
Bare ADTS also uses bounded synchronous pull as described above. Tagged
ADTS/LOAS retains the bounded worker. Early drop of a synchronous source releases
its reader and decoder without leaving a worker running.
The existing window consumer checks cancellation before and after receiving
each bounded block; synchronous pull does not add producer prefetch.

The combined worker candidate sends its first block immediately, then packs
at most two existing blocks into each channel message. The receiver exposes
them separately, preserving every resampler cut. Complete blocks precede a
real error; an incomplete tail is discarded on error. The channel holds one
packet and drop closes its receiver before joining. Including active, queued,
held and pending blocks, the explicit bound rises from four to six logical
blocks. This is a bounded memory tradeoff, not a claim of unchanged buffering.
Four isolated mechanics tests cover EOF/parity, error ordering, early drop
and the queue bound; full consumer qualification is recorded separately.

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
only when container presentation, explicitly bounded FFmpeg presentation,
candidate PCM count and HTTP duration agree. Source WAV duration must agree
within one sample or have the exact clock explanation below. Unbounded FFmpeg
decode is recorded separately and must cover the complete presentation span;
its padding is not silently relabeled as presented audio. A general duration difference does not
establish a particular priming/padding mechanism.

Two pinned inputs need an additional clock distinction. `golos_09` has 53,492
source samples at 16 kHz, while its movie/edit duration is 3.344 seconds
(53,504 samples); `golos_10` has 62,499 source samples and a 3.907-second
movie/edit duration (62,512 samples). In both, the 1 kHz movie clock rounds
up to the next tick. The media timescale and summed `stts` durations minus
1,024 priming samples equal the source WAV exactly. Actual `mvhd`, `tkhd` and
single `elst` durations equal that precise ceiling. The input/WAV hashes and
all integer clock/table values are checked by
[the narrow evidence parser](../consume/gigastt/mp4_clocks.py).

The 12/13-sample movie-clock surplus is explicit, not an unexplained padding
allowance. Candidate/container/bounded-FFmpeg comparisons still use the
one-sample tolerance, and full FFmpeg decode must actually cover the declared
presentation. Unknown inputs, shapes or clock relations are refused. The
[preflight](runs/task145-20261105/http-geometry-preflight/result.json) retains
all 30 WAV/M4A rows and the raw ffprobe evidence. The separate golos_04
priming diagnosis is not generalized to these clock cases.

The fresh current-model canary completed all **32 successful responses per
backend**. Its numeric, health and invalid/truncated-input prerequisites pass:

| HTTP measurement | Standard decoder | Final candidate |
|---|---:|---:|
| Median delay | 94.70 ms | 96.47 ms |
| p95 delay | 135.08 ms | 152.87 ms |
| p99 delay | 171.27 ms | 174.60 ms |
| Peak process RSS | 1,159.82 MiB | 1,161.03 MiB |

The original equality canary remains **false**. Fresh source-WAV/FFmpeg/model
evidence explains the supported text and presentation differences independently.
The first reference attempt also failed on golos_05/12: their source/container
difference is exactly one sample, but binary floating-point subtraction was
slightly larger than `1/16000`. This tooling error is retained with its source
snapshot and failed result. The correction compares decimal representations as
exact rational numbers at the same **one-sample** threshold, without epsilon.
All 17 offline regressions pass, including both signs of the actual boundaries,
the immediately adjacent representable floats outside/inside them and invalid
or nonfinite inputs. A fresh reference qualifies the unchanged raw canary and
independent evidence, retaining `original_canary_pass=false`; no response,
boolean or numeric threshold was edited. Admission/load/fault qualification is
recorded separately after this prerequisite succeeds.

The admission-aware load/fault result passes **all 12 gates**. Each backend
completed seven 32-input bursts at concurrency 1/2/4/8/8/8/8; real upload-busy
rejections exercised the unchanged retry path. Maximum paired burst p95/p99
ratios were **1.08078/1.14841**, below 1.5. Peak load-process RSS was
**1,022.86/1,066.72 MiB** (standard/candidate), within the existing 1.25 ratio
plus 32 MiB allowance; steady-growth checks pass their unchanged 64 MiB limit.
This is threshold compliance, not a lower-memory claim.

Both backends passed pool exhaustion, typed admission/queue timeouts, client
disconnect/read-timeout cancellation, post-fault recognition and watchdog
recovery. The candidate watchdog returned typed HTTP 504 and recovered in
148.71 ms, versus 126.06 ms for standard decoding; the recovery limit remains
30 seconds. All six owned load/fault servers shut down cleanly with exit zero.
The [raw equality canary](runs/task145-20261105/http-candidate5/canary/result.json),
[independent presentation](runs/task145-20261105/http-candidate5/independent/independent.json),
[failed first reference](runs/task145-20261105/http-candidate5/reference.json),
[qualified reference](runs/task145-20261105/http-candidate5/reference-attempt2/reference.json)
and [load/fault result](runs/task145-20261105/http-candidate5/load/result.json)
remain separate. Execution records retain every command, exit code and hash.

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
| HTTP reference policy | initial 12 offline tests pass; final expanded clock/boundary suite: 17 pass |

[Repository checks](runs/task145-20261105/upstream-checks/checks.json),
[MSRV summary](runs/task145-20261105/upstream-msrv/summary.json),
[fuzz results](runs/task145-20261105/upstream-fuzz/result.json) and
[packaged-consumer provenance](runs/task145-20261105/upstream-consumer/manifest.json)
retain commands, hashes and logs. The earlier locally packaged dirty-tree candidate is
469,226 bytes, SHA-256
`4254f6c31f9ba3fc5ca34baf9de55df49641f27f506d337eb8cad2038277c0df`.
It is not the published 0.10.0 archive and has not been uploaded.

After the complete consumer/HTTP qualification, all seven required repository
checks were repeated successfully, together with the 17-test offline policy
suite. The [final commands and logs](
runs/task145-20261105/final-root-checks/checks.json) record Rust 1.97.1, Linux
affinity and offline Cargo settings. The final documentation-bearing local
package is **469,313 bytes**, SHA-256
`0fe9b7db2ed4c614bb6ee90c3d69c20885da73c786d2939083fe6458e3df343d`;
it is also unpublished. The earlier archive hash remains historical evidence.

The frozen consumer passes default/extended workspace tests (1,269/1,305,
49 existing ignores each), all 27 AAC guards, both all-target Clippy profiles,
documentation/publication checks and a dedicated Rust 1.94 all-target check.
Release builds use Rust 1.98.1, fat LTO, one codegen unit and the same static
ORT archive. [Validation and source guards](
runs/task145-20261105/candidate5/validation.json) retain the initial failed
Clippy attempt separately from its successful test-only recovery; the
[reproduction snapshot](runs/task145-20261105/candidate5/reproduction/manifest.json)
contains exact source and commands rather than binaries or model weights.

## Downstream source handoff

The combined adapter is pinned to library commit
[`3e540231c9d71e3534be1ef856a91cf419efaead`](https://github.com/ekhodzitsky/syom/commit/3e540231c9d71e3534be1ef856a91cf419efaead),
version **0.10.1, unpublished candidate**. The
[nine-file patch](../consume/gigastt/adapter.patch) applies to consumer
`49faef1375dd4bef7f0d4ac9602b4dd68bc51dbf`. Its manifest and lockfile use the
canonical Git URL and full revision; every other dependency is unchanged.
The [handoff manifest](../consume/gigastt/adapter-manifest.json) records
SHA-256 `82467a7a14cd41cd8b416e53e94fc49b133b94f09a4f266d49650e3daee24968`,
successful clean apply and byte identity for all nine resulting files.

The earlier registry-0.10.0 adapter patch remains in
[candidate 2 evidence](runs/task145-20261105/candidate2/adapter.patch).
It is a failed historical candidate and does not include the FFT fix.
Combined timing passes on the preregistered CPU 1 setup; the current-model HTTP
reference and admission/load/fault campaign also pass. The unchanged equality
canary remains false and is retained separately from independent qualification.
