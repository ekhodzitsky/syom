#!/usr/bin/env python3
"""Wrap raw USAC AUs (+ ASC from the m4a esds) into LOAS for xaacdec (TASK-97).

LOAS frame = 11-bit sync 0x2B7 + 13-bit AudioMuxElement length +
AudioMuxElement{StreamMuxConfig(audioMuxVersion=0, ASC bits),
frameLengthType 0, PayloadLengthInfo, AU, byte align}.

  loas_wrap_usac.py IN.m4a IN.meta.txt IN.raw OUT.loas
"""
import struct
import sys


def boxes(d, s, e):
    off = s
    while off + 8 <= e:
        size, typ = struct.unpack(">I4s", d[off : off + 8])
        hdr = 8
        if size == 1:
            size, hdr = struct.unpack(">Q", d[off + 8 : off + 16])[0], 16
        if size == 0:
            size = e - off
        yield typ.decode("latin1"), off + hdr, off + size
        off += size


def walk(d, path, s=0, e=None):
    e = e or len(d)
    for want in path:
        for typ, cs, ce in boxes(d, s, e):
            if typ == want:
                s, e = cs, ce
                break
        else:
            raise SystemExit(f"box {want} missing")
    return s, e


def asc_of(d):
    s, e = walk(d, ["moov", "trak", "mdia", "minf", "stbl", "stsd"])
    esize = struct.unpack(">I", d[s + 8 : s + 12])[0]

    def descr(p, end):
        while p < end:
            tag = d[p]
            p += 1
            ln = 0
            while True:
                b = d[p]
                p += 1
                ln = (ln << 7) | (b & 0x7F)
                if not (b & 0x80):
                    break
            if tag == 0x05:
                return d[p : p + ln]
            if tag == 0x03:
                r = descr(p + 3, p + ln)  # ES_ID + flags
            elif tag == 0x04:
                r = descr(p + 13, p + ln)  # DecoderConfigDescriptor header
            else:
                r = None
            if r is not None:
                return r
            p += ln
        return None

    for typ, cs, ce in boxes(d, s + 8 + 36, s + 8 + esize):
        if typ == "esds":
            r = descr(cs + 4, ce)
            if r is not None:
                return r
    raise SystemExit("no esds/ASC")


class BitWriter:
    def __init__(self):
        self.bits = []

    def w(self, val, n):
        for i in range(n - 1, -1, -1):
            self.bits.append((val >> i) & 1)

    def wbytes(self, bs):
        for b in bs:
            self.w(b, 8)

    def align(self):
        while len(self.bits) % 8:
            self.bits.append(0)

    def out(self):
        o = bytearray()
        for i in range(0, len(self.bits), 8):
            v = 0
            for b in self.bits[i : i + 8]:
                v = (v << 1) | b
            o.append(v)
        return bytes(o)


def main():
    m4a, meta, raw, out = sys.argv[1:5]
    d = open(m4a, "rb").read()
    asc = asc_of(d)
    sizes = []
    for line in open(meta):
        if line.startswith("-ia_mp4_stsz_size:"):
            sizes.append(int(line.split(":")[1]))
    payload = open(raw, "rb").read()
    loas = bytearray()
    off = 0
    for sz in sizes:
        au = payload[off : off + sz]
        off += sz
        bw = BitWriter()
        bw.w(0, 1)  # audioMuxVersion
        bw.w(1, 1)  # allStreamsSameTimeFraming
        bw.w(0, 6)  # numSubFrames
        bw.w(0, 4)  # numProgram
        bw.w(0, 3)  # numLayer
        bw.wbytes(asc)  # AudioSpecificConfig
        bw.w(0, 3)  # frameLengthType 0
        bw.w(0, 8)  # latmBufferFullness
        bw.w(0, 1)  # otherDataPresent
        bw.w(0, 1)  # crcCheckPresent
        n = len(au)  # PayloadLengthInfo, frameLengthType 0
        while n >= 255:
            bw.w(255, 8)
            n -= 255
        bw.w(n, 8)
        bw.wbytes(au)
        bw.align()
        ame = bw.out()
        if len(ame) > 0x1FFF:
            raise SystemExit("LOAS frame too long")
        loas += struct.pack(">H", 0x2B7 << 5 | len(ame) >> 8)
        loas += bytes([len(ame) & 0xFF])
        loas += ame
    open(out, "wb").write(loas)
    print(f"asc={asc.hex()} frames={len(sizes)} loas_bytes={len(loas)}")


if __name__ == "__main__":
    main()
