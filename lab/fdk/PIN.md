# Native FDK-AAC pin (TASK-7)

Engine: **fdk-aac v2.0.3** (mstorsjo/fdk-aac), not the fdk-aac-rust translation
and not a process wrapper.

## Source

- URL: https://github.com/mstorsjo/fdk-aac/archive/refs/tags/v2.0.3.tar.gz
- SHA-256: `e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78`
- License: Fraunhofer FDK AAC (see upstream `NOTICE`). **Not MIT.** Keep FDK
  off the syom product crate and off ordinary `cargo test`.

```sh
curl -fL -o fdk-aac-2.0.3.tar.gz https://github.com/mstorsjo/fdk-aac/archive/refs/tags/v2.0.3.tar.gz
echo e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78  fdk-aac-2.0.3.tar.gz | sha256sum -c
cmake -S fdk-aac-2.0.3 -B build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DBUILD_PROGRAMS=OFF -DCMAKE_INSTALL_PREFIX=<prefix>
cmake --build build && cmake --install build
make -C lab/fdk FDK_PREFIX=<prefix>
```

## Runtime settings (adapter)

| Param | Decode | Encode |
|---|---|---|
| Transport | `TT_MP4_ADTS` | `AACENC_TRANSMUX=2` (ADTS) |
| AOT | from bitstream | `2` (AAC-LC) |
| Channels | stream info | `MODE_1` / `MODE_2` |
| Bitrate | n/a | caller bps (default 128000) |
| Afterburner | n/a | **off** (`AACENC_AFTERBURNER=0`) |
| Delay | `CStreamInfo` / encoder `nDelay` printed | encoder `AACENC_InfoStruct.nDelay` |
| Threads | FDK internal | FDK internal |

HE v1/v2: decoder may reconstruct SBR when the library is built with SBR;
this adapter reports `aot` from stream info. Encoder AOT 5/29 (HE) and
low-delay AOTs 23/39 are **unsupported cells** in this lab (LC-only encode).

M4A demux is outside codec timing (same rule as TASK-6).

## Host note (this checkout)

In-process smoke needs a C++ compiler to build libfdk-aac. The TASK-6
FFmpeg adapter is C and was smoked. If `g++`/`cc1plus` is missing, build
the FDK prefix on a machine that has it and point `FDK_PREFIX` here.
Ordinary syom tests never do that.
