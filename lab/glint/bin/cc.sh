#!/bin/sh
# Host rustc is GNU; zig defaults to musl. Force the GNU target.
exec zig-cc -target x86_64-linux-gnu "$@"
