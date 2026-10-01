#!/usr/bin/env python3
"""TASK-20 lab: pad every ADTS frame with K extra zero bytes after the coded
elements (legal per spec: the ADTS frame_length defines the AU; decoders must
skip to the next frame boundary). Usage: pad_adts.py in.adts out.adts K"""
import sys

src = open(sys.argv[1], "rb").read()
k = int(sys.argv[3])
out = bytearray()
off = 0
frames = 0
while off + 7 <= len(src):
    if src[off] != 0xFF or (src[off + 1] & 0xF0) != 0xF0:
        print(f"sync lost at {off}", file=sys.stderr)
        sys.exit(1)
    flen = ((src[off + 3] & 3) << 11) | (src[off + 4] << 3) | ((src[off + 5] & 0xE0) >> 5)
    frame = bytearray(src[off : off + flen])
    new_len = flen + k
    frame[3] = (frame[3] & 0xFC) | ((new_len >> 11) & 3)
    frame[4] = (new_len >> 3) & 0xFF
    frame[5] = (frame[5] & 0x1F) | ((new_len & 7) << 5)
    out += frame + bytes(k)
    off += flen
    frames += 1
open(sys.argv[2], "wb").write(bytes(out))
print(f"{frames} frames padded with {k} bytes each")
