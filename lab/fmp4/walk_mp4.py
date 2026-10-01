#!/usr/bin/env python3
"""TASK-64 — independent ISOBMFF/fMP4 box walker and fragment sample indexer.

Dependency-free (stdlib only). Walks the box tree, parses the fragment
semantics syom would need (mvhd/mdhd/elst/stsd/esds, mvex/trex, moof/mfhd/
traf/tfhd/tfdt/trun, styp, sidx) and computes every audio sample's byte
range and decode time from the fragment tables alone.

Used to inventory real fixtures and to cross-check against an independent
demuxer (ffprobe -show_packets). Emits JSON to stdout.
"""

import json
import struct
import sys

CONTAINERS = {
    "moov", "trak", "mdia", "minf", "stbl", "edts", "moof", "traf",
    "mvex", "mfra", "sinf", "schi", "wave", "udta", "meta",
}

# tfhd flag bits (ISO/IEC 14496-12 §8.8.7)
TFHD_BASE_DATA_OFFSET = 0x000001
TFHD_SAMPLE_DESC = 0x000002
TFHD_DEFAULT_DURATION = 0x000008
TFHD_DEFAULT_SIZE = 0x000010
TFHD_DEFAULT_FLAGS = 0x000020
TFHD_DURATION_IS_EMPTY = 0x010000
TFHD_DEFAULT_BASE_IS_MOOF = 0x020000

# trun flag bits (§8.8.8)
TRUN_DATA_OFFSET = 0x000001
TRUN_FIRST_FLAGS = 0x000004
TRUN_SAMPLE_DURATION = 0x000100
TRUN_SAMPLE_SIZE = 0x000200
TRUN_SAMPLE_FLAGS = 0x000400
TRUN_SAMPLE_CTO = 0x000800


def u32(b, o):
    return struct.unpack_from(">I", b, o)[0]


def u64(b, o):
    return struct.unpack_from(">Q", b, o)[0]


def s32(b, o):
    return struct.unpack_from(">i", b, o)[0]


def boxes(data, start, end):
    pos = start
    while pos + 8 <= end:
        size, typ = u32(data, pos), data[pos + 4:pos + 8].decode("latin1")
        head = 8
        if size == 1:
            size, head = u64(data, pos + 8), 16
        elif size == 0:
            size = end - pos
        if size < head or pos + size > end:
            raise ValueError(f"bad box {typ!r} at {pos}: size {size}")
        yield pos, head, size, typ
        pos += size


def parse_full(data, o):
    return data[o], u32(data, o) & 0xFFFFFF  # version, flags


