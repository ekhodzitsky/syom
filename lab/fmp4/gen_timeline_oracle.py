#!/usr/bin/env python3
"""TASK-126 — mint the committed fMP4 timeline oracle (src/goldens/
fmp4_timeline.txt) from ffprobe 7.0.2 (lab/fmp4/PIN.md; offline, product
tests never spawn ffprobe — they replay this committed text).

Per fixture: every audio packet's pos/size/dts/pts from
`ffprobe -show_packets` (libavformat mov demuxer), cross-checked against
the walk_mp4.py fragment resolution (crosscheck.py pattern), grouped per
moof with its mfhd sequence and tfdt. pts == dts is asserted for every
packet (audio cts == dts in every in-envelope fixture, REPORT §2).

ffprobe applies an init `elst` as a uniform timestamp shift (dash priming:
first pts == -media_time, Skip Samples). The oracle stores media dts/cts
(ffprobe value + media_time) plus a `present` line so product tests can
pin that trim against the untrimmed mov-muxer shape. mfhd sequence
numbers come from the walker; product tests reject a gap against them.

Usage: gen_timeline_oracle.py   (writes src/goldens/fmp4_timeline.txt)
"""

import json
import os
import subprocess
import sys
import tempfile

import walk_mp4

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
GOLD = os.path.join(ROOT, "src", "goldens")

# (oracle name, [committed golden files, concatenated in order])
FIXTURES = [
    ("fmp4_lc.mp4", ["fmp4_lc.mp4"]),
    ("fmp4_lc_notfdt.mp4", ["fmp4_lc_notfdt.mp4"]),
    ("fmp4_he.mp4", ["fmp4_he.mp4"]),
    ("fmp4_bbb", ["fmp4_bbb_init.m4a", "fmp4_bbb_seg1.m4a"]),
    ("fmp4_dash", [
        "fmp4_dash_init.m4s", "fmp4_dash_seg1.m4s", "fmp4_dash_seg2.m4s",
    ]),
]


def probed(path):
    out = subprocess.run(
        ["ffprobe", "-v", "error", "-select_streams", "a:0",
         "-show_entries", "packet=pos,size,dts,pts,duration",
         "-of", "json", path],
        check=True, capture_output=True, text=True)
    return json.loads(out.stdout)["packets"]


def edit(tree):
    """Single supported elst entry, or None when the init has no edit."""
    found = []

    def rec(nodes):
        for n in nodes:
            if n["type"] == "elst":
                found.append(n)
            rec(n.get("children", []))
    rec(tree)
    if not found:
        return None
    assert len(found) == 1, "multiple elst boxes"
    ents = found[0].get("entries", [])
    assert len(ents) == 1, ents
    assert ents[0]["rate"] == 65536, ents[0]
    return ents[0]


def declared_durations(data, tree):
    """Per-moof declared sample durations (trun field -> tfhd default ->
    trex default), mirroring walk_mp4.resolve_samples' defaults chain.
    ffprobe normalizes the final packet's duration, so the declared sum is
    the resolver's timeline; ffprobe cross-checks it at every non-final
    fragment boundary.
    """
    trex = {}

    def collect(nodes):
        for n in nodes:
            if n["type"] == "trex":
                trex[n["track_id"]] = n
            collect(n.get("children", []))
    collect(tree)

    out = []
    for n in tree:
        if n["type"] != "moof":
            continue
        frag_state = {"trex": trex, "cur_tfhd": None, "cur_tfdt": None,
                      "cur_truns": []}
        sub = []
        walk_mp4.walk(data, n["offset"] + 8, n["offset"] + n["size"], 0,
                      sub, frag_state)
        tfhd = frag_state["cur_tfhd"]
        defaults = trex.get(tfhd["track_id"], {})
        def_dur = tfhd.get("default_duration",
                           defaults.get("default_duration", 0))
        durs = [s.get("duration", def_dur)
                for t in frag_state["cur_truns"] for s in t["samples"]]
        out.append(durs)
    return out


