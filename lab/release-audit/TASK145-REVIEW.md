# TASK-145 solution review and refactor — 2026-11-06

The performance-qualified decoder and consumer adapter remain unchanged.
This review found reproducible weaknesses in the qualification tools and
test coverage, fixed them, and replayed the original evidence successfully.
It found no demonstrated new decoder correctness or memory-safety regression.

Audit base: `2d4c491a3cc336b8be5c47e95f461a5390ae5732`. The runtime remains
syom **0.10.1, unpublished candidate**, revision
`3e540231c9d71e3534be1ef856a91cf419efaead`, with the complete nine-file consumer
patch SHA-256 `82467a7a14cd41cd8b416e53e94fc49b133b94f09a4f266d49650e3daee24968`.
The [source guard](runs/task145-review-20261106/source-guard.json) verifies
the kernel, dispatch, Cargo files, patch and manifest against the audit base.
The original measurement archive and its evidence index were not rewritten.
The [review result](runs/task145-review-20261106/RESULT.json) and
[evidence index](runs/task145-review-20261106/evidence-index.json) bind the
new checks and source snapshots separately from the original campaign.

## Findings and fixes

| Priority | Finding | Change and evidence |
|---|---|---|
| P2 | Numeric/health/negative prerequisites trusted saved gate flags. Contradictory latency or invalid-input observations could remain qualified. | Recompute timing summaries from all 32 request observations, check original numeric limits against RSS, and validate recorded health and negative responses. Flags remain necessary but are insufficient. |
| P2 | Load input validation checked length and membership but not uniqueness. Replacing a required case with a duplicate could pass binding. | Require exactly the 32 distinct case IDs before indexing both reference and load manifests. |
| P2 | Only encoder bytes were globally pinned. Other model files and most input bytes were self-bound across phases but could differ from the named campaign. | Pin all four RNNT files and all 32 input hashes in the shared contract. Matching labels and matching self-reported hashes no longer substitute for the fixed corpus. |
| P2 | Reference used exact one-sample comparison, while load matching still used binary subtraction. The same exact boundary could pass reference and fail load. | Share one rational comparison, retain the inclusive `1/16000` bound, and reject the adjacent float outside it. |
| P2 | Equal ADTS responses could qualify with negative durations; word-count types and text hashes were weakly checked. | Require positive finite duration, positive integer word count and a valid text hash independently of response equality. |
| P3 | Manifest `apply_check`/`applied_files_identical` flags and plain `git apply` instructions were not executable verification of the complete handoff. | Add a read-only checker that reconstructs base blobs in a temporary repository, checks the complete patch path set and verifies every resulting file hash. Explicit application also requires the exact base and a clean tree. |
| P3 | An FFT test named `bits` compared float values, so signed zeros were indistinguishable. Both numerical matrices omitted the production FFT size 256. | Use a shared per-lane `to_bits()` assertion with mode/plane/index diagnostics; add 256 to both matrices. |
| P3 | New synchronous producer differentials covered Mono but omitted selected and missing channels. | Add 48 native selected-channel cells, with direct SDK plane indexing as the oracle. |

The [counterexample report](runs/task145-review-20261106/counterexamples.json)
executes the immutable pre-refactor policy from the audit base. Five invalid
synthetic observations were accepted before and rejected after: forged numeric
success, replaced corpus bytes, a replaced decoder model, negative ADTS duration
and duplicated load input. A sixth control demonstrates the exact one-sample
load boundary, incorrectly rejected before and accepted after. These are
offline tool counterexamples, not evidence that the original real campaign
used invalid data or that a decoder produced negative durations.
From the repository root, `python3 lab/release-audit/runs/task145-review-20261106/reproduce_findings.py`
reruns those synthetic before/after controls against the pinned Git revision.

## Refactor boundaries

- [http_contract.py](../consume/gigastt/http_contract.py) owns immutable campaign
  identities, exact duration comparison, sample conversion and corpus indexing.
- [http_prerequisites.py](../consume/gigastt/http_prerequisites.py) validates
  observations and recomputes numeric prerequisites using the original canary
  percentile definition. It does not change thresholds or request schedules.
- [http_reference.py](../consume/gigastt/http_reference.py) composes prerequisite,
  transcript, presentation and binding checks. Presentation validation is now
  a separate function rather than a large nested part of qualification.
- [presentation_evidence.py](../consume/gigastt/presentation_evidence.py) uses
  the same identities and rejects any existing output directory, including an
  incomplete prior attempt. [run_http_load.py](../consume/gigastt/run_http_load.py)
  shares exact duration comparison and removes a redundant manifest check.
- [check_handoff.py](../consume/gigastt/check_handoff.py) turns the handoff's
  historical claims into reproducible checks. It verifies SHA/size, exact base,
  path safety, the complete file set and nine resulting hashes. Default checking
  does not change consumer files, even when its working tree is dirty. `--apply`
  requires a clean tree and uses the same verified patch-byte snapshot.

The original `run_http_canary.py` remains byte-identical. The load scheduler,
admission behavior, retry definitions, fault procedures and numeric limits are
unchanged. The exact boundary correction changes a previously inconsistent
acceptance edge, not the declared tolerance.

## Numerical and producer coverage

[FFT tests](../../src/engine/imdct_tests.rs) now compare actual IEEE bits,
including the existing direct AVX-512 signed-zero/subnormal matrix and size 256.
The direct AVX-512 test bypasses global forced-mode state. Focused tests passed
on default Rust 1.97.1 and the declared Rust 1.89 MSRV: seven passed and one
manual benchmark ignored on each. AVX-512 was available and the direct test
was eligible on this Linux x86_64 host. [Logs and hashes](
runs/task145-review-20261106/fft/summary.json) distinguish these tests from
benchmarks or other-platform qualification.