def walk(data, start, end, depth, out, frag_state):
    for pos, head, size, typ in boxes(data, start, end):
        body = pos + head
        node = {"type": typ, "offset": pos, "size": size}
        if typ == "ftyp":
            node["brand"] = data[body:body + 4].decode("latin1")
        elif typ == "styp":
            node["brand"] = data[body:body + 4].decode("latin1")
        elif typ == "mvhd":
            v, _ = parse_full(data, body)
            node["timescale"] = u32(data, body + (20 if v == 1 else 12))
        elif typ == "mehd":
            v, _ = parse_full(data, body)
            node["duration"] = u64(data, body + 4) if v == 1 else u32(data, body + 4)
        elif typ == "trex":
            v, _ = parse_full(data, body)
            node.update(track_id=u32(data, body + 4),
                        default_desc=u32(data, body + 8),
                        default_duration=u32(data, body + 12),
                        default_size=u32(data, body + 16),
                        default_flags=u32(data, body + 20))
            frag_state["trex"][node["track_id"]] = node
        elif typ == "tkhd":
            v, _ = parse_full(data, body)
            node["track_id"] = u32(data, body + (20 if v == 1 else 12))
        elif typ == "hdlr":
            node["handler"] = data[body + 8:body + 12].decode("latin1")
        elif typ == "mdhd":
            v, _ = parse_full(data, body)
            o = body + (24 if v == 1 else 16)
            node["timescale"] = u32(data, body + (20 if v == 1 else 12))
            node["duration"] = u64(data, o) if v == 1 else u32(data, o)
        elif typ == "elst":
            v, _ = parse_full(data, body)
            n = u32(data, body + 4)
            ents = []
            o = body + 8
            for _ in range(n):
                if v == 1:
                    ents.append({"seg_dur": u64(data, o),
                                 "media_time": struct.unpack_from(">q", data, o + 8)[0],
                                 "rate": u32(data, o + 16)})
                    o += 20
                else:
                    ents.append({"seg_dur": u32(data, o),
                                 "media_time": s32(data, o + 4),
                                 "rate": u32(data, o + 8)})
                    o += 12
            node["entries"] = ents
        elif typ == "stsd":
            n = u32(data, body + 4)
            entries = []
            o = body + 8
            for _ in range(n):
                esize = u32(data, o)
                etyp = data[o + 4:o + 8].decode("latin1")
                ent = {"type": etyp, "size": esize}
                if etyp == "mp4a":
                    for cpos, chead, csize, ctyp in boxes(data, o + 8 + 28, o + esize):
                        if ctyp == "esds":
                            ent["esds_asc"] = extract_asc(data[cpos + chead + 4:])
                entries.append(ent)
                o += esize
            node["entries"] = entries
        elif typ == "mfhd":
            v, _ = parse_full(data, body)
            node["sequence"] = u32(data, body + 4)
        elif typ == "tfhd":
            v, flags = parse_full(data, body)
            o = body + 4
            tid = u32(data, o); o += 4
            node.update(track_id=tid, flags=flags)
            frag_state["cur_tfhd"] = node
            if flags & TFHD_BASE_DATA_OFFSET:
                node["base_data_offset"] = u64(data, o); o += 8
            if flags & TFHD_SAMPLE_DESC:
                node["sample_desc"] = u32(data, o); o += 4
            if flags & TFHD_DEFAULT_DURATION:
                node["default_duration"] = u32(data, o); o += 4
            if flags & TFHD_DEFAULT_SIZE:
                node["default_size"] = u32(data, o); o += 4
            if flags & TFHD_DEFAULT_FLAGS:
                node["default_flags"] = u32(data, o); o += 4
            node["default_base_is_moof"] = bool(flags & TFHD_DEFAULT_BASE_IS_MOOF)
            node["duration_is_empty"] = bool(flags & TFHD_DURATION_IS_EMPTY)
        elif typ == "tfdt":
            v, _ = parse_full(data, body)
            node["base_media_decode_time"] = u64(data, body + 4) if v == 1 else u32(data, body + 4)
            frag_state["cur_tfdt"] = node["base_media_decode_time"]
        elif typ == "trun":
            v, flags = parse_full(data, body)
            count = u32(data, body + 4)
            o = body + 8
            node.update(version=v, flags=flags, sample_count=count)
            if flags & TRUN_DATA_OFFSET:
                node["data_offset"] = s32(data, o); o += 4
            if flags & TRUN_FIRST_FLAGS:
                node["first_sample_flags"] = u32(data, o); o += 4
            samples = []
            for _ in range(count):
                s = {}
                if flags & TRUN_SAMPLE_DURATION:
                    s["duration"] = u32(data, o); o += 4
                if flags & TRUN_SAMPLE_SIZE:
                    s["size"] = u32(data, o); o += 4
                if flags & TRUN_SAMPLE_FLAGS:
                    s["flags"] = u32(data, o); o += 4
                if flags & TRUN_SAMPLE_CTO:
                    s["cto"] = s32(data, o) if v == 1 else u32(data, o); o += 4
                samples.append(s)
            node["samples"] = samples
            frag_state.setdefault("cur_truns", []).append(node)
        elif typ == "sidx":
            v, _ = parse_full(data, body)
            node["reference_id"] = u32(data, body + 4)
            node["timescale"] = u32(data, body + 8)
            o = body + 12
            if v == 0:
                node["earliest_presentation_time"] = u32(data, o)
                node["first_offset"] = u32(data, o + 4); o += 8
            else:
                node["earliest_presentation_time"] = u64(data, o)
                node["first_offset"] = u64(data, o + 8); o += 16
            node["reference_count"] = struct.unpack_from(">H", data, o + 2)[0]
        elif typ == "tenc":
            node["is_encrypted"] = u32(data, body + 4) >> 16
        if typ in CONTAINERS:
            node["children"] = []
            inner = body + 4 if typ == "meta" else body
            walk(data, inner, pos + size, depth + 1, node["children"], frag_state)
        out.append(node)