def oracle(name, parts):
    data = b"".join(open(os.path.join(GOLD, p), "rb").read() for p in parts)
    tree = []
    walk_mp4.walk(data, 0, len(data), 0, tree, {"trex": {}})
    idx = walk_mp4.resolve_samples(data, tree)
    with tempfile.NamedTemporaryFile(suffix=".mp4") as tmp:
        tmp.write(data)
        tmp.flush()
        pkts = probed(tmp.name)
    mine = idx["samples"]
    assert len(mine) == len(pkts), f"{name}: walker {len(mine)} vs ffprobe {len(pkts)}"
    elst = edit(tree)
    # ffprobe presents an elst by shifting every packet timestamp.
    shift = 0 if elst is None else int(elst["media_time"])
    assert shift >= 0
    for i, (s, p) in enumerate(zip(mine, pkts)):
        assert int(p["pts"]) == int(p["dts"]), f"{name}: packet {i} pts != dts"
        assert s["offset"] == int(p["pos"]), f"{name}: packet {i} pos"
        assert s["size"] == int(p["size"]), f"{name}: packet {i} size"
        assert s["dts"] == int(p["dts"]) + shift, f"{name}: packet {i} dts"
        assert s["cts"] == int(p["pts"]) + shift, f"{name}: packet {i} cts"
    first_pts = int(pkts[0]["pts"])
    if shift:
        assert first_pts == -shift, f"{name}: ffprobe first pts {first_pts}"
    else:
        assert first_pts == 0, f"{name}: untrimmed ffprobe first pts {first_pts}"
    if elst is None:
        present = (f"present elst none seg_dur none ffprobe_first_pts "
                   f"{first_pts} priming none")
    else:
        present = (f"present elst {shift} seg_dur {int(elst['seg_dur'])} "
                   f"ffprobe_first_pts {first_pts} priming {shift}")
    lines = [f"fixture {name} timescale {idx['timescale']}", present]
    declared = declared_durations(data, tree)
    at = 0
    fi = 0
    for ev in idx["fragments"]:
        if "moof" not in ev:
            continue  # sidx events carry no samples
        moof = ev["moof"]
        count = sum(t["count"] for t in moof["truns"])
        frag = pkts[at:at + count]
        assert frag, f"{name}: empty fragment seq {moof['seq']}"
        first = int(frag[0]["dts"]) + shift
        durs = declared[fi]
        assert len(durs) == count, f"{name}: declared durations vs packets"
        end = first + sum(durs)
        # ffprobe pins the boundary everywhere it is meaningful; its
        # normalized final-packet duration is presentation, not declaration.
        if at + count < len(pkts):
            assert end == int(pkts[at + count]["dts"]) + shift, \
                f"{name}: declared end {end} vs ffprobe {pkts[at + count]['dts']}"
        fi += 1
        tfdt = moof["tfdt"]
        tfdt = "none" if tfdt is None else str(tfdt)
        lines.append(
            f"frag seq {moof['seq']} tfdt {tfdt} first {first} end {end}"
            f" packets {count}")
        for p in frag:
            dts = int(p["dts"]) + shift
            cts = int(p["pts"]) + shift
            lines.append(f"pkt {p['pos']} {p['size']} {dts} {cts}")
        at += count
    assert at == len(pkts)
    return lines


def main():
    out = [
        "# TASK-126 fMP4 timeline oracle. Derived from ffprobe 7.0.2-static"
        " -show_packets",
        "# (lab/fmp4/PIN.md) by lab/fmp4/gen_timeline_oracle.py, cross-checked"
        " against the",
        "# walk_mp4.py fragment resolution. An init elst shifts ffprobe"
        " timestamps;",
        "# stored dts/cts are media time (ffprobe + media_time). pts == dts"
        " is asserted,",
        "# so each pkt's cts equals its dts. Product tests replay this file"
        " and never",
        "# spawn ffprobe.",
        "# present elst <u64|none> seg_dur <u64|none> ffprobe_first_pts <i64>"
        " priming <u64|none>",
        "# frag <mfhd seq> tfdt <u64|none> first <dts> end <dts> packets <n>",
        "# pkt <pos> <size> <dts> <cts>",
    ]
    for name, parts in FIXTURES:
        out.extend(oracle(name, parts))
    dst = os.path.join(GOLD, "fmp4_timeline.txt")
    open(dst, "w").write("\n".join(out) + "\n")
    print(f"{dst}: {len(out)} lines")


if __name__ == "__main__":
    sys.exit(main())
