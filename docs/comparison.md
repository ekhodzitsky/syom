# AAC library comparison and historical measurements

This comparison covers Rust AAC decoding into application PCM, including
container acquisition and output layout where the adapter performs them.
It is a pinned snapshot, not a ranking of the latest releases. The root
README integration table names published syom 0.10.3; peer facts retain their
2026-11-02 review date. The archived timings below remain attributed to syom 0.8.0
and must not be relabeled as measurements of later releases.

[Documentation index](README.md) ·
[Current release verification](../lab/release-audit/RELEASE-0.10.3.md)

## Selection and verification

Facts were checked on **2026-11-02** against the cached published crate
sources for rusty_aac 2.0.1, Symphonia 0.6.1 and oxideav-aac 0.1.7,
including their README, normalized Cargo manifest and public API. This
was a source review, without new benchmark runs or platform validation.
The historical benchmark version resolution date was 2026-10-28.

rusty_aac is a direct zero-runtime-dependency AAC alternative with a raw
access-unit API. Symphonia represents a broader media demux/decode pipeline,
including built-in MP4 support. oxideav-aac represents an AAC codec integrated
with the oxideav framework. These overlap the task through different
integration boundaries; raw-AU support does not establish a built-in M4A
reader, and an available codec does not establish full-profile output.

Primary pinned sources:

