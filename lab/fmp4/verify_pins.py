#!/usr/bin/env python3
"""TASK-126 — verify the sha256 pins of GOLDENS.md against the committed
src/goldens/fmp4_* fixtures. Exit 1 on any mismatch."""

import hashlib
import os
import re
import sys

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
GOLD = os.path.join(ROOT, "src", "goldens")


def main():
    table = open(os.path.join(os.path.dirname(__file__), "GOLDENS.md")).read()
    pins = re.findall(r"\| `([a-z0-9_]+\.(?:mp4|m4a|m4s))` \| `([0-9a-f]{64})` \|",
                      table)
    assert pins, "no pins parsed from GOLDENS.md"
    ok = True
    for name, want in pins:
        path = os.path.join(GOLD, name)
        try:
            got = hashlib.sha256(open(path, "rb").read()).hexdigest()
        except FileNotFoundError:
            print(f"{name}: MISSING")
            ok = False
            continue
        if got != want:
            print(f"{name}: pin {want[:12]}.. != file {got[:12]}..")
            ok = False
    print(f"{len(pins)} pins {'OK' if ok else 'MISMATCH'}")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
