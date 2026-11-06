# Root/repository audit evidence — 2026-11-06

See the [audit](../../../../docs/repository-audit-2026-11-06.md) and
[layout policy](../../../../docs/repository-layout.md).

- `inventory.json`: tracked-path inventory before new audit additions. Sizes
  reflect working files at inventory time, not compressed Git objects.
- `branch-integration.json`: exact patch equivalence and unchanged-tree ancestry
  merge for the obsolete `cargo-update` branch.
- `worktree-relocation.json`: four moves, pre/post HEAD/status/dirty-file hashes,
  no process using the old paths, and compatibility-link checks. Unrelated
  foreign worktree/ref inventories are omitted. No dirty files were deleted.
- `scope-guard.json`: all 179 non-test Rust sources and Cargo manifest/lockfile
  unchanged from published 0.10.1 source. No runtime change or new publication.
- `checks/checks.json`, logs and `run-checks.py`: complete local validation.
  1,185 library tests passed, seven ignored; five integration tests and 29
  doctests passed. Offline Python suites passed 67 repository-tool tests and
  36 gigastt-tool tests. One weak Rust text test was intentionally replaced by
  semantic package policy tests. fmt, size/changelog guards, clippy and package
  audit passed. Current local package is not a published artifact.
- `links.json`, `check-links.py`: active local documentation links/anchors;
  no external availability assertion.

Host: Linux x86_64, Rust 1.97.1, Python 3.14.4. Hosted workflow Python is 3.12;
this run does not claim that interpreter or Windows/macOS/ARM validation.
No hosted CI was dispatched. Runtime compatibility was already qualified for
0.10.1; this maintenance does not rerun or relabel that qualification.

Reproduction from the repository root: execute the commands in
`checks/checks.json` or AGENTS.md. The retained runner writes to ignored
`target/root-audit-20261106/checks` and deliberately refuses to overwrite an
existing check directory. Use a fresh output path for a new run; preserve this
historical result. Evidence hashes exclude their own index.
