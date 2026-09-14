#!/bin/sh
# Isolated FDK driver link. Not invoked by cargo test.
# Requires FDK_PREFIX with include/ + lib/libfdk-aac.a (C++17, zig-c++ GNU).
set -e
if [ -z "$FDK_PREFIX" ]; then
  echo "set FDK_PREFIX" >&2
  exit 2
fi
ROOT=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
CC=${CC:-gcc}
CXX=${CXX:-"$ROOT/../glint/bin/cxx.sh"}
"$CC" -O2 -std=c11 -Wall -I"$FDK_PREFIX/include" -I"$ROOT" -c "$ROOT/fdk_adapt.c" -o "$ROOT/fdk_adapt.o"
"$CC" -O2 -std=c11 -Wall -I"$FDK_PREFIX/include" -I"$ROOT" -c "$ROOT/fdk_driver.c" -o "$ROOT/fdk_driver.o"
"$CXX" -O2 -o "$ROOT/fdk_driver" "$ROOT/fdk_adapt.o" "$ROOT/fdk_driver.o" \
  -L"$FDK_PREFIX/lib" -lfdk-aac -lm
echo "$ROOT/fdk_driver"
