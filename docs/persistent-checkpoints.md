# Persistent LC decoder state (since 0.10.0)

[Documentation index](README.md) · [Usage and validation](usage-and-validation.md)

This API is available in syom 0.10.0 and later. Version 0.9.0 does not expose
it. It supports native AAC-LC continuation after process restart; it does not
restore application jobs or resamplers.

Version 0.10.2 also corrects tag-only M4A presentation behavior. Cached
checkpoint blobs from older versions must be recreated; restoration is bound
to the exact codec version. [Release versus checkout](verification.md#release-and-checkout).

`M4aSeek::save_exact_position` remains a source-instance-bound logical position.
Its restore replays history under an explicit AU budget. It contains no DSP
state and cannot be serialized. Persistent LC state is a separate, qualified
API; it cannot import into exact mode or bypass failed exact-state requirements.

## Eligibility and capture

Use `supports_checkpoint()` for the current source/options scope,
`checkpoint_compatible()` for observed DSP history, and `checkpoint()` to
capture an eligible interior AU boundary. `checkpoint()` returns `None` rather
than manufacturing state when unavailable.

Qualified scope is 1024-sample AAC-LC at 8, 16, 22.05, 24, 32, 44.1 and 48 kHz,
with mono output from mono or stereo input. Both channel filterbanks and PNS
history are retained, including the previous window shapes. Initial positions,
unfinished priming, partially consumed AUs and presentation EOF cannot be saved.
HE/SBR/PS, PCE, coupling, profile prediction, other frame grids/rates, split
output and tag-only sources without an edit list are excluded. In-band state is
checked after each decoded AU; opening an eligible ASC does not certify all
future payloads. Once a read/codec or unsupported-history fault has been seen,
that source handle stays checkpoint-ineligible even after an ordinary seek.

A callback can deliberately interrupt `decode` after accepting its complete
frame. The decoder has advanced before calling the callback, so the saved
position must describe all delivered samples. Persist accepted PCM/application
state and the matching checkpoint atomically at the application level. Merely
returning an error does not provide durable storage or exactly-once delivery.

## Version, binding and memory

Format version 1 is exactly **8,422 bytes**, with little-endian fields and
magic `SYOMLC01`. Treat it as opaque bytes, not a public structure to edit.
`CHECKPOINT_FORMAT_VERSION` and `CHECKPOINT_BYTES` describe the current encoding.

| State | Retained information |
|---|---|
| Header | Implementation/version marker, exact decode options and resource budgets, ASC/index/edit geometry tag |
| Position | Next AU, absolute native presentation position and remaining presentation samples |
| DSP | PNS RNG, two 1024-sample filterbank overlaps and previous window shapes |

The geometry tag is a noncryptographic mismatch check. **It is not source or
checkpoint authentication.** Identical indexes can contain different AAC
payloads. The caller must authenticate stored state and bind it to immutable
source bytes, exact syom implementation/build including local patches and CPU
policy, and options before calling `restore_checkpoint`. A cryptographic source
identity and an authenticated application envelope belong outside this codec.
Matching a package version alone does not establish cross-build PCM equivalence.
No conversion from `YTLC0001` or other consumer formats is promised.

`restore_checkpoint(&bytes)` performs no media reads or replay. It validates the
fixed length, header, counts, position relationships, window shapes and finite
overlap values before committing state. A rejected import leaves the existing
reader, DSP state and position unchanged. Blob counts cannot request allocation:
restore uses a temporary fixed-size boxed decoder and a bounded 200-byte header;
transform/PCM scratch is rebuilt normally. Successful import does not prove
that the DSP values came from the source: finite forged overlaps remain the
caller's authentication responsibility. The first resumed AU refuses nonfinite
output before a callback and makes that handle checkpoint-ineligible.

Package-version or format changes reject old blobs. Cross-architecture,
cross-compiler, patched-build and arbitrary future-version compatibility is not
qualified. Preserve the old executable/build identity for existing saved jobs,
or explicitly restart/replay under an application migration policy.

## Strict M4A presentation completion

Inspect `M4aSeek::presentation_completion()` or returned
`StreamInfo::presentation_completion`:

| Status | Meaning |
|---|---|
| `Complete` | The bounded presentation was delivered exactly |
| `Rounded { missing_samples }` | All indexed AUs were decoded; the positive edit-list remainder is smaller than both one movie tick and one millisecond |
| `Incomplete { .. }` | Required priming or presentation samples are still owed |
| `Unbounded { indexed_end_reached }` | No bounded presentation was declared; currently indexed EOF is not an external duration guarantee |
| `Unknown` | This API/container does not supply a qualified presentation boundary |

`presentation_complete()` is true only for `Complete` or qualified `Rounded`.
`bounded_presentation()`, `output_sample_rate()` and `remaining_samples()` expose
the native presentation scope. No rounding path pads PCM or accepts a missing
required AU. Unfinished priming, a whole movie tick, at least one millisecond or
a missing frame are errors. Known bounded push-fMP4 presentations require exact
completion; push has no complete lifetime AU index to justify rounding. Unknown
or open-ended fragments remain explicit instead of claiming completion.

Version 0.10.1 derives the seek-source boundary from the edit list.
Version 0.10.2 also applies a standard count-validated iTunSMPB window and
reports it as bounded. This does not make tag-only sources eligible for
persistent checkpoints.

Completion validates the requested presentation, not source authenticity or
unused indexed padding after its cap. Seeking directly to an end position is
not evidence that preceding payloads were read. Partial interval acquisition
still follows the existing exact-seek budget and prefix-replay contract.

## Consumer migration boundary

The agent-youtube reviewed patch supplied requirements and counterexamples;
it was not applied wholesale. Its checkout, vendor files, job state, transcripts,
network/authentication and resampler were not modified. Upstream qualification
does not automatically migrate that consumer. Adopting this version requires
its own backend identity decision, source/options authentication, native PCM and
resampled-tail comparisons, durable job-recovery tests and old-job refusal.
A resampler has independent phase/history that these bytes do not contain.

Local tests and retained evidence are indexed in the
[TASK-144 qualification report](../lab/release-audit/TASK144.md). They establish
only the listed source/build/platform combinations. Publication and consumer
migration remain separate actions.
