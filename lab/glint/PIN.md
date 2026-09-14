# glint-audio 0.11.0 pin (TASK-11)

Engine: **glint** C++17 AAC-LC (CrispStrobe), consumed through crates.io
**glint-audio 0.11.0** (safe wrapper) + **glint-audio-sys 0.11.0**
(vendored native sources + `cc` build). MIT. Keep off the syom product
crate and off ordinary `cargo test`.

This is the **crates.io 0.11.0 vendor snapshot**, not GitHub `main` and
not a `v0.11.0` git tag (that tag 404s). `glint_version()` in this
snapshot still returns **0.8.0**.

## Crates

| Crate | Version | .crate SHA-256 | Size |
|---|---|---|---|
| glint-audio | 0.11.0 | `df09912ed6bd5062c86fd42b646de28c5727f4fb8e2c2adbd58e9465dff8cc12` | 15083 |
| glint-audio-sys | 0.11.0 | `b8c79457db4164f6fe521359cb4aba0f25e32c5094d8d0214a98a2242dda4ac6` | 304797 |

MSRV 1.70. Wrapper is 862 Rust SLoC; sys vendors 50 C++ TUs.

```sh
sh lab/glint/build.sh   # sets CC/CXX and rustc linker to zig-c++ GNU
./lab/glint/target/release/glint_smoke smoke src/goldens/sine48.adts
```

Host has no `g++` in `PATH`. `zig-c++ -target x86_64-linux-gnu` compiles
the vendored C++17 (libc++); rustc's default gcc link (`-lstdc++`) fails,
so `build.sh` also sets the rustc linker to the same wrapper.

Ordinary `cargo test --workspace --lib` **must not** link or spawn this lab.

## Precision / ISA / quality (this snapshot)

| Knob | 0.11.0 crate behavior |
|---|---|
| Precision | **double** (desktop). `GLINT_MODE=fixed` / `GLINT_AAC_INT` are **not** set by `build.rs`. AAC `glint_aac_config` has no `path` field. |
| ISA | MP3 `glint_config.simd`: 0 auto / 1 AVX / 2 SSE2 / 3 none / 4 NEON. AAC config has **no** SIMD field; AAC TUs do not read `g_simd_level`. Host x86_64: compiler may emit SSE2 (`__SSE2__`). |
| Quality | `GLINT_QUALITY_SPEED=0` (no NMR shaping), `NORMAL=1` (default), `BEST=2` (more NMR iterations). AAC VBR: `vbr=1`, `vbr_quality` 0..9. |
| Rate | AAC `bitrate` is **kbps**, not bits/s. CBR-average bit-debt (comment in `aac_encoder.cpp`); ADTS frames vary. |
| Delay | Encoder delay **2048** samples; first encode is a silence priming frame; `flush` emits two tail frames. |

## AAC-only vs full suite (not configurable)

`glint-audio-sys` `build.rs` compiles **every** `vendor/src/*.cpp` (MP3,
Opus, Vorbis, FLAC, WAV, resample). **No Cargo feature** excludes MP3.
Footprint accounting for AAC cells must subtract non-AAC sources; the
linked rlib still contains the full suite.

Vendor source bytes (this crate): AAC 161434 (13 files), MP3-ish 334394,
Opus 501115, Vorbis 63863, FLAC 15747, shared/other 56737.

## Advertised vs this release

Upstream README quality/speed/RAM claims (PEAQ, 47.4 KB fixed, ~272× RT)
are **upstream measurements** until a common TASK-13 score. This lab
does not re-run that league.

HE-AAC encode/decode: **unavailable** (AAC-LC only).
