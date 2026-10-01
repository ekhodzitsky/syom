#!/bin/sh
# Isolated FDK profiles driver link (TASK-97). Not invoked by cargo test.
# Requires FDK_PREFIX with include/ + lib/libfdk-aac.a (C++17, zig-c++ GNU).
set -e
if [ -z "$FDK_PREFIX" ]; then
  echo "set FDK_PREFIX" >&2
  exit 2
fi
ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
CC=${CC:-gcc}
CXX=${CXX:-"$ROOT/../glint/bin/cxx.sh"}
"$CC" -O2 -std=c11 -Wall -I"$FDK_PREFIX/include" -c "$ROOT/profiles_driver.c" -o "$ROOT/profiles_driver.o"
"$CXX" -O2 -o "$ROOT/profiles_driver" "$ROOT/profiles_driver.o" \
  -L"$FDK_PREFIX/lib" -lfdk-aac -lm
echo "$ROOT/profiles_driver"
