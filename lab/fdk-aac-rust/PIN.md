# fdk-aac-rust 0.2.3 pin (TASK-5)

Engine: **fdk-aac-rust 0.2.3** (penguin425), a third-party Rust port of
the Fraunhofer FDK AAC Codec Library for Android. **Not** native
mstorsjo/fdk-aac (that is [lab/fdk](../fdk/), TASK-7) and **not** the
`fdk-aac` crates.io FFI crate (haileys/fdk-aac-rs).

This port **shares FDK 2.0.3 lineage** with TASK-7. It is a separately
identified competitor, not a second independent FDK oracle.

## Crate

- crates.io: `fdk-aac-rust = "=0.2.3"`
- `.crate` SHA-256: `607e6ba558b60e1219ccc8200bbf03fb96fad20c5d948ab25982ea964af13ae5`
- Size: 603806 bytes
- MSRV: 1.87.0
- License: Fraunhofer FDK AAC (`NOTICE`). **Not MIT.** Keep off the
  syom product crate and off ordinary `cargo test`.

## Features

| Path | Cargo | What it is | This host |
|---|---|---|---|
| Default | `default = ["ffi"]` → `fdk-aac-rust-sys` | Native FDK C++ via FFI | **blocked** (no `g++`, same as TASK-7) |
| Measured | `--no-default-features` | Pure-Rust port | smoked below |

`build.rs` still git-fetches `mstorsjo/fdk-aac` at
`d8e6b1a3aa606c450241632b64b703f21ea31ce3` for tables unless
`FDK_AAC_SOURCE_DIR` or `DOCS_RS` is set. First build needs GitHub or a
local fdk-aac tree.

## Table-source revision

- Intended: `d8e6b1a3aa606c450241632b64b703f21ea31ce3` (`build.rs`)
- Native FDK pin (TASK-7): tag **v2.0.3** (`716f4394641d53f0d79c9ddac3fa93b03a49f278`)
- Smoke used `FDK_AAC_SOURCE_DIR` checked out at `d8e6b1a3`.

```sh
export FDK_AAC_SOURCE_DIR=/path/to/fdk-aac   # commit d8e6b1a3
cargo build --release --manifest-path lab/fdk-aac-rust/Cargo.toml
./lab/fdk-aac-rust/target/release/fdk_aac_rust_smoke decode src/goldens/sine48.adts
```

Ordinary `cargo test --workspace --lib` **must not** link or spawn this lab.
