# Development workflow

GitHub Actions checks are enabled. Develop and validate locally before pushing,
then use the existing push/PR workflows to check the supported hosts.

Before committing code, run the applicable local checks:

```sh
cargo fmt --check
python3 -m unittest discover -s scripts -p 'test_*.py'
python3 scripts/check-doc-links.py
python3 scripts/check-file-size.py
python3 scripts/check-changelog.py
cargo clippy --all-targets -- -D warnings
cargo test --workspace --lib --tests
cargo test --doc
python3 scripts/check-package.py
```

Use the declared minimum Rust version for compatibility-sensitive changes.
Run the relevant isolated consumer/fuzz labs for decoder or streaming changes;
retain reproducible results and state which platforms were actually tested.
Local Linux checks do not establish Windows/macOS/ARM parity.

Retain compact validation summaries: commands, source identities, toolchains,
exit codes and meaningful totals. Keep routine successful stdout, empty logs and
duplicate runs in ignored `target/` or CI artifacts. Commit raw diagnostics or
measurements only when they contain essential evidence a summary cannot retain.

Keep Cargo versions, lockfiles and CHANGELOG.md consistent. Use semantic
versioning, preserve historical measurements, and distinguish a validated
candidate from a published release. Commit completed changes and push them
as requested; publishing crates or deploying a consumer is a separate action.
Do not commit recordings, model weights, binaries or ignored build output.

Keep Git worktrees outside `target/` and other disposable build/cache directories.
Move them with `git worktree move`, preserving uncommitted files; never clean a
build directory that still contains a live worktree. See
[repository layout](docs/repository-layout.md) for tracking and cleanup boundaries.
