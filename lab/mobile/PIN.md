# Mobile-lane pins (TASK-101)

Everything on this page is host-side; the missing resources are named in
REPORT.md "Qualification gaps". Reproduce with `python3 lab/mobile/capture.py`.

## Host (this checkout, 2026-09-22)

- Linux x86_64, rustc/cargo 1.97.1 (repo pin, `rust-toolchain.toml`),
  gcc 15.2.0, zig 0.14.1 (`~/.local/zig/zig`, used as `zig c++`).
- **Absent:** JDK, Android SDK/NDK, Xcode/macOS, iOS/Android devices or
  emulators, qemu-user. No on-device or emulated ARM execution is possible
  on this host.

## Rust targets (compile-only cells)

Added with `rustup target add` against toolchain 1.97.1:

`aarch64-linux-android`, `armv7-linux-androideabi`, `i686-linux-android`,
`x86_64-linux-android`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`,
`x86_64-apple-ios`, `aarch64-unknown-linux-musl` (ARM Linux control).

No NDK/Xcode linker exists here, so evidence stops at `cargo check` /
`cargo build --lib` (rlib codegen; no final link). Linking needs
`$NDK/toolchains/llvm/prebuilt/*/bin/aarch64-linux-android<api>-clang`
(Android) or Xcode `clang` with `-isysroot` (iOS).

## Platform-decoder codebases (offline oracles, in-process)

- **fdk-aac v2.0.3** — the codebase AOSP vendors as the platform software
  AAC decoder (`external/aac`; the tarball ships `Android.bp`). Pin and
  build recipe: `lab/fdk/PIN.md` (sha256
  `e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78`,
  170 CMake-listed TUs via zig c++ `-target x86_64-linux-gnu`,
  `lab/fdk/build_driver.sh`). Host build, x86_64 — the *decoder code* is
  the Android one, the CPU/OS is not.
- **FAAD2 2.11.3** — historic OEM/Android alternative (pre-FDK
  stagefright). Pin: `lab/faad2/PIN.md` (sha256
  `860ab62087e336c1844a70e33196c1790b525fb9a9e7b6ac4fab1a1a4e4d5ce8`;
  on this host gcc 15.2.0, `-DHAVE_INTTYPES_H=1 -DHAVE_STDINT_H=1
  -DHAVE_SYS_TYPES_H=1 -DHAVE_STRING_H=1 -DPACKAGE_VERSION=\"2.11.3\"
  -DHAVE_LRINTF=1 -DAPPLY_DRC=1 -ffloat-store`, then the `lab/faad2`
  Makefile steps by hand — no GNU make here).
- **libavcodec 9.0.1 native AAC** — in-tree golden oracle, `lab/PIN.md`
  (already-built `lab/libavcodec/avc_driver`). Control cell.

None of these are spawned by `cargo test`; they are offline oracles.
