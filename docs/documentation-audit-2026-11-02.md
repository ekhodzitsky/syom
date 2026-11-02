# Documentation and repository layout audit

The main documentation risk is **conflicting descriptions of current support**,
not an untidy root directory. Dated research results, current API contracts,
and release status are mixed together. A reader can encounter an old
“unsupported” statement after seeing a supported profile in the guide.
The README refactor improves entry and navigation; it does not resolve all
of that historical-status debt.

This audit is dated **2026-11-02**, against `1690601` plus the accompanying
README refactor. Priorities below concern documentation and maintenance;
they are not newly discovered codec defects or conformance findings.

## Scope and verification

Reviewed the root documents, package boundary, API guide, documentation
indexes, corpus coverage/conformance notes, footprint reports and release
evidence. Before this change the repository tracks **12 root files**,
106 Markdown documents, 639 files under `lab/` and 99 under `src/goldens/`.
A parsed link scan of the tracked documents, the two new reader guides
and this audit checked **151 relative links across 109 Markdown files**,
finding no missing targets or heading anchors. Existing external URLs were not exhaustively checked.

The new README example was executed against the **published syom 0.8.1**
on Rust 1.97.1 and Rust 1.89.0, with the stated output. Comparison facts were
reviewed against pinned published crate sources; their archive checksums
match the isolated audit's lockfile and the reviewed files match archive
bytes. No fresh competitor benchmark or coverage measurement was performed.

## Findings requiring follow-up

### P1  Coverage pages mix obsolete product decisions with corpus gaps

