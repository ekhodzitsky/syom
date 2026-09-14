# FAAC 1.31.1 pin (TASK-9)

Engine: **FAAC 1.31.1** (knik0/faac), in-process `libfaac` encode to ADTS.
Not the `faac` CLI. **LGPL-2.1-or-later** (ISO MPEG-4 reference notice in
`README`). Keep FAAC off the syom product crate and off ordinary
`cargo test`.

This is **current 1.31.1 release behavior**, not historical FAAC 1.28
hydrogenaudio lore.

## Source

- URL: https://github.com/knik0/faac/archive/refs/tags/faac-1.31.1.tar.gz
- Size: 244025 bytes
- SHA-256: `3191bf1b131f1213221ed86f65c2dfabf22d41f6b3771e7e65b6d29478433527`
- Tag: `faac-1.31.1` (commit `3aec0c1` per GitHub release)

```sh
curl -fL -o faac-1.31.1.tar.gz https://github.com/knik0/faac/archive/refs/tags/faac-1.31.1.tar.gz
echo 3191bf1b131f1213221ed86f65c2dfabf22d41f6b3771e7e65b6d29478433527  faac-1.31.1.tar.gz | sha256sum -c
```

Do not vendor the tarball.

## Advertised vs this release

| Claim | 1.31.1 `faaccfg.h` / `libfaac` |
|---|---|
| AAC-LC | **yes** (`LOW = 2`) |
| MAIN / SSR / LTP | object-type macros exist; this adapter uses **LOW only** |
| HE-AAC v1 (SBR) | **no** in 1.31.1 library sources (GitHub *current* README mentions HE v1; that is not this tag) |
| MPEG-2 / MPEG-4 | `mpegVersion` MPEG2=1 / MPEG4=0; adapter uses MPEG-4 |
| Rate control | `bitRate` is **per channel**; maps into `quantqual` (quality loop) |
| TNS | `useTns` (adapter **off**) |
| Joint stereo | `jointmode` MS/IS; adapter **JOINT_MS** |
| Input | FLOAT (4) |
| Output | ADTS_STREAM |

## Build (this host, 2026-09-14)

gcc 15.2.0, Linux x86_64. No GNU make/cmake/meson required:

```
gcc -c -O2 -fPIC -std=gnu99 -msse2 -DHAVE_CONFIG_H \
  -I<build> -Iinclude -Ilibfaac libfaac/{bitstream,fft,frame,blockswitch,util,channels,filtbank,tns,quantize,huff2,huffdata,stereo}.c
ar rcs libfaac.a *.o
```

`config.h`: `PACKAGE_VERSION "1.31.1"`, `HAVE_IMMINTRIN_H 1`. SSE2 path in
`quantize.c` is compiled in (`__SSE2__` on x86_64). DRM/`libfaac_drm` is
**not** built.

```
make -C lab/faac FAAC_PREFIX=<dir-with-include-and-libfaac.a>
```

Ordinary `cargo test --workspace --lib` **must not** link or spawn this lab.
