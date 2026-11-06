# syom 0.10.1 release verification

Release preparation dated 2026-11-06. Publication is authorized after the
repository audit and final local checks pass; registry upload and verification
are recorded separately below. No crate has been uploaded by this preparation.

The production change from 0.10.0 groups the first two AVX-512 FFT stages for
compiler vectorization, retaining the actual twiddles and scalar mul/add/sub
order. There is no new unsafe operation, public API, runtime dependency, build
script or Rust minimum change. The scalar/SSE/AVX paths remain unchanged.
Persistent checkpoint blobs remain bound to the creating codec version;
recreate cached 0.10.0 state when upgrading, as required by the existing
[checkpoint contract](../../docs/persistent-checkpoints.md#version-binding-and-memory).

The runtime is byte-identical to qualified source
`3e540231c9d71e3534be1ef856a91cf419efaead`. The
[original qualification](TASK145.md) and [detailed review](TASK145-REVIEW.md)
retain their unpublished-candidate status at measurement time. Their historical
archives and checksums are preserved rather than relabeled as release runs.

Qualification is Linux x86_64 only. Two preregistered CPU 1 consumer campaigns
pass all six 1.05 latency gates; the CPU 0 scan failure remains unexplained.
All 29 PCM cases, full consumer checks, cold 128 KiB proofs and current-model
HTTP/load/fault qualification are recorded with their exact adapter/source pins.
The raw HTTP equality canary remains false and its differences require the
separate independent presentation policy. Split-mode RSS remains higher than
standard decoding. These findings do not establish cross-platform parity,
universal performance superiority or SOTA recognition quality.

The repository audit distinguishes crate publication from the separately
qualified downstream adapter. TASK-146 is inherited downstream reader
robustness; TASK-147 is pre-existing test-mode isolation. Neither changes this
patch's runtime or blocks this crate release. No consumer default, merge or
deployment is part of publication.

## Repository cleanup and local checks

Only confirmed generated build/cache files are eligible for removal. Tracked
fixtures, licenses, raw successful/failed measurements and reconstruction
sources are retained. A cleanup manifest records exact removed directories and
space measurements; no Git history is rewritten.

Cleanup removed **7,672,926,208 allocated bytes** of Cargo incremental/coverage
caches, old package verification build directories and eight generated ORT
files. Observed free space increased by 7,641,493,504 bytes during cleanup;
concurrent validation can subsequently consume cache space. All four existing
Git worktrees and original models/media remain. Eight stale worktree metadata
entries pointed to already absent directories; their commits remain reachable
from main. Tracked duplicate evidence is retained: Git already deduplicates
identical blobs, and removing those files would break historical manifests.

Final repository checks, Rust 1.89 compatibility, packaged consumers and clean
publication dry-run results are retained under `runs/release-0.10.1/` after
execution. Cargo source versions and lockfiles remain at 0.10.1. CI configuration
is retained, but GitHub Actions is not dispatched; commits use `[skip ci]`.

All seven required local gates passed: 1,186 library tests (seven existing
ignores), five integration tests, 29 doctests, all-target Clippy and package,
format, size and changelog checks. The Python suite passes 36 tests; actual
nine-file handoff validation and model-free historical replay pass.
Rust 1.89 repeats the full test/doc matrix and passes six fresh 128 KiB
cold-stack profiles plus seven release IMDCT tests (one manual probe ignored).
Packaged flows, consumer patterns, exact seeking, checkpoints and strict
completion also pass on Rust 1.89.

Three fixed-seed 30-second fuzz workers pass with zero panics: parser 2,359,
state 54,638 and checkpoint 317,016 iterations. These concurrent correctness
smokes make no new performance claim. The
[preflight manifest](runs/release-0.10.1/preflight.json) records source identity,
commands, toolchains and the separate dirty preparation-package hashes. Those
archives are not upload artifacts; the clean commit is packaged again before
publication.

## Publication status

Registry 0.10.1 was absent at the initial check. A clean release source commit,
verified package archive, registry checksum/download, annotated tag and actual
registry README-example checks will be recorded after successful upload.
