# Documentation review — 2026-11-06

This refactor follows the example-first README and task-indexed documentation
used by `ryf` and `agent-youtube`, together with root README standard v1.
It changes documentation only. Published syom 0.10.1, its tag and registry
archive are unchanged; the changelog records this work under Unreleased.

## Findings and changes

| Finding | Resolution |
|---|---|
| Release implementation notes appeared before the first useful result. | Move release/migration detail to Status and limits; put the runnable example directly after the short opening, badges and three navigation links. |
| There was no `docs/README.md` entry point. | Add task-oriented user guides, verification/development routes and a separate dated-evidence map. |
| Exact-seek examples still required `syom = "0.9"`, including a path dependency incompatible with this 0.10.1 checkout. | Use the current `0.10` requirement for registry and local examples; validate local dependency resolution. |
| Usage validation began with 0.9 release evidence, despite the current 0.10.1 publication. | Lead with the current release receipt and scoped consumer qualification, then retain older checks under an explicit historical heading. |
| The comparison introduction incorrectly described the root overview as targeting 0.9.0. | Identify the root's pinned 0.10.0 integration row and the current 0.10.1 crate separately. Peer facts remain dated 2026-11-02; timings remain syom 0.8.0. |
| The lab index mixed successive release announcements with experiment navigation. | Separate current evidence, reproducible experiments and historical qualification. Replace tracking IDs in the evidence column with actual pin/report links. |
| Detailed pages did not provide a common route back to documentation. | Add index links to usage, comparison, checkpoint and contributor entry points. |

The complete root example, expected stdout, installation version, badge sources,
unfavorable HE v2 timing, platform limits and checkpoint migration contract are
preserved. No API, dependency, source-code, corpus or benchmark change is made.

## Reference patterns

The reviewed local `ryf` README uses a short codec/output description, concrete
examples and a compact guide index. `agent-youtube` separates user workflows,
current operation and dated qualification in its README and documentation index.
Those structural patterns are adapted to a library; CLI setup, unrelated codec
capabilities and other projects' measurements are not copied into syom.

The existing README v1 baseline remains the editorial reference: English prose,
a complete first result, material integration comparisons, measured evidence
with its limits, accurate badges and links to deeper documentation. The root
is about 550 prose/table words excluding fenced code and commands.

## Scope and evidence

| README v1 check | Status | Disposition |
|---|---|---|
| Purpose | Pass | The opening states AAC, planar `f32`, native rate and dependency boundary. |
| First result | Pass | The self-generated encode/decode example retains its observable output and stated registry version. |
| Alternatives | Pass, pinned scope | The integration table retains exact versions and dates; it does not rank today's latest releases. |
| Measurements | Partial for current performance | The historical 0.8.0 table retains absolute units, protocol, HE v2 loss and source links. No new peer benchmark is claimed. |
| Badges | Partial for hosted freshness | Registry indicators, dated local coverage, declared MSRV and last hosted main status retain distinct meanings. Hosted CI is paused. |
| Constraints | Pass | Checkpoint/version binding, prefix replay, platform gaps and unproved encoder leadership remain explicit. |
| Navigation | Pass | User tasks, contributor checks, current publication and historical evidence have separate entry points. |

The benchmark/peer comparison remains a dated snapshot, not fresh 0.10.1 peer
measurements. Hosted MSRV/coverage freshness cannot be claimed while CI is
paused; the documentation points to actual local results instead. This is
intentional scope disclosure, not a replacement measurement.

## Verification

All seven required repository gates pass, with 1,186 library tests (seven
existing ignores), five integration tests, 29 doctests and the package audit.
The Python suite passes 36 tests. The exact README example passes against
registry 0.10.1 on Rust 1.97.1 and MSRV 1.89, printing `1 channel at 48000 Hz`.
The documented local path dependency resolves to the 0.10.1 checkout.

The first suite attempt rejected the removed explicit `syom = "0.10"` install
text. That requirement and the per-band M/S default were restored before the
successful repeat; the test was not weakened. The failed attempt is retained
alongside the [successful check log](../lab/release-audit/runs/documentation-20261106/checks/checks.json).

The [link audit](../lab/release-audit/runs/documentation-20261106/links.json)
checks active local targets/headings and own-repository main links; it does not
claim external-service availability. The
[scope guard](../lab/release-audit/runs/documentation-20261106/scope-guard.json)
confirms unchanged runtime/Cargo, historical artifacts, comparison evidence,
historical timing section and executable Rust example.

No GitHub Actions dispatch, new crate upload or consumer deployment is part of
this documentation refactor.
