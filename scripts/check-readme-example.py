#!/usr/bin/env python3
"""Compile README Rust examples against the pinned published archive.

Run self-contained examples; rust,no_run blocks remain compile-only.
Disposable apps stay under target/ and share Cargo\'s build cache. Registry
access may be needed; --offline uses cached packages. This separate CI check
is not part of ordinary cargo test or the published package.
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


def examples_from_readme(text: str):
    versions = re.findall(r"cargo add syom@([0-9]+\.[0-9]+\.[0-9]+)", text)
    blocks = re.findall(r"^```rust([^\n]*)\n(.*?)^```", text, flags=re.S | re.M)
    expected = re.findall(r"Expected output: `([^`]+)`", text)
    starts = re.findall(r"^```rust[^\n]*\n", text, flags=re.M)
    if len(starts) != len(blocks):
        raise ValueError("unclosed Rust example")
    if len(versions) != 1 or not blocks or len(expected) != 1:
        raise ValueError("expected one published cargo-add version, Rust examples and primary output")
    examples = []
    for attributes, code in blocks:
        if attributes.strip() not in ("", ",no_run") or not code.strip():
            raise ValueError(f"unsupported or empty Rust example: {attributes!r}")
        examples.append((code, attributes.strip() != ",no_run"))
    if not examples[0][1]:
        raise ValueError("the first example must run")
    return versions[0], examples, expected[0]


def registry_dependency(lock: dict, version: str) -> dict:
    packages = [p for p in lock.get("package", []) if p["name"] == "syom"]
    source = "registry+https://github.com/rust-lang/crates.io-index"
    if (len(packages) != 1 or packages[0]["version"] != version
            or packages[0].get("source") != source or not packages[0].get("checksum")):
        raise ValueError("resolved syom is not the pinned crates.io archive")
    return packages[0]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--readme", type=Path, default=ROOT / "README.md")
    parser.add_argument("--msrv", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    text = args.readme.read_text(encoding="utf-8")
    version, examples, expected = examples_from_readme(text)
    identity = hashlib.sha256((version + repr(examples)).encode()).hexdigest()
    work = ROOT / "target" / "readme-example-check" / identity[:16]
    (work / "src").mkdir(parents=True, exist_ok=True)
    manifest = work / "Cargo.toml"
    manifest.write_text(
        '[package]\nname = "syom-readme-example-check"\nversion = "0.0.0"\n'
        'edition = "2021"\npublish = false\nautobins = false\n[workspace]\n[dependencies]\n'
        f'syom = "={version}"\n' + "".join(
            f'[[bin]]\nname = "example-{i}"\npath = "src/example-{i}.rs"\n'
            for i in range(1, len(examples) + 1)
        ), encoding="utf-8"
    )
    for i, (code, _) in enumerate(examples, 1):
        (work / f"src/example-{i}.rs").write_text(code, encoding="utf-8")
    cargo = ["cargo"]
    if args.msrv:
        msrv = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["rust-version"]
        cargo.append("+" + (msrv + ".0" if msrv.count(".") == 1 else msrv))
    env = os.environ.copy()
    env.setdefault("CARGO_TARGET_DIR", str(ROOT / "target"))
    if not Path(env["CARGO_TARGET_DIR"]).is_absolute():
        env["CARGO_TARGET_DIR"] = str((ROOT / env["CARGO_TARGET_DIR"]).resolve())
    env.setdefault("CARGO_INCREMENTAL", "0")

    def run(arguments: list[str]) -> subprocess.CompletedProcess[str]:
        command = cargo + arguments + ["--manifest-path", str(manifest)]
        if args.offline:
            command.append("--offline")
        result = subprocess.run(command, cwd=work, env=env, capture_output=True, encoding="utf-8")
        if result.returncode:
            sys.stderr.write(result.stdout + result.stderr)
            raise RuntimeError(f"command failed: {command}")
        return result

    run(["generate-lockfile"])
    dependency = registry_dependency(tomllib.loads((work / "Cargo.lock").read_text()), version)
    # A same-version app must not reuse code from a previous README check.
    run(["clean", "--release", "-p", "syom-readme-example-check"])
    run(["check", "--bins", "--locked"])
    executed = []
    for i, (_, execute) in enumerate(examples, 1):
        if not execute:
            continue
        result = run(["run", "--bin", f"example-{i}", "--release", "--locked", "--quiet"])
        output = result.stdout.strip()
        wanted = expected if i == 1 else ""
        if output != wanted:
            raise ValueError(f"example {i}: expected {wanted!r}, got {output!r}")
        executed.append({"example": i, "stdout": output})
    print(json.dumps({
        "registry_version": version, "registry_source": dependency["source"],
        "registry_checksum": dependency["checksum"], "msrv": args.msrv,
        "example_set_sha256": identity, "compiled_examples": len(examples),
        "executed": executed, "result": "pass",
    }))
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"FAIL: {error}", file=sys.stderr)
        raise SystemExit(1) from error
