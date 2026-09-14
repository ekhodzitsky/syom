# Native FAAD2 pin (TASK-8)

Engine: **FAAD2 2.11.3** (knik0/faad2). Decode-only oracle. Not a
process wrapper around the `faad` CLI. **GPL-2.0-or-later** — keep
FAAD2 off the syom product crate and off ordinary `cargo test`.

## Source

- URL: https://github.com/knik0/faad2/archive/refs/tags/2.11.3.tar.gz
- Size: 659176 bytes
- SHA-256: `860ab62087e336c1844a70e33196c1790b525fb9a9e7b6ac4fab1a1a4e4d5ce8`
- License: GPL-2.0-or-later (`COPYING`). Nero copyright notice in
  `include/neaacdec.h`.

```sh
curl -fL -o faad2-2.11.3.tar.gz https://github.com/knik0/faad2/archive/refs/tags/2.11.3.tar.gz
echo 860ab62087e336c1844a70e33196c1790b525fb9a9e7b6ac4fab1a1a4e4d5ce8  faad2-2.11.3.tar.gz | sha256sum -c
cmake -S faad2-2.11.3 -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF \
  -DCMAKE_INSTALL_PREFIX=<prefix>
cmake --build build && cmake --install build
make -C lab/faad2 FAAD2_PREFIX=<prefix>
```

This host (2026-09-14) had `gcc` 15.2.0 but no GNU `make`/`cmake`. libfaad
was compiled with `gcc -c` over `libfaad/*.c` and `ar rcs libfaad.a`
(`HAVE_LRINTF`, `APPLY_DRC=1`, `-ffloat-store`). The adapter links
`-lfaad -lm`. Record that path if cmake is missing.

## Runtime settings (adapter)

| Param | Value |
|---|---|
| API | `NeAACDecInit` + `NeAACDecDecode` (ADTS/ADIF) |
| Output | `FAAD_FMT_FLOAT` (IEEE f32, interleaved then split to planar) |
| SBR | `dontUpSampleImplicitSBR=0` (upsample) |
| downMatrix | 0 |
| s16 | not used; float is the oracle precision. s16 = `round(f32*32768)` if needed |
| Delay | not in `NeAACDecFrameInfo`; sample count is library-emitted PCM |
| Channel labels | `channel_position[]` (MPEG speaker ids, not lavc order) |

LATM/LOAS is **unsupported** in this adapter (`Init` fails). M4A needs
`NeAACDecInit2` + ASC (out of this decode-AU lane).

## Lanes

| Lane | What it measures |
|---|---|
| `in_process_adts` | libfaad only, ADTS as a byte buffer |
| `process_launch` | `faad` CLI (not a codec-time cell; not used here) |

Ordinary `cargo test --workspace --lib` **must not** link or spawn this lab.
