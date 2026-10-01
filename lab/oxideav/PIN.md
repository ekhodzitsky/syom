# oxideav-aac 0.1.7 pin (TASK-95)

Engine: **oxideav-aac 0.1.7** (OxideAV/oxideav-aac), pure-Rust AAC-LC +
HE-AAC v1 encoder/decoder. **MIT.** Lab-only dependency; never on the
syom product crate and never built by ordinary `cargo test`.

## Source

- Crate: `oxideav-aac = "=0.1.7"` (crates.io)
- Crate file SHA-256: `35453ffa827080f322dba5d2233879f008627d4ffb07c496c0c6fe3190b35048`
  (`https://static.crates.io/crates/oxideav-aac/oxideav-aac-0.1.7.crate`)
- Dependency: `oxideav-core 0.1.36` (locked in `Cargo.lock`)
- Upstream: https://github.com/OxideAV/oxideav-aac

## Adapter surface (`src/main.rs`, `oxideav_driver`)

| Command | Meaning |
|---|---|
| `id` | engine label |
| `encode-pcm RATE CH BPS lc\|he IN.f32 OUT.adts` | planar f32le in, ADTS out; `lc` = `StreamEncoder` (AAC-LC), `he` = `HeAacEncoder` (HE-AAC v1, LC core at half rate + SBR FIL) |

Encoder input is interleaved S16 (the adapter converts f32). `bitrate` is
the whole-stream target (core + SBR side info are charged against one
frame budget). HE v2 / PS encode: **not present** in 0.1.7 (decoder only).

## Build

```sh
cargo build --release --manifest-path lab/oxideav/Cargo.toml
```

Needs crates.io access once for the locked packages; no C/C++ toolchain.
