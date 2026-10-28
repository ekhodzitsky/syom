#!/usr/bin/env python3
"""Run bounded independent fuzz workers, preserving outputs and exit status."""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import resource
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--seconds", type=int, default=180)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--asan-bin", type=Path)
    args = parser.parse_args()
    if args.seconds < 1:
        parser.error("--seconds must be positive")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    jobs = [("release", kind, seed) for kind in ["syom_fuzz", "syom_fuzz_state"]
            for seed in [2801, 2802]]
    if args.asan_bin:
        jobs += [("asan", kind, 2803) for kind in ["syom_fuzz", "syom_fuzz_state"]]

    def run(job):
        mode, kind, seed = job
        name = f"{mode}-{kind}-{seed}"
        folder = (args.asan_bin.resolve() if mode == "asan" else ROOT / "lab/fuzz/target/release")
        cmd = [str(folder / kind), "--seconds", str(args.seconds), "--iters", "1000000000",
               "--seed", str(seed)]
        if kind == "syom_fuzz":
            cmd += ["--crash-dir", str(out / "crashes" / name)]
        started = time.monotonic()
        with (out / f"{name}.log").open("w") as log:
            try:
                result = subprocess.run(cmd, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT,
                                        env={**os.environ, "ASAN_OPTIONS": "detect_leaks=1"},
                                        timeout=args.seconds + 60)
                status = result.returncode
            except subprocess.TimeoutExpired:
                status = "timeout"
        return {"name": name, "command": cmd, "exit_code": status,
                "wall_seconds": round(time.monotonic() - started, 3)}

    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(jobs)) as pool:
        results = list(pool.map(run, jobs))
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    summary = {"seconds_per_worker": args.seconds, "workers": results,
               "child_cpu_seconds": after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime}
    (out / "fuzz-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    return int(any(r["exit_code"] != 0 for r in results))


if __name__ == "__main__":
    raise SystemExit(main())
