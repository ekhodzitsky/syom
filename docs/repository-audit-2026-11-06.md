# Repository audit — 2026-11-06

This audit follows the documentation review of the same date. Three subagents
reviewed repository inventory/tracking, source-size policy, and package/Git
boundaries. The maintainer requested consolidation in `main` and local checks;
GitHub Actions remain paused. This is repository maintenance after the published
0.10.1 release, not a new release or a new runtime qualification.

## Findings and disposition

| Priority | Finding | Disposition |
|---|---|---|
| High | Four dirty live worktrees were physically inside disposable `target/task145/` | Move with each owning repository's Git into sibling `.worktrees/`; verify HEAD/status/dirty-file hashes and retain old-path compatibility links |
| Medium | File-size guard exempted whole filename families, including future files, and described original DSP as vendored | Replace with 21 exact existing implementation paths, explicit reasons and frozen current ceilings; test files retain the ordinary 500-line limit |
| Medium | A Rust test inferred an empty dependency graph from a comment and a few dependency names | Replace it with semantic TOML checks of both source and normalized package manifests; reject arbitrary runtime/build dependencies and native/target configuration |
| Medium | Common credential/model files were not covered by shared ignore rules | Add narrow rules and regression cases; allow placeholder templates and existing intentional evidence formats |
| Low | A remote/local dependency branch appeared unmerged after squash integration | Verify equal patch IDs against PR #8, then record ancestry with an unchanged-tree merge; no replay or dependency rollback |
| Low | Root placement and tracking boundaries were implicit | Document the existing layout, package/Git distinction, archive policy and safe worktree placement |

Before new audit files, the tracked inventory contained 2,378 paths and about
40.6 MB of checkout payload. The root held 13 ordinary shared files: manifests,
lockfile, licenses, README, contributor/agent policy, changelog, benchmark entry
point and Rust/Git settings. All belong there and should be pushed. No root
folder shuffle or new workspace split is justified.

Most apparent bulk is intentional: `lab/` contains 1,856 files/about 30.3 MB of
source and dated evidence. Codec goldens and synthetic fixtures are required
regression inputs. Roughly 11.9 MB of duplicate checkout payload is mainly
repeated historical Cargo metadata; Git shares identical blobs. Deleting those
archives without replacing their references would damage reproducibility for
little repository-storage benefit. Future evidence should reference existing
immutable inventories instead of copying large metadata again.

Inventory/targeted scans found no accidentally tracked build directories, model
weights, native binaries or obvious credential files. This is a bounded review,
not proof that every possible secret pattern is absent. Ignore rules do not
replace staged-content review. Local build output, assistant state, backlog,
recordings and consumer working copies must stay local. The
[layout guide](repository-layout.md) gives the detailed directory/format policy.

## Consolidation without losing work

`cargo-update` at `e520a0cc00214754bcb6aea301f0d1c0611c6390` contains the same
patch as `cbfd5ddfe58198d31d7473d3fa5b4276dadcacb2` already in `main`.
`git cherry` marked it equivalent and stable patch IDs matched. Merge record
`21c37a187df726947b0c1440179b37b037d92e91` uses the existing main tree unchanged
and preserves the original commit as ancestry. It introduces no dependency or
changelog changes. The obsolete duplicate branch can then be retired safely.

The syom FFT worktree has no unique algorithm missing from `main`: the integrated
implementation is `3e540231c9d71e3534be1ef856a91cf419efaead`. Three other nested
worktrees belong to the separate gigastt repository, not syom. Their useful
adapter changes are already preserved as the reviewed
[adapter patch](../lab/consume/gigastt/adapter.patch) and TASK-145 reproduction
snapshot. Moving the dirty working copies preserves local work; importing the
foreign repository wholesale into syom would be incorrect.

The migration receipt records the four exact old/new paths, owning repositories,
HEADs, status and dirty-file checksums. Old-path symlinks remain local for dated
reproduction commands. No dirty checkout or foreign branch is deleted.

## Validation and limits

Run the complete local checklist in [AGENTS.md](../AGENTS.md), offline Python
policy tests and the isolated gigastt tooling tests. The retained
[run evidence](../lab/release-audit/runs/root-audit-20261106/README.md) records
commands, logs, host, counts and source scope. One weak Rust text assertion was
removed and replaced with semantic Python policy controls; the change in Rust
test count is intentional. The existing paused workflow also retains the Python
test command for when hosted checks resume; no Actions run was dispatched.

Library runtime files, Cargo versions, dependencies and the lockfile remain
unchanged. Historical benchmark, failed-run and release evidence remains intact.
No decoder/streaming change requires rerunning performance or fuzz qualification
for this maintenance scope. Only local Linux x86_64 is tested here; this does not
establish Windows/macOS/ARM parity. The symlink-specific Python test skips if a
host cannot create a symlink; it runs on the tested Linux host.

## Further refactoring worth doing

1. Isolate global SIMD controls used by tests/experiments (existing TASK-147).
   Concurrent tests must not silently change one another's backend selection.
2. Inventory doc-hidden public measurement modules (`decode_cmp`, `encode_cmp`,
   `mem_iso`, `ps_measure`) before moving tooling out of the crate. Public hidden
   APIs still need a compatibility plan; do not delete them in a cleanup patch.
3. Replace remaining narrative/document string assertions with structured
   evidence or behavioral checks where they can give false confidence. Keep
   genuine provenance assertions where they enforce a meaningful contract.
4. Extract oversized DSP modules gradually when changing their behavior. Use the
   new exact ceilings to prevent growth; retain PCM/performance proof for each
   extraction rather than undertaking a large cosmetic rewrite.

These are bounded follow-ups, not evidence that the root needs wholesale
reorganization. The present changes repair the concrete safety/policy gaps
without altering the released codec or erasing useful history.
