# TASK-100 — syom on WebAssembly

Scope of the claim: **`wasm32-unknown-unknown`, run under Node v22.19.0
(V8), no threads, no SIMD, no host imports.** Browsers share the engine
class but were not run; WASI, threads and `wasm32-wasip*` are untested.

## Integration (`lab/wasm`)

A `cdylib` with no dependency but syom and no bindgen: the module imports
nothing (`WebAssembly.instantiate(module, {})`). The C ABI in `src/lib.rs`
is the minimal example:

- **Ownership.** The host calls `syom_alloc(len)`, writes the stream (or
  f32 PCM) into linear memory — the one unavoidable copy — calls
  `syom_decode` / `syom_encode`, reads the 48-byte result block at
  `syom_result()` and, for encode, the output bytes at `out_ptr/out_len`
  (valid until the next call), then `syom_free`s its buffer.
- **Callbacks.** Decode uses `decode_streaming`; the per-frame callback
  runs inside the module on borrowed planes (no per-frame copy). Crossing
  to JS per frame would need an import; the example hashes in place.
- **Unavailable APIs.** `syom::read`, `write`, `write_with` need a
  filesystem and return an I/O error on this target; everything that takes
  bytes, slices, `io::Read` or `io::Write` works. No clock, thread or
  randomness is used by the library.
- **Limits.** `DecodeOptions` memory budgets apply unchanged; linear
  memory is 32-bit, so `unbounded()` still ends at 4 GiB.

Build and run:

    rustup target add wasm32-unknown-unknown
    cd lab/wasm
    cargo build --release --target wasm32-unknown-unknown
    cargo build --release --bin syom_wasm_native
    node run.mjs          # exit 0 = every cell within its contract

## Results (Ryzen AI 9 HX 370, release, `lto`, `panic = abort`)

- Payload: **918 329 bytes** of `.wasm` (whole codec: LC / HE v1 / HE v2
  decode and encode, M4A / ADTS / LATM; 270 KB gzip-class figures were not
  measured). Compile 2.4–13.8 ms, instantiate 0.2–3.7 ms (cold to warm).
- Linear memory: 1.31 MB at start, 4.65 MB after the whole table, **no
  growth over 200 further decodes**.
- First output: one ADTS frame in, PCM out in **0.028 ms** (median of 9).

| case | output | WASM vs native | wasm ms | native ms | wasm ×realtime |
|---|---|---|---:|---:|---:|
| decode sine48.adts | 1 ch 48 kHz, 13 frames | max 2.9e-3 LSB, 1 of 13 312 s16 samples differ | 0.54 | 0.48 | 513× |
| decode sine441.m4a | 1 ch 44.1 kHz, 11 frames | max 2.9e-3 LSB, 0 differ | 0.52 | 0.38 | 484× |
| decode he48.adts | 2 ch 48 kHz, 9 frames | max 2.9e-3 LSB, 2 of 36 864 | 2.73 | 5.00 | 141× |
| decode he48.m4a | 2 ch 48 kHz, 8 frames | max 2.9e-3 LSB, 2 of 32 640 | 2.89 | 4.22 | 118× |
| decode ps48.adts | 2 ch 48 kHz, 26 frames | max 1.2e-3 LSB, 8 of 106 496 | 8.99 | 16.37 | 123× |
| decode latm48.latm | 1 ch 48 kHz, 13 frames | max 7.8e-3 LSB, 1 of 13 312 | 0.51 | 0.33 | 544× |
| decode mc51.adts | 6 ch 48 kHz, 20 frames | max 1.5e-3 LSB, 1 of 122 880 | 4.80 | 2.46 | 89× |
| decode mc71.adts | 8 ch 48 kHz, 16 frames | max 1.5e-3 LSB, 1 of 131 072 | 3.85 | 2.29 | 89× |
| decode lecture.m4a | 2 ch 48 kHz, 12 frames | max 4.9e-4 LSB, 0 differ | 0.64 | 0.40 | 391× |
| encode LC 128k ADTS, 2 s stereo | 33 101 B | **byte-identical** | 48.1 | 27.2 | 42× |
| encode HE v1 48k ADTS, 2 s mono | 12 630 B | **byte-identical** | 16.7 | 9.3 | 120× |
| encode HE v2 32k ADTS, 2 s stereo | 8 532 B | **byte-identical** | 22.1 | 11.2 | 90× |
| encode LC 128k M4A, 2 s stereo | 33 428 B | **byte-identical** | 49.7 | 25.9 | 40× |

Native ms are single cold runs of the reference binary and include table
setup; WASM ms are medians of 5–7 warm runs. Treat the ratio as "about
half native speed for encode, comparable for decode", not a benchmark.

## Numeric determinism

- **Encode is bit-exact across native and WASM** for LC, HE v1 and HE v2:
  the decision path uses only `engine/det_math`.
- **Decode is not bit-exact and is not promised to be**: filterbank tables
  are built with the platform's `sin` / `cos`. The measured gap is at most
  0.008 of one s16 LSB; a handful of samples per stream land on the other
  side of a rounding tie. The product contract (≤ 1 LSB against
  libavcodec) holds with a wide margin.

## Not covered

AAC.js context cells (TASK-20 is not done), browsers, mobile JITs, SIMD
builds, streaming across the JS boundary with per-frame imports.
