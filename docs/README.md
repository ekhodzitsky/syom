# syom documentation

[Overview and examples](../README.md#examples) ·
[0.10.2 API guide](https://docs.rs/syom/0.10.2/syom/guide/)

Use the published crate for its documented API. This checkout also contains
unreleased gapless fixes; the Cargo version alone does not identify them.
[Release versus checkout](verification.md#release-and-checkout).

## Use syom

| Task | Guide |
|---|---|
| Install and run a complete example | [Examples](../README.md#examples) and [install](../README.md#install) |
| Choose a reader, output layout or streaming API | [Usage](usage-and-validation.md#choose-an-api) |
| Check supported profiles and containers | [Format matrix](usage-and-validation.md#formats-and-channel-layout) |
| Encode/decode, limits, borrowed PCM and raw AUs | [Published API examples](https://docs.rs/syom/0.10.2/syom/guide/) · [checkout source guide](../src/guide.rs) |
| Acquire an exact M4A interval | [Seeking and prefix replay](usage-and-validation.md#exact-interval-acquisition-since-090) |
| Save native LC decoder state or inspect completion | [Checkpoint contract](persistent-checkpoints.md) |
| Understand priming and tag behavior | [Gapless timing](usage-and-validation.md#gapless-timing) |
| Compare library boundaries and dated timings | [Pinned comparison](comparison.md) |

## Verify and contribute

| Task | Guide |
|---|---|
| Check published versus unreleased status | [Release and checkout](verification.md#release-and-checkout) |
| Inspect actual host/native results and their limits | [Verification](verification.md) |
| Interpret CI, MSRV and coverage | [Badge sources](verification.md#badges-and-measurement-gaps) |
| Build, validate examples and check documentation links | [Contributing](../CONTRIBUTING.md) · [canonical checklist](../AGENTS.md) |
| Decide what belongs in Git | [Repository layout](repository-layout.md) |
| Reproduce consumer, fuzz, speed or quality experiments | [Lab index](../lab/README.md) |
| Inspect corpus provenance | [Corpus inventory](../corpus/README.md) |
| Read behavior changes | [Changelog](../CHANGELOG.md) |

## Dated reviews

These records describe their original revisions; they do not override current
usage or verification. Benchmark, consumer, deployment and native codec scopes
remain separate.

| Record | Scope |
|---|---|
| [Documentation audit, 2026-11-09](audit/readme-2026-11-09.json) | README/reference structure, current status, links and published examples |
| [0.10.1 registry receipt](../lab/release-audit/RELEASE-0.10.1.md) | Published source/tag, archive and registry examples |
| [Gapless repeat audit](../lab/release-audit/runs/issue9-reaudit-20261109/audit.json) | 0.10.2 tag handling and native Apple interoperability |
| [TASK-145 qualification](../lab/release-audit/TASK145.md) · [review](../lab/release-audit/TASK145-REVIEW.md) | Exact 0.10.1 candidate and downstream adapter; not a deployment |
| [TASK-144](../lab/release-audit/TASK144.md) | Persistent LC state and strict completion before 0.10.0 publication |
| [Earlier release records](../lab/release-audit/REPORT.md#release-evidence-index) | Original 0.10.0, 0.9.0, 0.8.1 and 0.8.0 scopes |
| [Repository audit, 2026-11-06](repository-audit-2026-11-06.md) | Tracking policy, branch/worktree reconciliation and tooling |
| [Documentation review, 2026-11-06](documentation-audit-2026-11-06.md) | Earlier example-first structure and release routes |
| [Documentation audit, 2026-11-02](documentation-audit-2026-11-02.md) | Earlier wording and evidence corrections |
