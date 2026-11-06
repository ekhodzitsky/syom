# Repository layout and tracking policy

The root is the entry point for users and build tools. Keep README, contribution
and agent instructions, Cargo manifests/lockfile, toolchain/formatter settings,
changelog, benchmark overview and license notices there. These are shared project
files and belong in Git. Moving them merely to shorten a directory listing would
make discovery and tool defaults worse.

| Location | Purpose | What belongs in Git |
|---|---|---|
| `src/` | Codec/library implementation and API guide | Source, private unit tests near their modules, minimal deterministic codec fixtures |
| `tests/` | Public integration contracts | Tests and their minimal reproducible inputs |
| `benches/` | Rust benchmark entry points | Workload definitions and source; generated reports are separate evidence |
| `docs/` | Current guides and dated reviews | User/developer documentation; historical reports retain their original scope |
| `scripts/` | Repository verification and corpus tooling | Standard-library tooling, policy configuration and synthetic regression tests |
| `corpus/` | Fixture inventory and provenance | Manifests and documented minimal regression inputs; natural recordings stay local |
| `lab/` | Isolated experiments and qualification | Drivers, manifests, pins and reviewed text evidence, including relevant failures |
| `.github/` | Hosted workflow configuration | Retained configuration; Actions remain paused until explicitly resumed |
| `target/` | Disposable build and check output | Nothing: ignored caches, packages and temporary runs |
| Local assistant/planning state | Machine-specific work | Nothing: ignored `backlog/`, assistant directories and local bindings |

A physical file being useful locally does not make it suitable for Git. A file
tracked in Git does not necessarily belong in the published crate: Cargo's
package rules and `scripts/check-package.py` define that smaller boundary.
`Cargo.lock` remains tracked for reproducible development, even though library
consumers resolve their own dependencies.

## Before staging

Inspect `git status --short --ignored`, `git diff` and `git diff --cached`.
Do not force-add credentials, environment secrets, natural recordings, model
weights, native executables, vendored build caches or generated Cargo output.
The shared ignore file blocks common examples, but is not a secret scanner or a
complete list of private filenames. Allowed `.env.example`, `.env.sample` and
`.env.template` files must contain placeholders only.

Intentional AAC/MP4 goldens, synthetic `.s16`/`.f32`/`.bin` fixtures, provenance
JSON and reviewed text logs may be essential regression or qualification data.
Do not apply blanket extension bans to them. Inspect content, provenance and
size before adding evidence. Preserve historical measurements and failures;
remove or consolidate an archive only with an inventory of references and a
reproducible replacement. Do not rewrite old measurements to look current.

## Worktrees and cleanup

Create live worktrees outside `target/` and other disposable build/cache
locations, for example in a sibling `Projects/.worktrees/` directory. Each
worktree belongs to its own repository, even when a consumer checkout happens
to be under a library directory. Manage it through its owning Git repository.
Use `git worktree list --porcelain` before cleanup and `git worktree move` to
relocate it; preserve HEAD, dirty-file hashes and untracked files. Do not use
force-removal as a shortcut for an occupied build directory.

The 2026-11-06 migration left compatibility symlinks at the four old TASK-145
paths for historical local commands. Their live worktrees now reside outside
`target/`. Those links and their targets are local state, not repository content.
A cleanup tool must not follow those links and erase their destinations. Verify
the actual paths and live processes before cleaning build output.

## Refactoring boundaries

Keep this as one library crate. The existing stream/container modules and the
separation between production source, integration tests and isolated labs do
not justify a workspace split. Large DSP modules need incremental extraction
with PCM, performance and compatibility checks, not a directory-only shuffle.

The file-size guard allows 400 implementation lines and 500 test lines. Existing
oversized implementation modules have exact, justified ceilings in
[`file-size-exceptions.json`](../scripts/file-size-exceptions.json); naming a new
file `sbr_*` or `ps_*` grants no exception. Reduce an exception's ceiling when
extracting code and remove it once the file fits the ordinary limit.

Doc-hidden public measurement helpers remain compatibility surface. A future
migration to developer-only tooling needs an API inventory and a compatible
transition. Global SIMD test controls also require isolation before safe
parallel experiments; neither concern warrants an unreviewed runtime/API change
as part of root cleanup. See the [dated audit](repository-audit-2026-11-06.md).
