# Fractional M4A edit duration qualification

The fixes qualified below shipped in [syom 0.10.3](RELEASE-0.10.3.md).
The candidate versions, source identities and measurements below retain
their original pre-publication scope.

2026-11-09, Linux x86_64 only. Unreleased changes on syom
`a61cc6748c3e20087cbba749d060e5d3a3677d37` (package version remains
0.10.2); this is not a published release. No Windows, macOS or ARM run.

The initial implementation's changed `src/` files, including
`src/isomp4/timing.rs`, have aggregate
SHA-256 `a0b72ee643a5a3e90683ac2156a50df540568977cb7253ddaeec58fd7c4a017f`.
To reproduce that identity, sort the eight changed source paths and hash
the concatenation of each UTF-8 path, NUL, file bytes and NUL.

## Behavior

Movie durations round down to whole output samples. A flat edit is capped
against media remaining after priming only when its overhang is strictly
less than both one movie tick and one millisecond. Larger inconsistencies
still reach strict completion checks. Media-time offsets still require
exact conversion. Missing indexed payloads and unfinished priming remain
errors; fragmented input has no assumed final media bound.

The two reported examples produce 54,419 and 11,342,848 native samples.
Synthetic regressions cover decode, slice/seekable streaming, probe, exact
presentation seeking, trim reporting, doubled output rate, bounded
fragment push decoding, overflow and zero clocks. Flat windows capped by
`mdhd` now finish as `Complete`; indexed EOF without that bound retains
the existing `Rounded` completion behavior.

## Local gates

Toolchains: rustc 1.97.1 (`8bab26f4f`, repository pin) and MSRV 1.89.0
(`29483883e`), both `x86_64-unknown-linux-gnu`. All commands below exit 0.

| Command | Result |
| --- | --- |
| `cargo fmt --check` | Pass |
| `python3 -m unittest discover -s scripts -p 'test_*.py'` | 86 pass |
| `python3 scripts/check-doc-links.py` | Pass |
| `python3 scripts/check-file-size.py` | Pass |
| `python3 scripts/check-changelog.py` | Pass |
| `cargo clippy --all-targets -- -D warnings` | Pass |
| `cargo test --workspace --lib --tests` | 1,211 library and 5 integration pass; 7 ignored |
| `cargo test --doc` | 31 pass |
| `python3 scripts/check-package.py` | Pass |
| `cargo +1.89.0 test --workspace --lib --tests` | 1,211 library and 5 integration pass; 7 ignored |
| `cargo +1.89.0 test --doc` | 31 pass |
| `python3 lab/consume/run.py` | All four packaged consumer binaries and strict-completion test pass |
| `cargo run --release --manifest-path lab/fuzz/Cargo.toml --bin syom_fuzz -- --seconds 30 --seed 11` | 1,428 iterations; 0 panics |
| `cargo run --release --manifest-path lab/fuzz/Cargo.toml --bin syom_fuzz_state -- --seconds 30 --seed 11` | 29,064 iterations; 0 panics |

The final consumer archive is 473,638 bytes, SHA-256
`029c22eca2b1efbb4a2187c45d48777723d889c0d2826a6c5796b79819c256ea`.
Routine logs, test copies and media stay in ignored `target/issue-11/`.

## agent-youtube consumer

Tested a clean archive of agent-youtube
`bcd606c2ed8742c5e0fade0b4bfd4023e66bfcb1`, changing only the temporary
manifest's syom dependency to a local path. The real checkout and pinned
dependency remain unchanged. Its compiled backend label still names the
original dependency, so the manifest override, not that label, identifies
the tested codec. No deployment, network acquisition or ASR was involved.

FFmpeg 7.0.2-static generated the issue reproducer:

```sh
ffmpeg -f lavfi -i 'anoisesrc=color=pink:duration=1.2345:sample_rate=44100:seed=11' -ac 2 src.wav
ffmpeg -i src.wav -c:a aac -b:a 128k out.m4a
```

`ffprobe` reports 54,419 presentation samples. A second input keeps the
same AAC packets, sets `elst.media_time` to zero and sets its duration to
`ceil(mdhd.duration * 1000 / 44100)` = 1,258 movie ticks. It reproduces
the downloaded-file geometry: 55,477.8 requested samples versus 55,465
available media samples.

The existing ignored `audio::qualification::migration_probe` runs with
`AGENT_YOUTUBE_CODEC_PROBE` pointing to an owned output directory,
`AGENT_YOUTUBE_CODEC_SOURCE` to each input, and
`AGENT_YOUTUBE_CODEC_MODE` set to `full`, `interrupt`, then `resume` in
separate processes. Invoke it with
`cargo test --lib audio::qualification::migration_probe -- --ignored --exact`.

Before the fix, the original input fails with the reported noninteger
duration error (exit 101). With the fix all six runs exit 0. Full and
resumed PCM16 are byte-identical: 19,744 frames at 16 kHz for the original
and 20,124 for the zero-priming variant. The complete application
`cargo test` suite also exits 0: 435 pass, 24 opt-in tests ignored.

