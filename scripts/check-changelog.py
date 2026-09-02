#!/usr/bin/env python3
"""Fail if CHANGELOG.md drifts from workspace.package.version."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARGO = ROOT / "Cargo.toml"
LOG = ROOT / "CHANGELOG.md"


def package_version() -> str:
    text = CARGO.read_text(encoding="utf-8")
    in_pkg = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped in ("[workspace.package]", "[package]"):
            in_pkg = True
            continue
        if in_pkg and line.startswith("["):
            break
        if in_pkg:
            m = re.match(r'version\s*=\s*"([^"]+)"', stripped)
            if m:
                return m.group(1)
    raise SystemExit("FAIL: package version missing in Cargo.toml")


def main() -> int:
    if not LOG.is_file():
        print("FAIL: CHANGELOG.md is missing")
        return 1
    body = LOG.read_text(encoding="utf-8")
    if "## [Unreleased]" not in body:
        print("FAIL: CHANGELOG.md needs an ## [Unreleased] heading")
        return 1
    if "keepachangelog.com" not in body or "semver.org" not in body:
        print("FAIL: CHANGELOG.md must cite Keep a Changelog and SemVer")
        return 1
    ver = package_version()
    heading = f"## [{ver}]"
    if heading not in body:
        print(f"FAIL: CHANGELOG.md has no {heading} (Cargo.toml is {ver})")
        return 1
    print(f"OK: CHANGELOG.md has Unreleased and {heading}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
