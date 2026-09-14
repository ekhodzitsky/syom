#!/usr/bin/env python3
"""Deterministic PCM generators for the evaluation corpus (no network)."""

from __future__ import annotations

import hashlib
import math
import struct
from typing import Any


def _pack(planes: list[list[float]]) -> bytes:
    # Planar f32le, plane-major. This is the hashed representation.
    buf = bytearray()
    for plane in planes:
        for x in plane:
            buf.extend(struct.pack("<f", x))
    return bytes(buf)


def _tone(n: int, rate: int, freq: float, amp: float = 0.25) -> list[float]:
    return [
        amp * math.sin(2.0 * math.pi * freq * i / rate) for i in range(n)
    ]


def _impulse(n: int, at: int, amp: float = 0.9) -> list[float]:
    p = [0.0] * n
    if 0 <= at < n:
        p[at] = amp
    return p


def generate(spec: dict[str, Any]) -> bytes:
    fn = spec["fn"]
    n = int(spec.get("n", 1024))
    ch = int(spec.get("ch", 1))
    rate = int(spec.get("rate", 48_000))
    if fn == "silence":
        planes = [[0.0] * n for _ in range(ch)]
    elif fn == "impulse":
        at = int(spec["at"])
        planes = [_impulse(n, at) for _ in range(ch)]
        which = spec.get("which")
        if which is not None:
            planes = [[0.0] * n for _ in range(ch)]
            planes[int(which)] = _impulse(n, at)
    elif fn == "dc":
        v = float(spec.get("value", 0.25))
        planes = [[v] * n for _ in range(ch)]
    elif fn == "fullscale":
        planes = [[1.0] * n for _ in range(ch)]
    elif fn == "tiny":
        planes = [[1.0e-20] * n for _ in range(ch)]
    elif fn == "clipped":
        planes = [[1.5 if i % 2 == 0 else -1.5 for i in range(n)] for _ in range(ch)]
    elif fn == "antiphase":
        l = _tone(n, rate, 440.0)
        r = [-x for x in l]
        planes = [l, r]
    elif fn == "tone":
        freq = float(spec["freq"])
        planes = [_tone(n, rate, freq) for _ in range(ch)]
    elif fn == "sweep":
        planes = [
            [
                0.2
                * math.sin(
                    math.pi * (8_000.0 / rate) * (i * i) / max(n, 1)
                )
                for i in range(n)
            ]
            for _ in range(ch)
        ]
    elif fn == "noise":
        # LCG, independent of libm.
        seed = int(spec.get("seed", 1)) & 0xFFFFFFFF
        planes = []
        for _c in range(ch):
            row = []
            for _i in range(n):
                seed = (1664525 * seed + 1013904223) & 0xFFFFFFFF
                row.append((seed / 0xFFFFFFFF) * 0.04 - 0.02)
            planes.append(row)
    elif fn == "empty":
        planes = [[] for _ in range(max(ch, 1))]
    elif fn == "one_sample":
        planes = [[0.1] for _ in range(ch)]
    else:
        raise ValueError(f"unknown generator {fn}")
    return _pack(planes)


def sha256_of(spec: dict[str, Any]) -> str:
    return hashlib.sha256(generate(spec)).hexdigest()
