# Mobile integration notes (TASK-101)

Minimal guidance for embedding `syom` in an iOS/Android app. Everything
here is architecture fact verified on this host (compile-only targets) or
already-executed lanes; nothing is a device measurement.

## Why it is a small job

- Pure Rust, **zero `[dependencies]`**, no build script, no C/FFI on the
  product path. `cargo add syom` is the whole dependency story; nothing to
  vendor, patch or license-audit beyond LICENSE/NOTICE.
- `std` is used (allocation, `io`), which exists on both Android and iOS
  Rust targets. Unlike the WASM lane, `syom::read` / `write` (filesystem)
  work on mobile targets.
- Single-threaded engine; no threads, atomics-based global state, clocks
  or RNG on the codec path. The caller picks the thread.
- aarch64 NEON is baseline (no runtime probe — legal on every AArch64
  phone); x86_64 SSE2/AVX are runtime-probed. **armv7 (32-bit Android)
  compiles but runs the scalar path** — budget CPU accordingly.

## Linkage

Android (Rust side of an NDK app):

```toml
# wrapper crate, e.g. syom-android/
[lib]
crate-type = ["staticlib"]   # or "cdylib" for JNI loadLibrary
```

```
rustup target add aarch64-linux-android   # + armv7/x86_64 as needed
cargo build --release --target aarch64-linux-android
# final link with the NDK clang:
#   $NDK/toolchains/llvm/prebuilt/<host>/bin/aarch64-linux-android<api>-clang
# (not done on this host — no NDK; the rlib codegen cell is green)
```

iOS: same shape — `staticlib`, targets `aarch64-apple-ios` (device) /
`aarch64-apple-ios-sim` + `x86_64-apple-ios` (simulator), final link by
Xcode `clang -isysroot <sdk>`, then lipo/xcframework if both simulator
slices ship. Pin `RUSTFLAGS` per release build and record them; the
encoder's byte-exactness makes cross-build drift detectable.

C ABI surface: keep it as thin as `lab/wasm/src/lib.rs` (the wasm lane's
48-byte result block + alloc/free is a working template). The product
crate deliberately ships no FFI wrapper; the wrapper is app-owned.

## Buffer ownership (the part integrators get wrong)

- **One-shot** `syom::decode(&bytes)` returns an owned `DecodedAac`
  (planar `Vec<f32>` per channel). Caller owns it; drop whenever.
  Convenience, costs one output allocation.
- **Streaming** `Decoder::feed(&bytes, |frame| …)` hands you
  `Frame<'a>` whose `planar` **borrows decoder scratch — valid only inside
  the callback**. Copy out (or consume, e.g. write to an
  `AudioTrack`/AudioUnit ring) before returning. `Frame::meta` is `Copy`.
- After warm-up, the speech/stereo borrowed path is **zero-alloc per
  frame**; `Decoder::reset` reuses the prepared workspace, so a format
  change does not imply fresh heap. Feed from one thread; `Decoder` is a
  state machine, not a shared object.
- Errors are terminal for the instance until `reset` — a parser/limit/
  callback failure is sticky on purpose; do not keep feeding.
- Budgets are independent of stream duration: 1 GiB compressed input,
  4 GiB collected PCM, 8 channels, 8 MiB workspace caps (see
  `DecodeOptions`; `speech()` is the conservative default). Streaming
  `feed` caps the resident buffer, so infinite streams plateau.
- Stack: release decode/encode paths were probed ≤ 96 KiB (TASK-118);
  safe for the small default stacks of audio render callbacks.

## Determinism contract across platforms

- **Encode is byte-exact on every target** (decision path uses only
  in-crate `det_math`; NEON avoids FMA to match scalar rounding). A golden
  diff on a new phone means the toolchain/flags changed, not the chip.
- **Decode may differ ≤ 0.008 of one s16 LSB** across platforms
  (filterbank table setup uses the platform `sin`/`cos`); the product
  contract vs libavcodec (≤ 1 LSB) holds with a wide margin.

## Known interop caveat (from `REPORT.md` finding 1)

Until the follow-up lands, syom's ABR stuffing (zero bytes after
`ID_END`) is rejected by the fdk-aac decoder codebase that ships as
Android's platform software AAC decoder. If the product both encodes with
syom and plays through MediaCodec on Android, qualify that path on a
device first — or track the stuffing-fix follow-up task.
