#!/usr/bin/env python3
"""Opt-in FAAC encode smoke. Not invoked by cargo test --workspace."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def main() -> int:
    driver = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("faac_driver")
    verify = ROOT / "lab/faac/verify/target/release/faac_verify"
    if not driver.is_file():
        print("missing driver; see lab/faac/PIN.md", file=sys.stderr)
        return 2
    print(subprocess.check_output([str(driver), "id"], text=True).strip())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
