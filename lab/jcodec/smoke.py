#!/usr/bin/env python3
"""Opt-in JCodec (Java lane) AAC smoke. Not run by cargo test --workspace --lib.

TASK-19. Requires a JDK (javac/java) on PATH. The pinned artifact is
fetched to target/tmp/jcodec/ (git-ignored) and hash-checked.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
GOLD = ROOT / "src" / "goldens"
CACHE = ROOT / "target" / "tmp" / "jcodec"

JAR = "jcodec-0.2.5.jar"
JAR_URL = f"https://repo1.maven.org/maven2/org/jcodec/jcodec/0.2.5/{JAR}"
JAR_SHA256 = "890329dad124e8b739c1d6602a59a53c8a474daddff265c2561e21c498496c81"

# (case-id, file, mode, expected shape or None when the decode must fail)
# `samples` is interleaved s16 values, matching JCodecSmoke's counter.
# HE/PS rows are the recorded JCodec 0.2.5 result, not the ffprobe
# presentation: ADTS HE/PS yields an empty buffer, HE M4A is core-only.
CASES = [
    ("lc-adts", "sine48.adts", "adts", {
        "rate": 48000, "channels": 1, "samples": 13312,
        "pcm_s16_sha256": "dd3ba138ba04a2c0ddc59296463ca4ba4442aa475e53912db5c7439f7ad29ae5",
    }),
    ("lc-m4a", "sine441.m4a", "m4a", {
        "rate": 44100, "channels": 1, "samples": 12288,
        "pcm_s16_sha256": "de676bae28a480011d3d012db14bef539324e62a841a9627863c689bea168af3",
    }),
    ("lc-tns48-adts", "tns48.adts", "adts", {
        "rate": 48000, "channels": 1, "samples": 1024,
        "pcm_s16_sha256": "9d556828f3f50cc09fda7180ea27831ea4a767d751340e1f02f1367fdd0eb54e",
    }),
    ("he-adts", "he48.adts", "adts", {
        "rate": 0, "channels": 0, "samples": 0, "frames": 9,
        "pcm_s16_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    }),
    ("he-m4a", "he48.m4a", "m4a", {
        "rate": 24000, "channels": 1, "samples": 9216,
        "pcm_s16_sha256": "f7b586904e3678145aa47e4232587c913139cef0102d6d8e9276fc80c35cbad3",
    }),
    ("ps-adts", "ps48.adts", "adts", {
        "rate": 0, "channels": 0, "samples": 0, "frames": 26,
        "pcm_s16_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    }),
    ("mc51-adts", "mc51.adts", "adts", {
        "rate": 48000, "channels": 6, "samples": 122880,
        "pcm_s16_sha256": "4fe9b404f7a5e9cddf9383ea135c83a7b06f095e29467ee6f5a3940114162581",
    }),
    ("latm-unsupported", "he48.latm", "adts", None),
]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def ensure_jar() -> Path:
    jar = CACHE / JAR
    if jar.is_file() and sha256(jar) == JAR_SHA256:
        return jar
    CACHE.mkdir(parents=True, exist_ok=True)
    print(f"fetch {JAR_URL}", file=sys.stderr)
    with urllib.request.urlopen(JAR_URL, timeout=60) as r:  # noqa: S310 pinned URL
        jar.write_bytes(r.read())
    if sha256(jar) != JAR_SHA256:
        jar.unlink()
        raise SystemExit(f"FAIL: {JAR} sha256 mismatch")
    return jar


def run_java(cp: str, args: list[str]) -> dict:
    p = subprocess.run(["java", "-cp", cp, "JCodecSmoke", *args], capture_output=True, text=True)
    lines = (p.stdout or "").strip().splitlines()
    try:
        rec = json.loads(lines[-1]) if lines else {}
    except json.JSONDecodeError:
        rec = {"ok": False, "error": lines[-1] if lines else p.stderr.strip()[:200]}
    rec["returncode"] = p.returncode
    return rec


def main() -> int:
    javac, java = shutil.which("javac"), shutil.which("java")
    if not javac or not java:
        print("FAIL: no JDK on PATH (javac/java) — TASK-19 recorded gap; see lab/jcodec/REPORT.md")
        return 2
    jar = ensure_jar()
    cp = f"{jar}:{HERE}"
    subprocess.run([javac, "-cp", str(jar), "-d", str(CACHE), str(HERE / "JCodecSmoke.java")], check=True)
    cp = f"{jar}:{CACHE}"

    report = {"engine": "org.jcodec:jcodec:0.2.5", "java": subprocess.check_output(
        [java, "-version"], stderr=subprocess.STDOUT, text=True).splitlines()[0], "cases": {}}
    fail = 0
    for cid, fname, mode, want in CASES:
        path = GOLD / fname
        if not path.is_file():
            report["cases"][cid] = {"skipped": f"missing {fname}"}
            continue
        rec = run_java(cp, [mode, str(path)])
        ok = rec.get("ok") is True
        if want and ok:
            for k, v in want.items():
                if rec.get(k) != v:
                    rec["mismatch"] = f"{k}: got {rec.get(k)} want {v}"
                    ok = False
        if want is None and cid == "latm-unsupported":
            ok = not ok  # must fail
        report["cases"][cid] = rec
        fail += 0 if ok else 1
    print(json.dumps(report, indent=1))
    return 1 if fail else 0


if __name__ == "__main__":
    sys.exit(main())
