# TASK-10 Apple AAC quality and delay — report

Recorded 2026-09-22 on Linux x86_64. **No Apple AAC cell was executed.
This is not a pass; qualification is pending.**

## Host probe

Reproduce: `sh lab/apple/probe.sh`. Current output is committed as
[probe-linux-x86_64.json](probe-linux-x86_64.json):

| Capability | Result |
|---|---|
| OS/arch | Linux 7.0.0-31-generic x86_64 |
| macOS (`sw_vers`) | absent |
| `afconvert` | absent |
| AudioToolbox.framework | absent |
| CoreAudio.framework | absent |
| qaac / refalac / wine | absent |
| `apple_aac_executable` | **false** |

## Concrete missing resource

An **authorized macOS host** (any supported macOS with AudioToolbox; CI
runners exist in the project's matrix for syom itself but were not used
for this offline oracle task). Secondary lane, not a substitute: an
authorized Windows host with Apple Application Support (CoreAudioToolbox)
for qaac v3.07. Linux refalac-under-Wine is explicitly disqualified by
DOC-2: it neither identifies the backend version nor runs on an
authorized host.

## Cell status (maps to the task's acceptance criteria)

| Cell | Status |
|---|---|
| AC#1 backend/frontend versions, settings | **pending** — no host; nothing recorded, nothing invented |
| AC#2 LC/HE rate points, bytes, priming/remainder, independent decode | **pending** — no host; the measurement plan and validation commands are fixed in [PIN.md](PIN.md) |
| AC#3 timing lanes | **pending** — lane separation (native in-process vs CLI process launch) is predefined in PIN.md; zero numbers produced |
| AC#4 reproduction instructions + missing-resource record | **done** — this report + PIN.md + probe.sh |

## What a qualified session must deliver

Follow [PIN.md](PIN.md): probe JSON, `sw_vers` + framework bundle version,
frontend versions with download checksums, the smoke matrix (LC mono/stereo
ABR, VBR steps, HE where exposed; ADTS + M4A), per-artifact SHA-256 and
QA1636 priming/remainder/valid PCM, independent decode via syom and
pinned lavc 9.0.1, scoring via lab/score, timing in the two labeled lanes.
Artifacts with recorded checksums then become eligible for the
lab/listen blind protocol (TASK-109) and the cross-language scorecard
(TASK-110), both of which list this cell as open.

## No invented numbers

No backend version, bitrate, delay, timing or quality figure for Apple
AAC appears anywhere in this lab. Unavailability blocks the Apple
comparison claim only (DOC-2); unrelated development is unaffected.