[corpus/coverage.md](../corpus/coverage.md#profiles--bitrates) still says
960-sample frames are a 0.x no-go and LD480 remains `Unsupported`.
The [current guide](../src/guide.rs) and
[frame-grid qualification](../lab/frame960/REPORT.md) describe supported
960-sample LC and 480-sample LD. Its layouts table also marks 7.1 as a
TASK-62 gap, although `corpus/manifest.json` contains `lc-mc71-adts`,
`lc-mc71-m4a` and `lc-mc71p-adts`.

**Impact:** corpus registration, lack of natural recordings, implementation
support and independent qualification can be mistaken for the same status.
A real corpus gap must not be marked covered just because decoding works.

**Refactor:** separate columns for implementation support, registered
fixtures and independent qualification. Link the implementation column to
the canonical guide. Retain earlier no-go decisions in a dated history
section, with superseding evidence. The read-only generator comparison
found all **82 committed cases identical** to `cases()`; this is a stale
coverage narrative, not a demonstrated manifest-generation mismatch.

### P1  Initial syntax findings read as unresolved bugs

[corpus/conformance/go-nogo.md](../corpus/conformance/go-nogo.md) describes
one-rate explicit HE ASC, ignored LATM subframes, incorrect `latmGetValue`
and PCE omissions as present-tense findings. Its non-claims still say
explicit two-rate AOT5 is rejected. These are the original task inventory,
while current tests, transport code and shipped goldens include subsequent
fixes. The same page preserves the superseded LD480/960 rejection policy.

**Impact:** a reader cannot distinguish a historical motivation from a
known current correctness defect. Neither this old list nor a passing test
suite establishes ISO 14496-26 certification.

**Refactor:** label the inventory as historical at the top. Add a short
supersession table linking each original finding to the current contract
and independent regression/evidence. Keep the original rationale and
unobtained normative-vector limitations intact.

### P2  Old reports retain current-policy language

- [BENCH.md](../BENCH.md) opens with “Current 0.8.0” and later says “current
  tree” about a dated 2026-09-29 rerun. Its C-peer-unavailable wording also
  needs a nearby indication that it describes that historical harness.
- [lab/footprint/REPORT.md](../lab/footprint/REPORT.md#toolchain-policy)
  describes Rust 1.88 and its CI job as toolchain policy. Current Cargo and
  CI configuration declare 1.89.
- [corpus/footprint/report.md](../corpus/footprint/report.md#usability-gaps-ranked)
  lists raw AU input and borrowed encode as gaps and retains old toolchain
  floors. Its 2026-09-14 measurement date is useful, but policy conclusions
  and unresolved-work labels are not clearly superseded.

**Refactor:** put a small date/revision/status/superseded-by block on old
reports and replace relative-time labels with explicit measurement dates.
Preserve all timing, size, toolchain and negative-result rows. Do not
relabel measurements as results of the current source tree.

### P2  Release evidence has a weak entry point

[lab/release-audit/REPORT.md](../lab/release-audit/REPORT.md) begins with
the original 0.8.0 conditional/no-go findings; the current candidate section
is more than 200 lines later. [lab/README.md](../lab/README.md) foregrounds
exact-seek evidence and does not lead with the broader cold-start/package
qualification. The new root README now links directly to the current section,
but other entry paths still expose the ambiguity.

**Refactor:** add a compact index at the top: published 0.8.1, locally
qualified 0.9.0 candidate, and historical 0.8.0 audit. Keep publication,
consumer deployment and comparative superiority as separate statuses.

### P2  A platform-specific stack result can look universal

The migration paragraph in [src/guide.rs](../src/guide.rs) says cold
initialization fits the optimized 128 KiB thread-stack budget. The retained
regression evidence covers six fixture families on Linux x86_64, including
MSRV 1.89; it does not freshly qualify every target ABI or content shape.

**Refactor:** describe heap-based table construction as the implementation
fact and place the tested Linux release/profile scope beside the stack
number. Do not weaken the actual regression or replace the preserved
baseline failures with warm-process results.

### P2  Human contributor setup is incomplete

[AGENTS.md](../AGENTS.md) provides checks and the hosted-CI restriction,
but there is no dedicated contributor entry describing prerequisites,
lab isolation or generated outputs. `scripts/check-package.py` imports
`tomllib` and invokes external `tar`; the workflow selects Python 3.12.
That is more information than a new contributor gets from “run the checks.”

**Refactor:** add a useful `CONTRIBUTING.md` describing the tested Python
setup, Rust toolchains, `tar`, offline checks, one shared build directory
and the package boundary. Distinguish verification from generators that
rewrite corpus/provenance files. Link to authoritative commands rather
than creating another independently maintained policy list. A security
reporting policy should identify a real approved channel before adding
`SECURITY.md`; an empty template would not help.

### P2  Local metadata protection is not portable

`.git/info/exclude` hides agent-state directories and `backlog/`, while
those rules are absent from the tracked `.gitignore`. This protects the
current checkout, but the local excludes do not travel with a clone.
There is no evidence here that private files have been committed.

**Refactor:** document clone/bootstrap hygiene and the intended tracking
policy. Decide explicitly which shared ignore rules belong in `.gitignore`;
do not silently start tracking a local backlog or publish agent state.

### P2  Coverage and current hosted status remain unmeasured

The old 93.88% library-line measurement excludes tests, benches and std;
it is not current candidate coverage. Hosted CI is paused. A branch badge
shows its last hosted state, not validation of the present README commit.

**Refactor:** keep those distinctions visible, as the new README does.
When capacity and scope permit, run a fresh local coverage measurement
with a declared revision, metric and exclusions. Add a badge only when it
points to that real scoped report. Do not dispatch hosted CI to satisfy
README cosmetics.

### P3  Laboratory and script indexes lag the repository

The adapter table in [lab/README.md](../lab/README.md) omits existing
`fmp4`, `frame960`, `mobile` and `jcodec` work. Scripts are listed mostly
where a historical task happened to use them. For example,
`build_corpus_manifest.py` and `build_oracle_provenance.py` write tracked
outputs, while the corresponding verifiers are read-only checks.

**Refactor:** finish the lab index and provide a compact command table:
verify, generate, measure; prerequisite; output path; mutates tracked data.
Clarify that “natural excerpts not obtained” in the registered corpus does
not erase later external consumer-recording qualification.

### P3  Wording tests are weaker than an executable quick start

[src/docs_tests.rs](../src/docs_tests.rs) checks version strings and a few
required phrases. The unpublished-candidate fallback cannot establish
that a registry example or a future publication status is correct.

**Refactor:** retain the lightweight invariants, but make the release
checklist execute the actual README example against its recommended
published dependency. This audit did that on current Rust and MSRV. Avoid
adding tests that merely duplicate every heading or editorial sentence.

## Root structure verdict

Keep the conventional root files: `Cargo.toml`, `Cargo.lock`, toolchain/
formatter files, `README.md`, `CHANGELOG.md`, licenses/notices, `.gitignore`,
`.gitattributes`, `AGENTS.md` and the historical `BENCH.md` entry.
There is no evidence that moving these files improves use or packaging.

Keep `src/goldens/`, `corpus/` and `lab/` in their current locations for now.
Tests, generators, manifests and links depend on their paths. Their size
or file count alone is not a reason to reorganize them. Natural recordings
and build artifacts remain distinct from intentionally tracked goldens.
The library-only Cargo include list is explicit and package-audited;
`docs/` is intentionally outside the crate, so root links use full GitHub
URLs to remain reachable from a packaged README.

The sustainable division of responsibility is:

| Reader question | Canonical destination |
|---|---|
| What is it, how do I try it? | Root README |
| What does an API guarantee? | Public API/Rustdoc and `src/guide.rs` |
| Which integration route/format applies? | `docs/usage-and-validation.md` |
| How do pinned alternatives differ? | `docs/comparison.md` and primary sources |
| What was measured on which source? | Dated `lab/` report plus raw evidence |
| What inputs/licenses/provenance are registered? | Corpus manifests and verifiers |
| How do I contribute locally? | Proposed contributor guide, with AGENTS policy preserved |

## Applied changes and recommended order

Applied in this documentation change: the 535-word root README, a complete
verified published-crate quick start, compact comparison with an unfavorable
HE v2 result, badge/version provenance, and two linked detail guides. Existing
measurements, source labels and integration links were preserved. No runtime
code, Cargo versions, package include policy, corpus data or CI workflows
were changed. The wider findings above remain proposed work.

Next, address the two P1 status/supersession problems in one small change.
Then improve release/lab navigation and contributor setup. Finally add
lightweight drift checks and a fresh scoped coverage result when warranted.
Large physical moves should follow an explicit path-migration plan, not
precede the information-architecture fixes.

## README v1 acceptance

| Check | Before | After | Evidence or remaining gap |
|---|---|---|---|
| Purpose | Partial | Pass | Opening states task, output and zero-dependency boundary |
| Hook | Partial | Pass | Immediate encode/decode scenario with observable output |
| First run | Partial | Pass | Actual registry 0.8.1 execution on current Rust and 1.89 |
| Alternatives | Partial | Pass | Two principal alternatives, real selection reasons, pinned source verification |
| Benchmarks | Pass | Pass | Absolute historical medians, environment, negative HE v2 case and raw evidence |
| Badges | Partial | Partial | Downloads/MSRV/provenance added; current coverage and hosted validation remain unavailable |
| Constraints | Pass | Pass | Candidate, replay, quality and platform limits stay explicit |
| Presentation | Partial | Partial | English, 535 words, parsed HTML/tables and links checked; browser visual review unavailable |

No indicator was declared inapplicable: coverage is applicable but lacks a
current measurement. The preview was rendered to HTML; no browser was
available for a screenshot or light/dark visual inspection. The machine
[verification record](audit/readme-v1-2026-11-02.json) retains example output,
primary package hashes, commands and limitations. These partial checks are
reported rather than converted into fabricated badge or visual results.
