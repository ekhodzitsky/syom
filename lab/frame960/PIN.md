# TASK-63 lab pin — 960-frame AAC externals

All entries are offline oracles/lab tools; nothing here links into the syom
product crate and nothing is spawned by `cargo test`.

| Tool | Version | How obtained / pinned | Role |
|---|---|---|---|
| FFmpeg CLI | 7.0.2-static (johnvansickle build, gcc 8) | `~/.local/bin/ffmpeg`, `ffmpeg -version` | LOAS + M4A decode oracle (lavc AAC) |
| `lab/libavcodec/avc_driver` | FFmpeg 9.0.1 native AAC, threads=1 | built binary in-tree (see lab/libavcodec) | M4A container decode oracle, 2nd lavc generation |
| libfdk-aac runtime | 2.0.2-3~ubuntu5 (dpkg `libfdk-aac2:amd64`) | system `/usr/lib/x86_64-linux-gnu/libfdk-aac.so.2`, driven via ctypes (no dev headers on host) | FDK decoder lane (raw AU + ASC), FDK encoder capability probe |
| libfaad2 runtime | 2.11.2-1build1 (dpkg `libfaad2:amd64`) | system `/usr/lib/x86_64-linux-gnu/libfaad.so.2`, via ctypes | faad2 decoder lane (NeAACDecInit2 + raw AU) |
| FDK-AAC encoder header | v2.0.3 `libAACenc/include/aacenc_lib.h` | github.com/mstorsjo/fdk-aac raw, fetched 2026-09-22 | parameter enum truth for the encoder probe |
| FFmpeg AAC decoder source | n7.0.2 `libavcodec/aacdec.c`, `aacdec_template.c`, `aactab.c`, `aactab.h`, `loasdec.c` | github.com/FFmpeg/FFmpeg raw, fetched 2026-09-22 | 960/120 sfb tables, frameLengthFlag + SBR-960 handling facts |
| faad2 source | 2.11.3 `libfaad/specrec.c`, `sbr_dec.c`, `sbr_syntax.c`, `mdct.c`, `error.c`, `include/neaacdec.h` | github.com/knik0/faad2 raw, fetched 2026-09-22 | cross-check of 960 band counts; SBR 15-slot fact |
| ETSI TS 102 563 | V1.1.1 PDF | etsi.org deliver URL (see REPORT) | DAB+ 960-transform mandate |

## Host gaps recorded

- No JDK, no Apple/Android hosts — irrelevant for this task.
- **No encoder on this host can produce 960-frame AAC**: FDK encoder rejects
  `AACENC_GRANULE_LENGTH=960` for every AOT (`AACENC_UNSUPPORTED_PARAMETER`,
  measured 2026-09-22; the v2.0.3 header documents only
  1024/512/480/256/240/128/120); FFmpeg's native `aac` encoder has no
  frame-length option; FAAC has none. Vectors are therefore hand-built from
  ISO/IEC 14496-3 syntax and validated by four independent decoders.
- ISO/IEC 14496-26 conformance bitstreams not obtained (same gap as the
  TASK-103 campaign).
