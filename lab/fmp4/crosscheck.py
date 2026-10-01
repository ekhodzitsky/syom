#!/usr/bin/env python3
"""TASK-64 — cross-check walk_mp4.py sample resolution against ffprobe.

For each fMP4 fixture: ffprobe (libavformat mov demuxer, independent of
walk_mp4.py) reports every audio packet's pos/size/dts/pts/duration.
The walker resolves the same fields from moof/tfhd/tfdt/trun alone.
Any mismatch is reported; exit 1 on failure.
"""

import json
import subprocess
import sys

import walk_mp4


def resolved(path):
    data = open(path, "rb").read()
    tree = []
    walk_mp4.walk(data, 0, len(data), 0, tree, {"trex": {}})
    return walk_mp4.resolve_samples(data, tree)


def probed(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "packet=pos,size,dts,pts,duration",
         "-of", "json", path],
        check=True, capture_output=True, text=True)
    return json.loads(out.stdout)["packets"]


def check(path):
    idx = resolved(path)
    pkts = probed(path)
    mine = idx["samples"]
    ts = idx["timescale"]
    ok = True
    if len(mine) != len(pkts):
        print(f"{path}: sample count walker={len(mine)} ffprobe={len(pkts)}")
        ok = False
    for i, (s, p) in enumerate(zip(mine, pkts)):
        want = {
            "offset": int(p["pos"]),
            "size": int(p["size"]),
            "dts": int(p["dts"]) * 48000 // ts if ts != 48000 else int(p["dts"]),
        }
        # ffprobe reports dts/pts in the stream timebase = media timescale.
        for k, a, b in (("offset", s["offset"], int(p["pos"])),
                        ("size", s["size"], int(p["size"])),
                        ("dts", s["dts"], int(p["dts"])),
                        ("cts", s["cts"], int(p["pts"]))):
            if a != b:
                print(f"{path}: sample {i} {k}: walker={a} ffprobe={b}")
                ok = False
        if "duration" in p and i + 1 < len(mine):
            dur = mine[i + 1]["dts"] - s["dts"]
            if dur != int(p["duration"]):
                print(f"{path}: sample {i} duration: walker={dur} ffprobe={p['duration']}")
                ok = False
    print(f"{path}: {'OK' if ok else 'MISMATCH'} "
          f"({len(mine)} samples, timescale {ts})")
    return ok


def main():
    ok = True
    for path in sys.argv[1:]:
        ok = check(path) and ok
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
