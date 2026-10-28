# syom 0.8.1 publication and downstream handoff

[syom 0.8.1](https://crates.io/crates/syom/0.8.1) was published on
2026-10-28 at 21:02:54 UTC (2026-10-29 Moscow). Annotated tag `v0.8.1`
points to release commit `580f2546396870503eced5609a491a5e4bcf2797` and is
pushed to origin. The release includes the fill-allocation and PNS seek fixes,
regressions and [local load/fault qualification](HTTP-LOAD-0.8.1.md).

The downloaded registry package is **449644 bytes**, SHA256:

```text
9720ac01d202d983ee3b83bce3c72c4d0822a5e5533feadd03946e897b7a20d7
```

The registry's checksum matches the downloaded bytes. Packaged VCS metadata
points to the clean release commit, and all **168 runtime Rust source files**
match the locally qualified sources. The published archive differs from
earlier candidate archive hashes because README/CHANGELOG and VCS metadata
changed; runtime sources did not. It retains Rust 1.89 and zero runtime
dependencies. [Machine-readable publication verification](runs/http-load-0.8.1/published.json).

Local release checks passed: formatting, Clippy with warnings denied,
file-size/changelog/package checks, 1125 library tests, one integration test,
28 doc tests and an independent packaged build through `cargo publish --dry-run`.
The actual `cargo publish --locked` succeeded from a clean working tree.
GitHub CI was skipped; tag pushes do not match the existing main-only push
trigger. These checks establish the tested Linux scope, not hosted platform
matrix results.

## Consumer upgrade

[gigastt-version.patch](gigastt-version.patch) changes only the integration's
`syom` version requirement from 0.7.1 to 0.8.1 and the registry package's
version/checksum in `Cargo.lock`. `cargo update -p syom --precise 0.8.1`
generated the lockfile in an isolated copy; all other package entries remained
identical. `git apply --check` confirms that the complete patch applies to the
sibling gigastt working tree without modifying it.

The isolated consumer copy also passed `cargo check --release --locked -p
gigastt --no-default-features --features gigastt-core/ort-load-dynamic` against
registry syom 0.8.1, using the same lab ORT configuration as the canary. This
compile check is not a production feature-matrix result; the patch does not
change ORT configuration. [Check log](runs/http-load-0.8.1/published-consumer-check.log).

The integration itself currently exists as uncommitted consumer changes;
upstream gigastt main still uses the previous AAC path and has no syom
dependency. Applying this version patch to upstream main alone is therefore
not a complete integration. The consumer's existing changes were preserved.
The patch is ready to accompany that integration's own review and commit.

Once the integration is committed on its intended consumer branch, build with
the deployment's normal features/runtime, repeat the paired canary, then
use the [readiness/rollback sequence](HTTP-CANARY-0.8.1.md#reproduce-and-promote).
No live deployment endpoint or traffic router was configured by this work;
the installed gigastt binary and service state were preserved.
