# oxideav-aac 0.1.7 encode/decode smoke (TASK-95)

Recorded 2026-09-23, rustc 1.97.1, Linux x86_64. Isolated `lab/oxideav`;
locked `Cargo.lock` (`oxideav-aac =0.1.7`, `oxideav-core 0.1.36`). Not
invoked by `cargo test`. Pin: `PIN.md`.

## Smoke (2 s stereo tones+HF 48 kHz planar f32 → s16 interleaved)

| Mode | requested bps | ADTS bytes | ffprobe profile / rate / ch |
|---|---:|---:|---|
| `lc` (`StreamEncoder`) | 48 000 | 11 970 | LC / 48000 / 2 |
| `he` (`HeAacEncoder`) | 48 000 | 9 562 | HE-AAC / 48000 / 2 |

Both streams decode through the neutral ffmpeg 7.0.2 CLI at full rate and
channel count (TASK-95 matrix in `lab/quality/HE_QUALIFY.md`).

## Capability map (0.1.7 source, verified by build + smoke)

| Cell | Status |
|---|---|
| AAC-LC encode mono/stereo | **go** (`StreamEncoder::encode_all`, ADTS) |
| HE-AAC v1 encode mono/stereo | **go** (`HeAacEncoder`, implicit ADTS SBR; ffprobe HE-AAC) |
| HE-AAC v2 / PS **encode** | **no-go** (decoder-only PS modules; no PS encoder in 0.1.7) |
| Input | interleaved S16 only (adapter converts f32; one extra quantization) |
| Encoder delay | not declared by the API; measured per cell in the matrix |

## Go / no-go

| Lane | Decision |
|---|---|
| Product `[dependencies]` | **no-go** (competitor; lab-only) |
| Ordinary `cargo test` | **no-go** |
| Rust HE v1 peer cell | **go** with actual bytes + neutral decode |
| Rust HE v2 peer cell | **no-go** (no PS encoder) |
| Independent oracle for syom | **no-go** for claims (engine lineage independent, but conformance of either side is established via lavc/FDK/FAAD2, not each other) |
