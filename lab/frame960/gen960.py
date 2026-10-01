#!/usr/bin/env python3
"""TASK-63 lab: hand-built AAC-LC 960/120-frame vectors (lab only, no product path).

Bitstreams are written directly from ISO/IEC 14496-3 syntax (raw_data_block,
GASpecificConfig) with frameLengthFlag=1, then wrapped as LATM/LOAS
(ISO/IEC 14496-3 subpart 1, AudioSyncStream) and a minimal M4A
(ISO/IEC 14496-12/-14, esds carries the same ASC).

Table provenance (lab material, not product code):
- 960/120 scale-factor band offsets: FFmpeg n7.0.2 libavcodec/aactab.c
  (swb_offset_960_48 / swb_offset_120_48), carrying the ISO/IEC 14496-3
  960/120 tables; band counts cross-checked against faad2 2.11.3
  libfaad/specrec.c (num_swb_960_window / num_swb_120_window).
  At 48 kHz: 49 long bands ending at 960, 14 short bands ending at 120.
- Huffman book 1 (ISO/IEC 14496-3 Table 4.A.2): syom src/engine/huff_quad.rs.
- Scale-factor Huffman (Table 4.A.1): FFmpeg n7.0.2 aactab.c
  (index 60 = 1-bit zero-diff codeword).
- LOAS layout mirrors syom src/engine/latm_write.rs (lavc-validated golden
  format): StreamMuxConfig in every frame, audioMuxVersion 0,
  frameLengthType 0, latmBufferFullness 0xFF, no CRC.

Vector conformity is established by independent decoders (run.sh / REPORT.md),
not by the table sources. Content is deterministic: book-1 quads over bands
0..15 (long) / band 0 x 8 windows (short), all scale factors == global_gain
(84) so every coded coefficient decodes to +/-2^-4 before the IMDCT.

Usage: python3 gen960.py OUTDIR
Writes lc960-m48.loas (8 x SCE, ONLY_LONG), lc960-s48.loas (8 x CPE,
LONG/LONG/START/SHORT/STOP/LONG/LONG/LONG), lc960-s48.m4a (same CPE AUs),
and per-vector .au (concatenated raw access units) + .asc files.
"""

import json
import struct
import sys

