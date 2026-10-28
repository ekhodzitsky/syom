# Development workflow

GitHub Actions capacity is currently limited. Develop and validate locally;
do not dispatch GitHub CI until the maintainer explicitly lifts this restriction.
Use `[skip ci]` in commit messages when pushing so the existing push/PR workflows
do not consume Actions capacity. Keep the workflow configuration available for
when hosted checks resume.

Before committing code, run the applicable local checks:

```sh
cargo fmt --check
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

Keep Cargo versions, lockfiles and CHANGELOG.md consistent. Use semantic
versioning, preserve historical measurements, and distinguish a validated
candidate from a published release. Commit completed changes and push them
as requested; publishing crates or deploying a consumer is a separate action.
Do not commit recordings, model weights, binaries or ignored build output.
