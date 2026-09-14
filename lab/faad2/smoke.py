#!/usr/bin/env python3
"""Opt-in FAAD2 lab smoke. Not run by cargo test --workspace --lib."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
GOLD = ROOT / "src" / "goldens"
DRIVER = Path(__file__).resolve().parent / "faad_driver"
AVC = ROOT / "lab" / "libavcodec" / "avc_driver"

# Metadata we require FAAD2 to get right. Sample counts are recorded, not
# required to match libavcodec (priming is a disagreement class).
CASES = [
    ("lc-adts", "sine48.adts", {"rate": 48000}),
    ("he-adts", "he48.adts", {"rate": 48000}),
    ("ps-adts", "ps48.adts", {"rate": 48000}),
    ("mc51-adts", "mc51.adts", {"rate": 48000, "channels": 6}),
    ("latm-unsupported", "he48.latm", None),
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
        print("FAIL: faad_driver not built (opt-in lab)")
        return 2
    report = {
        "engine": subprocess.check_output([str(driver), "id"], text=True).strip(),
        "precision": "FAAD_FMT_FLOAT planar-split FNV-1a",
        "cases": [],
    }
    bad = 0
    for name, fn, expect in CASES:
        rec = run([str(driver), "decode-adts", str(GOLD / fn)])
        rec["case"] = name
        rec["file"] = fn
        if AVC.is_file() and expect is not None:
            rec["libavcodec"] = run([str(AVC), "decode-au", str(GOLD / fn)])
        if expect is None:
            rec["expected"] = "unsupported/error"
            rec["match"] = rec.get("ok") is False
        else:
            rec["expected"] = expect
            rec["match"] = bool(rec.get("ok") and rec.get("rate") == expect["rate"])
            if "channels" in expect:
                rec["match"] = rec["match"] and rec.get("channels") == expect["channels"]
        if not rec["match"]:
            bad += 1
        report["cases"].append(rec)
    print(json.dumps(report, indent=2))
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
