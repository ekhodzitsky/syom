# Offline native comparison lab

Isolated from the `syom` package. Not a Cargo workspace member. Product
`[dependencies]` stay empty; ordinary tests never download, link, or spawn
ffmpeg / FDK / FAAD2 / fdk-aac-rust / FAAC / glint.

| Adapter | Pin | Status |
|---|---|---|
| [libavcodec](libavcodec/) | FFmpeg 9.0.1 native `aac` | TASK-6 |
| [fdk](fdk/) | fdk-aac v2.0.3 | TASK-7 |
| [faad2](faad2/) | FAAD2 2.11.3 (`FAAD_FMT_FLOAT`) | TASK-8 |
| [fdk-aac-rust](fdk-aac-rust/) | fdk-aac-rust 0.2.3 (Rust FDK port) | TASK-5 |
| [faac](faac/) | FAAC 1.31.1 (`libfaac` ADTS LC) | TASK-9 |
| [glint](glint/) | glint-audio 0.11.0 (C++17 AAC-LC) | TASK-11 |
| [score](score/) | alignment / SNR diagnostics (not PEAQ) | TASK-13 |
| [baseline](baseline/) | matched-output LC/HE/PS timings (TASK-15) | TASK-15 |
| [prime](prime/) | encoder priming / omitted tail (TASK-40) | TASK-40 |
| [quality](quality/) | same-bitrate LC encode SNR/rate (TASK-16) | TASK-16 |
| [wasm](wasm/) | `wasm32-unknown-unknown` smoke run under Node, native reference ([REPORT](wasm/REPORT.md)) | TASK-100 |
| [listen](listen/) | preregistered MUSHRA/BS.1116 protocol (dry-run only) | TASK-14 |
| [fuzz](fuzz/) | std mutational parser + stateful stream fuzz | TASK-48/49 |

See [PIN.md](PIN.md) for checksums, configure flags, ISA and lanes.

```sh
# after a local FFmpeg 9.0.1 prefix exists:
make -C lab/libavcodec FFMPEG_PREFIX=/path/to/prefix
python3 lab/smoke.py --driver lab/libavcodec/avc_driver
python3 lab/faad2/smoke.py lab/faad2/faad_driver
```
