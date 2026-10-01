#!/usr/bin/env python3
"""Score decoded s16le against the source s16le (TASK-97 cells).

Aligns by cross-correlation lag on channel 0 over +/-8192 samples, then
reports per-channel SNR (dB) and the detected lag (= measured roundtrip
algorithmic delay + container priming, at the decoded rate).

  score_cells.py REF.s16le DEC.s16le CH [REF_RATE DEC_RATE]

If DEC_RATE differs from REF_RATE the reference is resampled offline by the
caller (ffmpeg) before scoring; this script assumes equal rates.
"""
import struct
import sys


def read(path, ch):
    raw = open(path, "rb").read()
    n = len(raw) // 2
    planes = [[] for _ in range(ch)]
    for i in range(n):
        v = struct.unpack_from("<h", raw, 2 * i)[0] / 32768.0
        planes[i % ch].append(v)
    return planes


def best_lag(ref, dec, max_lag=8192):
    # coarse energy-normalized correlation on a stride for speed
    stride = 4
    r = ref[::stride]
    d = dec[::stride]
    n = min(len(r), len(d))
    best, bl = -1e30, 0
    for lag in range(-max_lag // stride, max_lag // stride + 1):
        s = 0.0
        if lag >= 0:
            for i in range(min(n - lag, 20000)):
                s += r[i] * d[i + lag]
        else:
            for i in range(min(n + lag, 20000)):
                s += r[i - lag] * d[i]
        if s > best:
            best, bl = s, lag
    return bl * stride


def refine_lag(ref, dec, lag, span=8):
    best, bl = -1e30, lag
    for l in range(lag - span, lag + span + 1):
        s = 0.0
        n = min(len(ref), len(dec))
        if l >= 0:
            for i in range(min(n - l, 48000)):
                s += ref[i] * dec[i + l]
        else:
            for i in range(min(n + l, 48000)):
                s += ref[i - l] * dec[i]
        if s > best:
            best, bl = s, l
    return bl


def main():
    ref_path, dec_path, ch = sys.argv[1], sys.argv[2], int(sys.argv[3])
    ref = read(ref_path, ch)
    dec = read(dec_path, ch)
    lag = refine_lag(ref[0], dec[0], best_lag(ref[0], dec[0]))
    import math

    out = {"lag": lag}
    for c in range(ch):
        r, d = ref[c], dec[c]
        n = min(len(r), len(d) - max(lag, 0))
        n = min(n, len(r) - max(-lag, 0), 96000)
        start_r = max(-lag, 0)
        start_d = max(lag, 0)
        e = s = 0.0
        for i in range(n):
            rv = r[start_r + i]
            dv = d[start_d + i]
            s += rv * rv
            e += (rv - dv) ** 2
        out[f"snr_ch{c}"] = round(10 * math.log10(s / e), 2) if e > 0 else 99.0
    print(out)


if __name__ == "__main__":
    main()
