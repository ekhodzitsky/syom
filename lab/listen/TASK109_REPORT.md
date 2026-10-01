# TASK-109 held-out blind encoder qualification — execution attempt

Recorded 2026-09-22. Host: Linux x86_64, rustc 1.97.1, HEAD `a0d230a`.
**This file is not listening evidence. No human scores exist. The
qualification study did not run; qualification remains PENDING.**

## What was executed

The registered design (TASK-14, `PROTOCOL.md`, locked 2026-09-14) cannot
collect human scores on this host. Per the task's verification clause,
what runs without humans was run and audited:

### 1. Blinding / manifest integrity audit

```sh
python3 scripts/listen_protocol.py dry-run --out lab/listen/dryrun --seed task-14
python3 scripts/listen_protocol.py verify --out lab/listen/dryrun
python3 scripts/verify_corpus.py
```

| Check | Result |
|---|---|
| Dry-run regenerates committed TASK-14 numbers | OK — 8 items (7 MUSHRA 64k + 1 BS.1116 128k), 8 valid / 1 excluded, syom−lavc mean −1.135, sd 0.719, 95% CI [−1.737, −0.534], n_qual 69, burden 17.5 min |
| `verify` on fresh packet | OK, zero errors |
| Tamper: `syom` injected into a packet filename | `verify` FAILs (`filename leaks syom`) |
| Tamper: SYNTHETIC label stripped from scores | `verify` FAILs (`scores missing SYNTHETIC label`) |
| Corpus manifest verifier | OK — 82 cases, 24 naturals, offline checks pass |

### 2. Independent reproduction of the reported interval

`lab/listen/reproduce_ci.py` (stdlib-only, does **not** import
`scripts/listen_protocol.py`) recomputes screening and the paired
(syom − lavc) 95% t-CI from the anonymized `scores.synthetic.json` +
sealed `key.json` + `design.json`:

```sh
python3 lab/listen/reproduce_ci.py   # exit 0
```

All quantities match `analysis.json`: valid/excluded listener sets, n,
mean, sd, noninferior/superior flags, and the CI within one unit of the
protocol's 3-decimal rounding (this script uses the exact t₀.₉₇₅,₇ =
2.3646; the registered script's table stores 2.365 — observed bound
delta 1e-3, decisions unchanged). The planted interval is a procedure
check on SYNTHETIC grades, **not** a quality result.

### 3. Frozen-candidate integrity at HEAD

FREEZE.md pins `b5660e9` + the TASK-108 preset commit; HEAD is the later
`a0d230a`. The byte-exactness tripwires prove the encoder bytes are
unchanged since the freeze:

```sh
cargo test --lib lavc_matches            # 5 passed (enc48 ADTS/M4A/transient/lookahead/LATM)
cargo test --lib -- lavc_decodes_our_he_adts_and_m4a \
  libavcodec_decodes_our_surround_m4a_and_latm \
  libavcodec_decodes_our_he_v2_streams_as_stereo_with_the_same_image   # 3 passed
cargo test --lib listen_protocol         # 4 passed
```

## Acceptance-criteria disposition

| AC | Status | Evidence / missing resource |
|---|---|---|
| #1 Execute registered design, justified n, valid ref/anchor checks | **PENDING — unqualified** | Registered design validated end-to-end on synthetic grades (above). Missing: **human participants** (consented experienced listeners; n = 69 valid for qualification, 20 exploratory). No simulated grades were substituted (DOC-3 forbids). |
| #2 Strongest peers at matched actual bitrate, hidden labels, LC/HE/spatial separated | **PENDING — unqualified** | Blinding machinery proven (tamper checks). Missing: **holdout natural bytes** (8/8 manifest rows `gap-until-obtained`), so no matched-bitrate same-PCM encodes can exist for any peer. HE categories belong to TASK-95 (In Progress); spatial/multichannel has no registered listening cell. |
| #3 Publish anonymized data, exclusions, CIs, NI/sup conclusions | **PENDING — unqualified** | Publication pipeline demonstrated on labeled synthetic data (dry-run + `reproduce_ci.py`). No human data exists to publish. |
| #4 Post-study tuning requires new held-out evidence; failures → bounded follow-ups | **HONORED (no study, no tuning, no claim)** | Encoder freeze intact at HEAD (byte-exact tripwires). No post-freeze tuning was performed or claimed. Follow-up TASK-120 opened for the one actionable gap (holdout bytes). |

## Gap inventory (2026-09-22) vs TASK-14 REPORT.md (2026-09-14)

| Gap | 2026-09-14 | 2026-09-22 |
|---|---|---|
| Human participants | none | **unchanged** — none; requires consented expert listeners off this host |
| Holdout natural bytes (8 recordings, TASK-2) | gap-until-obtained | **unchanged** — 8/8 `gap-until-obtained` in `corpus/manifest.json`; do-not-vendor rule stands; → TASK-120 |
| Apple AAC host (TASK-10) | no AudioToolbox | **unchanged** — TASK-10 To Do, no macOS host |
| Encoder defaults frozen | TASK-108 To Do | **resolved** — TASK-108 Done, FREEZE.md, byte-exact at `a0d230a` |
| HE v1/v2 encoders | absent | **resolved** — TASK-90/93/94 Done; HE listening cells still need humans + bytes |
| FFmpeg 9.0.1 native `aac` | pinned adapter | **present** — `lab/libavcodec/avc_driver id` → `ffmpeg-9.0.1-native-aac threads=1` |
| FDK 2.0.3 prefix | not installed | **unchanged** — tarball at `/tmp/task101/fdk-aac-2.0.3.tar.gz`, no installed prefix/driver; optional peer, needs same-PCM encodes |
| FAAC 1.31.1 | prefix not installed (listen PIN) | lab harness sources present (`lab/faac`); optional peer, needs same-PCM encodes |
| glint 0.11.0 | encode-pcm rebuild | `glint_smoke` binary built; optional peer, needs same-PCM encodes |
| Dense-music holdout item | coverage gap | **unchanged** — one tonal-music item only |
| PEAQ / ViSQOL | unavailable (TASK-13) | unchanged; listening does not require them |

## Conclusion

The preregistered protocol, blinding, screening, and statistics survive
audit and independent reproduction. The study itself cannot be executed
on this host: the concrete missing resources are (1) consented human
listeners, (2) the 8 registered holdout recordings, (3) an Apple AAC
host or an explicit exclusion amendment to the protocol. Qualification
is **pending**; syom makes **no** blind audible-quality claim. Any
future post-study tuning re-opens the freeze and requires new held-out
evidence (AC#4).

## Reproduce

```sh
python3 scripts/listen_protocol.py dry-run --out lab/listen/dryrun --seed task-14
python3 scripts/listen_protocol.py verify --out lab/listen/dryrun
python3 lab/listen/reproduce_ci.py
python3 scripts/verify_corpus.py
cargo test --lib lavc_matches
cargo test --lib listen_protocol
```
