#!/usr/bin/env python3
"""Prepare (and optionally execute) exact native selected-channel differentials.

No qualified consumer source, adapter.patch, model, or historical artifact changes.
The oracle directly indexes real SDK planes; the generated fixture matrix is native
only and does not establish public ASR/resampler/error-budget behavior.
"""
import argparse
import datetime
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

SDK_REVISION = "3e540231c9d71e3534be1ef856a91cf419efaead"
PACKAGE = "syom-channel-selection-proof"
SOURCES = ["aac.rs", "aac/adts_pull.rs", "aac/pull.rs", "aac/input.rs", "aac/buffered.rs"]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--consumer", type=Path, default=Path("lab/release-audit/runs/task145-20261105/candidate5/reproduction/consumer"))
    parser.add_argument("--output", type=Path, default=Path("target/task145-review/channels"))
    parser.add_argument("--target-dir", type=Path, default=Path("target/task145/native-stages-target"))
    parser.add_argument("--execute", action="store_true", help="Build cached isolated lab and run 48 native cells on CPUs14-17/jobs2")
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    root = here.parents[3]
    audio = args.consumer.resolve() / "crates/gigastt-core/src/inference/audio"
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    snapshot = output / "src/snapshot"
    (snapshot / "aac").mkdir(parents=True)
    adapter = here.parent / "adapter-manifest.json"
    adapter_data = json.loads(adapter.read_text())
    if adapter_data["syom"]["rev"] != SDK_REVISION:
        raise ValueError("Adapter manifest SDK revision differs from the pinned oracle SDK")
    hashes = {}
    for name in SOURCES:
        path = audio / name
        expected = adapter_data["files"]["crates/gigastt-core/src/inference/audio/" + name]
        data = path.read_bytes()
        actual = hashlib.sha256(data).hexdigest()
        if actual != expected:
            raise ValueError(f"Qualified source mismatch: {name}")
        (snapshot / name).write_bytes(data)
        hashes[name] = actual
    aac = (snapshot / "aac.rs").read_text()
    start = aac.index("fn append_plane(")
    helpers = aac[start:aac.index("\n#[cfg(test)]", start)]
    template = here / "channel_selection.rs.in"
    source = template.read_text().replace("@@HELPERS@@", helpers)
    for name in ["adts_pull", "pull", "input", "buffered"]:
        source = source.replace("@@" + name.upper() + "@@", json.dumps(str(snapshot / "aac" / (name + ".rs"))))
    (output / "src/main.rs").write_text(source)
    (output / "Cargo.toml").write_text(here.joinpath("adts_cold_stack.toml.in").read_text().replace("syom-adts-cold-stack-proof", PACKAGE))
    (output / "Cargo.lock").write_text(here.joinpath("adts_cold_stack.lock").read_text().replace('name = "syom-adts-cold-stack-proof"', f'name = "{PACKAGE}"'))
    metadata = {
        "sdk_git_revision": SDK_REVISION,
        "consumer": str(args.consumer.resolve()),
        "qualified_source_sha256": hashes,
        "adapter_manifest_sha256": digest(adapter),
        "template_sha256": digest(template),
        "preparer_sha256": digest(Path(__file__)),
        "python": {"version": sys.version, "optimization_level": sys.flags.optimize,
                   "invocation": [sys.executable, *sys.argv]},
        "generated_oracle_driver_sha256": digest(output / "src/main.rs"),
        "helpers_sha256": hashlib.sha256(helpers.encode()).hexdigest(),
        "scope": "Native selected plane only; exact copied producers/helpers, direct SDK plane[index] oracle, 24*1024 cuts; no public ASR/resampler/budget claims",
        "commands": [],
        "prepared_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }
    report = output / "provenance.json"

    def save():
        report.write_text(json.dumps(metadata, indent=2) + "\n")

    save()
    if not args.execute:
        print(output / "Cargo.toml")
        return
    env = os.environ.copy()
    for name in list(env):
        if name.startswith("CARGO_PROFILE_RELEASE_") or name == "CARGO_ENCODED_RUSTFLAGS":
            env.pop(name)
    env.update(CARGO_TARGET_DIR=str(args.target_dir.resolve()), CARGO_BUILD_JOBS="2", RUSTFLAGS="")
    metadata["environment_override"] = {name: env[name] for name in ["CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "RUSTFLAGS"]}
    metadata["compiler"] = subprocess.check_output(["rustup", "run", "stable", "rustc", "-Vv"], text=True)
    if not metadata["compiler"].startswith("rustc 1.98.1 "):
        raise ValueError("Use the qualified stable compiler")

    def run(command, log_name):
        start_time = time.monotonic()
        result = subprocess.run(["taskset", "-c", "14-17"] + command, cwd=root, env=env, capture_output=True, text=True)
        (output / log_name).write_text(result.stdout + result.stderr)
        metadata["commands"].append({"command": ["taskset", "-c", "14-17"] + command,
                                     "exit_code": result.returncode, "seconds": time.monotonic() - start_time, "log": log_name})
        save()
        if result.returncode:
            raise SystemExit(f"Native channel proof failed; retained {output / log_name}")
        return result.stdout

    run(["cargo", "+stable", "build", "--release", "--locked", "--offline", "--manifest-path", str(output / "Cargo.toml")], "build.log")
    packages = json.loads(run(["cargo", "+stable", "metadata", "--format-version", "1", "--locked", "--offline", "--manifest-path", str(output / "Cargo.toml")], "cargo-metadata.json"))["packages"]
    sdk = next(package for package in packages if package["name"] == "syom")
    if sdk["version"] != "0.10.1" or not sdk["source"].endswith("#" + SDK_REVISION):
        raise ValueError("Resolved SDK version/revision differs from the qualified oracle")
    sdk_root = Path(sdk["manifest_path"]).parent
    metadata["resolved_sdk"] = {"package_id": sdk["id"], "source_sha256": {
        str(path.relative_to(sdk_root)): digest(path) for path in sorted((sdk_root / "src").rglob("*.rs"))}}
    binary = args.target_dir.resolve() / "release" / PACKAGE
    metadata["binary_sha256"] = digest(binary)
    fixtures = json.loads(run([str(binary), "generate", str(output / "fixtures")], "generate.log"))
    for fixture in fixtures:
        fixture["sha256"] = digest(Path(fixture["path"]))
    metadata["fixtures"] = fixtures
    cells = []
    dump_dir = output / "native-dumps"
    dump_dir.mkdir()
    for fixture in fixtures:
        for reader in ["file", "bytes"]:
            for index in [0, 1, 8]:
                name = f"{fixture['profile']}-{fixture['transport']}-{reader}-one{index}"
                prefix = dump_dir / name
                command = [str(binary), "check", fixture["path"], fixture["profile"], fixture["transport"], reader, str(index), str(prefix)]
                cell = json.loads(run(command, name + ".log"))
                cell["fixture_sha256"] = fixture["sha256"]
                cell["oracle_sha256"] = digest(Path(str(prefix) + ".oracle"))
                cell["actual_sha256"] = digest(Path(str(prefix) + ".actual"))
                if cell["pass"] is not True or cell["oracle_sha256"] != cell["actual_sha256"]:
                    raise ValueError(f"Selected-channel oracle mismatch: {name}")
                cells.append(cell)
                (output / "result.json").write_text(json.dumps({"pass": False, "cells": cells}, indent=2) + "\n")
    if len(cells) != 48:
        raise ValueError(f"Expected 48 proof cells, got {len(cells)}")
    if any(digest(audio / name) != expected for name, expected in hashes.items()):
        raise ValueError("Frozen source changed during proof")
    metadata["finished_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    metadata["source_stable"] = True
    save()
    (output / "result.json").write_text(json.dumps({"pass": True, "cells_passed": 48, "cells": cells}, indent=2) + "\n")
    print(json.dumps({"pass": True, "cells_passed": 48, "result": str(output / "result.json")}))


if __name__ == "__main__":
    main()
