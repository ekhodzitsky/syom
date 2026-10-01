# JCodec AAC evaluation (TASK-19)

Status: **runtime lane run on 2026-09-28**. JDK is not installed
system-wide; the run used a user-local Temurin
`21.0.12.1+1-LTS` (mixed mode, sharing, G1). Host: Linux x86_64, AMD
Ryzen AI 9 HX 370, 24 threads, frequency boost left on (not a pinned
power policy). Artifact pin is unchanged (PIN.md).

## AC#1 — Artifact/source pin and capabilities

Pin: `org.jcodec:jcodec:0.2.5` (Maven Central, SHA-256 verified against
Maven SHA-1s, `evidence/artifact-hashes.txt`); source HEAD
`4ba922aca445c4df59f99e183c79d0538e5cb53f` (no v0.2.5 tag exists;
`evidence/github-source.txt`).

Capabilities read from the pinned sources, then checked by the run below:

- **Decode profiles** (`net/sourceforge/jaad/aac/Profile.java`,
  `isDecodingSupported`): AAC Main (1), **AAC-LC (2)**, **AAC SBR / HE-AAC
  v1 (5)**, ER AAC LC (17). Not decodable: SSR, LTP, scalable, TwinVQ, LD,
  and all other ER profiles. PS classes exist and are wired into SBR
  (`sbr/SBR.java` `processPS`, `EXTENSION_ID_PS`), so **HE-AAC v2 decode is
  present in source**. Executed cells below: ADTS HE and PS produce no
  PCM, and HE M4A comes back as 24 kHz mono.
- **Encode: none.** `org/jcodec/codecs/aac/BlockWriter` is a stub (writes
  the block-type triplet and only handles `TYPE_END`). No AAC encoder is
  advertised or present in 0.2.5.
- **Containers**: ADTS parse/wrap (`org.jcodec.codecs.aac.ADTSParser`),
  ADIF header read (`jaad/aac/transport/ADIFHeader`), MP4/MOV demux with
  `esds` ASC extraction (`AACUtils.getCodecPrivate`). No LATM/LOAS.
- Corpus runs are `python3 lab/jcodec/smoke.py` (exit 0 pins the table).

## AC#1 — Executed shapes (2026-09-28)

`samples` is interleaved s16 values. ffprobe 7.0.2 names the
presentation; it is not a second PCM decoder.

| case | JCodec result | ffprobe presentation | pcm sha256 |
|---|---|---|---|
| `sine48.adts` LC | 48 kHz mono, 13312 samples, 13 frames | 48 kHz mono, 0.277 s | `dd3ba138ba04a2c0ddc59296463ca4ba4442aa475e53912db5c7439f7ad29ae5` |
| `sine441.m4a` LC | 44.1 kHz mono, 12288 samples, 12 frames | 44.1 kHz mono, 0.250 s (edit) | `de676bae28a480011d3d012db14bef539324e62a841a9627863c689bea168af3` |
| `tns48.adts` LC | 48 kHz mono, 1024 samples, 1 frame | 48 kHz mono, 0.021 s | `9d556828f3f50cc09fda7180ea27831ea4a767d751340e1f02f1367fdd0eb54e` |
| `mc51.adts` LC | 48 kHz, 6 ch, 122880 interleaved (20×1024×6) | 48 kHz, 6 ch, 0.427 s | `4fe9b404f7a5e9cddf9383ea135c83a7b06f095e29467ee6f5a3940114162581` |
| `he48.m4a` HE-AAC | 24 kHz mono, 9216 samples, 9 frames | 48 kHz stereo, 0.340 s | `f7b586904e3678145aa47e4232587c913139cef0102d6d8e9276fc80c35cbad3` |
| `he48.adts` HE-AAC v2 | 9 frames, 0 samples (`unexpected end of frame`) | 48 kHz stereo, 0.384 s | empty (`e3b0c442…`) |
| `ps48.adts` HE-AAC v2 | 26 frames, 0 samples (same warning) | 48 kHz stereo, 1.109 s | empty (`e3b0c442…`) |
| `he48.latm` | exit 1, NPE in `parseMP4DecoderSpecificInfo` (`cp` is null) | LATM, not ADTS | — |