## Audit and refactoring

The follow-up audit found three reproducible defects in that initial
implementation. Each regression failed before its correction:

- Indexed EOF tested the already floored sample deficit. At 44.1 kHz and
  a 2,000 Hz movie clock, an edit requesting 66.15 samples with only 44
  available was incorrectly accepted: the integer deficit of 22 hid the
  fact that 22.15 exceeds the 22.05-sample clock tick.
- Computing an edit bound demanded an exact `mdhd` conversion even when
  no cap was needed. A 47-tick duration at a 1,000 Hz media clock becomes
  2,072.7 samples at 44.1 kHz and wrongly rejected a valid 1,024-sample edit.
- Flat decode returned remainder in media ticks while probe returned
  output samples. A 22.05 kHz media clock, 44.1 kHz output and 11 ms edit
  produced remainder 782 instead of 1,563; seek completion also inherited
  the wrong units.

One shared rational comparison now governs both capping and indexed EOF.
Duration rescaling floors quantized clocks with a `u128` intermediate and
checks the final `u64` result, avoiding false intermediate overflow.
Media-time offsets still require exact conversion. Probe reuses the
available-media count instead of converting it again; flat decode reports
the remaining output samples after the absolute presentation position.
The timing module is private, with only the needed crate-level exports.
Media-unit presentation/remainder helpers are now test-only.

Four additional tests cover these regressions, valid wide products and an
independent check of both rational tolerance inequalities across multiple
sample rates and movie clocks. Larger mismatches, missing payloads and
unfinished priming remain errors.

The audited source uses the same base revision and toolchains as above.
Its nine changed source files have aggregate SHA-256
`b8ce77d31ac569d6f9ecd74d14396170b9e598bd0c21d3a75e5948020955f48b`,
using the same path/NUL/bytes/NUL procedure. The consumer archive is
473,919 bytes, SHA-256
`6399b59851e2f63f4a5f228ffadb72e07ea2fae095879262f8b7e56ab5876948`.

Every command in the local-gates table was rerun after the audit, with
exit 0. Updated totals: 1,215 library and 5 integration tests pass on
both Rust versions (7 ignored); 31 doctests and 86 Python tests pass.
The packaged consumer lab passes all four binaries and strict completion.
The same 30-second fuzz commands and seed 11 complete 1,132 parser and
36,010 stateful iterations, with zero panics.

The same agent-youtube archive and two retained inputs were retested
against the audited syom source. All six full/interrupt/resume processes
exit 0 and both full/resumed PCM comparisons are byte-identical. The
application suite again passes 435 tests, with 24 opt-in tests ignored.
The real application checkout remains unchanged. Follow-up routine logs
and owned output stay in ignored `target/issue-11-audit/`.

## Second audit and refactoring

The next pass found that an unknown `mdhd` duration was still treated as
a numeric bound. Version 1's all-one duration at a 22.05 kHz media clock
overflowed when converted to 44.1 kHz, rejecting a valid fractional edit.
The new parser and public-API regressions failed before correction.

Normalize the version-specific all-one marker to the existing zero/unknown
representation, matching [FFmpeg's media-header handling](https://ffmpeg.org/doxygen/7.1/mov_8c_source.html#l01865).
Tests distinguish the 32- and 64-bit markers and preserve the valid numeric
value `u32::MAX` in a version 1 header. An integration regression covers
both header versions with retimed sample-table entries, slice and seekable
decode, probe, trim and exact seeking. Unknown media duration never supplies
a cap or a decoded remainder.

Completion now receives the absolute delivered presentation position from
the slice decoder or `M4aSeek`, rather than reconstructing it by converting
the edit window again. The shared rational comparison is unchanged.
Expanded EOF tests cover seeks to the last 17 samples and the last sample,
plus refusal of an exact seek requiring a nonexistent sample at physical
EOF. Refusal preserves position and the existing rounded-completion status.

The ten changed source files have aggregate SHA-256
`c78f087db2347757c364e67b1c1b8a2c9a98dd82599c1ae8374fcfdc6a5acd98`
using the same identity procedure, base revision and toolchains. The
consumer archive is 473,997 bytes, SHA-256
`d26f48d5c34b3e24fe3f807e19250c0100be1c11399b8bc2b11a79e8bf4a3996`.

All eleven local-gate commands were rerun with exit 0: 1,216 library and
5 integration tests pass on both Rust versions (7 ignored), together with
31 doctests and 86 Python tests. The packaged consumer lab passes. The
same 30-second seed-11 fuzz commands complete 1,209 parser and 19,387
stateful iterations with zero panics.

The agent-youtube suite again passes 435 tests (24 ignored). Its existing
restart probe passes all nine processes for the two prior inputs and a
third header mutation carrying a version 1 unknown duration, a 22.05 kHz
media clock and zero edit start. All three full/resumed PCM comparisons
are byte-identical. No application checkout or dependency pin was changed.
Routine output remains in ignored `target/issue-11-audit2/`.
