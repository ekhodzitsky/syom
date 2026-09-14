# glint-audio 0.11.0 AAC smoke (TASK-11)

Recorded 2026-09-14, rustc 1.97.1, zig-c++ clang 19.1.7
(`-target x86_64-linux-gnu`, rustc linker = same), Linux x86_64.
Isolated `lab/glint`. Not invoked by `cargo test`.

`glint_version()` = **0.8.0** on this 0.11.0 crate snapshot.
Precision **double** (`GLINT_AAC_INT` off). AAC config has no SIMD field.
Quality 0=speed / 1=normal / 2=best. AAC `bitrate` is **kbps**.
Independent decode: syom `decode_with(..., unbounded())`.

## Decode (syom golden `sine48.adts`)

| Engine | rate | ch | samples | finite |
|---|---|---|---|---|
| glint `AacDecoder` / `decode_audio` | 48000 | 1 | 13312 | yes |
| syom unbounded | 48000 | 1 | 13312 | yes |

Length matches. Unaligned max \|Δ\| = **9.39e-2** (not sample-identical).
Do not treat glint decode as a drop-in PCM match for syom goldens.

## Encode (2 s 440 Hz sine, peak 0.5, 48 kHz, CBR kbps)

Input 96000 samples/ch. Independent syom decode is **98304** samples/ch
(+2304 = 48.00 ms vs source; claimed encoder delay 2048 plus flush).
All cells finite planar f32.

| ch | kbps | quality | ADTS bytes | actual bps | notes |
|---|---|---|---|---|---|
| 1 | 64 | 0 speed | 14880 | 58125 | |
| 1 | 64 | 1 normal | 16206 | 63305 | |
| 1 | 64 | 2 best | 16206 | 63305 | **byte-identical to normal** on this sine |
| 1 | 128 | 1 normal | 32406 | 126586 | |
| 2 | 128 | 0 speed | 15858 | 61945 | speed misses the 128k target |
| 2 | 128 | 1 normal | 32283 | 126105 | |
| 2 | 128 | 2 best | 32283 | 126105 | **byte-identical to normal** on this sine |

Normal/best CBR-average lands near the requested kbps on this tonal
sine (unlike FAAC 1.31.1 `quantqual`). Speed does not. BEST vs NORMAL
is **not** distinguished here — do not use this fixture as a quality-tier
gate.

## Wrapper vs native C ABI (same `libglint`, 200 silent 1024-sample frames)

Single-run, not a league:

| Path | ns/frame | extra copies |
|---|---|---|
| Rust `AacEncoder::encode` | 29457 | interleaved→planar `Vec` + output `to_vec` |
| `glint_aac_encode` (pre-planar i16) | 28142 | none (borrowed C pointer) |

~5 % on this host/this fixture. Setup is one `glint_aac_create` either way.

## Footprint (full suite — MP3 not configurable)

| Artifact | bytes |
|---|---|
| `glint-audio` .crate | 15083 |
| `glint-audio-sys` .crate | 304797 |
| vendor AAC sources | 161434 (13 files) |
| vendor non-AAC sources | 971856 |
| cc `.o` AAC | 1521488 |
| cc `.o` other | 15192368 |
| `libglint.a` | 8397452 |
| `libglint_audio_sys*.rlib` | 8416864 |
| `libglint*.rlib` (wrapper) | 121532 |
| `glint_smoke` | 1705728 |

No Cargo feature drops MP3/Opus/Vorbis/FLAC. AAC-only accounting is
source/object subtraction, not the linked rlib.

## Upstream claims

README PEAQ/NMR league, 47.4 KB `GLINT_MODE=fixed` RAM, and ~272× RT
are **upstream** until TASK-13 common scoring. This host did not run
that league. `fixed` / no-FPU was not this build.

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` / ordinary tests | **no-go** |
| AAC-LC encode cell vs syom | **go** with actual ADTS bitrate, quality 0 vs 1, independent-decode length |
| BEST vs NORMAL quality gate | **no-go** on this sine (identical ADTS) |
| Sample-identical decode vs syom goldens | **no-go** (max \|Δ\| 9.39e-2) |
| HE encode/decode peer | **no-go** (LC only) |
| Treat rlib as AAC-only RAM | **no-go** (full suite linked) |
| Upstream PEAQ/speed numbers | **labeled**, not reproduced |
