#!/usr/bin/env python3
"""TASK-102: build and run the consumer cells against the *packaged* crate.

1. `cargo package` syom (no verify; the consumer build below compiles the
   packaged sources itself).
2. Unpack the `.crate` into lab/consume/vendor/syom (gitignored).
3. Audit the packaged manifest (no dependencies, no build script).
4. `cargo run --release` every consumer binary, streaming its output.

Ordinary workspace tests never build this lab; no external codec runs here.
Needs network only if cargo decides to refresh a registry index (the crate
has no dependencies, so in practice it does not).
"""
from __future__ import annotations

import hashlib
import shutil
import subprocess
import sys
import tarfile
import tomllib
from pathlib import Path

LAB = Path(__file__).resolve().parent
ROOT = LAB.parents[1]
BINS = ["flows", "consumers"]


def run(*args: str, cwd: Path) -> str:
    proc = subprocess.run(args, cwd=cwd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stdout.write(proc.stdout)
        sys.stderr.write(proc.stderr)
        raise SystemExit(f"FAIL: {' '.join(args)} exited {proc.returncode}")
    return proc.stdout


def main() -> int:
    run("cargo", "package", "--allow-dirty", "--no-verify", cwd=ROOT)
    meta = tomllib.loads((ROOT / "Cargo.toml").read_text())
    version = meta["package"]["version"]
    crate = ROOT / "target" / "package" / f"syom-{version}.crate"
    digest = hashlib.sha256(crate.read_bytes()).hexdigest()

    vendor = LAB / "vendor"
    shutil.rmtree(vendor / "syom", ignore_errors=True)
    (vendor / "syom").mkdir(parents=True)
    with tarfile.open(crate) as tar:
        for member in tar.getmembers():
            # Strip the syom-<version>/ prefix; refuse path escapes.
            parts = Path(member.name).parts
            if len(parts) < 2 or parts[0] != f"syom-{version}":
                raise SystemExit(f"unexpected crate member: {member.name}")
            member.name = str(Path(*parts[1:]))
            if member.name == ".":
                continue
            tar.extract(member, vendor / "syom", filter="data")

    packed = tomllib.loads((vendor / "syom" / "Cargo.toml").read_text())
    assert not packed.get("dependencies"), "packaged crate gained a dependency"
    assert not packed.get("build-dependencies"), "packaged crate gained a build dependency"
    assert not packed["package"].get("build"), "packaged crate gained a build script"
    print(f"packaged syom {version}: {crate.stat().st_size} bytes, sha256 {digest[:16]}…")
    print(f"packaged manifest: no dependencies, no build.rs, "
          f"rust-version {packed['package'].get('rust-version')}")

    run("cargo", "build", "--release", cwd=LAB)  # fail early on a dep drift
    for name in BINS:
        print(f"--- {name} ---")
        sys.stdout.write(run("cargo", "run", "--release", "--bin", name, cwd=LAB))
    print("OK: all consumer cells passed against the packaged crate")
    return 0


if __name__ == "__main__":
    sys.exit(main())
