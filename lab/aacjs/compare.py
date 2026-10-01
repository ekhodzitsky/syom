#!/usr/bin/env python3
"""TASK-20 lab: compare two interleaved float32 raw PCM files.

Usage: compare.py a.f32 b.f32 [max_offset_samples]
Reports per-file length, best small alignment offset, max abs diff and RMS
diff in s16 LSB units (x * 32768). Exit 0 always; this is a diagnostic.
"""
import struct
import sys

import array


def load(path):
    a = array.array("f")
    with open(path, "rb") as fh:
        a.frombytes(fh.read())
    return a


def main():
    a = load(sys.argv[1])
    b = load(sys.argv[2])
    search = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    n = min(len(a), len(b))

    best_off, best_rms = 0, None
    for off in range(-search, search + 1):
        lo = max(0, -off)
        hi = min(n, len(a), len(b) - off) if off >= 0 else min(n, len(a) + off, len(b))
        cnt = hi - lo
        if cnt <= 0:
            continue
        acc = 0.0
        for i in range(lo, hi):
            d = a[i] - b[i + off]
            acc += d * d
        rms = (acc / cnt) ** 0.5
        if best_rms is None or rms < best_rms:
            best_rms, best_off = rms, off

    off = best_off
    lo = max(0, -off)
    hi = min(n, len(a), len(b) - off) if off >= 0 else min(n, len(a) + off, len(b))
    max_abs = 0.0
    acc = 0.0
    for i in range(lo, hi):
        d = abs(a[i] - b[i + off])
        max_abs = max(max_abs, d)
        acc += d * d
    rms = (acc / (hi - lo)) ** 0.5
    print(f"{sys.argv[1]}: {len(a)} floats")
    print(f"{sys.argv[2]}: {len(b)} floats")
    print(f"best_offset={off} compared={hi - lo}")
    print(f"max_abs={max_abs:.6g} ({max_abs * 32768:.3f} s16 LSB)")
    print(f"rms={rms:.6g} ({rms * 32768:.3f} s16 LSB)")


if __name__ == "__main__":
    main()
