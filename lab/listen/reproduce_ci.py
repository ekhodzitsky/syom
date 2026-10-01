#!/usr/bin/env python3
"""TASK-109 independent reproduction of the dry-run analysis interval.

Recomputes the MUSHRA screening and the paired (syom - lavc) 95% t-CI
straight from the anonymized dry-run files (scores + sealed key +
design) without importing scripts/listen_protocol.py. Stdlib only.
The dry-run scores are SYNTHETIC-NOT-LISTENING-EVIDENCE; this is a
procedure check of the registered analysis, not a quality claim.

Usage: python3 lab/listen/reproduce_ci.py [lab/listen/dryrun]
Exit 0 when the recomputed interval matches analysis.json.
"""

import json
import math
import sys
from pathlib import Path

# Exact t_0.975 quantiles (not interpolated) for the dfs this packet can
# produce; falls back to the normal 1.959964 only past df 120.
T975_EXACT = {
    1: 12.7062, 2: 4.3027, 3: 3.1824, 4: 2.7764, 5: 2.5706,
    6: 2.4469, 7: 2.3646, 8: 2.3060, 9: 2.2622, 10: 2.2281,
    15: 2.1314, 20: 2.0860, 30: 2.0423, 60: 2.0003, 120: 1.9799,
}


def t975(df: int) -> float:
    for n in sorted(T975_EXACT):
        if df <= n:
            return T975_EXACT[n]
    return 1.959964


def main() -> int:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "lab/listen/dryrun")
    design = json.loads((out / "design.json").read_text(encoding="utf-8"))
    scores = json.loads((out / "scores.synthetic.json").read_text(encoding="utf-8"))
    key = json.loads((out / "key.json").read_text(encoding="utf-8"))
    want = json.loads((out / "analysis.json").read_text(encoding="utf-8"))

    codec_of = {(m["item"], m["file"]): m["codec"] for m in key["map"]}

    # Screening (BS.1534-3 post-hoc): hidden ref < 90 or lp35 > 90 on
    # more than 15% of a listener's MUSHRA items -> excluded.
    by_listener = {}
    for r in scores["rows"]:
        by_listener.setdefault(r["listener"], []).append(r)
    valid, excluded = [], []
    for lis, rows in sorted(by_listener.items()):
        mu = [r for r in rows if r["method"] == "BS.1534-3"]
        n_items = len({r["item"] for r in mu})
        ref_fail = sum(
            1
            for r in mu
            if codec_of[(r["item"], r["file"])] == "hidden_ref" and r["score"] < 90.0
        )
        anc_fail = sum(
            1
            for r in mu
            if codec_of[(r["item"], r["file"])] == "lp35" and r["score"] > 90.0
        )
        if n_items and (ref_fail / n_items > 0.15 or anc_fail / n_items > 0.15):
            excluded.append(lis)
        else:
            valid.append(lis)

    # Primary: per-listener mean of (syom-lc - lavc9-native-aac) over
    # MUSHRA items, then a paired t 95% CI across listeners.
    lookup = {
        (r["listener"], r["item"], codec_of[(r["item"], r["file"])]): r["score"]
        for r in scores["rows"]
    }
    xs = []
    for lis in valid:
        diffs = [
            lookup[(lis, it["index"], "syom-lc")]
            - lookup[(lis, it["index"], "lavc9-native-aac")]
            for it in design["items"]
            if it["method"] == "BS.1534-3"
            and (lis, it["index"], "syom-lc") in lookup
            and (lis, it["index"], "lavc9-native-aac") in lookup
        ]
        if diffs:
            xs.append(sum(diffs) / len(diffs))
    n = len(xs)
    mean = sum(xs) / n
    sd = math.sqrt(sum((x - mean) ** 2 for x in xs) / (n - 1))
    half = t975(n - 1) * sd / math.sqrt(n)
    got = {
        "n": n,
        "mean": round(mean, 3),
        "sd": round(sd, 3),
        "ci95": [round(mean - half, 3), round(mean + half, 3)],
        "noninferior": (mean - half) > -3.0,
        "superior": (mean - half) > 0.0,
        "valid": valid,
        "excluded": excluded,
    }
    ref = want["syom_minus_lavc"]
    # The registered script's t-table stores t_0.975,7 rounded to 2.365;
    # this reproduction uses the exact 2.3646, so a bound can differ by
    # one unit in the protocol's 3-decimal rounding (observed: 1e-3).
    ci_close = all(
        abs(g - r) <= 1.5e-3 for g, r in zip(got["ci95"], ref["ci95"])
    )
    checks = {
        "valid_listeners": valid == want["screening"]["valid"],
        "excluded_listeners": excluded
        == [e["listener"] for e in want["screening"]["excluded"]],
        "n": got["n"] == ref["n"],
        "mean": got["mean"] == ref["mean"],
        "sd": got["sd"] == ref["sd"],
        "ci95_within_1e-3": ci_close,
        "noninferior": got["noninferior"] == ref["noninferior"],
        "superior": got["superior"] == ref["superior"],
    }
    print(json.dumps({"recomputed": got, "analysis_json": ref}, indent=2))
    ok = all(checks.values())
    print(("OK" if ok else "MISMATCH") + ": " + json.dumps(checks))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
