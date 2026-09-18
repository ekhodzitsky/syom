#!/usr/bin/env python3
"""TASK-99: audit the packaged crate (not the workspace manifest).

Checks, from `cargo package --list` and the normalized manifest cargo
writes into the package:
- only library sources, README, CHANGELOG, LICENSE and cargo's own files
  ship; no tests, goldens, benches, lab, corpus, backlog or tooling;
- `[dependencies]` and `[build-dependencies]` are empty, there is no
  `build.rs` and no `links` key (no native build, link or download step);
- the compressed crate stays under the size budget.
Exit 1 on any violation. Needs network only for cargo's index update.
"""
from __future__ import annotations

import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CRATE_BUDGET_BYTES = 512 * 1024  # 355 KB measured on 2026-09-18
ALLOWED_TOP = {
    ".cargo_vcs_info.json", "Cargo.lock", "Cargo.toml", "Cargo.toml.orig",
    "CHANGELOG.md", "LICENSE", "README.md",
}


def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def main() -> int:
    errors: list[str] = []
    files = run("cargo", "package", "--list", "--allow-dirty").split()
    for f in files:
        if f in ALLOWED_TOP:
            continue
        if not (f.startswith("src/") and f.endswith(".rs")):
            errors.append(f"unexpected file in package: {f}")
        elif f.endswith("_tests.rs"):
            errors.append(f"test source in package: {f}")
    if "build.rs" in files:
        errors.append("build.rs ships: native build step")
    subprocess.run(["cargo", "package", "--allow-dirty", "--no-verify"], cwd=ROOT,
                   check=True, capture_output=True)
    meta = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = meta["package"]["version"]
    crate = ROOT / "target" / "package" / f"syom-{version}.crate"
    unpacked = ROOT / "target" / "package" / f"syom-{version}" / "Cargo.toml"
    subprocess.run(["tar", "-xzf", str(crate), "-C", str(crate.parent)], check=True)
    packed = tomllib.loads(unpacked.read_text())
    for table in ("dependencies", "build-dependencies"):
        if packed.get(table):
            errors.append(f"packaged manifest has [{table}]: {sorted(packed[table])}")
    for key in ("links", "build"):
        if packed["package"].get(key):
            errors.append(f"packaged manifest sets package.{key}")
    if re.search(r"^\[target\.", unpacked.read_text(), re.M):
        errors.append("packaged manifest has target-specific tables")
    size = crate.stat().st_size
    if size > CRATE_BUDGET_BYTES:
        errors.append(f"crate is {size} bytes (budget {CRATE_BUDGET_BYTES})")
    if errors:
        print("FAIL: package audit")
        for e in errors:
            print(f"  - {e}")
        return 1
    print(f"OK: {len(files)} files, {size} bytes compressed (budget {CRATE_BUDGET_BYTES}), "
          f"rust-version {packed['package'].get('rust-version')}, no dependencies, no build script")
    return 0


if __name__ == "__main__":
    sys.exit(main())
