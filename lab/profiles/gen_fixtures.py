#!/usr/bin/env python3
"""Deterministic synthetic PCM fixtures for TASK-97 profile cells.

Natural licensed excerpts are not vendored (corpus/manifest.json marks them
gap-until-obtained), so capability/delay/rate cells use reproducible
synthetic speech-like and music-like signals. Output: interleaved s16le.

  gen_fixtures.py OUTDIR [RATE DUR]
"""
import math
import random
import struct
import sys

RATE = 48000
DUR = 2.0


def s16le(path, planes):
    if not isinstance(planes[0], list):
        planes = [planes]
    n = len(planes[0])
    ch = len(planes)
    with open(path, "wb") as f:
        out = bytearray()
        for i in range(n):
            for c in range(ch):
                v = planes[c][i]
                v = max(-1.0, min(1.0, v))
                out += struct.pack("<h", int(round(v * 32767.0)))
        f.write(out)


def speech(rng, n, f0=110.0):
    """Voiced harmonic stack + unvoiced noise bursts, 4 Hz syllable AM."""
    out = []
    for i in range(n):
        t = i / RATE
        seg = int(t / 0.4)
        ph = t - seg * 0.4
        vib = 1.0 + 0.006 * math.sin(2 * math.pi * 5.5 * t)
        if seg % 2 == 0 and ph < 0.34:  # voiced
            s = 0.0
            for k in range(1, 25):
                s += math.exp(-k / 7.0) * math.sin(2 * math.pi * k * f0 * vib * t)
            s *= 0.30 * (0.55 + 0.45 * math.sin(2 * math.pi * 4.0 * t))
        else:  # unvoiced / pause
            s = 0.10 * rng.uniform(-1, 1) * math.sin(2 * math.pi * 3.0 * t) ** 2
        out.append(s)
    return out


def music(rng, n, det=1.0):
    """Tonal triad + low noise bed + castanet impulses every 0.3 s."""
    out = []
    for i in range(n):
        t = i / RATE
        s = 0.22 * math.sin(2 * math.pi * 440.0 * det * t)
        s += 0.16 * math.sin(2 * math.pi * 554.37 * det * t)
        s += 0.12 * math.sin(2 * math.pi * 659.26 * det * t)
        s *= 0.8 + 0.2 * math.sin(2 * math.pi * 2.0 * t)
        s += 0.03 * rng.uniform(-1, 1)
        ph = t % 0.3
        if ph < 0.02:  # castanet transient
            s += 0.5 * math.exp(-ph * 400.0) * math.sin(2 * math.pi * 3500.0 * ph)
        out.append(s)
    return out


def main():
    outdir = sys.argv[1]
    global RATE, DUR
    if len(sys.argv) > 2:
        RATE = int(sys.argv[2])
    if len(sys.argv) > 3:
        DUR = float(sys.argv[3])
    n = int(RATE * DUR)
    r1 = random.Random(9701)
    r2 = random.Random(9702)
    r3 = random.Random(9703)
    r4 = random.Random(9704)
    sm = speech(r1, n)
    mm = music(r2, n)
    ss = [sm, speech(r3, n, f0=123.0)]
    ms = [mm, music(r4, n, det=1.003)]
    s16le(f"{outdir}/speech_m.s16le", sm)
    s16le(f"{outdir}/music_m.s16le", mm)
    s16le(f"{outdir}/speech_s.s16le", ss)
    s16le(f"{outdir}/music_s.s16le", ms)
    print(f"wrote speech_m/music_m/speech_s/music_s ({n} samples/ch @ {RATE})")


if __name__ == "__main__":
    main()
