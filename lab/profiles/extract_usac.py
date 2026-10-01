#!/usr/bin/env python3
"""Extract raw USAC AUs + xaacdec metadata text from an exhale .m4a (TASK-97).

  extract_usac.py IN.m4a OUT.raw OUT.meta.txt
"""
import struct
import sys


def boxes(data, start, end):
    off = start
    while off + 8 <= end:
        size, typ = struct.unpack(">I4s", data[off : off + 8])
        hdr = 8
        if size == 1:
            size = struct.unpack(">Q", data[off + 8 : off + 16])[0]
            hdr = 16
        if size == 0:
            size = end - off
        yield typ.decode(), off + hdr, off + size, off
        off += size


def find(data, path, start=0, end=None):
    end = end or len(data)
    for depth, want in enumerate(path):
        for typ, cstart, cend, _ in boxes(data, start, end):
            if typ == want:
                start, end = cstart, cend
                break
        else:
            raise SystemExit(f"box {want} not found at depth {depth}")
    return start, end


def main():
    src, out_raw, out_meta = sys.argv[1], sys.argv[2], sys.argv[3]
    data = open(src, "rb").read()
    mdat_s, mdat_e = find(data, ["mdat"])
    stsz_s, stsz_e = find(data, ["moov", "trak", "mdia", "minf", "stbl", "stsz"])
    stts_s, stts_e = find(data, ["moov", "trak", "mdia", "minf", "stbl", "stts"])
    mdhd_s, mdhd_e = find(data, ["moov", "trak", "mdia", "mdhd"])
    # stsz: version/flags(4) sample_size(4) count(4) entries
    _, _, count = struct.unpack(">III", data[stsz_s : stsz_s + 12])
    sizes = struct.unpack(f">{count}I", data[stsz_s + 12 : stsz_s + 12 + 4 * count])
    # mdhd timescale (v0: version+flags(4) ctime(4) mtime(4) timescale(4))
    timescale = struct.unpack(">I", data[mdhd_s + 12 : mdhd_s + 16])[0]
    # stts total duration: version/flags(4) entries(4) [count,delta]*
    nstts = struct.unpack(">I", data[stts_s + 4 : stts_s + 8])[0]
    total = 0
    for i in range(nstts):
        c, d = struct.unpack(">II", data[stts_s + 8 + 8 * i : stts_s + 16 + 8 * i])
        total += c * d
    payload = data[mdat_s:mdat_e]
    open(out_raw, "wb").write(payload)
    with open(out_meta, "w") as f:
        f.write("-dec_info_init:1\n")
        f.write("-g_track_count:1\n")
        f.write(f"-movie_time_scale:{timescale}\n")
        f.write(f"-media_time_scale:{timescale}\n")
        f.write(f"-ia_mp4_stsz_entries:{count}\n")
        f.write(f"-playTimeInSamples:{total}\n")
        f.write("-startOffsetInSamples:0\n")
        f.write("-useEditlist:0\n")
        for s in sizes:
            f.write(f"-ia_mp4_stsz_size:{s}\n")
    print(f"raw bytes={len(payload)} frames={count} timescale={timescale} play={total}")


if __name__ == "__main__":
    main()
