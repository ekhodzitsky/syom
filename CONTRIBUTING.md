# Contributing to syom

[User documentation](docs/README.md) · [Release verification](lab/release-audit/RELEASE-0.10.1.md)

Start with [AGENTS.md](AGENTS.md), the canonical development and validation
policy. Hosted GitHub checks are paused; validate locally and use `[skip ci]`
in pushed commit messages until the maintainer lifts that restriction.

## Local setup

- Install Rust through rustup. [rust-toolchain.toml](rust-toolchain.toml)
  selects Rust 1.97.1 with rustfmt and clippy for ordinary development.
  The declared minimum is Rust 1.89 in [Cargo.toml](Cargo.toml); install it
  separately for compatibility checks: `rustup toolchain install 1.89`.
- Use Python 3.12, matching the configured workflow. The scripts use the
  standard library; `tomllib` requires Python 3.11 or newer. Commands below
  assume that `python3` selects that interpreter.
- Have `tar` on PATH: the package audit extracts the generated `.crate`
  with it. Git and Cargo are required. Optional native, Java, browser and
  mobile labs have their own prerequisites and pins in [lab/](lab/README.md).

Run commands from the repository root. Ordinary Cargo commands share the
ignored root `target/`; reuse it across routine checks. Keep live Git worktrees
outside build/cache directories, for example in a sibling `.worktrees/`
directory. Follow the [repository layout](docs/repository-layout.md) before
cleaning or adding files. Isolated lab
manifests may have their own target directory. Follow the lab's recorded
`CARGO_TARGET_DIR`, toolchain and flags when reproducing a result. Do not
run competing timing jobs together. When comparing different revisions
under the same package version in a shared target, explicitly rebuild the
compared library and retain the commands so cached artifacts cannot stand
in for the source under test.

Shared [.gitignore](.gitignore) rules protect root-local `backlog/`, agent
state directories and local assistant bindings in every clone. `AGENTS.md`
is intentionally tracked as the shared workflow. `.git/info/exclude` is
still available for additional machine-specific exclusions; it is not copied
by Git. Review `git status --short --ignored` and the staged diff before
committing. Do not force-add private planning state, credentials, recordings
or build output merely because a local tool generated them.

## Checks, generation and measurements

Run the applicable complete check list in [AGENTS.md](AGENTS.md#development-workflow)
before committing. Compatibility-sensitive changes also need the declared
minimum Rust version; decoder and streaming changes need the relevant
isolated consumer/fuzz labs. Record the actual host and toolchain. Local
Linux results do not establish Windows, macOS or ARM parity.

| Purpose | Command or entry point | Outputs and tracked-file effects |
|---|---|---|
| Required local checks | [AGENTS.md](AGENTS.md#development-workflow) | Cargo writes build/test artifacts under `target/`; `cargo fmt --check` checks formatting without rewriting sources. |
| Published README example | `python3 scripts/check-readme-example.py` and `--msrv` | Pins the stated registry version, builds a disposable app under `target/`, and checks actual stdout; optional `--offline` uses cached dependencies. Does not rewrite tracked files. |
| Library-line coverage | `cargo llvm-cov --lib` with the [recorded exclusions](lab/baseline/COVERAGE.md#local-measurement-2026-11-02) | Requires cargo-llvm-cov and llvm-tools; creates instrumented output under `target/`. Keep source revision, metric and exclusions with retained reports. Not a hosted-CI run. |
| Offline tooling tests | `python3 -m unittest discover -s scripts -p 'test_*.py'` | Exercises package, size, ignore and corpus/acquisition policies using synthetic local inputs. |
| Package audit | `python3 scripts/check-package.py` | Builds and extracts `target/package/syom-<version>.crate`, runs packaged guide doctests; may update Cargo's index/cache. It does not publish. |
| Verify corpus and provenance | `python3 scripts/verify_corpus.py`, `python3 scripts/verify_oracle_provenance.py` | Check committed inventories and fixture hashes; no regenerated manifest. See [corpus/README.md](corpus/README.md). |
| Verify syntax inventory | `python3 scripts/verify_conformance_inventory.py` | Checks the committed conformance inventory; no ISO conformance certificate is produced. |
| Regenerate corpus inventory | `python3 scripts/build_corpus_manifest.py` | **Overwrites tracked** `corpus/manifest.json`; inspect and commit the intended diff. |
| Regenerate oracle provenance | `python3 scripts/build_oracle_provenance.py` | **Overwrites tracked** `corpus/oracles/provenance.json`; inspect the diff and preserve explicit evidence gaps. |
| Measure Rust benchmarks | `cargo bench --bench baseline`, [BENCH.md](BENCH.md) | Writes build output and prints measurements; Criterion benches also write reports under `target/criterion/`. Retaining a report in Git is a separate reviewed change. |
| Reproduce isolated labs | [lab/README.md](lab/README.md) and the selected lab's pin/report | May build drivers, generate fixtures or write evidence. Inspect each command's output paths before running; preserve historical reports and do not stage recordings or build output. |

Do not run generators merely to satisfy a verification failure. Identify
whether the fixture, inventory or implementation changed, then regenerate
only the intended derived data. Keep natural recordings, model weights,
native binaries and ignored build output out of commits.

## Preparing a change or release candidate

Keep Cargo versions, lockfiles and [CHANGELOG.md](CHANGELOG.md) consistent
with semantic versioning. Describe the behavior changed, validation run,
and any remaining limitation. Preserve dated measurements with their
source revisions and workload definitions.

For documentation changes affecting onboarding, verify the actual
[README quick start](README.md#quick-start) against its stated published
dependency. The opt-in checker extracts that code and expected output,
pins the stated release, and rebuilds a disposable app under `target/`:

```sh
python3 scripts/check-readme-example.py
python3 scripts/check-readme-example.py --msrv
```

Use `--offline` when the crate and toolchains are already cached. The
checker shares the root build cache and does not rewrite tracked files.
A local path dependency does not validate the registry example. This
check is intentionally separate from offline unit tests; it does not infer
registry publication status or make candidate-only APIs part of a release.

Before proposing a release, check that the README, changelog, package
version and qualification report agree about what is published and what
is an unpublished candidate. Current evidence is indexed in
[usage and validation](docs/usage-and-validation.md#validation-and-integration).
Passing local checks and committing or pushing a candidate are separate
from publishing a crate or deploying a consumer, as defined in AGENTS.md.
