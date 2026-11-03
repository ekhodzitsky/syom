# TASK-144: persistent LC state and strict M4A completion

Qualified locally on 2026-11-03 as **0.10.0, unpublished candidate**. This work
adds upstream capabilities needed by resumable consumers; it does not migrate
agent-youtube, publish a crate, deploy a service or establish SOTA quality.
The [public contract](../../docs/persistent-checkpoints.md) defines eligibility,
source/build authentication responsibilities, failure behavior and completion scope.

## Counterexample and resulting behavior

A synthetic one-sample LC M4A was assigned an edit duration longer than its
available decoded presentation. Unmodified 0.9.0 from base `91ad57d` reported
success on all three public paths: slice streaming, seek-reader streaming and
`M4aSeek`. The [baseline regression](runs/task144-20261103/baseline-regression.rs)
failed with exit 101 and `slice=false reader=false seek=false`:
[red log](runs/task144-20261103/baseline-red.log).
The same public-API regression now passes against the packaged candidate;
see [consumer output](runs/task144-20261103/lab-4.log).

Persistent state is a new API: 0.9.0 logical exact positions contain no DSP
history and still require prefix replay. The reviewed downstream patch was a
requirements/counterexample source, not a patch applied wholesale. The new
`SYOMLC01` format is distinct from downstream `YTLC0001` and has no automatic
conversion or consumer-job compatibility promise.

## Qualified state and boundaries

Version 1 captures exactly **8,422 bytes**: fixed implementation/options/geometry
header, next AU/native position/presentation remainder, PNS RNG, and both LC
filterbank overlaps/window shapes. Import is transactional and reads no media.
Rejected imports preserve the old decoder and position. Arbitrary blob lengths
cannot allocate; restore uses one fixed-size temporary boxed decoder and a
bounded header. A first resumed AU that produces nonfinite PCM is refused before
its callback. Caller-authenticated source/blob/build binding remains mandatory;
a geometry tag is not a digest of AAC payloads.

Qualified mono output covers mono/stereo 1024-sample LC at seven rates:
8/16/22.05/24/32/44.1/48 kHz. Initial/EOF/unfinished-priming and partial-AU states,
HE/PS, PCE/coupling/prediction history, unsafe tag-only trim, failed or exact-mode
imports are excluded. A callback interruption after a valid delivered frame is
allowed. A successful ordinary seek can leave exact mode; observed read/codec or
unsupported-history faults remain checkpoint-ineligible on that handle.

The integration suite runs 28 synthetic scenarios (seven rates × two input
channel counts × PNS option on/off), with two successive fresh-process restores
per scenario. Complete native f32 bits/counts and core/channel/priming/remainder
metadata match uninterrupted decode. This matrix passed on current debug, MSRV
debug and current optimized builds: 56 fresh restores per run. PNS option does
not prove PNS was selected in every cell; an independent unit regression observes
an actually advanced PNS RNG and checks its restored continuation.

Seven checkpoint unit regressions cover real PNS history, transactional
malformed/truncated/nonfinite imports, incompatible options/index/version,
PCE/CCE and read-fault exclusion, finite forged overlap overflow refusal, and the
explicit limitation that different payloads with identical geometry require
caller authentication. Three integration tests include exact/failed-state
exclusions and restore/decode on 512 KiB debug and 128 KiB optimized thread stacks.

Ten completion regressions cover exact end, sub-tick/submillisecond remainder,
one-tick and at-least-one-millisecond excess, a whole missing AU, required
priming, missing payload, supported/stale gapless tags, unknown/open fragment
scope and bounded push completion. Rounded success requires every indexed AU
decoded and remainder strictly below both bounds. No PCM is padded. Push lacks
a complete lifetime index, so it requires exact bounded completion. Existing
exact-seek/PNS/read-budget/failed-state and fragmented-MP4 tests continue passing.

Completion describes the requested presentation, not unplayed indexed padding
or authentication of previous media. Seeking to the end does not validate the
preceding source. This preserves existing exact interval read budgets.

## Local verification and retained evidence

Host: Linux x86_64, AMD Ryzen AI 9 HX 370; Rust 1.97.1 and declared MSRV 1.89.0.
No Windows/macOS/ARM, cross-build PCM or consumer migration qualification is claimed.

| Check | Result |
|---|---|
| Seven AGENTS.md checks | pass; 1,185 library tests, seven existing ignores, five integration tests, 29 doctests |
| Rust 1.89.0 library/integration/doctests | pass with the same counts |
| Optimized persistent integration | pass, including 128 KiB restore/decode |
| Packaged consumer flows and exact-seek cells | pass against the actual `.crate`, including native resume and the red/green completion regression |
| Parser fuzz, seed 1440 | 1,604 iterations / 30-second campaign, zero panics |
| Lifecycle/state fuzz, seed 1441 | 40,436 iterations / 30-second campaign, zero panics |
| Persistent restore mutation, seed 1442 | 5,000 iterations, 2,777 accepted / 2,223 rejected, zero panics; rejection leaves prior checkpoint intact |
| Package | 187 shipped files, 468,705 compressed bytes; zero runtime/build dependencies; no build script |

The package SHA-256 is
`ee6cf3c8c4a7e7ab59a554b2aaea9aa76bc71a63d5a33b511c851e2c59a290db`.
It is a locally packaged dirty-tree candidate, not an uploaded release.
The [run report](runs/task144-20261103/report.json),
[repository/MSRV checks](runs/task144-20261103/checks.json),
[lab commands](runs/task144-20261103/labs.json) and
[source manifest](runs/task144-20261103/source-manifest.json) retain exact
commands, logs, source hashes, compiler identity and executable/package hashes.
Only source/text evidence is committed; no media, state binaries or executables.

An initial repository check caught the README still lacking a source installation
pin for the new candidate. The README now separately documents published 0.9.0
and the unpublished 0.10 source dependency; final required checks all pass.
Earlier exploratory runs remain local and are not substituted for final results.

## Reproduce

```sh
cargo test --test persistent_checkpoint
cargo test --lib m4a_completion_tests
cargo +1.89.0 test --locked --workspace --lib --tests
cargo +1.89.0 test --locked --doc
cargo test --release --test persistent_checkpoint
CARGO_TARGET_DIR="$PWD/target" cargo build --release --offline --manifest-path lab/fuzz/Cargo.toml --bins
target/release/syom_fuzz --seconds 30 --seed 1440
target/release/syom_fuzz_state --seconds 30 --iters 50000 --seed 1441
target/release/syom_fuzz_checkpoint --seconds 30 --iters 5000 --seed 1442
CARGO_TARGET_DIR="$PWD/target" python3 lab/consume/run.py
```

Fuzz iteration counts depend on the time cap and host load; the bounded
checkpoint campaign completes its explicit count quickly. The package consumer
runner regenerates an ignored vendor directory from the candidate archive.
No command dispatches hosted CI or writes to the downstream application.
