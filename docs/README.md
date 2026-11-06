# syom documentation

[Project overview and first example](../README.md) ·
[Published API guide](https://docs.rs/syom/0.10.1/syom/guide/)

Start with the published 0.10.1 crate. syom handles AAC and its supported
containers, returns planar `f32` at the stream's native rate, and has no runtime
dependencies. It does not resample or perform speech recognition.

## Use and integrate

| Task | Guide |
|---|---|
| Install and run a complete example | [Root quick start](../README.md#quick-start) |
| Choose a profile, container, output layout or streaming API | [Usage and validation](usage-and-validation.md) |
| Encode/decode, borrowed PCM, limits, raw AUs and seeking | [Compiling API examples](https://docs.rs/syom/0.10.1/syom/guide/) and [source guide](../src/guide.rs) |
| Persist eligible LC decoder state or validate M4A completion | [Checkpoint contract](persistent-checkpoints.md) |
| Compare integration boundaries and historical timings | [Pinned comparison](comparison.md) |

Checkpoint blobs are version/build-bound. Recreate cached 0.10.0 state when
moving to 0.10.1. Exact seeking can require prefix replay; persistent decoder
state and exact-seek logical positions are different contracts.

## Verify and contribute

| Need | Entry point |
|---|---|
| Verify the published version, source/tag and archive | [0.10.1 registry receipt](../lab/release-audit/RELEASE-0.10.1.md) |
| Understand current validation and platform limits | [Validation map](usage-and-validation.md#validation-and-integration) |
| Interpret version, CI, coverage and MSRV badges | [Badge sources and scope](usage-and-validation.md#badges-and-measurement-gaps) |
| Set up local development and run checks | [Contributing](../CONTRIBUTING.md) and [shared workflow](../AGENTS.md) |
| Reproduce consumer, performance, quality or fuzz experiments | [Lab index](../lab/README.md) |
| Verify fixtures and acquisition provenance | [Corpus inventory](../corpus/README.md) |
| Read behavior changes | [Changelog](../CHANGELOG.md) |

Hosted CI is paused. Current release checks are local Linux x86_64, including
Rust 1.89; they do not establish Windows/macOS/ARM parity, full ISO conformance,
physical-mobile readiness or listening superiority.

## Dated evidence

Reports retain the version, source identity and status at measurement time.
Use the release receipt for publication status; a historical candidate report
does not become a new benchmark when a version is published.

| Evidence | Original scope |
|---|---|
| [TASK-145 qualification](../lab/release-audit/TASK145.md) and [review](../lab/release-audit/TASK145-REVIEW.md) | 0.10.1 candidate, exact downstream adapter, decoder/HTTP/load/fault checks |
| [TASK-144](../lab/release-audit/TASK144.md) | Persistent LC state and strict completion before 0.10.0 publication |
| [Earlier release receipts](../lab/release-audit/REPORT.md#release-evidence-index) | 0.10.0, 0.9.0, 0.8.1 and the original 0.8.0 audit |
| [Documentation review, 2026-11-06](documentation-audit-2026-11-06.md) | Example-first README, task index, current release routes and corrected dependency examples |
| [Documentation audit, 2026-11-02](documentation-audit-2026-11-02.md) | Earlier structure, wording and evidence corrections |

The native codec, a consumer adapter, a deployed service and SOTA eligibility
are separate validation scopes. Successful runs and known failures remain in
their original reports.
