#!/bin/sh
# Isolated glint-audio 0.11.0 build. Not invoked by cargo test.
# zig-c++ GNU target + zig-c++ as rustc linker (objects use libc++; gcc+libstdc++ fails).
set -e
ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
export CC="$ROOT/bin/cc.sh"
export CXX="$ROOT/bin/cxx.sh"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$ROOT/bin/cxx.sh"
exec cargo build --release --manifest-path "$ROOT/Cargo.toml" "$@"