# --- tables (see module docstring for provenance) ---
H1_LEN = [
    11, 9, 11, 10, 7, 10, 11, 9, 11, 10, 7, 10, 7,
    5, 7, 9, 7, 10, 11, 9, 11, 9, 7, 9, 11, 9,
    11, 9, 7, 9, 7, 5, 7, 9, 7, 9, 7, 5, 7,
    5, 1, 5, 7, 5, 7, 9, 7, 9, 7, 5, 7, 9,
    7, 9, 11, 9, 11, 9, 7, 9, 11, 9, 11, 10, 7,
    9, 7, 5, 7, 9, 7, 10, 11, 9, 11, 10, 7, 9,
    11, 9, 11,
]
H1_CODE = [
    0x07f8, 0x01f1, 0x07fd, 0x03f5, 0x0068, 0x03f0, 0x07f7, 0x01ec, 0x07f5, 0x03f1, 0x0072, 0x03f4, 0x0074,
    0x0011, 0x0076, 0x01eb, 0x006c, 0x03f6, 0x07fc, 0x01e1, 0x07f1, 0x01f0, 0x0061, 0x01f6, 0x07f2, 0x01ea,
    0x07fb, 0x01f2, 0x0069, 0x01ed, 0x0077, 0x0017, 0x006f, 0x01e6, 0x0064, 0x01e5, 0x0067, 0x0015, 0x0062,
    0x0012, 0x0000, 0x0014, 0x0065, 0x0016, 0x006d, 0x01e9, 0x0063, 0x01e4, 0x006b, 0x0013, 0x0071, 0x01e3,
    0x0070, 0x01f3, 0x07fe, 0x01e7, 0x07f3, 0x01ef, 0x0060, 0x01ee, 0x07f0, 0x01e2, 0x07fa, 0x03f3, 0x006a,
    0x01e8, 0x0075, 0x0010, 0x0073, 0x01f4, 0x006e, 0x03f7, 0x07f6, 0x01e0, 0x07f9, 0x03f2, 0x0066, 0x01f5,
    0x07ff, 0x01f7, 0x07f4,
]
SF_CODE = [
    0x3ffe8, 0x3ffe6, 0x3ffe7, 0x3ffe5, 0x7fff5, 0x7fff1, 0x7ffed, 0x7fff6, 0x7ffee, 0x7ffef, 0x7fff0, 0x7fffc, 0x7fffd,
    0x7ffff, 0x7fffe, 0x7fff7, 0x7fff8, 0x7fffb, 0x7fff9, 0x3ffe4, 0x7fffa, 0x3ffe3, 0x1ffef, 0x1fff0, 0xfff5, 0x1ffee,
    0xfff2, 0xfff3, 0xfff4, 0xfff1, 0x7ff6, 0x7ff7, 0x3ff9, 0x3ff5, 0x3ff7, 0x3ff3, 0x3ff6, 0x3ff2, 0x1ff7,
    0x1ff5, 0x0ff9, 0x0ff7, 0x0ff6, 0x07f9, 0x0ff4, 0x07f8, 0x03f9, 0x03f7, 0x03f5, 0x01f8, 0x01f7, 0x00fa,
    0x00f8, 0x00f6, 0x0079, 0x003a, 0x0038, 0x001a, 0x000b, 0x0004, 0x0000, 0x000a, 0x000c, 0x001b, 0x0039,
    0x003b, 0x0078, 0x007a, 0x00f7, 0x00f9, 0x01f6, 0x01f9, 0x03f4, 0x03f6, 0x03f8, 0x07f5, 0x07f4, 0x07f6,
    0x07f7, 0x0ff5, 0x0ff8, 0x1ff4, 0x1ff6, 0x1ff8, 0x3ff8, 0x3ff4, 0xfff0, 0x7ff4, 0xfff6, 0x7ff5, 0x3ffe2,
    0x7ffd9, 0x7ffda, 0x7ffdb, 0x7ffdc, 0x7ffdd, 0x7ffde, 0x7ffd8, 0x7ffd2, 0x7ffd3, 0x7ffd4, 0x7ffd5, 0x7ffd6, 0x7fff2,
    0x7ffdf, 0x7ffe7, 0x7ffe8, 0x7ffe9, 0x7ffea, 0x7ffeb, 0x7ffe6, 0x7ffe0, 0x7ffe1, 0x7ffe2, 0x7ffe3, 0x7ffe4, 0x7ffe5,
    0x7ffd7, 0x7ffec, 0x7fff4, 0x7fff3,
]
SF_BITS = [
    18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    19, 19, 19, 19, 19, 19, 18, 19, 18, 17, 17, 16, 17,
    16, 16, 16, 16, 15, 15, 14, 14, 14, 14, 14, 14, 13,
    13, 12, 12, 12, 11, 12, 11, 10, 10, 10, 9, 9, 8,
    8, 8, 7, 6, 6, 5, 4, 3, 1, 4, 4, 5, 6,
    6, 7, 7, 8, 8, 9, 9, 10, 10, 10, 11, 11, 11,
    11, 12, 12, 13, 13, 13, 14, 14, 16, 15, 16, 15, 18,
    19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19,
    19, 19, 19, 19,
]
SWB960_48 = [
    0, 4, 8, 12, 16, 20, 24, 28, 32, 36, 40, 48, 56,
    64, 72, 80, 88, 96, 108, 120, 132, 144, 160, 176, 196, 216,
    240, 264, 292, 320, 352, 384, 416, 448, 480, 512, 544, 576, 608,
    640, 672, 704, 736, 768, 800, 832, 864, 896, 928, 960,
]
SWB120_48 = [
    0, 4, 8, 12, 16, 20, 28, 36, 44, 56, 68, 80, 96,
    112, 120,
]

NUM_SWB_960_48 = len(SWB960_48) - 1  # 49
NUM_SWB_120_48 = 14  # ff_aac_num_swb_120[48k] = 14 (faad2 agrees); the
# swb_offset_120_48 table carries one extra offset entry beyond num_swb

GLOBAL_GAIN = 172  # |x| = 2^((172-100)/4) = 2^18 per unit coefficient;
# measured lavc decode gain ~= 2^-20.5 -> PCM peak ~= 0.2 (gg=84 gives ~4e-8)