def extract_asc(desc):
    """Pull DecoderSpecificInfo (tag 0x05) bytes out of an esds body."""
    def rdlen(d, p):
        n = 0
        for i in range(4):
            n = (n << 7) | (d[p + i] & 0x7F)
            if not d[p + i] & 0x80:
                return n, p + i + 1
        raise ValueError("desc len > 4 bytes")

    if desc[0] != 0x03:
        return None
    ln, p = rdlen(desc, 1)
    es_end = p + ln
    flags = desc[p + 2]
    p += 3
    if flags & 0x80:
        p += 2
    if flags & 0x40:
        p += 1 + desc[p]
    if flags & 0x20:
        p += 2
    if desc[p] != 0x04:
        return None
    ln, p = rdlen(desc, p + 1)
    dc_end = p + ln
    p += 13
    while p < dc_end:
        tag = desc[p]
        ln, q = rdlen(desc, p + 1)
        if tag == 0x05:
            return desc[q:q + ln].hex()
        p = q + ln
    return None


def resolve_samples(data, tree):
    """Compute absolute byte offset + decode time of every trun sample.

    Handles: tfhd base_data_offset / default_base_is_moof / implicit base
    (end of previous fragment), trex and tfhd defaults, per-sample trun
    duration/size, tfdt or accumulated decode time, trun data_offset.
    """
    trex = {}

    def collect(nodes):
        for n in nodes:
            if n["type"] == "trex":
                trex[n["track_id"]] = n
            collect(n.get("children", []))
    collect(tree)

    # first mdhd timescale (single audio track scope)
    ts = None
    def find_mdhd(nodes):
        nonlocal ts
        for n in nodes:
            if n["type"] == "mdhd" and ts is None:
                ts = n["timescale"]
            find_mdhd(n.get("children", []))
    find_mdhd(tree)

    samples = []
    prev_end = None  # implicit base: end of previous fragment's sample data
    decode_time = 0
    events = []
    for n in tree:
        if n["type"] == "sidx":
            events.append({"sidx": {k: v for k, v in n.items() if k not in ("offset", "size")}})
        if n["type"] != "moof":
            continue
        moof_pos = n["offset"]
        st = {"trex": {}, "cur_truns": []}
        # re-walk this moof for tfhd/tfdt/trun in order
        frag_state = {"trex": trex, "cur_tfhd": None, "cur_tfdt": None, "cur_truns": []}
        sub = []
        walk(data, n["offset"] + 8, n["offset"] + n["size"], 0, sub, frag_state)
        tfhd = frag_state["cur_tfhd"]
        tfdt = frag_state["cur_tfdt"]
        truns = frag_state["cur_truns"]
        if tfdt is not None:
            decode_time = tfdt
        defaults = trex.get(tfhd["track_id"], {})
        def_dur = tfhd.get("default_duration", defaults.get("default_duration", 0))
        def_size = tfhd.get("default_size", defaults.get("default_size", 0))
        if "base_data_offset" in tfhd:
            base = tfhd["base_data_offset"]
        elif tfhd.get("default_base_is_moof"):
            base = moof_pos
        else:
            base = prev_end if prev_end is not None else moof_pos
        frag_first = len(samples)
        for trun in truns:
            off = base + trun.get("data_offset", 0)
            for s in trun["samples"]:
                dur = s.get("duration", def_dur)
                size = s.get("size", def_size)
                samples.append({"offset": off, "size": size,
                                "dts": decode_time, "cts": decode_time + s.get("cto", 0)})
                off += size
                decode_time += dur
        # fragment data ends at the last sample's end (moof's mdat sibling)
        if len(samples) > frag_first:
            prev_end = samples[-1]["offset"] + samples[-1]["size"]
        events.append({"moof": {"seq": sub[0].get("sequence") if sub and sub[0]["type"] == "mfhd" else None,
                                "tfhd_flags": f"0x{tfhd['flags']:06x}",
                                "tfdt": tfdt,
                                "truns": [{"count": t["sample_count"],
                                           "flags": f"0x{t['flags']:06x}"} for t in truns]}})
    return {"timescale": ts, "samples": samples, "fragments": events}


def main():
    path = sys.argv[1]
    data = open(path, "rb").read()
    tree = []
    walk(data, 0, len(data), 0, tree, {"trex": {}})
    idx = resolve_samples(data, tree)

    def strip(nodes):
        for n in nodes:
            for t in n.get("children", []):
                pass
            if n["type"] == "trun":
                n["samples"] = n["samples"][:2] + ["..."] if n["sample_count"] > 2 else n["samples"]
            strip(n.get("children", []))
    strip(tree)
    print(json.dumps({"file": path, "bytes": len(data),
                      "boxes": tree,
                      "resolved": {"timescale": idx["timescale"],
                                   "sample_count": len(idx["samples"]),
                                   "first": idx["samples"][:3],
                                   "last": idx["samples"][-3:],
                                   "fragments": idx["fragments"]}},
                     indent=1))


if __name__ == "__main__":
    main()
