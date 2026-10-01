#!/usr/bin/env python3
"""TASK-101 mobile-lane capture. Opt-in lab; never run by cargo test.

Reproduces the platform-decoder capture table in REPORT.md:

    python3 lab/mobile/capture.py            # capture matrix + FDK trim probe

Requires the opt-in drivers, built per lab/fdk/PIN.md and lab/faad2/PIN.md
(on this host: FDK_PREFIX/FAAD2_PREFIX under /tmp/task101, zig-c++ shim):

    lab/fdk/fdk_driver, lab/faad2/faad_driver, lab/libavcodec/avc_driver

fdk-aac 2.0.3 is the codebase AOSP ships as the platform software AAC
decoder (external/aac); FAAD2 is the historic OEM alternative. Both are
host builds here, not on-device MediaCodec/AudioToolbox runs.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GOLD = ROOT / "src" / "goldens"
FDK = ROOT / "lab" / "fdk" / "fdk_driver"
FAAD = ROOT / "lab" / "faad2" / "faad_driver"
AVC = ROOT / "lab" / "libavcodec" / "avc_driver"

# (name, golden, origin) — origin "syom" = syom-encoded, "fixture" = pinned input.
CASES = [
    ("lc-mono-fixture", "sine48.adts", "fixture"),
    ("lc-tns-fixture", "tns48.adts", "fixture"),
    ("lc-tns-gain-fixture", "tns_gain.adts", "fixture"),
    ("lc-pns-fixture", "pns48.adts", "fixture"),
    ("lc-syom", "enc48.adts", "syom"),
    ("lc-syom-transient", "enc48t.adts", "syom"),
    ("lc-syom-lookahead", "enc48l.adts", "syom"),
    ("he1-fixture", "he48.adts", "fixture"),
    ("he2-fixture", "ps48.adts", "fixture"),
    ("he1-syom", "he48e.adts", "syom"),
    ("he2-syom", "he2_48e.adts", "syom"),
    ("mc30-fixture", "mc30.adts", "fixture"),
    ("mc40-fixture", "mc40.adts", "fixture"),
    ("mc50-fixture", "mc50.adts", "fixture"),
    ("mc51-fixture", "mc51.adts", "fixture"),
    ("mc71-fixture", "mc71.adts", "fixture"),
    ("mc71-pce-fixture", "mc71p.adts", "fixture"),
    ("mc51-syom", "enc_mc51.adts", "syom"),
    ("mc71-syom", "enc_mc71.adts", "syom"),
]


def run(cmd: list[str]) -> dict:
    p = subprocess.run(cmd, capture_output=True, text=True)
    lines = (p.stdout or "").strip().splitlines()
    try:
        rec = json.loads(lines[-1]) if lines else {}
    except json.JSONDecodeError:
        rec = {"ok": False, "error": lines[-1] if lines else p.stderr}
    return rec


def frames(data: bytes) -> list[tuple[int, int]]:
    out = []
    pos = 0
    while pos + 7 <= len(data):
        h = data[pos : pos + 7]
        if ((h[0] << 4) | (h[1] >> 4)) != 0xFFF:
            break
        flen = ((h[3] & 3) << 11) | (h[4] << 3) | (h[5] >> 5)
        out.append((pos, flen))
        pos += flen
    return out


def strip_stuffing(path: Path, dst: Path) -> int:
    """Re-write each ADTS frame without trailing zero bytes; returns count."""
    data = path.read_bytes()
    out = bytearray()
    trimmed = 0
    for pos, flen in frames(data):
        frame = data[pos : pos + flen]
        n = flen
        while n > 7 and frame[n - 1] == 0:
            n -= 1
        if n != flen:
            trimmed += 1
        b = bytearray(frame[:n])
        b[3] = (b[3] & 0xFC) | ((n >> 11) & 3)
        b[4] = (n >> 3) & 0xFF
        b[5] = (b[5] & 0x1F) | ((n & 7) << 5)
        out += b
    dst.write_bytes(bytes(out))
    return trimmed


def brief(rec: dict) -> str:
    if not rec.get("ok"):
        return f"FAIL {rec.get('error', '?')}"
    return f"{rec.get('rate')}Hz/{rec.get('channels')}ch/{rec.get('samples')}smp delay={rec.get('delay')}"


def main() -> int:
    for drv in (FDK, FAAD, AVC):
        if not drv.is_file():
            print(f"FAIL: {drv} not built (opt-in lab; see its PIN.md)")
            return 2
    print("engine fdk :", subprocess.check_output([str(FDK), "id"], text=True).strip())
    print("engine faad:", subprocess.check_output([str(FAAD), "id"], text=True).strip())
    print("engine avc :", subprocess.check_output([str(AVC), "id"], text=True).strip())
    print()
    for name, fn, origin in CASES:
        fdk = run([str(FDK), "decode-adts", str(GOLD / fn)])
        faad = run([str(FAAD), "decode-adts", str(GOLD / fn)])
        avc = run([str(AVC), "decode-au", str(GOLD / fn)])
        print(f"{name} ({fn}, {origin})")
        print("  fdk :", brief(fdk))
        print("  faad:", brief(faad))
        print("  avc :", brief(avc))
    print()
    print("FDK stuffing probe (strip trailing-zero bytes after ID_END):")
    tmp = Path("/tmp/task101_capture_trim.adts")
    for name, fn, origin in CASES:
        if origin != "syom" or not fn.endswith(".adts"):
            continue
        n = strip_stuffing(GOLD / fn, tmp)
        rec = run([str(FDK), "decode-adts", str(tmp)])
        print(f"  {name}: stripped {n} frames -> {brief(rec)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
