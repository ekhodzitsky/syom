# AAC profile-expansion feasibility lab (TASK-97)

Pinned primary engines for LD / ELD / USAC capability, delay and rate cells.
Everything here is an offline oracle; ordinary `cargo test` never touches it.

## Engines

| engine | pin | sha-256 (archive) | role |
|---|---|---|---|
| fdk-aac v2.0.3 | https://github.com/mstorsjo/fdk-aac/archive/refs/tags/v2.0.3.tar.gz | `e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78` | LD (AOT 23) / ELD (AOT 39, ±SBR) encode+decode, LC anchor |
| libxaac v0.1.13 | https://github.com/ittiam-systems/libxaac/archive/refs/tags/v0.1.13.tar.gz | `606ac21d806b120c588e669464bb7f2157d5a638f60f54d3b9ab9b9d2eecd309` | xHE-AAC (USAC) decode |
| exhale v1.2.2 | https://gitlab.com/ecodis/exhale/-/archive/v1.2.2/exhale-v1.2.2.tar.gz (tag commit `8cddccddec9d`) | `a46a085e3f8049ece2cba5ab3dd80e85fd198e81277b5292ca198a25a80088f1` | xHE-AAC (USAC) encode |
| ffmpeg 7.0.2-static | host `ffmpeg`/`ffprobe` | n/a | neutral decode where supported |
| syom | this crate, in-tree | n/a | LC/HE comparison + current decoder-support cells |

Licenses: Fraunhofer FDK (not MIT), libxaac Apache-2.0 per `LICENSE`
(itself noting third-party patent terms), exhale BSD-style — all stay
lab-only, never product dependencies.

## Build (this host)

No cmake/make/g++ on PATH; zig 0.14.1 (`~/.local/zig/zig`) provides
`zig cc/c++ -target x86_64-linux-gnu`, gcc 15.2.0 compiles the C drivers,
GNU ar archives. Sources unpack under `target/tmp/profiles/` (git-ignored).

```sh
STAGE=target/tmp/profiles   # git-ignored staging for sources, builds, cells
cd $STAGE
curl -fLO https://github.com/mstorsjo/fdk-aac/archive/refs/tags/v2.0.3.tar.gz
echo 'e25671cd96b10bad896aa42ab91a695a9e573395262baed4e4a2ff178d6a3a78  fdk-aac-2.0.3.tar.gz' | sha256sum -c
tar xzf fdk-aac-2.0.3.tar.gz
curl -fLO https://github.com/ittiam-systems/libxaac/archive/refs/tags/v0.1.13.tar.gz
echo '606ac21d806b120c588e669464bb7f2157d5a638f60f54d3b9ab9b9d2eecd309  libxaac-0.1.13.tar.gz' | sha256sum -c
tar xzf libxaac-0.1.13.tar.gz
curl -fLO https://gitlab.com/ecodis/exhale/-/archive/v1.2.2/exhale-v1.2.2.tar.gz
echo 'a46a085e3f8049ece2cba5ab3dd80e85fd198e81277b5292ca198a25a80088f1  exhale-v1.2.2.tar.gz' | sha256sum -c
tar xzf exhale-v1.2.2.tar.gz
L=lab/profiles/build   # from the repo root
$L/build_fdk.sh fdk-aac-2.0.3        # 170 CMake-listed TUs -> fdk-prefix/{lib,include}
$L/build_exhale.sh exhale-v1.2.2     # -> exhale-v1.2.2/bin/exhale
$L/build_xaac_dec.sh libxaac-0.1.13  # -> libxaacdec.a + xaacdec (testbench, see REPORT gap)
$L/build_xaac_enc.sh libxaac-0.1.13  # -> xaacenc
# minimal direct-API decoder harness (the piece that actually decodes):
gcc -O2 -std=gnu99 -w -Ilibxaac-0.1.13/decoder -Ilibxaac-0.1.13/common \
    -c $L/xaac_mini_dec.c -o libxaac-0.1.13/obj_dec/mini.o
gcc -O2 -o libxaac-0.1.13/xaac_mini_dec libxaac-0.1.13/obj_dec/mini.o \
    libxaac-0.1.13/libxaacdec.a -lm
FDK_PREFIX=$STAGE/fdk-prefix lab/profiles/build_driver.sh
```

Note: `build_xaac_dec.sh` also carries `-DLATM_LOAS`-free flags matching
upstream cmake defines (`-DX86_64 -D_X86_64_`); the stock testbench binary
is built but its init loop never finishes on this host — use
`xaac_mini_dec` (direct API) instead.

## Driver

`lab/profiles/profiles_driver` (links libfdk-aac.a, never in cargo):

```
profiles_driver id
profiles_driver decode <adts|loas> IN OUT.s16le        # FDK in-process decode
profiles_driver encode IN.s16le RATE CH BPS AOT <adts|loas|raw> OUT [sbr_mode]
```

Input PCM is interleaved s16le; JSON lines on stdout record rate, channels,
stream bytes, encoder `nDelay`, `frameLength`, granule, decoder `aot` /
`outputDelay` and an FNV-1a checksum of the decoded f32 planes.
