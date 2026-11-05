#!/usr/bin/env python3
"""Prepare native decode/mix and crossed PCM/cadence resampling attribution."""
import argparse
import hashlib
import json
import shutil
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--consumer", type=Path, default=Path("target/task145/gigastt"))
    parser.add_argument("--output", type=Path, default=Path("target/task145/native-stages-repro"))
    args = parser.parse_args()
    here = Path(__file__).resolve().parent
    root = here.parents[3]
    audio = args.consumer.resolve() / "crates/gigastt-core/src/inference/audio"
    output = args.output.resolve()
    snapshot = output / "src/snapshot"
    snapshot.mkdir(parents=True, exist_ok=True)
    source = here.joinpath("native_stages.rs.in").read_text()
    aac = audio.joinpath("aac.rs").read_text()
    helpers = aac[aac.index("fn append_plane("):aac.index("\n#[cfg(test)]\nmod mono_plane_tests")]
    source = source.replace("@@HELPERS@@", helpers)
    for module in ["buffered", "input", "pull"]:
        path = snapshot / (module + ".rs")
        shutil.copyfile(audio / "aac" / path.name, path)
        source = source.replace("@@" + module.upper() + "@@", json.dumps(str(path)))
    for name, origin in [("resample_original.rs", audio / "resample.rs"),
                         ("file_original.rs", audio / "stream/file.rs"),
                         ("aac_parent_original.rs", audio / "aac.rs")]:
        shutil.copyfile(origin, snapshot / name)
    output.joinpath("src/main.rs").write_text(source)
    output.joinpath("Cargo.toml").write_text(
        '[package]\nname="syom-native-stages"\nversion="0.0.0"\nedition="2024"\npublish=false\n'
        '[workspace]\n[dependencies]\nsyom={path=' + json.dumps(str(root)) + '}\n'
        'anyhow="=1.0.104"\nbytes="=1.12.1"\nrubato="=5.0.1"\nserde_json="1"\n'
        'audioadapter-buffers="=5.1.0"\naudioadapter-sample="=5.1.0"\nrealfft="=3.5.0"\n'
        'symphonia={version="=0.6.1",features=["aac","isomp4","mkv","mp3","ogg","flac","pcm","wav"]}\n'
        '[features]\ndefault=["file-decode"]\nfile-decode=[]\n'
        '[profile.release]\nlto=true\ncodegen-units=1\nopt-level=3\n')
    output.joinpath("Cargo.lock").write_bytes(here.joinpath("native_stages.lock").read_bytes())
    output.joinpath("source-manifest.json").write_text(json.dumps(
        {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in snapshot.iterdir()}, indent=2) + "\n")
    print(output / "Cargo.toml")


if __name__ == "__main__":
    main()
