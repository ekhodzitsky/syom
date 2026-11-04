# Usage and validation

This page keeps integration detail separate from the root quick start.
The recommended registry dependency is syom 0.10.0 (`syom = "0.10"`).
Exact interval planning and logical seek positions were introduced in
0.9.0; the existing `seek` API remains available. Version 0.10.0 adds
[persistent LC state and strict completion](persistent-checkpoints.md).

## API guide

The [published guide](https://docs.rs/syom/latest/syom/guide/) contains a
compiling example for each task. Migration and source documentation are in
[the checkout's guide](../src/guide.rs).

| Task | Guide section |
|---|---|
| One-call encode/decode, speech vs full fidelity | 1 |
| Borrowed PCM, `Read` / `Write` / `Seek` | 2 |
| Priming, tail, exact length | 3 |
| Push `Encoder` / `Decoder`, bounded memory | 4 |
| Raw access units and `AudioSpecificConfig` | 5 |
| Probe, sample-exact M4A seek | 6 |
| LC, quality VBR, HE v1/v2, surround | 7 |
| Limits and typed errors | 8 |
| Profiles and containers | 9 |
| Behaviour changes since 0.6.0 | 10 |

## Formats and channel layout

| Operation | ADTS | M4A | LATM/LOAS | fMP4 |
|---|---|---|---|---|
| AAC-LC decode | Yes | Yes | Yes | Yes |
| HE-AAC v1/v2 decode | Yes | Yes | Yes | Yes |
| AAC-LD decode (512/480) | No | Yes | Yes | Same ASC path; no separate LD fMP4 golden |
| LC encode | Yes | Yes | Yes | No |
| HE v1/v2 encode | Yes | Yes | Yes | No |
| AAC-LD encode (512) | No | Yes | Yes | No |

LC encode is the default; M/S is per-band. `with_he`, `with_he_v2` and
`with_ld` are opt-in. ADTS cannot signal AAC-LD. Fragmented MP4 supports
one unencrypted AAC track and bounded init/fragment processing; see the
[profile and container contract](../src/guide.rs).

Surround LC uses 3, 4, 5, 6 or 8 planes (`channel_configuration` 3–7).
Seven planes have no supported layout. `DecodeOptions::audio()` keeps
coded channels; `decode()` uses the speech mono mix (non-LFE mean).
The speech defaults cap duration at the sample budget for two hours at
48 kHz; higher sample rates consume that same budget sooner. There is
no resampling. HE output uses the extension's output rate, twice its core
rate for the supported half-rate-core streams.

## Exact interval acquisition since 0.9.0

For the registry release:

```toml
[dependencies]
syom = "0.9"
```

To develop against a local checkout at `../syom`:

```toml
[dependencies]
syom = { version = "0.9", path = "../syom" }
```

`exact_seek_plan` and budgeted `seek_exact` cover LC/LD mono/stereo
interval consumers. Every nonempty exact plan requires the coded prefix
from access unit zero; arbitrary cold sparse seeking is not qualified.
Logical positions are source-bound and restore by replay, not by restoring
an opaque DSP checkpoint. [The guide](../src/guide.rs) explains borrowed
byte ranges, native presentation coordinates, restore and resource limits.
The old `seek` API remains available.

## Validation and integration

[Release 0.9.0 publication verification](../lab/release-audit/REPORT.md#release-090)
records the registry checksum, clean source commit/tag and actual
registry quick-start checks on current Rust and MSRV 1.89. Historical
prepublication qualifications retain their original status below.

The historical [0.8.1 qualification](../lab/release-audit/CANDIDATE-0.8.1.md)
passed 19 cases from 16 recordings against FFmpeg and the actual gigastt
decode adapter. The [published release](../lab/release-audit/PUBLISHED-0.8.1.md)
includes the padded-LC allocation fix and PNS-history preservation during
M4A seek. Those source revisions and recordings remain historical evidence,
not a new run on the current checkout.

The [paired HTTP ASR canary](../lab/release-audit/HTTP-CANARY-0.8.1.md)
matched normalized transcripts and durations on 32 speech-format cases.
The [local load/fault qualification](../lab/release-audit/HTTP-LOAD-0.8.1.md)
passed 224 measured requests per binary at 1/2/4/8 clients, including queue
and watchdog timeouts and recovery after client cancellation. Those checks
use the original consumer snapshot. The maintained
[opt-in gigastt integration](https://github.com/ekhodzitsky/gigastt/pull/442)
has [separate refactored-adapter qualification](https://github.com/ekhodzitsky/gigastt/blob/c64de2e19997ba7c4ecfe1f184488f85274a5b1f/docs/measurements/aac-20261029/README.md).

Cold/stateful seeks can replay the prefix; verified noise-free LC/LD
prefixes retain short preroll. Tagged/untagged LC ABR/CBR packaged-consumer
checks allocate zero times after warmup. The
[0.9.0 candidate qualification](../lab/release-audit/REPORT.md#candidate-090--cold-start-fix)
records local checks, exact decoded/encoded comparisons, parser/state fuzz
and cold 128 KiB release-stack regressions. No Windows/macOS/ARM parity,
physical-device runtime result, full ISO conformance, listening superiority
or live-service deployment follows from those Linux checks.

## Badges and measurement gaps

Registry version, monthly downloads and published API docs badges describe
the distribution channel, not the development candidate or a user count.
The `main` CI badge displays the last hosted workflow state. Hosted runs
are paused to preserve capacity; the configured workflow and Rust 1.89 job
remain in [.github/workflows/ci.yml](../.github/workflows/ci.yml).
MSRV is declared in [Cargo.toml](../Cargo.toml) and locally tested for the
release; the badge does not claim a new hosted run.

The [2026-11-02 local library-line report](../lab/baseline/COVERAGE.md#local-measurement-2026-11-02)
measures **25802 / 27830 lines (92.71%)** on the recorded runtime sources.
It excludes `_tests.rs` files, integration tests, doctests, benches,
dependencies and std. Branch/MC/DC coverage was not enabled. The badge
links to that dated source-pinned report; it is not a rolling hosted score.
The earlier 2026-09-29 result of 93.88% and its original scope are preserved
in the same report. Different source populations make those percentages
unsuitable for a direct coverage-regression claim.
