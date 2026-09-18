#!/usr/bin/env python3
"""TASK-83: compare two syom_speed runs (baseline, candidate).

Per cell: median ratio (candidate / baseline) with a 95% bootstrap CI of
the ratio of medians (10 000 resamples, fixed seed), and the FNV hash
check for byte identity. Exit 1 if any hash differs.
"""
import random
import sys


def load(path):
    rows = {}
    for line in open(path):
        if line.startswith("#") or line.startswith("cell\t"):
            continue
        f = line.rstrip("\n").split("\t")
        rows[f[0]] = {
            "ch": f[1], "secs": f[2], "bps": f[3], "median": float(f[4]),
            "bytes": f[7], "hash": f[8], "reps": [float(x) for x in f[9].split(",")],
        }
    return rows


def median(xs):
    s = sorted(xs)
    return s[len(s) // 2]


def main():
    base, cand = load(sys.argv[1]), load(sys.argv[2])
    rng = random.Random(83)
    bad = 0
    print("| cell | base ms | cand ms | speedup | 95% CI of ratio | bytes/hash |")
    print("|---|---:|---:|---:|---|---|")
    gains = []
    for cell, b in base.items():
        c = cand.get(cell)
        if c is None:
            continue
        ratios = []
        for _ in range(10_000):
            rb = median(rng.choices(b["reps"], k=len(b["reps"])))
            rc = median(rng.choices(c["reps"], k=len(c["reps"])))
            ratios.append(rc / rb)
        ratios.sort()
        lo, hi = ratios[249], ratios[9_749]
        same = b["hash"] == c["hash"] and b["bytes"] == c["bytes"]
        bad += not same
        gain = 1.0 - c["median"] / b["median"]
        gains.append(gain)
        print(f"| {cell} | {b['median']:.2f} | {c['median']:.2f} | {gain*100:+.1f}% | "
              f"[{(1-hi)*100:+.1f}%, {(1-lo)*100:+.1f}%] | {'identical' if same else 'DIFFERENT'} |")
    print(f"\nmedian gain over cells: {median(gains)*100:+.1f}% ({len(gains)} cells)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
