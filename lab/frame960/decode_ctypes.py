#!/usr/bin/env python3
"""TASK-63 lab: decode raw AU + ASC lanes through libfaad2 / libfdk-aac
runtimes via ctypes (no dev headers on this host). Reports per-call frame
info. Lab only; never invoked by cargo test.

Usage: python3 decode_ctypes.py ENGINE AUFILE ASCFILE [NFRAMES]
  ENGINE: faad2 | fdk
faad2: NeAACDecInit2 + NeAACDecDecode per AU (frameLength from NeAACDecFrameInfo).
fdk:   aacDecoder_Open(TT_MP4_RAW) + ConfigRaw + Fill/DecodeFrame.
"""
import ctypes
import struct
import sys

FAAD_FRAMEINFO_FIELDS = [
    ("bytesconsumed", ctypes.c_ulong),
    ("samples", ctypes.c_ulong),
    ("channels", ctypes.c_ubyte),
    ("error", ctypes.c_ubyte),
    ("samplerate", ctypes.c_ulong),
    ("sbr", ctypes.c_ubyte),
    ("object_type", ctypes.c_ubyte),
    ("header_type", ctypes.c_ubyte),
    ("channel_config", ctypes.c_ubyte),
    ("ps", ctypes.c_ubyte),
]


class FaadFrameInfo(ctypes.Structure):
    _fields_ = FAAD_FRAMEINFO_FIELDS


def read_aus(path, sizes):
    data = open(path, "rb").read()
    aus, pos = [], 0
    for s in sizes:
        aus.append(data[pos:pos + s])
        pos += s
    assert pos == len(data)
    return aus


def run_faad2(aus, asc):
    lib = ctypes.CDLL("libfaad.so.2")
    lib.NeAACDecOpen.restype = ctypes.c_void_p
    h = lib.NeAACDecOpen()
    rate = ctypes.c_ulong(0)
    ch = ctypes.c_ubyte(0)
    rc = lib.NeAACDecInit2(h, asc, len(asc), ctypes.byref(rate), ctypes.byref(ch))
    print(f"faad2 Init2 rc={rc} rate={rate.value} channels={ch.value}")
    if rc < 0:
        return
    total = 0
    for i, au in enumerate(aus):
        fi = FaadFrameInfo()
        buf = ctypes.create_string_buffer(4 * 960 * 8)
        lib.NeAACDecDecode.restype = ctypes.c_void_p
        lib.NeAACDecDecode.argtypes = [ctypes.c_void_p, ctypes.POINTER(FaadFrameInfo),
                                       ctypes.c_char_p, ctypes.c_ulong]
        ptr = lib.NeAACDecDecode(h, ctypes.byref(fi), au, len(au))
        print(f"faad2 frame{i}: err={fi.error} samples={fi.samples} ch={fi.channels} "
              f"rate={fi.samplerate} consumed={fi.bytesconsumed} sbr={fi.sbr} ps={fi.ps}")
        total += fi.samples
    print(f"faad2 total_samples={total}")
    # skip NeAACDecClose: libfaad2 2.11.2 runtime segfaults at interpreter
    # teardown after ctypes calls regardless; results above are complete
    import os
    sys.stdout.flush()
    os._exit(0)


def run_fdk(aus, asc):
    lib = ctypes.CDLL("libfdk-aac.so.2")
    lib.aacDecoder_Open.restype = ctypes.c_void_p
    lib.aacDecoder_Open.argtypes = [ctypes.c_int, ctypes.c_uint]
    h = lib.aacDecoder_Open(0, 1)  # TT_MP4_RAW
    asc_buf = ctypes.create_string_buffer(bytes(asc))
    conf = ctypes.c_void_p(ctypes.addressof(asc_buf))
    confs = (ctypes.c_void_p * 1)(conf)
    lens = (ctypes.c_uint * 1)(len(asc))
    rc = lib.aacDecoder_ConfigRaw(h, confs, lens)
    print(f"fdk ConfigRaw rc={rc}")
    if rc != 0:
        return
    # CStreamInfo: read aacSampleRate / frameSize / numChannels via known offsets
    lib.aacDecoder_GetStreamInfo.restype = ctypes.POINTER(ctypes.c_int)
    total = 0
    for i, au in enumerate(aus):
        inbuf = ctypes.create_string_buffer(au)
        bufs = (ctypes.c_void_p * 1)(ctypes.addressof(inbuf))
        sizes = (ctypes.c_uint * 1)(len(au))
        valid = (ctypes.c_uint * 1)(len(au))
        rc = lib.aacDecoder_Fill(h, bufs, sizes, valid)
        pcm = ctypes.create_string_buffer(2 * 960 * 8)
        rc = lib.aacDecoder_DecodeFrame(h, pcm, 960 * 8, 0)
        info = lib.aacDecoder_GetStreamInfo(h)
        # CStreamInfo: INT aacSampleRate; SCHAR profile; SCHAR aot? read via bytes
        raw = ctypes.string_at(info, 64)
        rate, = struct.unpack_from("<i", raw, 0)
        # find frameSize/numChannels: CStreamInfo layout v2:
        # INT aacSampleRate; SCHAR profile; AUDIO_OBJECT_TYPE aot (INT);
        # SCHAR channelConfig; UCHAR metadataProfile; ...
        # safer: scan for plausible values below
        aot, = struct.unpack_from("<i", raw, 8)
        frame_size, num_ch, samples = None, None, None
        # numChannels and frameSize: search INTs 1..8 and 960 near the start
        ints = struct.unpack_from("<16i", raw, 0)
        cand_fs = [v for v in ints if v == 960]
        cand_ch = [v for v in ints if v in (1, 2)]
        print(f"fdk frame{i}: rc={rc} bytesValid_after_fill={valid[0]} "
              f"rate={rate} aot={aot} ints={ints[:12]}")
        total += 960
    print(f"fdk frames_decoded={len(aus)} (960 each if rc==0)")


def main():
    engine, au_path, asc_path = sys.argv[1], sys.argv[2], sys.argv[3]
    import json
    manifest = json.load(open(__file__.rsplit("/", 1)[0] + "/vectors/manifest.json"))
    name = au_path.rsplit("/", 1)[-1].replace(".au", "")
    sizes = manifest["vectors"][name]["au_sizes"]
    aus = read_aus(au_path, sizes)
    asc = open(asc_path, "rb").read()
    if engine == "faad2":
        run_faad2(aus, asc)
    else:
        run_fdk(aus, asc)


if __name__ == "__main__":
    main()
