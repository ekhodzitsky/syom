#!/usr/bin/env python3
"""Build static fdk-aac 2.0.3 the way lab/fdk/PIN.md describes.

Not invoked by cargo test. Needs FDK_SRC (the unpacked v2.0.3 tree) and
writes libfdk-aac.a to FDK_PREFIX (default: <this dir>/fdk-prefix).
Compiler is zig-c++ -target x86_64-linux-gnu -O2 -fno-exceptions -fno-rtti.
"""

import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

HERE = Path(__file__).resolve().parent
SRC = Path(os.environ["FDK_SRC"]).resolve()
PREFIX = Path(os.environ.get("FDK_PREFIX", HERE / "fdk-prefix")).resolve()
CXX = os.environ.get("CXX", "zig-c++")
JOBS = int(os.environ.get("JOBS", "8"))

INCS = [
    "libAACdec/include",
    "libAACenc/include",
    "libSYS/include",
    "libArithCoding/include",
    "libDRCdec/include",
    "libSACdec/include",
    "libSACenc/include",
    "libSBRdec/include",
    "libSBRenc/include",
    "libMpegTPDec/include",
    "libMpegTPEnc/include",
    "libFDK/include",
    "libPCMutils/include",
    "libAACdec/src",
    "libAACenc/src",
    "libSYS/src",
    "libFDK/src",
    "libSBRdec/src",
    "libSBRenc/src",
    "libSACdec/src",
    "libSACenc/src",
    "libMpegTPDec/src",
    "libMpegTPEnc/src",
    "libDRCdec/src",
    "libPCMutils/src",
    "libArithCoding/src",
]


def sources() -> list[str]:
    text = (SRC / "CMakeLists.txt").read_text()
    return re.findall(r"(lib\S+?\.cpp)", text)


def compile_one(cpp: str, obj_dir: Path) -> None:
    obj = obj_dir / (cpp.replace("/", "_") + ".o")
    cmd = [
        CXX,
        "-target",
        "x86_64-linux-gnu",
        "-O2",
        "-fno-exceptions",
        "-fno-rtti",
        "-Wno-date-time",
        "-c",
        str(SRC / cpp),
        "-o",
        str(obj),
    ]
    for inc in INCS:
        cmd.extend(["-I", str(SRC / inc)])
    subprocess.run(cmd, check=True)


def main() -> int:
    cpps = sources()
    if len(cpps) < 100:
        print(f"expected ~170 cpp files, found {len(cpps)}", file=sys.stderr)
        return 1
    obj_dir = PREFIX / "obj"
    lib_dir = PREFIX / "lib"
    inc_dir = PREFIX / "include"
    obj_dir.mkdir(parents=True, exist_ok=True)
    lib_dir.mkdir(parents=True, exist_ok=True)
    inc_dir.mkdir(parents=True, exist_ok=True)
    for name in ("aacdecoder_lib.h", "aacenc_lib.h", "FDK_audio.h", "genericStds.h", "machine_type.h", "syslib_channelMapDescr.h"):
        src = next(SRC.rglob(name))
        (inc_dir / name).write_bytes(src.read_bytes())
    print(f"compiling {len(cpps)} files with {JOBS} jobs")
    with ThreadPoolExecutor(max_workers=JOBS) as pool:
        futs = [pool.submit(compile_one, cpp, obj_dir) for cpp in cpps]
        for fut in as_completed(futs):
            fut.result()
    lib = lib_dir / "libfdk-aac.a"
    if lib.exists():
        lib.unlink()
    subprocess.run(["ar", "rcs", str(lib), *sorted(str(p) for p in obj_dir.glob("*.o"))], check=True)
    print(lib)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
