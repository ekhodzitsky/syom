# Native libavcodec pin (TASK-6)

Intended engine: **FFmpeg 9.0.1** native AAC (`aac` decoder / `aac` encoder).
Not `libfdk_aac`. Not a wrapper crate.

## Source

- URL: https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz
- Size: 12036420 bytes
- SHA-256: `cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635`
- Tag: n9.0.1

Do not vendor the tarball in this crate. Fetch offline:

```sh
curl -fL -o ffmpeg-9.0.1.tar.xz https://ffmpeg.org/releases/ffmpeg-9.0.1.tar.xz
echo cf38e0e28c7e5605942c4a77755349b0145804a397af37eb1fb4c77cb237f635  ffmpeg-9.0.1.tar.xz | sha256sum -c
```

## Configure (AAC-only static prefix)

Recorded 2026-09-14, gcc 15.2.0, Linux x86_64.

```
./configure --prefix=<prefix> --disable-all --disable-autodetect --disable-network
  --disable-doc --disable-programs --disable-debug --disable-x86asm
  --enable-static --disable-shared
  --enable-avcodec --enable-avutil --enable-avformat --enable-swresample
  --enable-decoder=aac --enable-encoder=aac --enable-parser=aac
  --enable-demuxer=aac --enable-demuxer=mov --enable-muxer=adts
  --enable-protocol=file --enable-bsf=aac_adtstoasc
  --extra-cflags="-fPIC -O2"
```

`--disable-x86asm`: no nasm in the lab host; **runtime ISA is compiler inline only**
(HAVE_AVX*=yes in configure, but `!HAVE_AVX_EXTERNAL`). Adapter runtime
sets `threads=1`.

Enabled: `CONFIG_AAC_DECODER`, `CONFIG_AAC_ENCODER`, `CONFIG_AAC_PARSER`.
Not enabled: `aac_latm` decoder, `libfdk_aac`, hardware AAC.

## Lanes (do not mix in one timing cell)

| Lane | What it measures |
|---|---|
| `in_process_au` | libavcodec only: ADTS frames as access units |
| `in_process_container` | libavformat + libavcodec (demux cost included) |
| `process_launch` | `ffmpeg` CLI process start + decode/encode (not a codec-time cell) |

## Build the adapter

```sh
make -C lab/libavcodec FFMPEG_PREFIX=<prefix>
```

Ordinary `cargo test --workspace --lib` **must not** link or spawn this lab.