- [rusty_aac 2.0.1 README and source](https://docs.rs/crate/rusty_aac/2.0.1/source/README.md):
  HE v1/v2 and LD decoding, LC encoding, interleaved f32 output, raw AUs
  from a caller's container demuxer, and LATM/LOAS. Its manifest declares
  no runtime dependencies and an Apache-2.0 license.
- [Symphonia 0.6.1 README](https://docs.rs/crate/symphonia/0.6.1/source/README.md)
  and [manifest](https://docs.rs/crate/symphonia/0.6.1/source/Cargo.toml):
  AAC-LC and ISO/MP4 are optional feature-selected components; multiple
  non-AAC codecs and formats share the pipeline. Its built-in AAC decoder
  does not supply HE SBR/PS reconstruction. The project documents a
  separate FDK adapter, which is outside the measurements here. License:
  MPL-2.0.
- [oxideav-aac 0.1.7 README](https://docs.rs/crate/oxideav-aac/0.1.7/source/README.md)
  and [manifest](https://docs.rs/crate/oxideav-aac/0.1.7/source/Cargo.toml):
  HE v1/v2 decoding, LD, LC/HE v1 encoding, LATM/LOAS, interleaved i16,
  and an oxideav-core dependency. The README's “Not yet supported” section
  documents wrong PCM for deployed TNS-bearing LD access units despite
  resolution of the structural field-width issue. License: MIT.

Choose rusty_aac when its interleaved f32/raw-AU boundary fits an existing
demuxer, or when its documented Main/LTP/ELD core decoding is required;
ELD low-delay SBR remains outside that crate's stated support. Choose
Symphonia when one demux/decode interface across AAC and other formats is
more useful than an AAC-specific API; enable its AAC/MP4 features explicitly.
Choose oxideav-aac when an application already uses oxideav-core or needs
its interleaved i16 codec boundary, while accounting for the documented LD
limitation. These are integration reasons, not new correctness or speed
claims. Adopting syom requires planar output handling and its own options,
container and error APIs; adapters and feature choices change the work timed.

## Archived capability and speed snapshot

The following sections preserve the previous root README's pinned snapshot
and all four measured cells, including the HE v2 loss and 5.1 result.
Capability declarations are not conformance certification; benchmark exclusions
and “decoder unavailable” describe the tested adapter and fixture.

## Compared with Rust AAC libraries

Capabilities below refer to the pinned published versions, not their main
branches. Raw-AU support does not imply a built-in M4A demuxer.

| | syom 0.8.0 | rusty_aac 2.0.1 | symphonia 0.6.1 | oxideav-aac 0.1.7 |
|---|---|---|---|---|
| Job | AAC codec | AAC codec | media pipeline | AAC codec |
| Runtime deps | none | none | several | oxideav-core |
| HE-AAC v1/v2 decode | yes | yes | LC core only | yes |
| AAC-LD decode | yes | yes | no | yes, with documented TNS limitations |
| Built-in M4A demux | yes | caller supplies raw AUs | yes | caller supplies raw AUs |
| LATM/LOAS decode | yes | yes | no built-in LATM reader | yes |
| Encode profiles | LC, HE v1/v2, LD | LC | none | LC, HE v1 |
| Decoded output | planar f32 | interleaved f32 | audio buffers | interleaved i16 |

Sources: [rusty_aac](https://docs.rs/crate/rusty_aac/2.0.1),
[Symphonia](https://github.com/pdeljanov/Symphonia),
[oxideav-aac](https://docs.rs/crate/oxideav-aac/0.1.7).

## Historical decoding measurements

Linux x86_64, AMD Ryzen AI 9 HX 370, rustc 1.97.1, `taskset -c 0`,
release thin LTO, 8 warmup + 101 measured reps, 2026-10-28.
Same bitstream and collected planar f32 shape; adapters include layout
conversion. Checksums differ; matching shape is not a quality score.
Median wall times from the Rust fast-peer pass:

| Workload | syom 0.8.0 | rusty_aac 2.0.1 | symphonia 0.6.1 |
|---|---|---|---|
| LC ADTS, 48 kHz mono, 13312 samples | 59.6 µs | 152.3 µs | 135.2 µs |
| HE ADTS, 48 kHz stereo, 18432 samples | 190.8 µs | 354.9 µs | core-only, excluded |
| HE-AAC v2 ADTS, 48 kHz stereo, 53248 samples | 1.211 ms | 1.120 ms | core-only, excluded |
| LC 5.1 ADTS, 48 kHz, 20480 samples | 601.3 µs | 678.3 µs | decoder unavailable |

This busy-host sample favors syom on LC/HE v1 and rusty_aac on HE v2;
it is not a universal speed ranking. The separate four-peer run has
substantial scheduling noise. Both runs, exact inputs, protocol and
limitations are in the [release audit](../lab/release-audit/REPORT.md).
[BENCH.md](../BENCH.md) retains historical results against older peers.
Encoder quality superiority is not established; the earlier in-process
LC encode comparison was slower than FDK (see the audit).

## Raw evidence and reproduction

The [release audit's Rust decoder comparison](../lab/release-audit/REPORT.md#current-rust-decoder-comparison)
records exact fixtures, adapters and protocol. Its raw
[fast-peer samples](../lab/release-audit/runs/peers-fast.log) and
[four-peer samples](../lab/release-audit/runs/peers.log) are separate runs;
do not pool their medians. The isolated
[release-audit package](../lab/release-audit/Cargo.toml) and
[lockfile](../lab/release-audit/Cargo.lock), together with the audit's commands,
record the reproducible harness and dependency versions.

Historical engine revision: `997bf56efda8700792dbc4b0136a36ad90eca328`
(`v0.8.0`). The harness commands from the repository root are:

```sh
cargo build --release --locked --manifest-path lab/release-audit/Cargo.toml
taskset -c 0 lab/release-audit/target/release/syom-release-audit --fast
taskset -c 0 lab/release-audit/target/release/syom-release-audit
```

The current harness uses a path dependency on the current checkout. Running
these commands now measures that checkout; reproducing the archived engine
requires the recorded historical sources and matching harness configuration.

Initialization, decoding, planar f32 collection, layout conversion and
destruction are inside the timer; input reads and preflight checksums are
outside. Preflight requires equal rate, channels, samples per channel and
finite output. Distinct checksums are recorded; neither equal shape nor a
shorter duration establishes equivalent waveform or perceptual quality.
Peers rotate per repetition. The four-peer pass uses 4 warmup + 21 measured
repetitions; its starting load average was 20.97. No outliers were removed.
The fast-peer median/p95 values in microseconds were:

| Fixture | syom 0.8.0 | rusty_aac 2.0.1 | Symphonia 0.6.1 |
|---|---:|---:|---:|
| LC mono | 59.6 / 68.3 | 152.3 / 160.7 | 135.2 / 143.7 |
| HE v1 stereo | 190.8 / 198.2 | 354.9 / 366.9 | core-only, excluded |
| HE v2 stereo | 1211.1 / 1304.8 | 1120.2 / 1195.4 | core-only, excluded |
| LC 5.1 | 601.3 / 633.1 | 678.3 / 697.4 | decoder unavailable |

On `sine441.m4a`, syom emitted 11025 samples and Symphonia 12288, so
the historical harness did not time them against each other. On
`lecture.m4a`, Symphonia returned `unsupported feature: aac: predictor data`.
Both historical benchmark commands exited 1 after earlier valid groups;
these are partial results, not a passing complete matrix. The rusty_aac and
oxideav adapters did not demux M4A. oxideav's `he48.adts` output was mono
while syom's was dual-mono stereo, excluding that cell.

The [2026-10-16 in-process report](../lab/baseline/SOTA-SPEED-2026-10-16.md)
found syom LC encoding 2.46x / 1.65x slower than FDK at 128 / 24 kbps on
two synthetic stereo cells. Those original encoder ratios used the old FDK
adapter, which omitted delayed tail output; they must not be reused as
complete-input comparisons. The report preserves the 2026-11-01 full-drain
correction and later source-pinned increments separately. The recorded
long speech cases still trail FDK. That report's original libavcodec build
disabled x86 assembly. These are historical conditions, not current release claims.
No result here establishes Windows, macOS, ARM, mobile, listening-test or
state-of-the-art performance/quality parity. A version, fixture, feature,
adapter, output-shape or hardware change requires a new comparison.
