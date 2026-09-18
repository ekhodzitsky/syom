# TASK-105 — `no_std`, feature splitting and fixed workspaces: decision

Measured 2026-09-18 on x86_64 Linux, rustc 1.97.1 (release, fat LTO).

## Inventory: what the library takes from `std`

| need | where | `no_std + alloc` answer |
|---|---|---|
| float math: `sqrt` ×21, `sin`/`cos`/`sin_cos` ×45, `abs` ×39, `floor`/`round`/`ceil`/`trunc` ×14, `powf`/`powi` ×12, `exp2` ×9, `ln`/`log2` ×6, `atan2` ×1 | filterbank tables, SBR / PS math, quantizer, psy | **not in `core` on stable.** Probe crate for `thumbv7em-none-eabihf`: `f32::abs` compiles; `sqrt`, `floor`, `sin`, `powf` are "no method named …" (E0599). Needs the `libm` crate (breaks zero dependencies) or in-tree replacements for every call |
| `std::io::{Read, Write, Seek}` | 17 uses in 8 files: `decode_read*`, `encode_write*`, `M4aSeek`, `AacError::Io` | API surface; would have to be feature-gated |
| `std::fs`, `std::path` | `read`, `write`, `write_with` | gate or drop |
| `std::sync::OnceLock` | 14 uses in 9 files: lazily built tables | `core` has no equivalent without a spin / critical-section choice |
| `std::error::Error`, `fmt`, `mem`, `collections::VecDeque`, `cell` | everywhere | fine (`core` / `alloc`) |
| `std::arch` + runtime feature detection | 1 file (SIMD dispatch) | compile-time `cfg` only |

## Measured resource needs (the real embedded constraint)

Smallest thread stack that completes the call (`lab/footprint` `stack`
probe, bisection to 4 KiB):

| operation | stack |
|---|---:|
| decode LC stereo | 115 KiB |
| decode HE v2 (SBR + PS) | **391 KiB** |
| encode LC stereo | 163 KiB |
| encode HE v2 | 163 KiB |
| encode 5.1 | 163 KiB |

**After TASK-118** (frame matrices moved into reusable heap state, tables
and boxed state built through `engine::heap::heap_array`):

| operation | before | after |
|---|---:|---:|
| decode LC stereo | 115 KiB | 39 KiB |
| decode HE v2 (SBR + PS) | 391 KiB | 39 KiB |
| encode LC stereo | 163 KiB | 83 KiB |
| encode HE v2 | 163 KiB | 83 KiB |
| encode 5.1 | 163 KiB | 91 KiB |

What was on the stack: PS mixing coefficients (70 KiB) and QMF matrices
(104 KiB) per frame, the envelope adjuster's tables (77 KiB), SBR `XLow` /
`XHigh` (61 KiB), the `POW43` table initializer (52 KiB, first frame only),
the 25 KiB decoder state by value in `decode_with`, and in the encoder the
quantizer channels built on the stack inside `Box::new` (44 KiB) plus two
8 KiB frame buffers. PCM and bytes are identical on every golden; decode
speed by the minimum of ten alternating pinned runs: LC −3.1%, HE v2 −0.2%;
encode cells scatter −12% … +8% around a −1.7% median on this noisy host
with identical stream hashes. Unoptimized (debug) builds still need about
320 KiB. `gdb` frame walks at the overflow point located every item.

Retained heap (earlier lab cells): LC decode session 37–40 KiB retained,
5.1 decode 627–851 KiB, `Encoder::new` 82 KiB in 8 allocations;
`codec_workspace_bytes` gives the HE / PS state lower bounds. Code size:
decode +687 KB, LC encode +183 KB (`REPORT.md`).

The stack figures matter on hosted targets too: musl's default thread
stack is 128 KiB and macOS secondary threads get 512 KiB, so HE v2 decode
on a musl worker thread overflows today.

Native alternative: no AAC-only native decoder was built for a bare-metal
target in this environment (FDK / FAAD2 / Helix are not available
offline), so the comparison cell of the task is **not measured**; their
published fixed-point designs target tens of KiB of RAM, which the numbers
above do not approach.

## Decisions

| proposal | decision | reason | revisit when |
|---|---|---|---|
| `no_std + alloc` build | **no-go** | float math is outside stable `core`; replacing 150 call sites or adding `libm` contradicts the zero-dependency product rule; a 687 KB float decoder with a 115–391 KiB stack is not an MCU component anyway | `core_float_math` stabilizes, or a named target with FPU, ≥ 1 MB flash and ≥ 512 KiB RAM asks for it |
| cargo features for decode / encode / HE | **no-go** | link-time elimination already gives +183 KB encode-only vs +687 KB decode-only with zero configuration (`REPORT.md`) | a compile-time exclusion requirement (certification, `no_std`) appears |
| fixed-workspace (caller-provided buffers) API | **no-go as API, go as an internal goal** | the public cost is a second API family; the measured problem is stack, not heap: frame-sized arrays live on the stack | — |
| **reduce stack use** | **go** (new task) | 391 KiB HE v2 decode and 163 KiB encode exceed a 128 KiB thread stack | — |

`cargo add syom` stays feature-free and dependency-free.
