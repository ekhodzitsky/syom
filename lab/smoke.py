#!/usr/bin/env python3
"""Opt-in libavcodec lab smoke. Not run by cargo test --workspace --lib."""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GOLD = ROOT / "src" / "goldens"
DRIVER = ROOT / "lab" / "libavcodec" / "avc_driver"

CASES = [
    ("lc-adts", "sine48.adts", "decode-au", {"rate": 48000, "channels": 1, "samples": 13312}),
    ("he-adts", "he48.adts", "decode-au", {"rate": 48000, "channels": 2, "samples": 18432}),
    ("ps-adts", "ps48.adts", "decode-au", {"rate": 48000, "channels": 2, "samples": 53248}),
    ("mc51-adts", "mc51.adts", "decode-au", {"rate": 48000, "channels": 6, "samples": 20480}),
    ("lc-m4a", "sine441.m4a", "decode-container", {"rate": 44100, "channels": 1, "samples": 11264}),
    ("he-m4a", "he48.m4a", "decode-container", {"rate": 48000, "channels": 2, "samples": 16320}),
    ("latm-au-unsupported", "he48.latm", "decode-au", None),
]


def run_driver(mode: str, path: Path) -> dict:
    p = subprocess.run([str(DRIVER), mode, str(path)], capture_output=True, text=True)
    line = (p.stdout or "").strip().splitlines()
    last = line[-1] if line else "{}"
    try:
        rec = json.loads(last)
    except json.JSONDecodeError:
        rec = {"ok": False, "error": last or p.stderr}
    rec["returncode"] = p.returncode
    rec["stderr"] = (p.stderr or "").strip()[:200]
    return rec


def main() -> int:
    if not DRIVER.is_file():
        print("FAIL: avc_driver not built (opt-in lab)")
        return 2
    ffmpeg = shutil.which("ffmpeg")
    report = {
        "engine": subprocess.check_output([str(DRIVER), "id"], text=True).strip(),
        "lane_note": "in_process_au / in_process_container are codec cells; process_launch is version-only",
        "process_launch": {
            "ffmpeg_path": ffmpeg,
            "ffmpeg_version": None,
            "note": "CLI process start is not an in-process codec timing cell",
        },
        "cases": [],
    }
    if ffmpeg:
        v = subprocess.run(
            [ffmpeg, "-version"], capture_output=True, text=True, check=False
        )
        report["process_launch"]["ffmpeg_version"] = (v.stdout or "").splitlines()[:2]
    bad = 0
    for name, fn, mode, expect in CASES:
        rec = run_driver(mode, GOLD / fn)
        rec["case"] = name
        rec["file"] = fn
        rec["mode"] = mode
        if expect is None:
            rec["expected"] = "unsupported/error"
            rec["match"] = rec.get("ok") is False
        else:
            rec["expected"] = expect
            rec["match"] = bool(
                rec.get("ok")
                and rec.get("rate") == expect["rate"]
                and rec.get("channels") == expect["channels"]
                and rec.get("samples") == expect["samples"]
            )
            # M4A elst: lavc may trim differently; record, do not silently rewrite expect.
            if (
                not rec["match"]
                and rec.get("ok")
                and rec.get("rate") == expect["rate"]
                and rec.get("channels") == expect["channels"]
            ):
                rec["match"] = False
                rec["note"] = "sample-count differs from syom (elst/priming); not a throughput cell"
        if expect is not None and not rec["match"] and name.endswith("adts"):
            bad += 1
        if expect is None and rec.get("ok"):
            bad += 1
        report["cases"].append(rec)
        print(f"{name}: match={rec.get('match')} {json.dumps({k: rec.get(k) for k in ('ok','rate','channels','samples','error','note')})}")
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("-")
    text = json.dumps(report, indent=2) + "\n"
    if str(out) != "-":
        out.write_text(text, encoding="utf-8")
    if bad:
        print(f"FAIL: {bad} ADTS shape mismatches")
        return 1
    print("OK: lab libavcodec ADTS smoke")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
