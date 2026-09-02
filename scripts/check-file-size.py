#!/usr/bin/env python3
"""Fail if a Rust source file exceeds the AGENTS.md line caps."""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IMPL_MAX = 400
TEST_MAX = 500


def is_test(path: Path) -> bool:
    return path.name.endswith("_tests.rs") or "tests" in path.parts


def is_vendored_aac(path: Path) -> bool:
    # ISO-BMFF demux and HE-AAC SBR/PS tables (QMF, Huffman, PS).
    # The LC engine is original and stays capped.
    name = path.name
    if name == "isomp4.rs":
        return True
    prefixes = (
        "sbr_",
        "ps_",
        "extension_payload",
        "crc",
        "latm",
    )
    return name.startswith(prefixes) or name in {
        "extension_payload.rs",
        "crc.rs",
        "latm.rs",
    }


def line_count(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    if text == "":
        return 0
    n = text.count("\n")
    if not text.endswith("\n"):
        n += 1
    return n


def main() -> int:
    failed = False
    files = sorted(ROOT.glob("src/**/*.rs"))
    for path in files:
        if is_vendored_aac(path):
            continue
        n = line_count(path)
        cap = TEST_MAX if is_test(path) else IMPL_MAX
        rel = path.relative_to(ROOT)
        if n > cap:
            print(f"FAIL: {rel} has {n} lines (max {cap})")
            failed = True
    if failed:
        return 1
    print("OK: all Rust sources within line caps")
    return 0


if __name__ == "__main__":
    sys.exit(main())
