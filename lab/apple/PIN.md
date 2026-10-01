# Apple AAC pin (TASK-10)

Isolated from the `syom` package. **Not invoked by `cargo test`.** Apple AAC
runs only inside Apple's AudioToolbox/CoreAudio; there is no redistributable
Linux build. This pin records what a qualifying host must supply and the
exact pins to capture once one is available. No cell here is executed on
the current host — see [REPORT.md](REPORT.md).

## What counts as "Apple AAC"

The encoder is `AppleAAC` inside CoreAudio
(`/System/Library/Frameworks/AudioToolbox.framework` on macOS). DOC-2 and
this task's premise: a **QAAC version or a Linux refalac run does not
identify or execute Apple's AAC encoder** — qaac/refalac are frontends over
a Windows `CoreAudioToolbox.dll` lifted from iTunes, whose provenance,
version and authorization are not established on this host. Qualification
requires an **authorized macOS host** (or an authorized Windows host with
Apple Application Support installed, recorded as a separate lane).

## Frontends to pin (on the qualified host)

| Frontend | Pin to record |
|---|---|
| `afconvert` (ships with macOS) | macOS version + build (`sw_vers`), `afconvert -hf` AAC codec listing |
| qaac v3.07 | https://github.com/nu774/qaac/releases/tag/v3.07 — download SHA-256 at fetch time, `qaac --version`, reported CoreAudioToolbox DLL version |
| AudioToolbox backend | framework bundle version (`defaults read .../AudioToolbox.framework/Resources/Info.plist CFBundleVersion`), macOS build number |

Do not copy checksums from this file into a report: record the actual hash
of the actual download, like `lab/PIN.md` does for FFmpeg 9.0.1.

## Smoke set (once a host exists)

Clips: the `lab/quality` synth dev clips (`sine440` tonal, `noise`,
`click` transient, `tremolo` stereo — 48 kHz f32, 1 s). Held-out corpus is
**not** touched by a smoke. Per DOC-3, never silently change profile or
channel count; unsupported combinations are recorded as unsupported.

Rate/profile points (record actual elementary-stream bytes, not requests):

| Cell | Settings |
|---|---|
| LC mono 48 kHz | 64 / 96 / 128 kbps ABR |
| LC stereo 48 kHz | 64 / 96 / 128 / 192 / 256 kbps ABR |
| LC stereo VBR | every `afconvert -q` / qaac `--tvbr` step the backend exposes |
| HE v1 stereo | 32 / 48 kbps, only if the backend exposes it (qaac `--he`; afconvert `aach`) |
| HE v2 stereo | only if exposed; else record "unavailable" |

Containers: ADTS (raw AU bytes) **and** M4A. For M4A, extract
priming/remainder/valid duration from `elst` + `iTunSMPB` per Apple QA1636
(https://developer.apple.com/library/archive/qa/qa1636/_index.html);
padded decode length is not valid duration (lab/prime/REPORT.md).

Per artifact record (DOC-3 manifest contract): source and output SHA-256,
channel layout, exact valid PCM, priming/remainder, all profile/quality/
rate settings, frontend version, backend/framework version, OS/arch.

## Validation (this host, offline)

1. Independent decode of every artifact: syom `decode_with(..., unbounded())`
   plus pinned lavc 9.0.1 `lab/libavcodec/avc_driver decode-au` /
   `decode-container` (shape check like `lab/smoke.py`).
2. Objective scoring on the common system: `cargo run --release
   --manifest-path lab/score/Cargo.toml -- adts ref.adts apple.adts`
   (delay / valid_samples / actual_bps / snr_db; SNR is not PEAQ — lab/score/PIN.md).
3. Timing lanes stay separate (lab/PIN.md discipline, DOC-3): native
   library in-process timing (an AudioToolbox adapter) vs CLI
   process-launch timing (`afconvert` / qaac) are distinct cells;
   ≥20 reps after warm-up, median + p95 + bootstrap CI.

Artifacts feed blind tests (lab/listen protocol) only with recorded
checksums and hidden-label renaming; dry-run scores are not
listening evidence.