class BitWriter:
    def __init__(self):
        self.bits = []

    def w(self, value, n):
        assert 0 <= value < (1 << n) or n == 0
        for i in range(n - 1, -1, -1):
            self.bits.append((value >> i) & 1)

    def nbytes(self):
        return (len(self.bits) + 7) // 8

    def finish(self):
        while len(self.bits) % 8:
            self.bits.append(0)
        out = bytearray()
        for i in range(0, len(self.bits), 8):
            b = 0
            for bit in self.bits[i:i + 8]:
                b = (b << 1) | bit
            out.append(b)
        return bytes(out)


def asc_bytes(aot, sf_index, channel_config):
    bw = BitWriter()
    bw.w(aot, 5)
    bw.w(sf_index, 4)
    bw.w(channel_config, 4)
    bw.w(1, 1)  # frameLengthFlag: 960-line frames
    bw.w(0, 1)  # dependsOnCoreCoder
    bw.w(0, 1)  # extensionFlag
    assert bw.nbytes() == 2
    return bw.finish()


def quad_pattern(frame, band, quad, ch):
    # deterministic in {-1, 0, 1}, alternating sign per position parity
    v = [0, 0, 0, 0]
    for i in range(4):
        if (i + band + quad + ch) % 2 == 0:
            v[i] = 1 if (frame + band + i) % 2 == 0 else -1
    return v


def book1_quad(bw, v):
    idx = 0
    for x in v:
        assert x in (-1, 0, 1)
        idx = idx * 3 + (x + 1)
    bw.w(H1_CODE[idx], H1_LEN[idx])


def sf_zero_diff(bw):
    bw.w(SF_CODE[60], SF_BITS[60])  # scalefactor == global_gain


def sect_len_incr(bw, n, bits, esc):
    # sect_len_incr with the escape continuation (incr == esc -> more follows);
    # applies to EVERY section, including ZERO_HCB (Table 4.5 / 13818-7 T.17)
    while True:
        incr = min(n, esc)
        bw.w(incr, bits)
        n -= incr
        if incr < esc:
            break


