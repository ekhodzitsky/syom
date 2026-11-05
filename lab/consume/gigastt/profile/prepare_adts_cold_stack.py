#!/usr/bin/env python3
"""Prepare exact native-only cold ADTS producer proof; no build or decode runs."""
import argparse
import hashlib
import json
import shutil
from pathlib import Path

SDK_REVISION = "3e540231c9d71e3534be1ef856a91cf419efaead"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--consumer", type=Path, default=Path("target/task145/gigastt"))
    parser.add_argument("--output", type=Path, default=Path("target/task145/adts-cold-stack-repro"))
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    root = here.parents[3]
    audio = args.consumer.resolve() / "crates/gigastt-core/src/inference/audio"
    output = args.output.resolve()
    snapshot = output / "src/snapshot"
    snapshot.joinpath("aac").mkdir(parents=True, exist_ok=True)
    for name in ["aac.rs", "aac/adts_pull.rs", "aac/input.rs", "aac/buffered.rs"]:
        shutil.copyfile(audio / name, snapshot / name)
    parent = snapshot.joinpath("aac.rs").read_text()
    helpers = parent[parent.index("fn append_plane("):parent.index("\n#[cfg(test)]\nmod mono_plane_tests")]
    source = here.joinpath("adts_cold_stack.rs.in").read_text().replace("@@HELPERS@@", helpers)
    for module in ["adts_pull", "input", "buffered"]:
        source = source.replace("@@" + module.upper() + "@@", json.dumps(str(snapshot / "aac" / (module + ".rs"))))
    output.joinpath("src/main.rs").write_text(source)
    output.joinpath("Cargo.toml").write_bytes(here.joinpath("adts_cold_stack.toml.in").read_bytes())
    output.joinpath("Cargo.lock").write_bytes(here.joinpath("adts_cold_stack.lock").read_bytes())
    output.joinpath("source-manifest.json").write_text(json.dumps({
        str(path.relative_to(snapshot)): digest(path) for path in sorted(snapshot.rglob("*.rs"))
    }, indent=2) + "\n")
    fixtures = {}
    for profile, name in [("lc", "sine48.adts"), ("he1", "he48e.adts"),
                          ("ps", "ps48.adts"), ("implicit-ps", "he48.adts")]:
        path = root / "src/goldens" / name
        raw = path.read_bytes()
        skip = 0
        if raw.startswith(b"ID3"):
            assert len(raw) >= 10 and all(byte < 128 for byte in raw[6:10])
            skip = 10 + sum(byte << (7 * (3 - i)) for i, byte in enumerate(raw[6:10]))
            if raw[5] & 16:
                skip += 10
            assert skip <= len(raw)
        fixtures[profile] = {"path": str(path), "original_bytes": len(raw),
                             "original_sha256": hashlib.sha256(raw).hexdigest(),
                             "stripped_id3_bytes": skip, "bare_bytes": len(raw) - skip,
                             "bare_sha256": hashlib.sha256(raw[skip:]).hexdigest()}
    output.joinpath("preparation.json").write_text(json.dumps({
        "sdk_git_revision": SDK_REVISION, "fixtures": fixtures,
        "note": "Preparation only; byte parsing/hashing, no encoder/probe/decoder initialization."
    }, indent=2) + "\n")
    print(output / "Cargo.toml")


if __name__ == "__main__":
    main()
