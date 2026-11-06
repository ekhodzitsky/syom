# Isolated AAC cost attribution

These four drivers explain where a consumer spends time. They do not replace
[gigastt's unchanged six-case acceptance harness](../README.md), its `<= 1.05`
median paired limit, PCM qualification, or model-backed HTTP/load checks. They
never load an ASR model. Nothing in this directory is linked into the syom
product crate or its ordinary tests.

Current path dependencies and generator lock templates follow the
0.10.1 checkout. The historical attribution and cold-stack results below used
0.10.0 sources; rerunning against this checkout produces new evidence and must
retain its own source manifest. Published 0.10.0 remains the registry baseline.

The recorded diagnostics used Linux x86_64, Rust **1.98.1**, release **fat LTO**,
one codegen unit and optimization level 3. `Cargo.lock` pins diagnostic
rubato **5.0.1**; syom is a path dependency on this checkout. Reproducing a
historical run therefore also requires its recorded syom revision/source
manifest and fixture hashes. The promoted sources preserve the experiments;
relocation and formatting can change executable hashes. Original binary hashes
and raw samples remain in the parent evidence instead of being relabeled.

## Build and fixture inputs

Run from the syom repository root, with the pinned toolchain installed and the
original fixture directory available locally. No audio is committed here.
The historical cache name is not an assertion that the current consumer uses
syom 0.8.1. It contains `speech0.m4a`, `lc.adts`, and `long-stereo.m4a`.

```sh
profile_manifest="lab/consume/gigastt/profile/Cargo.toml"
profile_target="$PWD/target/gigastt-attribution"
fixture_dir="$HOME/.cache/gigastt-syom-081/fixtures"
CARGO_TARGET_DIR="$profile_target" CARGO_BUILD_JOBS=2 taskset -c 2-7 \
  cargo +1.98.1 build --release --locked --offline --manifest-path "$profile_manifest" --bins
sha256sum "$fixture_dir/speech0.m4a" "$fixture_dir/lc.adts" "$fixture_dir/long-stereo.m4a"
sha256sum "$profile_target/release/"syom-*
```

The original fixture hashes are:

| Fixture | SHA-256 |
|---|---|
| `speech0.m4a` | `e7f0f3ee0300ca9951e22ca79a4ee0a22619eba37b56f208d4f644cd24781f13` |
| `lc.adts` | `6d3e903bdfe47d9e1cf7502132bae8a9f6cdf2992493d99541acf94c6bc6b596` |
| `long-stereo.m4a` | `020cca4fdb044c9daf0c74f7d35fa03ef3392af5e7a2ae0f5d8051f7a8edd328` |

CPU 0 was the recorded measurement CPU; CPU 12 is its sibling. Check the host's
allowed affinity and topology before reusing those numbers. Build on separate
cores and finish all builds before a qualified timing campaign. Stop other
profiling/model workloads while that campaign runs. Some exploratory diagnostics
were deliberately collected while LTO ran on cores 2–7: their frequency/load
shifts remain visible and **are not acceptance evidence**. Do not discard those
samples or promote their favorable medians into a performance claim.

## Driver boundaries and commands

| Binary | What it measures or reports |
|---|---|
| `syom-direct-attribution` | Preloaded slice `decode_streaming`: split decode, split with the old manual channel average, or the native mono API. No file syscalls, resampler, worker or handoff. |
| `syom-io-allocation-attribution` | Actual `M4aSeek` open/decode through different readers, with allocation and underlying reader-call counters. These are instrumented counts, not timings. |
| `syom-io-timing` | Actual `M4aSeek` reader path for M4A and read-streaming path for ADTS, same manual average and bounded 24 Ki-sample scratch. Includes file open for file cases. |
| `syom-resampler-attribution` | The copied consumer resampler on the same native PCM, varying its input cadence. Native AAC decode happens before the timer. No worker or HTTP work. |

Slice decode is **not** the gigastt M4A path: that adapter uses `M4aSeek`, and
its file reader originally performed one seek/read per AU. Use the I/O driver
before assigning a slice-vs-consumer difference to codec arithmetic. Native mono
and manual averaging can apply different channel policies outside mono/stereo
(for example, LFE handling); these three fixtures do not qualify a general mode
switch.

```sh
# Native mono versus old split/manual-average PCM: reports lengths and bit differences.
taskset -c 0 "$profile_target/release/syom-direct-attribution" verify "$fixture_dir/speech0.m4a"
taskset -c 0 "$profile_target/release/syom-direct-attribution" verify "$fixture_dir/lc.adts"
taskset -c 0 "$profile_target/release/syom-direct-attribution" verify "$fixture_dir/long-stereo.m4a"

# One explicit slice cell: MODE FILE WARMUPS REPETITIONS.
taskset -c 0 "$profile_target/release/syom-direct-attribution" split "$fixture_dir/long-stereo.m4a" 2 7
taskset -c 0 "$profile_target/release/syom-direct-attribution" splitmix "$fixture_dir/long-stereo.m4a" 2 7
taskset -c 0 "$profile_target/release/syom-direct-attribution" mono "$fixture_dir/long-stereo.m4a" 2 7

# Existing syom stage timers; clock probes perturb timings and report no resampling.
SYOM_PROF=1 taskset -c 0 "$profile_target/release/syom-direct-attribution" split "$fixture_dir/long-stereo.m4a" 0 1

# Underlying read/seek and allocation counts: FILE READER MODE.
taskset -c 2 "$profile_target/release/syom-io-allocation-attribution" "$fixture_dir/long-stereo.m4a" file manual
taskset -c 2 "$profile_target/release/syom-io-allocation-attribution" "$fixture_dir/long-stereo.m4a" buffer8k manual
taskset -c 2 "$profile_target/release/syom-io-allocation-attribution" "$fixture_dir/long-stereo.m4a" smart64k manual

# Native full-bit reader comparisons plus embedded relative/end/error recovery checks.
taskset -c 0 "$profile_target/release/syom-io-timing" verify "$fixture_dir/speech0.m4a"
taskset -c 0 "$profile_target/release/syom-io-timing" verify "$fixture_dir/lc.adts"
taskset -c 0 "$profile_target/release/syom-io-timing" verify "$fixture_dir/long-stereo.m4a"

# Same callback/output work; two warmups then seven recorded decodes per invocation.
taskset -c 0 "$profile_target/release/syom-io-timing" file "$fixture_dir/long-stereo.m4a"
taskset -c 0 "$profile_target/release/syom-io-timing" smart64k "$fixture_dir/long-stereo.m4a"

# Exact resampler, shared native PCM; verify reports each cadence against 1024.
taskset -c 0 "$profile_target/release/syom-resampler-attribution" verify "$fixture_dir/long-stereo.m4a"
taskset -c 0 "$profile_target/release/syom-resampler-attribution" 1024 "$fixture_dir/long-stereo.m4a"
taskset -c 0 "$profile_target/release/syom-resampler-attribution" 24576 "$fixture_dir/long-stereo.m4a"
```

For timing diagnostics, repeat both orders rather than running every baseline
before every candidate. The recorded slice campaign used three orders of
`split`, `splitmix`, `mono`; the I/O and resampler campaigns used opposite orders.
Keep every `samples_ms` value. `syom-io-timing verify` asserts equal lengths and
bits. Slice/resampler `verify` modes print diagnostics: independently require
matching lengths and zero `differing_bits`; process success alone is insufficient.
The observed cadence equality applies to the measured 48 kHz inputs, not every
rate or source. Changing chunk cadence can affect other resampling cases.

The allocation driver intentionally wraps `System` with an instrumented global
allocator; its unsafe delegation is isolated here. It counts allocation calls,
reallocations and requested bytes during decoder open/decode. Requested bytes
are cumulative, not peak live memory or RSS. Buffer construction outside that
window is excluded. Each allocation invocation starts cold, so initial Huffman/
twiddle/workspace setup is counted; this is not a steady-state per-frame
allocation count. The reader counter wraps the underlying source, so buffered
cases count buffer misses rather than public `Read` calls. It accepts M4A only;
reader kinds are `bytes`, `file`, `buffer8k`, `smart8k`, `smart64k`, and modes are
`split`, `manual`, `mono`. Its early smart-buffer prototype assumes a successful
fresh reader; it is not the production error-recovery implementation.

## Whole-consumer syscall and scheduling attribution

These commands need the exact previously hashed probe binaries. `aac_probe`
adds two warmups to its requested one measured iteration, so counts below cover
three complete decodes, plus process initialization. They are not recognition
or current-model HTTP measurements.

```sh
candidate_probe="$HOME/.cache/gigastt/aac-090-target/release/examples/aac_probe"
standard_probe="$HOME/.cache/gigastt/aac-current-main-target/release/examples/aac_probe"
strace -f -c -e trace=read,lseek,futex,clone,clone3 -o target/aac-candidate-syscalls.log \
  taskset -c 2 "$candidate_probe" file "$fixture_dir/long-stereo.m4a" 1
strace -f -c -e trace=read,lseek,futex,clone,clone3 -o target/aac-standard-syscalls.log \
  taskset -c 2 "$standard_probe" file "$fixture_dir/long-stereo.m4a" 1
/usr/bin/time -v -o target/aac-candidate-scheduling.log \
  taskset -c 2 "$candidate_probe" file "$fixture_dir/long-stereo.m4a" 1
/usr/bin/time -v -o target/aac-standard-scheduling.log \
  taskset -c 2 "$standard_probe" file "$fixture_dir/long-stereo.m4a" 1
```

`strace` perturbs execution and thread scheduling; its elapsed/syscall times
must not be substituted for the unchanged acceptance harness. `/usr/bin/time`
CPU/context-switch figures include all three decodes and startup. They locate
I/O and worker handoffs; they do not establish a paired latency ratio.

## Adapter candidate versus diagnostic prototypes

The proposed downstream adapter keeps syom 0.10.0 and uses synchronous bounded M4A
pull with the same logical 24 Ki-sample batch boundaries. ADTS/LOAS and the
existing split/scan paths retain their prior dispatch. Only M4A file pull gets
the 64 KiB buffered reader; it does not change channel selection or trim to
reproduce standard-decoder priming/padding.

Unlike the earliest counter prototype, candidate buffering starts with an
unknown logical position, invalidates it after every I/O error, checks offset
arithmetic, and has an absolute `force_rewind` that discards cached data.
File clones share their physical cursor, so clone/fallback initialization must
force that absolute rewind. The I/O timing prototype tests error recovery but
does not implement candidate clone/fallback ownership. The adapter also has
explicit terminal decoder errors, deliberate callback-yield classification,
NotAac fallback and cancellation/budget tests. Do not copy the diagnostic reader
as a hardened general-purpose adapter.

## Copied source and license

`src/resample_original.rs` is byte-identical to gigastt commit
`49faef1375dd4bef7f0d4ac9602b4dd68bc51dbf`, file
`crates/gigastt-core/src/inference/audio/resample.rs`, SHA-256:

```text
aed9dce69efc65ee4fae0c8644e38aedff031f68f788fb5c9a5cc3573359e1c3
```

Its MIT notice is retained in [LICENSE.gigastt](LICENSE.gigastt), with copy
provenance in [origin.json](origin.json). The copied source is an isolated
measurement oracle, not a syom implementation replacement. Diagnostic crates
may use rubato/anyhow while the syom product retains zero runtime dependencies.

## Cold HE/PS caller stack regression

`prepare_cold_stack.py` generates an isolated crate that includes the proposed
consumer's **exact** `aac/pull.rs`, `aac/input.rs`, and `aac/buffered.rs` by path,
with their `cfg(test)` modules disabled. It extracts the parent append/options/
error-mapping functions verbatim. Minimal parent glue supplies `ChannelSelect`,
`AacBlock`, constants and an `AudioTooLong` error type; no duration budget is set,
so this stub does not establish error-type compatibility. The generator records
consumer and fixture SHA-256 hashes. Keep that manifest with the run logs.

The first decode in each fresh process runs in a **128 KiB thread**, including
M4A open and the production `Box<Pull>` construction. Bytes and File inputs use
the unchanged reader dispatch, including cloned File/SmartBuffered ownership.
After cold decode, a warm decode must match every native PCM bit and each
batch's frame count, sample rate and channel count. This covers the decoder and
pull stack, including resume across batch boundaries, without loading a model or
resampler. It does not establish a full application or another platform's stack
budget. Upstream `tests/cold_stack.rs` separately covers optimized 128 KiB
`decode_with` for LC, HE1, HE2, implicit PS, multichannel and LD.

Schedule this separately from timing/HTTP qualification, from the repository root:

```sh
python3 lab/consume/gigastt/profile/prepare_cold_stack.py \
  --consumer target/task145/gigastt --output target/task145/stack-proof-repro
stack_manifest="$PWD/target/task145/stack-proof-repro/Cargo.toml"
stack_target="$PWD/target/task145/stack-proof-repro-target"
taskset -c 8,9 env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$stack_target" \
  cargo +1.98.1 build --release --locked --offline --manifest-path "$stack_manifest" \
  > target/task145/stack-proof-repro/build.log 2>&1
for fixture in he48 he48em he2_48em ps48; do
  for reader in bytes file; do
    taskset -c 8 "$stack_target/release/syom-pull-stack-proof" \
      "$PWD/src/goldens/$fixture.m4a" "$reader" \
      > "target/task145/stack-proof-repro/$fixture.$reader.log" 2>&1 || exit 1
  done
done
sha256sum "$stack_target/release/syom-pull-stack-proof"
rustc +1.98.1 -Vv
```

The original Linux x86_64 Rust 1.98.1 run passed all eight fresh processes:
`he48` 16,320 samples, `he48em` 28,800, `he2_48em` 62,217, and `ps48` 51,136,
for both reader types, with zero native bit differences. The explicit HE1/HE2
fixtures exceed a logical batch, exercising resumed decoding. `he48.m4a` and
`ps48.m4a` also occur in the consumer's 29-case PCM qualification; the two
explicit-profile goldens add coverage. Raw original statuses, logs and hashes
remain in `target/task145/stack-proof`; no timing claim follows from this test.

## Native decoder versus resampler attribution

`prepare_native_stages.py` generates a second isolated crate from
`native_stages.rs.in`. It snapshots the supplied consumer's pull/input/buffered
modules and original resampler before building, extracts the mono append/options
helpers verbatim, and records their hashes. The standard branch uses Symphonia
0.6.1 with the production format/decoder options and exact production stereo
interleave/add/divide arithmetic. Its packet loop is limited to valid fixture
attribution: production cancellation, budgets and recoverable EOF handling are
outside this driver. The same minimal error-type stub as the cold-stack proof
is used. These snapshots derive from gigastt under [its MIT license](LICENSE.gigastt).

Native timings include open, decode, mono conversion, destruction, and a bounded
49,152-sample scratch sink. They exclude resampling and duration-sized PCM
retention. The scratch sink is cleared after `black_box`, whereas the real
consumer hands batches to the resampler. The pull branch still allocates its
actual native blocks and logical-pause errors. Verification separately retains
full native PCM and exact packet/batch geometry. Native outputs from the two
backends can differ in priming/trimming and floating-point values; this is cost
attribution, not an assertion of native PCM equivalence.

The resampler matrix crosses **PCM source** (standard/pull) with **batch geometry**
(standard/pull). Input decoding happens before the clock. Both use the unchanged
copied resampler, including `drain_ready_into` after each batch. If one geometry
runs out before the other PCM does, the remainder is explicitly flushed. At the
48 kHz fixture, compare hashes within each PCM source across geometries; do not
compare the two backends' hashes as a correctness gate.

The lock pins the consumer's rubato 5.0.1, audioadapter 5.0.0,
audioadapter-buffers/sample 5.1.0, and realfft 3.5.0. Allocator counters are
**disabled during ordinary timings**. `COUNT_ALLOCS=1` enables separate cumulative
allocation/reallocation/requested-byte counts, including cold initialization.
They are not peak live memory, and instrumented times should not be substituted
for the ordinary samples.

```sh
python3 lab/consume/gigastt/profile/prepare_native_stages.py \
  --consumer target/task145/gigastt --output target/task145/native-stages-repro
native_manifest="$PWD/target/task145/native-stages-repro/Cargo.toml"
native_target="$PWD/target/task145/native-stages-repro-target"
taskset -c 2-7 env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$native_target" \
  cargo +1.98.1 build --release --locked --offline --manifest-path "$native_manifest" \
  > target/task145/native-stages-repro/build.log 2>&1
native_bin="$native_target/release/syom-native-stages"
long_fixture="$HOME/.cache/gigastt-syom-081/fixtures/long-stereo.m4a"
taskset -c 0 "$native_bin" verify "$long_fixture" \
  "$PWD/target/task145/native-stages-repro/long" \
  > target/task145/native-stages-repro/geometry.json
for mode in native-std native-pull resample-std-std resample-pull-std resample-std-pull resample-pull-pull; do
  taskset -c 0 "$native_bin" "$mode" "$long_fixture" \
    > "target/task145/native-stages-repro/$mode.json"
  taskset -c 0 env COUNT_ALLOCS=1 "$native_bin" "$mode" "$long_fixture" \
    > "target/task145/native-stages-repro/$mode.allocations.json"
done
sha256sum target/task145/native-stages-repro/*.f32 "$native_bin"
```

For comparisons, repeat the ordinary six-mode sequence forward/reverse/forward
in separate processes, after all builds and unrelated timed workloads stop.
Keep every raw sample (two warmups and seven retained measurements per process).
The original Linux x86_64 Rust 1.98.1 proof retained three sequences in
`target/task145/native-stages/timing.json`. Native round medians were standard
27.75/27.20/28.19 ms and pull 33.33/33.65/35.02 ms. Pooled resampler medians ranged
34.00–34.94 ms across the four combinations. This supports investigating native
decode/ingest before another cadence change; these lab figures do not replace
the unchanged consumer gate.

Actual geometry: standard emitted 3,841,024 native samples in 3,751 packets,
flushing 48,128 samples at a time; pull emitted 3,840,000 presentation samples in
157 batches, flushing 49,152 at a time. Within each PCM source, both resampler
geometries produced identical output hashes. The proposed adapter's separate
29-case qualification establishes preservation of its prior public PCM contract.
Native requested bytes were 1.63 MB for standard and 17.09 MB for pull; much of
the difference is the pull path's 157 freshly allocated float blocks. Resampler
allocation counts depended on cadence, and were identical across PCM sources.
Original unpinned runs with a duration-sized collection sink are preserved in
`target/task145/native-stages/initial-full-collector`, separately from the final
pinned bounded-sink evidence.

## Userspace hot-loop sampling when perf is restricted

`sigprof_sampler.c` and `resolve_sigprof.py` provide a Linux **x86_64-only**
diagnostic fallback when hardware/perf events are unavailable. They require a
single-threaded, owned, unstripped benchmark, GNU `nm`/`readelf` and a C compiler.
The original run used the direct slice `splitmix` driver; it does not attribute
consumer worker threads, resampling, other platforms or the full application.
No kernel settings, sysctls or another process's configuration are changed.

The preload constructor captures `/proc/self/maps`, installs a process-local
SIGPROF handler and starts a 1 ms `ITIMER_PROF` CPU timer. The handler preserves
`errno` and uses an async-signal-safe `write` to append only the interrupted RIP.
It does not allocate, format text or unwind the stack. The destructor disarms the
timer and records delivered/failed counts. Outputs use `O_EXCL`; use a fresh
prefix for every run and verify `.meta` exists with zero failed writes. Apply
`LD_PRELOAD` only to this owned benchmark, which must not already use SIGPROF or
`ITIMER_PROF`. Do not preload an application server. The process CPU timer is
unsuitable for per-thread attribution in a multithreaded consumer.

```sh
sampling_dir="$PWD/target/gigastt-sigprof"
mkdir -p "$sampling_dir"
cc -O2 -g -Wall -Wextra -Werror -Wno-format-truncation -shared -fPIC \
  lab/consume/gigastt/profile/sigprof_sampler.c -o "$sampling_dir/sampler.so"
# Select an already built unstripped direct driver; do not build during sampling.
sampling_bin="$PWD/target/gigastt-attribution/release/syom-direct-attribution"
sampling_prefix="$sampling_dir/long-splitmix"
taskset -c 0 env LD_PRELOAD="$sampling_dir/sampler.so" \
  SYOM_SAMPLE_PREFIX="$sampling_prefix" "$sampling_bin" splitmix \
  "$HOME/.cache/gigastt-syom-081/fixtures/long-stereo.m4a" 2 700 \
  > "$sampling_prefix.stdout.json" 2> "$sampling_prefix.stderr.log"
python3 lab/consume/gigastt/profile/resolve_sigprof.py "$sampling_prefix" "$sampling_bin"
cat "$sampling_prefix.meta"
sha256sum "$sampling_bin" "$sampling_dir/sampler.so" \
  lab/consume/gigastt/profile/sigprof_sampler.c \
  lab/consume/gigastt/profile/resolve_sigprof.py
```

The resolver verifies the benchmark's first ELF `PT_LOAD` virtual address is zero,
then subtracts its captured PIE load bias and resolves functions through
`nm -n -S -C`. It reports module counts, symbol counts and ELF instruction offsets.
These offsets are meaningful only with the exact sampled binary hash. Shared
library samples are counted by module, not assigned to benchmark symbols.
This is periodic interrupted-instruction sampling: it provides approximate hot
regions, with timer/handler overhead and possible periodic sampling bias. It
provides no call stacks or precise per-instruction cycle costs.

The original run recorded **30,755 samples**, zero failed writes and zero unknown
benchmark-symbol samples. `SpectralBook::fill` accounted for 31.5%,
`ifft_stages_avx512` 24.1%, `apply_soa` 10.4%, and `ifft_soa` 6.1%. Within the FFT,
the scalar butterfly block had **4,125 samples: 13.41% of all samples and 55.59%
of FFT samples**. At power-of-two sizes this block handles `half=1/2`; larger
stages use the explicit 4/8/16-lane paths. Loop/control instructions accounted
for another 5.30% of all samples. This supports measuring an early-stage SIMD
specialization while retaining scalar multiplication/addition/subtraction order.

For the original binary SHA-256
`0a7bfcaed05bb95e6f5f06356f3160900afcc9432a98dddea5d337f35dcdd0ba`,
that scalar block is ELF `0xd9660..0xd96d7`, at function offsets
`+0x5a0..+0x617`. Hottest instructions include `+0x5a6` scalar load,
`+0x5e1` scalar store and `+0x5d0` scalar multiply. Inspect the matching binary:

```sh
objdump -d -C --no-show-raw-insn --start-address=0xd90c0 --stop-address=0xd96d7 "$sampling_bin"
```

Original commands, source/binary hashes, captured maps, region counts, numeric
ELF-offset histogram and disassembly are retained in the
[committed sampling archive](../../../release-audit/runs/task145-20261105/sigprof/).
New local sampling output belongs under `target/task145/sigprof`.
The ignored `.so` and binary RIP stream are local evidence, never package or Git
artifacts. If this fallback is unsupported, retain the failure and use an owned
child debugger sampling session; do not relax host security policy.

## Fresh cold ADTS producer stack proof

`prepare_adts_cold_stack.py` prepares the additional **C5 / SDK 0.10.1** proof,
using exact Git revision `3e540231c9d71e3534be1ef856a91cf419efaead`. Apply the
matching [consumer handoff](../README.md#consumer-adapter-handoff) first. The
generator snapshots that supplied checkout's `adts_pull.rs`, `input.rs` and
`buffered.rs`, and extracts its parent append/options/error-mapping helpers.
Source hashes identify what was copied; a later checkout requires new evidence.
The Rust template, manifest template and lock preserve the tested standalone
harness and pinned SDK. The copied consumer code uses [gigastt's MIT license](LICENSE.gigastt).

Each of four existing goldens runs through both File and Bytes in a **fresh
process**, with the first Decoder construction and full producer decode inside
a **128 KiB thread**. Before that thread, the harness only reads bytes, allocates
input storage and performs byte-only optional ID3 stripping. It never calls an
encoder, probe or decoder to warm codec tables. Afterwards, a warm producer run
must match every native float bit and each batch's frames/rate/channels. Profile
metadata is decoded and checked only after both runs: LC at 48 kHz mono, HE1
with 24 kHz core/48 kHz stereo output, and PS/implicit PS with mono wire headers
and stereo output. These assertions prevent an accidentally LC-encoded fixture
from being labeled HE.

Preparation records original and stripped fixture hashes without invoking the
codec. The four selected goldens are all untagged, so their original/bare hashes
are identical and no stripped file is written. The test-only optional tagged
input branch writes a `.bare-owned` sibling; use an owned fixture copy when
trying other tagged inputs. The minimal parent `AudioTooLong` error type is a
normal-path stub. This native caller-stack proof does not establish public
consumer budget/cancellation/error contracts, resampler behavior or HTTP/model
qualification; those require the actual consumer workspace and separate checks.

```sh
python3 lab/consume/gigastt/profile/prepare_adts_cold_stack.py \
  --consumer target/task145/gigastt --output target/task145/adts-cold-stack-repro
cold_dir="$PWD/target/task145/adts-cold-stack-repro"
cold_target="$PWD/target/task145/adts-cold-stack-repro-target"
# Historical compiler was stable=1.98.1; use the same installed version.
taskset -c 8-11 env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$cold_target" \
  cargo +1.98.1 build --release --locked --offline --manifest-path "$cold_dir/Cargo.toml" \
  > "$cold_dir/build.log" 2>&1
cold_bin="$cold_target/release/syom-adts-cold-stack-proof"
proof_status=0
for profile in lc he1 ps implicit-ps; do
  case "$profile" in
    lc) fixture=sine48.adts ;;
    he1) fixture=he48e.adts ;;
    ps) fixture=ps48.adts ;;
    implicit-ps) fixture=he48.adts ;;
  esac
  for reader in file bytes; do
    taskset -c 8 "$cold_bin" "$profile" "$PWD/src/goldens/$fixture" "$reader" \
      > "$cold_dir/$profile-$reader.log" 2>&1 || proof_status=1
  done
done
sha256sum "$cold_bin" "$cold_dir/src/main.rs" "$cold_dir/Cargo.lock"
rustc +1.98.1 -Vv > "$cold_dir/compiler.txt"
test "$proof_status" -eq 0
```

The original Linux x86_64 Rust 1.98.1 run passed **8/8** fresh processes. Per-reader
native counts were LC 13,312; HE1 32,768; PS 53,248; implicit PS 18,432, all with
zero cold/warm bit or geometry differences. HE1 and PS also cross logical batch
boundaries. Raw per-cell logs, exact commands, original/bare fixture hashes,
compiler/binary/source hashes and freeze comparison are in the
[committed cold-stack archive](../../../release-audit/runs/task145-20261105/adts-cold-stack/).
Local reruns use a fresh directory under `target/`.
The generator output was verified identical to the tested harness except its
snapshot paths; generated Cargo files and all source/fixture hash maps matched.
Preparation and this promotion ran no additional builds or decodes.

## Native selected-channel differential

The post-qualification proof adds `One(0)`, `One(1)` and missing-channel
coverage for the frozen synchronous M4A and bare-ADTS producers. Eight generated
LC mono/stereo, HE and PS fixtures run from File and Bytes, for 48 cells. The
oracle indexes real SDK planar frames directly, without `append_plane`, and
compares original 24,576-frame block geometry and full sample-bit hashes.
Stereo inputs are required to produce different channel data. Missing channels
retain the historical empty-selected-plane behavior and terminal EOF remains
stable. This is native producer coverage, not public ASR/resampler/budget proof.

Preparation defaults to the committed consumer snapshot, verifies its hashes
and needs no ignored consumer checkout or build. Output must be fresh:

```sh
python3 lab/consume/gigastt/profile/prepare_channel_selection.py \
  --output target/task145-review/selected-channel-repro
```

Add `--execute` for the isolated native build and 48-cell run. Execution uses
the qualified Rust 1.98.1 compiler, locked offline dependencies and SDK Git
revision `3e540231c9d71e3534be1ef856a91cf419efaead`; its dependencies must
already be cached. The runner uses CPUs 14–17/jobs 2 and records commands,
source/fixture hashes and result logs. Generated media and native dumps stay
in ignored output, outside Git. This proof runs after timing campaigns.