def write_long_ics(bw, frame, ch, seq, coded_bands=16):
    bw.w(GLOBAL_GAIN, 8)
    bw.w(0, 1)  # ics reserved
    bw.w(seq, 2)  # window_sequence: 0 long, 1 start, 3 stop
    bw.w(0, 1)  # window_shape: sine
    bw.w(NUM_SWB_960_48, 6)  # max_sfb
    bw.w(0, 1)  # predictor_present
    # section_data: one book-1 section over bands 0..coded_bands-1, rest zero
    bw.w(1, 4)  # sect_cb = 1
    sect_len_incr(bw, coded_bands, 5, 31)
    if NUM_SWB_960_48 > coded_bands:
        bw.w(0, 4)  # ZERO_HCB section over the remaining bands
        sect_len_incr(bw, NUM_SWB_960_48 - coded_bands, 5, 31)
    # scale_factor_data
    for _ in range(coded_bands):
        sf_zero_diff(bw)
    bw.w(0, 1)  # pulse_data_present
    bw.w(0, 1)  # tns_data_present
    bw.w(0, 1)  # gain_control_data_present
    # spectral_data
    q = 0
    for band in range(coded_bands):
        width = SWB960_48[band + 1] - SWB960_48[band]
        assert width % 4 == 0
        for _ in range(width // 4):
            book1_quad(bw, quad_pattern(frame, band, q, ch))
            q += 1


def write_short_ics(bw, frame, ch):
    bw.w(GLOBAL_GAIN, 8)
    bw.w(0, 1)  # ics reserved
    bw.w(2, 2)  # EIGHT_SHORT_SEQUENCE
    bw.w(0, 1)  # window_shape
    bw.w(NUM_SWB_120_48, 4)  # max_sfb
    bw.w(0b1111111, 7)  # scale_factor_grouping: 1 = window joins current group; all 8 in one group
    # section_data (3-bit length fields for short windows): band 0 coded
    bw.w(1, 4)
    sect_len_incr(bw, 1, 3, 7)
    if NUM_SWB_120_48 > 1:
        bw.w(0, 4)
        sect_len_incr(bw, NUM_SWB_120_48 - 1, 3, 7)
    # scale_factor_data: one band x one group
    sf_zero_diff(bw)
    bw.w(0, 1)
    bw.w(0, 1)
    bw.w(0, 1)
    # spectral_data: group 0, band 0, windows 0..7 interleaved
    for w in range(8):
        book1_quad(bw, quad_pattern(frame, w, 0, ch))


def raw_data_block(channels, frame, seq):
    bw = BitWriter()
    if channels == 1:
        bw.w(0, 3)  # ID_SCE
        bw.w(0, 4)  # element_instance_tag
        if seq == 2:
            write_short_ics(bw, frame, 0)
        else:
            write_long_ics(bw, frame, 0, seq)
    else:
        bw.w(1, 3)  # ID_CPE
        bw.w(0, 4)  # element_instance_tag
        bw.w(1, 1)  # common_window
        # shared ics_info
        bw.w(0, 1)
        bw.w(seq, 2)
        bw.w(0, 1)
        if seq == 2:
            bw.w(NUM_SWB_120_48, 4)
            bw.w(0b1111111, 7)  # grouping: one group of 8 windows
        else:
            bw.w(NUM_SWB_960_48, 6)
            bw.w(0, 1)
        bw.w(0, 2)  # ms_mask_present: no M/S
        for ch in range(2):
            # per-channel payload without the shared ics_info
            bw.w(GLOBAL_GAIN, 8)
            if seq == 2:
                bw.w(1, 4)
                sect_len_incr(bw, 1, 3, 7)
                if NUM_SWB_120_48 > 1:
                    bw.w(0, 4)
                    sect_len_incr(bw, NUM_SWB_120_48 - 1, 3, 7)
                sf_zero_diff(bw)
                bw.w(0, 1)
                bw.w(0, 1)
                bw.w(0, 1)
                for w in range(8):
                    book1_quad(bw, quad_pattern(frame, w, 0, ch))
            else:
                bw.w(1, 4)
                sect_len_incr(bw, 16, 5, 31)
                bw.w(0, 4)
                sect_len_incr(bw, NUM_SWB_960_48 - 16, 5, 31)
                for _ in range(16):
                    sf_zero_diff(bw)
                bw.w(0, 1)
                bw.w(0, 1)
                bw.w(0, 1)
                q = 0
                for band in range(16):
                    width = SWB960_48[band + 1] - SWB960_48[band]
                    for _ in range(width // 4):
                        book1_quad(bw, quad_pattern(frame, band, q, ch))
                        q += 1
    bw.w(7, 3)  # ID_END
    return bw.finish()


def loas_frame(asc, au):
    bw = BitWriter()
    bw.w(0, 1)  # useSameStreamMux = 0
    bw.w(0, 1)  # audioMuxVersion = 0
    bw.w(1, 1)  # allStreamsSameTimeFraming
    bw.w(0, 6)  # numSubFrames
    bw.w(0, 4)  # numProgram
    bw.w(0, 3)  # numLayer
    for b in asc:
        bw.w(b, 8)  # 16 ASC bits, byte-aligned by construction
    bw.w(0, 3)  # frameLengthType = 0
    bw.w(0xFF, 8)  # latmBufferFullness (VBR)
    bw.w(0, 1)  # otherDataPresent
    bw.w(0, 1)  # crcCheckPresent
    left = len(au)
    while left >= 255:
        bw.w(255, 8)
        left -= 255
    bw.w(left, 8)
    for b in au:
        bw.w(b, 8)
    body = bw.finish()
    assert len(body) <= 0x1FFF
    hdr = (0x2B7 << 13) | len(body)
    return bytes([(hdr >> 16) & 0xFF, (hdr >> 8) & 0xFF, hdr & 0xFF]) + body


# --- minimal M4A (v0 32-bit boxes, stco) ---

def box(typ, payload):
    return struct.pack(">I4s", 8 + len(payload), typ) + payload


def full_box(typ, version, flags, payload):
    return box(typ, bytes([version]) + flags.to_bytes(3, "big") + payload)


def m4a(aus, channels, rate, asc, frame_len):
    n = len(aus)
    duration = n * frame_len
    mvhd = full_box(b"mvhd", 0, 0, struct.pack(
        ">IIIIIIHH", 0, 0, 1000, duration * 1000 // rate,
        0x00010000, 0x0100, 0, 0)
        + b"\x00" * 8
        + struct.pack(">9I", 0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000)
        + b"\x00" * 24 + struct.pack(">I", 2))
    tkhd = full_box(b"tkhd", 0, 3, struct.pack(
        ">IIII", 0, 0, 1, 0)
        + struct.pack(">I", duration * 1000 // rate)
        + b"\x00" * 8
        + struct.pack(">hhhh", 0, 0, 0x0100, 0)
        + struct.pack(">9I", 0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000)
        + struct.pack(">II", 0, 0))
    mdhd = full_box(b"mdhd", 0, 0, struct.pack(
        ">IIIIHH", 0, 0, rate, duration, 0x55C4, 0))
    hdlr = full_box(b"hdlr", 0, 0, struct.pack(">I4sIII", 0, b"soun", 0, 0, 0)
                    + b"SoundHandler\x00")
    avg_bitrate = sum(len(au) for au in aus) * 8 * rate // duration
    dec_specific = bytes([0x05, len(asc)]) + asc
    dec_config = bytes([0x04, 13 + len(dec_specific)]) + struct.pack(
        ">BBBHII", 0x40, 0x15, 0, 6144, avg_bitrate, avg_bitrate) + dec_specific
    es_descr = bytes([0x03, 3 + len(dec_config) + 3]) + struct.pack(">HB", 1, 0) \
        + dec_config + bytes([0x06, 1, 2])
    esds = full_box(b"esds", 0, 0, es_descr)
    mp4a_payload = b"\x00" * 6 + struct.pack(">H", 1) + b"\x00" * 8 \
        + struct.pack(">HHHHI", channels, 16, 0, 0, rate << 16) + esds
    stsd = full_box(b"stsd", 0, 0, struct.pack(">I", 1) + box(b"mp4a", mp4a_payload))
    stts = full_box(b"stts", 0, 0, struct.pack(">II", 1, n) + struct.pack(">I", frame_len))
    stsc = full_box(b"stsc", 0, 0, struct.pack(">IIII", 1, 1, 1, 1))
    stsz = full_box(b"stsz", 0, 0, struct.pack(">II", 0, n)
                    + b"".join(struct.pack(">I", len(au)) for au in aus))
    ftyp = box(b"ftyp", b"M4A " + struct.pack(">I", 0) + b"M4A isommp42")
    # stco offsets depend on the moov-independent prefix only
    offsets = []
    pos = len(ftyp) + 8
    for au in aus:
        offsets.append(pos)
        pos += len(au)
    stco = full_box(b"stco", 0, 0, struct.pack(">I", n)
                    + b"".join(struct.pack(">I", o) for o in offsets))
    stbl = box(b"stbl", stsd + stts + stsc + stsz + stco)
    minf = box(b"minf", full_box(b"smhd", 0, 0, struct.pack(">HH", 0, 0))
               + box(b"dinf", full_box(b"dref", 0, 0, struct.pack(">I", 1)
                                       + full_box(b"url ", 0, 1, b"")))
               + stbl)
    mdia = box(b"mdia", mdhd + hdlr + minf)
    trak = box(b"trak", tkhd + mdia)
    moov = box(b"moov", mvhd + trak)
    mdat = box(b"mdat", b"".join(aus))
    return ftyp + mdat + moov


def main():
    outdir = sys.argv[1] if len(sys.argv) > 1 else "vectors"
    manifest = {"frame_len": 960, "rate": 48000, "vectors": {}}

    for name, channels, seqs in [
        ("lc960-m48", 1, [0] * 8),
        ("lc960-s48", 2, [0, 0, 1, 2, 3, 0, 0, 0]),
    ]:
        asc = asc_bytes(2, 3, channels)  # LC, 48 kHz, frameLengthFlag=1
        aus = [raw_data_block(channels, f, seq) for f, seq in enumerate(seqs)]
        loas = b"".join(loas_frame(asc, au) for au in aus)
        with open(f"{outdir}/{name}.loas", "wb") as f:
            f.write(loas)
        with open(f"{outdir}/{name}.au", "wb") as f:
            f.write(b"".join(aus))
        with open(f"{outdir}/{name}.asc", "wb") as f:
            f.write(asc)
        meta = {
            "channels": channels,
            "frames": len(seqs),
            "window_sequences": seqs,
            "asc_hex": asc.hex(),
            "au_sizes": [len(au) for au in aus],
            "expected_core_samples_per_channel": len(seqs) * 960,
        }
        if channels == 2:
            with open(f"{outdir}/{name}.m4a", "wb") as f:
                f.write(m4a(aus, channels, 48000, asc, 960))
        manifest["vectors"][name] = meta

    with open(f"{outdir}/manifest.json", "w") as f:
        json.dump(manifest, f, indent=2)
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
