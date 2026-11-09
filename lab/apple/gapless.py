#!/usr/bin/env python3
"""Native Apple gapless interoperability, isolated from ordinary cargo tests."""
from __future__ import annotations

import argparse
import array
import hashlib
import json
import math
from pathlib import Path
import platform
import plistlib
import struct
import subprocess
import sys
import wave

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "lab/apple/gapless/Cargo.toml"


def boxes(data: bytes, start: int = 0, end: int | None = None):
    end = len(data) if end is None else end
    if not 0 <= start <= end <= len(data):
        raise ValueError("invalid parent bounds")
    while start < end:
        if end - start < 8:
            if any(data[start:end]):
                raise ValueError("nonzero trailing box bytes")
            return
        size = struct.unpack_from(">I", data, start)[0]
        typ = data[start + 4:start + 8]
        header = 8
        if size == 1:
            if end - start < 16:
                raise ValueError("truncated large header")
            size = struct.unpack_from(">Q", data, start + 8)[0]
            header = 16
        elif size == 0:
            size = end - start
        if size < header or start + size > end:
            raise ValueError("box exceeds its parent")
        yield typ, start + header, start + size, start + 4
        start += size


def path(data: bytes, names: tuple[bytes, ...], start=0, end=None):
    spans = [(start, len(data) if end is None else end, 0)]
    for name in names:
        spans = [(a, b, at) for lo, hi, _ in spans
                 for typ, a, b, at in boxes(data, lo, hi) if typ == name]
    return spans


def inspect(data: bytes):
    tags = []
    for lo, hi, _ in path(data, (b"moov", b"udta", b"meta")):
        if data[lo:lo + 4] != bytes(4):
            raise ValueError("unsupported meta version")
        for a, b, _ in path(data, (b"ilst", b"----"), lo + 4, hi):
            fields = {}
            for typ, x, y, _ in boxes(data, a, b):
                if typ in (b"mean", b"name", b"data"):
                    if typ in fields:
                        raise ValueError("duplicate freeform field")
                    fields[typ] = data[x:y]
            if (fields.get(b"mean") != bytes(4) + b"com.apple.iTunes"
                    or fields.get(b"name") != bytes(4) + b"iTunSMPB"):
                continue
            value = fields[b"data"]
            if len(value) < 8:
                raise ValueError("short data field")
            text = value[8:].rstrip(b"\0").decode("ascii")
            words = text.split()
            if [len(s) for s in words[:4]] != [8, 8, 8, 16]:
                raise ValueError("unexpected sample field widths")
            reserved, priming, remainder, samples = [int(s, 16) for s in words[:4]]
            if reserved != 0:
                raise ValueError("nonzero reserved field")
            tags.append(dict(data_type=int.from_bytes(value[:4], "big"),
                             text=text, priming=priming, remainder=remainder,
                             samples=samples))
    if len(tags) != 1:
        raise ValueError(f"expected one native iTunSMPB item, got {len(tags)}")
    # Renaming preserves all byte offsets and leaves the native tag untouched.
    result = bytearray(data)
    edits = path(data, (b"moov", b"trak", b"edts", b"elst"))
    for _, _, at in edits:
        result[at:at + 4] = b"free"
    return tags[0], bytes(result), len(edits)


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_source(file: Path, rate: int, channels: int, samples: int):
    pcm = array.array("h", (round(9000 * math.sin(2 * math.pi * (380 + 233 * c)
                           * i / rate)) for i in range(samples) for c in range(channels)))
    if sys.byteorder != "little":
        pcm.byteswap()
    with wave.open(str(file), "wb") as stream:
        stream.setparams((channels, 2, rate, samples, "NONE", "not compressed"))
        stream.writeframes(pcm.tobytes())


def run(args: list[str], report: dict, *, allowed=(0,), include_stderr=False) -> str:
    print("RUN", " ".join(args), flush=True)
    proc = subprocess.run(args, cwd=ROOT, capture_output=True, text=True)
    report["commands"].append(dict(args=args, exit_code=proc.returncode,
                                   allowed_exit_codes=list(allowed),
                                   stdout=proc.stdout, stderr=proc.stderr))
    if proc.returncode not in allowed:
        raise RuntimeError(f"command failed: {args}: {proc.stderr[-4000:]}")
    return proc.stdout + proc.stderr if include_stderr else proc.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/apple-gapless")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    report = dict(platform=platform.platform(), machine=platform.machine(),
                  native_executed=False, status="running", commands=[], cells=[])
    try:
        cargo = ["cargo", "run", "--release", "--locked", "--manifest-path", str(MANIFEST), "--"]
        if args.self_test:
            report["self_test"] = json.loads(run(cargo + ["--self-test"], report))
        else:
            if platform.system() != "Darwin":
                raise RuntimeError("native execution requires macOS; use --self-test on Linux")
            report["sw_vers"] = run(["sw_vers"], report)
            # Native help exits 2 and writes its valid listing to stderr.
            report["afconvert_formats"] = run(["afconvert", "-hf"], report,
                                               allowed=(0, 2), include_stderr=True)
            if "Audio file and data formats:" not in report["afconvert_formats"]:
                raise ValueError("afconvert help did not return a format listing")
            report["afconvert_sha256"] = digest(Path("/usr/bin/afconvert").read_bytes())
            info = Path("/System/Library/Frameworks/AudioToolbox.framework/Resources/Info.plist")
            report["audio_toolbox"] = plistlib.loads(info.read_bytes()) if info.exists() else None
            run(["rustc", "-vV"], report)
            for rate, channels, samples, bitrate in [
                (44100, 2, 352800, 256000), (48000, 1, 48013, 128000),
            ]:
                name = f"lc-{rate}-{channels}ch"
                source = out / f"{name}.wav"
                encoded = out / f"{name}.m4a"
                patched = out / f"{name}-no-elst.m4a"
                write_source(source, rate, channels, samples)
                run(["afconvert", "-f", "m4af", "-d", "aac", "-b", str(bitrate),
                     str(source), str(encoded)], report)
                report["native_executed"] = True
                afinfo = run(["afinfo", str(encoded)], report)
                tag, data, edits = inspect(encoded.read_bytes())
                if tag["samples"] != samples or tag["priming"] <= 0:
                    raise ValueError("native counts do not describe the source")
                patched.write_bytes(data)
                cell = dict(name=name, rate=rate, channels=channels, samples=samples,
                            tag=tag, edits_removed=edits, afinfo=afinfo,
                            source_sha256=digest(source.read_bytes()),
                            encoded_sha256=digest(encoded.read_bytes()),
                            tested_sha256=digest(data))
                report["cells"].append(cell)
                cell["consumer"] = json.loads(run(cargo + [str(patched), str(tag["priming"]),
                    str(tag["remainder"]), str(samples), str(rate)], report))
                if cell["consumer"]["channels"] != channels:
                    raise ValueError("consumer changed the native channel count")
        report["status"] = "passed"
    except Exception as error:
        report["status"] = "failed"
        report["error"] = str(error)
        raise
    finally:
        # Only this report is uploaded; generated recordings stay in ignored output.
        (out / "report.json").write_text(json.dumps(report, indent=2, default=str) + "\n")
    print(json.dumps(dict(status=report["status"], native_executed=report["native_executed"],
                         cells=len(report["cells"]))))


if __name__ == "__main__":
    main()
