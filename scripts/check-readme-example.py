#!/usr/bin/env python3
"""Opt-in release check: run the README example against its published crate.

Builds one disposable application under target/ and shares Cargo's build
cache. Registry access may be needed; --offline uses only cached packages.
This is intentionally not part of ordinary cargo test or the package.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--readme", type=Path, default=ROOT / "README.md")
    parser.add_argument("--msrv", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    text = args.readme.read_text(encoding="utf-8")
    version = re.findall(r"cargo add syom@([0-9]+\.[0-9]+\.[0-9]+)", text)
    examples = re.findall(r"```rust\n(.*?)```", text, flags=re.S)
    expected = re.findall(r"Expected output: `([^`]+)`", text)
    if len(version) != 1 or len(examples) != 1 or len(expected) != 1:
        raise ValueError("expected one published cargo-add version, Rust example and output")

    work = ROOT / "target" / "readme-example-check"
    (work / "src").mkdir(parents=True, exist_ok=True)
    manifest = work / "Cargo.toml"
    manifest.write_text(
        '[package]\nname = "syom-readme-example-check"\nversion = "0.0.0"\n'
        'edition = "2021"\npublish = false\n[workspace]\n[dependencies]\n'
        f'syom = "={version[0]}"\n', encoding="utf-8"
    )
    (work / "src/main.rs").write_text(examples[0], encoding="utf-8")
    cargo = ["cargo"]
    if args.msrv:
        msrv = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["rust-version"]
        cargo.append("+" + (msrv + ".0" if msrv.count(".") == 1 else msrv))
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    env.setdefault("CARGO_INCREMENTAL", "0")

    def run(arguments: list[str]) -> subprocess.CompletedProcess[str]:
        command = cargo + arguments + ["--manifest-path", str(manifest)]
        if args.offline:
            command.append("--offline")
        result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, encoding="utf-8")
        if result.returncode:
            sys.stderr.write(result.stdout + result.stderr)
            raise RuntimeError(f"command failed: {command}")
        return result

    run(["generate-lockfile"])
    # A same-version app must not reuse code from a previous README check.
    run(["clean", "--release", "-p", "syom-readme-example-check"])
    result = run(["run", "--release", "--locked", "--quiet"])
    if result.stdout.strip() != expected[0]:
        raise ValueError(f"expected {expected[0]!r}, got {result.stdout.strip()!r}")
    print(json.dumps({
        "registry_version": version[0], "msrv": args.msrv,
        "example_sha256": hashlib.sha256(examples[0].encode()).hexdigest(),
        "stdout": result.stdout.strip(), "result": "pass",
    }))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error
