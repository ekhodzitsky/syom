#!/usr/bin/env python3
"""TASK-99: clean release builds of the minimal consumers; prints a table
of stripped size, text / rodata sections and the delta over `base`.
Flags are fixed in Cargo.toml (opt 3, fat LTO, 1 CGU, panic=abort, strip).
Usage: python3 measure.py [toolchain]   (default: the pinned one)."""
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
BINS = ["base", "decode", "encode", "encode_all", "both"]
tc = [f"+{sys.argv[1]}"] if len(sys.argv) > 1 else []


def sections(path: Path) -> dict[str, int]:
    out = subprocess.run(["size", "-A", str(path)], capture_output=True, text=True, check=True).stdout
    sec = {}
    for line in out.splitlines():
        f = line.split()
        if len(f) >= 2 and f[0].startswith("."):
            sec[f[0]] = int(f[1])
    return sec


shutil.rmtree(HERE / "target", ignore_errors=True)
rows = []
for b in BINS:
    t = time.time()
    subprocess.run(["cargo", *tc, "build", "--release", "--bin", b], cwd=HERE, check=True, capture_output=True)
    secs = time.time() - t
    p = HERE / "target" / "release" / b
    s = sections(p)
    rows.append((b, p.stat().st_size, s.get(".text", 0), s.get(".rodata", 0), s.get(".data.rel.ro", 0), secs))
base = rows[0]
print(subprocess.run(["rustc", *tc, "--version"], capture_output=True, text=True).stdout.strip())
print("| consumer | stripped ELF | Δ over base | .text | Δ .text | .rodata | Δ .rodata | build s |")
print("|---|---:|---:|---:|---:|---:|---:|---:|")
for name, size, text, ro, _rel, secs in rows:
    print(f"| {name} | {size} | {size - base[1]} | {text} | {text - base[2]} | {ro} | {ro - base[3]} | {secs:.1f} |")
