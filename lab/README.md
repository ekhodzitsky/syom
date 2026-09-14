# Offline native comparison lab

Isolated from the `syom` package. Not a Cargo workspace member. Product
`[dependencies]` stay empty; ordinary tests never download, link, or spawn
ffmpeg / FDK.

| Adapter | Pin | Status |
|---|---|---|
| [libavcodec](libavcodec/) | FFmpeg 9.0.1 native `aac` | TASK-6 |
| FDK | v2.0.3 | TASK-7 (separate) |

See [PIN.md](PIN.md) for checksums, configure flags, ISA and lanes.

```sh
# after a local FFmpeg 9.0.1 prefix exists:
make -C lab/libavcodec FFMPEG_PREFIX=/path/to/prefix
python3 lab/smoke.py --driver lab/libavcodec/avc_driver
```
