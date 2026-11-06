#!/usr/bin/env python3
"""TASK-99: audit the original product manifest and packaged library boundary.

Checks the original manifest, `cargo package --list` and the normalized
manifest cargo writes into the package:
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
    "CHANGELOG.md", "LICENSE", "NOTICE", "README.md",
}


def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, check=True, capture_output=True, text=True).stdout


def manifest_errors(text: str, *, build_rs_present: bool = False,
                    label: str = "manifest") -> list[str]:
    """Check the product boundary semantically, including the original manifest."""
    try:
        manifest = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        return [f"{label}: invalid TOML: {error}"]
    errors = []
    package = manifest.get("package")
    if not isinstance(package, dict):
        errors.append(f"{label}: missing package table")
        package = {}
    for table in ("dependencies", "build-dependencies", "build_dependencies"):
        dependencies = manifest.get(table, {})
        if not isinstance(dependencies, dict):
            errors.append(f"{label}: [{table}] must be a table")
        elif dependencies:
            errors.append(f"{label} has [{table}]: {sorted(dependencies)}")
    if "links" in package:
        errors.append(f"{label} sets package.links")
    if "build" in package and package["build"] is not False:
        errors.append(f"{label} sets package.build")
    if "target" in manifest:
        errors.append(f"{label} has target-specific tables")
    if build_rs_present:
        errors.append(f"{label} contains build.rs: native build step")
    return errors


def audit_failure(errors: list[str]) -> int:
    print("FAIL: package audit")
    for error in errors:
        print(f"  - {error}")
    return 1


def main() -> int:
    original = (ROOT / "Cargo.toml").read_text()
    errors = manifest_errors(original, build_rs_present=(ROOT / "build.rs").exists(),
                             label="root manifest")
    if errors:
        return audit_failure(errors)
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
    meta = tomllib.loads(original)
    version = meta["package"]["version"]
    crate = ROOT / "target" / "package" / f"syom-{version}.crate"
    unpacked = ROOT / "target" / "package" / f"syom-{version}" / "Cargo.toml"
    subprocess.run(["tar", "-xzf", str(crate), "-C", str(crate.parent)], check=True)
    packed = tomllib.loads(unpacked.read_text())
    errors.extend(manifest_errors(unpacked.read_text(),
                                  build_rs_present=(unpacked.parent / "build.rs").exists(),
                                  label="packaged manifest"))
    # TASK-104: every shipped file is UTF-8 text (no binaries, fixtures or
    # comparator objects) and the MIT notice ships with it.
    for f in files:
        path = unpacked.parent / f
        try:
            path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, FileNotFoundError):
            errors.append(f"not a UTF-8 text file in package: {f}")
    lic = (unpacked.parent / "LICENSE").read_text() if (unpacked.parent / "LICENSE").is_file() else ""
    if "MIT License" not in lic or "Permission is hereby granted" not in lic:
        errors.append("LICENSE (MIT notice) missing from the package")
    notice = unpacked.parent / "NOTICE"
    if not notice.is_file() or "patent" not in notice.read_text().lower():
        errors.append("NOTICE (patent disclaimer the README links to) missing from the package")
    if packed["package"].get("license") != "MIT":
        errors.append("package.license is not MIT")
    # TASK-107: the guide's examples are self-contained; they must run from
    # the packaged sources (other doc tests need the repository's goldens).
    doc = subprocess.run(["cargo", "test", "--doc", "guide"], cwd=unpacked.parent,
                         capture_output=True, text=True)
    ran = re.search(r"test result: ok\. (\d+) passed; 0 failed", doc.stdout)
    if doc.returncode != 0 or not ran or int(ran.group(1)) < 8:
        errors.append("guide doc tests fail from the packaged crate:\n" + doc.stdout[-600:])
    size = crate.stat().st_size
    if size > CRATE_BUDGET_BYTES:
        errors.append(f"crate is {size} bytes (budget {CRATE_BUDGET_BYTES})")
    if errors:
        return audit_failure(errors)
    print(f"OK: {len(files)} files, {size} bytes compressed (budget {CRATE_BUDGET_BYTES}), "
          f"rust-version {packed['package'].get('rust-version')}, no dependencies, no build script")
    return 0


if __name__ == "__main__":
    sys.exit(main())
