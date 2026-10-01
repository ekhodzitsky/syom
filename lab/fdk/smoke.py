#!/usr/bin/env python3
"""Opt-in FDK lab smoke. Not run by cargo test --workspace --lib.

TASK-121 regression gate: every syom-encoded ADTS golden must decode to
full length through fdk-aac (the AOSP platform software AAC decoder
codebase). Before TASK-121 the ABR stuffing (zero bytes after ID_END)
made FDK answer the first stuffed frame with AAC_DEC_UNKNOWN and kill
the stream; the padding is now EXT_FILL fill_element()s before ID_END.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GOLD = ROOT / "src" / "goldens"
DRIVER = Path(__file__).resolve().parent / "fdk_driver"

# (name, golden, expected sample count per channel) — counts pinned to the
# libavcodec full-length decode (see lab/mobile/REPORT.md).
CASES = [
    ("lc-syom", "enc48.adts", 30720),
    ("lc-syom-transient", "enc48t.adts", 30720),
    ("lc-syom-lookahead", "enc48l.adts", 30720),
    ("he1-syom", "he48e.adts", 32768),
    ("he2-syom", "he2_48e.adts", 65536),
    ("mc51-syom", "enc_mc51.adts", 16384),
    ("mc71-syom", "enc_mc71.adts", 16384),
]


def run(cmd: list[str]) -> dict:
    p = subprocess.run(cmd, capture_output=True, text=True)
    line = (p.stdout or "").strip().splitlines()
    last = line[-1] if line else "{}"
    try:
        rec = json.loads(last)
    except json.JSONDecodeError:
        rec = {"ok": False, "error": last or p.stderr}
    rec["returncode"] = p.returncode
    return rec


def main() -> int:
    driver = Path(sys.argv[1]) if len(sys.argv) > 1 else DRIVER
    if not driver.is_file():
        print("FAIL: fdk_driver not built (opt-in lab; see PIN.md)")
        return 2
    report = {
        "engine": subprocess.check_output([str(driver), "id"], text=True).strip(),
        "cases": [],
    }
    bad = 0
    for name, fn, samples in CASES:
        rec = run([str(driver), "decode-adts", str(GOLD / fn)])
        rec["case"] = name
        rec["file"] = fn
        rec["expected_samples"] = samples
        rec["match"] = bool(rec.get("ok") and rec.get("samples") == samples)
        if not rec["match"]:
            bad += 1
        report["cases"].append(rec)
    print(json.dumps(report, indent=2))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
