#!/usr/bin/env python3
"""TASK-126 — derive a tfdt-less fMP4 golden from fmp4_lc_dbmoof.mp4.

ffmpeg's mov muxer always writes tfdt, so the tfdt-less accumulation path
(lab/fmp4/REPORT.md §2: "sample dts: tfdt when present, else accumulated")
needs box surgery: drop every tfdt and shrink each trun's data_offset by
the removed bytes (default-base-is-moof addressing keeps the fix local to
the fragment). The result must decode bit-identically to the source and
resolve the same timeline by accumulation (ffprobe oracle in
fmp4_timeline.txt).

Usage: strip_tfdt.py SRC DST
"""

import struct
import sys


def u32(b, o):
    return struct.unpack_from(">I", b, o)[0]


def boxes(data, start, end):
    pos = start
    while pos + 8 <= end:
        size, typ = u32(data, pos), data[pos + 4:pos + 8]
        assert size >= 8 and pos + size <= end, f"bad box at {pos}"
        yield pos, size, typ
        pos += size
    assert pos == end, f"trailing {end - pos} bytes"


def pack(typ, payload):
    return struct.pack(">I4s", len(payload) + 8, typ) + payload


def strip_trun(data, pos, size, removed):
    """Re-emit one trun with data_offset shrunk by `removed` bytes."""
    body = bytearray(data[pos + 8:pos + size])
    flags = u32(body, 0) & 0xFFFFFF
    assert flags & 0x1, "trun without data_offset"
    off = u32(body, 8) - removed
    body[8:12] = struct.pack(">I", off)
    return pack(b"trun", bytes(body))


def strip_moof(data, pos, size):
    out = []
    removed = 0
    for cpos, csize, ctyp in boxes(data, pos + 8, pos + size):
        if ctyp != b"traf":
            out.append(data[cpos:cpos + csize])
            continue
        traf = []
        for tpos, tsize, ttyp in boxes(data, cpos + 8, cpos + csize):
            if ttyp == b"tfdt":
                removed += tsize
                continue
            if ttyp == b"trun":
                traf.append(strip_trun(data, tpos, tsize, removed))
            else:
                traf.append(data[tpos:tpos + tsize])
        out.append(pack(b"traf", b"".join(traf)))
    assert removed, "moof without tfdt"
    return pack(b"moof", b"".join(out))


def main():
    src, dst = sys.argv[1], sys.argv[2]
    data = open(src, "rb").read()
    out = []
    n = 0
    for pos, size, typ in boxes(data, 0, len(data)):
        if typ == b"moof":
            out.append(strip_moof(data, pos, size))
            n += 1
        else:
            out.append(data[pos:pos + size])
    open(dst, "wb").write(b"".join(out))
    print(f"{dst}: {n} moofs stripped of tfdt")


if __name__ == "__main__":
    main()
