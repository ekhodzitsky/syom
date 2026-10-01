#!/usr/bin/env python3
"""Run one lab encoder command, print wall time and peak RSS as JSON.

One child per invocation: resource.getrusage(RUSAGE_CHILDREN).ru_maxrss is
the max over this process's children, so the value is the child's peak.
Not invoked by cargo test.
"""
import json
import resource
import subprocess
import sys
import time

t = time.perf_counter()
r = subprocess.run(sys.argv[1:], capture_output=True)
wall_ms = (time.perf_counter() - t) * 1e3
ru = resource.getrusage(resource.RUSAGE_CHILDREN)
last = r.stdout.decode(errors="replace").strip().splitlines()
print(json.dumps({
    "rc": r.returncode,
    "wall_ms": round(wall_ms, 2),
    "maxrss_kb": ru.ru_maxrss,
    "stdout": last[-1] if last else "",
}))
sys.exit(0 if r.returncode == 0 else 1)