M4A LC is one AAC frame longer than the 0.250 s edit: JCodec does not
apply `elst`. 5.1 matches the presentation when samples are counted
interleaved. HE and PS are not a usable oracle for those goldens.

`getAudioTracks()` in 0.2.5 returns `List<DemuxerTrack>`, so the M4A
path takes the `SOUND` track from `getTracks()`.

## AC#2 — Startup, warm-up, throughput, heap

Flags: `-Xms64m -Xmx256m -XX:+UseG1GC`. JIT is the default mixed mode.
Clip: `sine48.adts`, 13312 samples, 0.277 s.

JVM process start (`java -version`, 20 launches, no decoder): median
28.1 ms, p95 33.6 ms.

One JVM, `bench-adts` × 25. First five reps are warm-up
(42.0, 8.8, 6.2, 9.1, 5.3 ms). Next 20, wall time per decode of the
file: median 3.08 ms, p95 4.83 ms, min 2.10, max 5.01. About 90×
realtime at the median. Raw samples, ms: 3.36, 2.50, 2.89, 2.99,
4.82, 3.15, 3.29, 3.19, 3.47, 3.08, 3.08, 2.77, 3.07, 3.98, 5.01,
3.18, 2.40, 2.10, 2.11, 2.15.

No GC collection during those 25 reps. At exit the G1 heap was
67584K committed, 9185K used, max 256M; metaspace used 1748K. No
pause time to report. Power policy was not pinned.

## AC#3 — API/setup differences vs syom

- JCodec AAC decode is a `org.jcodec.common.AudioDecoder` wrapper
  (`AACDecoder`) over the embedded JAAD engine; ctor takes an ADTS header
  (>=7 bytes) or MP4 ASC. Frames must be **ADTS-wrapped** even when the
  source is raw MP4 AUs (`ADTSParser.write` + `AACUtils.streamInfoToADTS`).
  syom takes ADTS/LATM/M4A directly and exposes `from_asc`/`decode_au` for
  raw AUs — no re-wrapping.
- Output is interleaved s16 bytes (`SampleBuffer`, endianness flagged), not
  planar f32; sample-accurate trim/priming handling is not exposed (no
  `elst`/priming API surfaced in the AAC lane).
- Setup cost: Maven/Gradle coordinate or a fat jar on the classpath vs
  `cargo add syom`; runtime is JVM (JIT warm-up, GC) vs native static.
- MP4 lane needs `MP4Demuxer` + sample-entry plumbing; syom `decode_seek` /
  `decode_streaming` handle M4A in one call with exact duration/trim.
- License shape: FreeBSD + embedded Public-Domain JAAD (no copyleft), so no
  licensing obstacle — the obstacles are maintenance and runtime cost.

## AC#4 — Go/no-go for sustained regression tracking

**No-go stands** after the runtime cells. Rationale:

1. **Decode-only peer, unmaintained**: last Maven release 2019-06-16
   (0.2.5); no AAC encoder exists, so it can never enter an encode lane.
2. **Contextual lane by design** (doc-2): "Java/JS establish cross-runtime
   context" — JCodec is not in the mandatory baseline set; a one-off
   qualification suffices.
3. **Low differentiation risk**: the decoder is JAAD-derived (a 2009-era
   lineage); syom already tracks FAAD2 and libavcodec as independent
   native lineages with corpus oracles.
4. The executed lane confirms the gap: LC ADTS/M4A/5.1 decode, but HE
   and PS ADTS yield no PCM and HE M4A stays at the core rate. It is not
   an oracle for the streams syom actually ships as HE/PS.

Raw data archived: `evidence/maven-metadata.xml`, `evidence/jcodec-0.2.5.pom`,
`evidence/artifact-hashes.txt`, `evidence/github-source.txt`,
`evidence/aac-classes.txt` (115 AAC/JAAD classes in the pinned jar).

TASK-119 ran this lane. The no-go is unchanged.