The selected-channel proof uses exact copies of the qualified M4A/bare-ADTS
producer, reader, buffer and selection helpers. Its oracle directly indexes
SDK planar frames, without calling production `append_plane`. The matrix is:

| Dimension | Values |
|---|---|
| Profiles | LC mono, LC stereo, HE, PS |
| Transport | M4A, bare ADTS |
| Reader | File, Bytes |
| Selection | One(0), One(1), missing channel 8 |

All **48 unique cells** match native block geometry and full selected-sample
bits at the original 24,576-frame cuts. Stereo fixtures are verified to have
different left/right decoded data. There are 28 populated cells, 20 intentionally
empty missing-plane cells and 144 terminal EOF checks. Missing channels retain
the inherited empty-plane behavior; this review does not introduce a new
missing-channel error contract.

The portable generator defaults to the committed consumer snapshot, rather
than an ignored checkout. Proof-critical Python checks raise explicit errors,
so optimized Python cannot disable source/SDK/result verification. A fresh
`python -O` repeat passed all 48 cells with the same eight fixture hashes and
48 native result hashes. See the [portable summary](
runs/task145-review-20261106/channels-portable/summary.json) and
[preparation instructions](../consume/gigastt/profile/README.md#native-selected-channel-differential).
This proves native selection; it does not rerun public ASR, resampling, budgets
or cancellation. Their original consumer qualification remains separately
recorded against the unchanged patch.

## Historical replay and evidence limits

The strengthened policy reproduces **every saved policy field**, including
all 18 explanations, from the original canary and independent presentation
JSON. The updated load matcher reproduces all **448** recorded load-response
verdicts. Neither replay initializes a decoder, ASR model or server. This shows
the historical observations also satisfy the stricter checks; it is not a new
latency or recognition campaign.

The original result remains narrower than a general production or SOTA claim:

- CPU 0's first final scan campaign failed at 1.06908; its separate campaign
  passed. CPU 1 was chosen after that failure using passive same-class load
  observations, before two successful confirmations. This records the selection
  chronology; it does not establish the CPU 0 failure's cause or remove all
  possible shared-host bias.
- Six timing cells cover a small fixed corpus and several modes of the same
  long stereo file. Two same-host confirmations do not establish cross-host,
  fleet or cross-platform performance. Some mono cases remain slower than
  standard decoding while meeting the 1.05 gate.
- Split-probe RSS remains about 53 MiB versus 36 MiB for standard decoding.
  The adapter's fallback worker also explicitly permits six logical blocks
  rather than four. Neither is a universal memory improvement.
- The raw equality canary remains false. Independent presentation checks
  explain the supported differences; the human/WAV/FFmpeg transcript correction
  specifically proves golos_04, not general ASR accuracy or broad WER superiority.
- Only Linux x86_64 was exercised. No Windows/macOS/ARM parity is inferred.

## Remaining work

**TASK-146 — inherited reader robustness.** Generic `classify_file` still uses
one prefix read. A valid `Read + Seek` source returning one to five bytes can
be misclassified. The regular-file implementation had the same weakness before
TASK-145; no new regular-file regression was demonstrated. A bounded prefix
fill must cover bare/ID3 inputs, Interrupted/EOF and error/rewind precedence.
The same follow-up should forward `Reader::stream_position`, whose current
default can bypass cached `SmartBuffered` positioning. Current SDK calls do
not exercise that latent path. This changes downstream runtime code and must
produce a separately qualified patch, not overwrite the existing measurements.

**TASK-147 — pre-existing SIMD test isolation.** Process-global forced-mode
flags can influence an automatic-backend call made outside another test's
override guard. The direct AVX-512 regression avoids that ambiguity, but broader
test hermeticity needs explicit backend selection or a scoped test-only design.
The runtime dispatch must not change merely to simplify tests.

Other refactoring candidates are deliberately lower priority: three producer
block assemblers repeat rate/layout/batch mechanics, while pause/error handling
differs by transport. Extracting a shared assembler should preserve completed
blocks before errors, partial-tail discard and queue bounds. The AVX-512 scalar
tail is unreachable for valid power-of-two inputs after the specialized early
stages; removing it would reduce unsafe duplication, but would change the
measured kernel. Neither change has a demonstrated present performance benefit.

## Validation and reproduction

All seven required local repository checks pass: formatting, file-size and
changelog audits, all-target Clippy, 1,186 library tests (seven existing ignores),
five integration tests, 29 doctests and the package audit. The additional Python
suite passes **36 tests**, including adversarial observations, handoff safety,
historical replay and source checks under optimized Python. The actual frozen
nine-file handoff check and model-free reference CLI replay also pass. Commands,
versions and logs are in [final checks](runs/task145-review-20261106/final-checks/checks.json).

Run from the repository root:

```sh
python3 -m unittest discover -s lab/consume/gigastt -p 'test_*.py'
python3 lab/consume/gigastt/check_handoff.py --consumer /path/to/gigastt-at-49faef1
python3 lab/consume/gigastt/profile/prepare_channel_selection.py \
  --output target/task145-review/fresh-selected-channel-proof
```

Preparation alone does not build or decode. Add `--execute` only with the
documented cached dependencies/toolchain and after timing campaigns finish.
The [HTTP tool README](../consume/gigastt/README.md) also documents portable
model-free replay and explicit consumer application.

The crate version, runtime dependencies and MSRV are unchanged. CHANGELOG
records the test/tool improvements under the unpublished 0.10.1 candidate.
GitHub Actions remains paused; commits use `[skip ci]`. No crate was published,
no consumer was merged/deployed, and no recording, model, generated native dump
or binary is part of this review's Git artifacts.
