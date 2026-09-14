#!/usr/bin/env python3
"""Preregistered AAC listening protocol (TASK-14).

Randomizes blinded fixtures and analyzes clearly labeled SYNTHETIC
scores. Output is a procedure check, not listening evidence.
Ordinary cargo test never needs human listeners, PEAQ, or naturals.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / "corpus" / "manifest.json"

# DOC-3 / BS.1534-3 / BS.1116-3 — locked before qualification.
MUSHRA = "BS.1534-3"
BS1116 = "BS.1116-3"
MARGIN_MUSHRA = 3.0
MARGIN_BS1116 = 0.1
HIDDEN_REF_MIN = 90.0
LOW_ANCHOR_MAX = 90.0
FAIL_FRAC = 0.15
ALPHA = 0.05
POWER = 0.80
Z_A = 1.64485
Z_B = 0.84162
EXCERPT_SEC = 12
SESSION_MAX_MIN = 45
ITEMS_MAX = 12
PRIMING = 1024
LOUDNESS_LUFS = -23.0
TRUE_PEAK_DBTP = -1.0
FADE_MS = 20
N_EXPLORATORY = 20
PLANNING_SD_MUSHRA = 10.0  # literature residual; live pilot replaces this
SYNTH_TAG = "SYNTHETIC-NOT-LISTENING-EVIDENCE"
REQUIRED_PEERS = (
    {"id": "syom-lc", "pin": "in-tree encode_with", "status": "available"},
    {
        "id": "lavc9-native-aac",
        "pin": "FFmpeg 9.0.1 native aac (TASK-6/16)",
        "status": "available",
    },
)
OPTIONAL_PEERS = (
    {"id": "fdk-2.0.3", "status": "prefix-not-installed"},
    {"id": "faac-1.31.1", "status": "prefix-not-installed"},
    {"id": "glint-0.11.0", "status": "encode-pcm-rebuild"},
    {"id": "apple-audiotoolbox", "status": "no-host"},
)
# Approximate t_0.975 by df for paired CIs (stdlib only).
_T975 = (
    (1, 12.706),
    (2, 4.303),
    (3, 3.182),
    (4, 2.776),
    (5, 2.571),
    (6, 2.447),
    (7, 2.365),
    (8, 2.306),
    (9, 2.262),
    (10, 2.228),
    (15, 2.131),
    (20, 2.086),
    (30, 2.042),
    (60, 2.000),
    (120, 1.980),
)


def t975(df: int) -> float:
    if df < 1:
        return _T975[0][1]
    prev_n, prev_t = _T975[0]
    for n, t in _T975[1:]:
        if df <= n:
            w = (df - prev_n) / (n - prev_n)
            return prev_t + w * (t - prev_t)
        prev_n, prev_t = n, t
    return 1.96


def n_noninferiority(sd: float, margin: float) -> int:
    if sd <= 0 or margin <= 0:
        raise ValueError("sd and margin must be positive")
    z = Z_A + Z_B
    return max(2, math.ceil((z * sd / margin) ** 2))


def select_method(category: str, bitrate_bps: int, profile: str = "lc") -> dict:
    if profile != "lc":
        return {
            "method": None,
            "reason": "HE encode does not exist; cell deferred until TASK-90/93",
        }
    transient = category in (
        "speech-voiced",
        "speech-unvoiced",
        "speech-sibilants",
        "speech-radio",
        "percussion-transient",
    )
    if bitrate_bps <= 96_000 or transient:
        return {"method": MUSHRA, "reason": "intermediate impairment expected"}
    if bitrate_bps >= 128_000 and category in ("tonal-music", "dense-music"):
        return {
            "method": BS1116,
            "reason": "near-transparency; MUSHRA ceiling risk",
        }
    return {"method": MUSHRA, "reason": "default intermediate"}


def digest(seed: str, *parts: object) -> bytes:
    h = hashlib.sha256(seed.encode())
    for p in parts:
        h.update(b"\0")
        h.update(str(p).encode())
    return h.digest()


def fisher_yates(items: list, seed: str, *parts: object) -> list:
    a = list(items)
    for i in range(len(a) - 1, 0, -1):
        j = int.from_bytes(digest(seed, *parts, i)[:4], "big") % (i + 1)
        a[i], a[j] = a[j], a[i]
    return a


def blind_id(seed: str, *parts: object) -> str:
    alphabet = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ"
    d = digest(seed, "id", *parts)
    return "".join(alphabet[d[i] % len(alphabet)] for i in range(8))


def load_holdout(path: Path) -> list[dict]:
    man = json.loads(path.read_text(encoding="utf-8"))
    rows = []
    for c in man["cases"]:
        cid = str(c.get("id", ""))
        if not cid.startswith("nat-"):
            continue
        if c.get("split") != "holdout":
            continue
        rows.append(
            {
                "id": cid,
                "recording_id": c["recording_id"],
                "category": c.get("class") or c.get("category"),
                "presence": c.get("presence"),
                "license": c.get("license") or c.get("redistribution"),
            }
        )
    rows.sort(key=lambda r: r["id"])
    return rows


def item_cell(category: str) -> dict:
    if category == "tonal-music" or category == "dense-music":
        br = 128_000
    else:
        br = 64_000
    method = select_method(category, br, "lc")
    return {
        "profile": "lc",
        "bitrate_bps": br,
        "method": method["method"],
        "method_reason": method["reason"],
    }


def conditions_for(method: str) -> list[str]:
    if method == BS1116:
        return ["hidden_ref", "syom-lc", "lavc9-native-aac"]
    return ["hidden_ref", "lp35", "lp7k", "syom-lc", "lavc9-native-aac"]


def build_design(holdout: list[dict], seed: str) -> dict:
    items = []
    for i, row in enumerate(holdout):
        cell = item_cell(row["category"])
        method = cell["method"]
        conds = conditions_for(method)
        # Unequal raw lengths (encoder drain) before pad — duration must not leak.
        raw = {
            name: EXCERPT_SEC * 48000 + (k + 1) * 64 + (i % 3) * 32
            for k, name in enumerate(conds)
        }
        n_pad = max(raw.values())
        ui = fisher_yates(list(range(len(conds))), seed, "ui", i)
        files = []
        for k, name in enumerate(conds):
            files.append(
                {
                    "file": f"item{i:02d}/c{k:02d}.meta.json",
                    "blind_id": blind_id(seed, i, k),
                    "n_samples": n_pad,
                    "raw_samples": raw[name],
                    "rate": 48000,
                }
            )
        items.append(
            {
                "index": i,
                "source_id": row["id"],
                "recording_id": row["recording_id"],
                "category": row["category"],
                "presence": row["presence"],
                **cell,
                "conditions": conds,
                "files": files,
                "ui_order": ui,
                "open_reference": True,
            }
        )
    return {
        "task": "TASK-14",
        "synthetic": True,
        "label": SYNTH_TAG,
        "seed": seed,
        "excerpt_sec": EXCERPT_SEC,
        "session_max_min": SESSION_MAX_MIN,
        "items_max": ITEMS_MAX,
        "priming": PRIMING,
        "loudness_lufs": LOUDNESS_LUFS,
        "true_peak_dbtp": TRUE_PEAK_DBTP,
        "fade_ms": FADE_MS,
        "margin_mushra": MARGIN_MUSHRA,
        "margin_bs1116": MARGIN_BS1116,
        "hidden_ref_min": HIDDEN_REF_MIN,
        "low_anchor_max": LOW_ANCHOR_MAX,
        "fail_frac": FAIL_FRAC,
        "n_exploratory": N_EXPLORATORY,
        "planning_sd_mushra": PLANNING_SD_MUSHRA,
        "required_peers": list(REQUIRED_PEERS),
        "optional_peers": list(OPTIONAL_PEERS),
        "holdout": holdout,
        "items": items,
    }


def write_packet(design: dict, out: Path) -> dict:
    packet = out / "packet"
    packet.mkdir(parents=True, exist_ok=True)
    key = {"synthetic": True, "label": SYNTH_TAG, "map": []}
    for it in design["items"]:
        d = packet / f"item{it['index']:02d}"
        d.mkdir(exist_ok=True)
        for k, name in enumerate(it["conditions"]):
            meta = {
                "blind_id": it["files"][k]["blind_id"],
                "n_samples": it["files"][k]["n_samples"],
                "rate": 48000,
                "loudness_lufs": LOUDNESS_LUFS,
                "label": SYNTH_TAG,
            }
            (d / f"c{k:02d}.meta.json").write_text(
                json.dumps(meta, indent=2) + "\n", encoding="utf-8"
            )
            key["map"].append(
                {
                    "item": it["index"],
                    "file": it["files"][k]["file"],
                    "codec": name,
                    "source_id": it["source_id"],
                }
            )
        (d / "ui_order.json").write_text(
            json.dumps(
                {
                    "ui_order": it["ui_order"],
                    "label": SYNTH_TAG,
                }
            )
            + "\n",
            encoding="utf-8",
        )
    (out / "key.json").write_text(json.dumps(key, indent=2) + "\n", encoding="utf-8")
    return key


def plant_scores(design: dict, seed: str, n_good: int = 8) -> dict:
    """Clearly labeled synthetic scores. Not listening evidence."""
    rows = []
    # Listener 0 is inattentive (fails hidden-ref screen).
    for li in range(n_good + 1):
        bad = li == 0
        for it in design["items"]:
            method = it["method"]
            for k, name in enumerate(it["conditions"]):
                u = int.from_bytes(digest(seed, "sc", li, it["index"], k)[:4], "big")
                jitter = (u % 1000) / 1000.0
                if method == BS1116:
                    base = {"hidden_ref": 4.9, "syom-lc": 4.65, "lavc9-native-aac": 4.72}
                    score = base[name] + (jitter - 0.5) * 0.08
                    if bad and name == "hidden_ref":
                        score = 2.0
                else:
                    base = {
                        "hidden_ref": 97.0,
                        "lp35": 28.0,
                        "lp7k": 52.0,
                        "syom-lc": 72.0,
                        "lavc9-native-aac": 73.0,
                    }
                    score = base[name] + (jitter - 0.5) * 4.0
                    if bad and name == "hidden_ref":
                        score = 40.0
                rows.append(
                    {
                        "listener": f"L{li:02d}",
                        "item": it["index"],
                        "file": it["files"][k]["file"],
                        "score": round(score, 3),
                        "method": method,
                    }
                )
    return {
        "synthetic": True,
        "label": SYNTH_TAG,
        "n_listeners_submitted": n_good + 1,
        "rows": rows,
    }


def _mean_sd(xs: list[float]) -> tuple[float, float]:
    n = len(xs)
    m = sum(xs) / n
    var = sum((x - m) ** 2 for x in xs) / (n - 1) if n > 1 else 0.0
    return m, math.sqrt(var)


def screen(design: dict, scores: dict, key: dict) -> dict:
    codec_of = {(m["item"], m["file"]): m["codec"] for m in key["map"]}
    by_listener: dict[str, list[dict]] = {}
    for row in scores["rows"]:
        by_listener.setdefault(row["listener"], []).append(row)
    excluded = []
    valid = []
    for lis, rows in sorted(by_listener.items()):
        n_ref_fail = 0
        n_anc_fail = 0
        for row in rows:
            if row["method"] != MUSHRA:
                continue
            codec = codec_of[(row["item"], row["file"])]
            if codec == "hidden_ref" and row["score"] < HIDDEN_REF_MIN:
                n_ref_fail += 1
            if codec == "lp35" and row["score"] > LOW_ANCHOR_MAX:
                n_anc_fail += 1
        n_items = len({r["item"] for r in rows if r["method"] == MUSHRA})
        ref_frac = (n_ref_fail / n_items) if n_items else 0.0
        anc_frac = (n_anc_fail / n_items) if n_items else 0.0
        if ref_frac > FAIL_FRAC or anc_frac > FAIL_FRAC:
            excluded.append(
                {
                    "listener": lis,
                    "hidden_ref_fail_frac": round(ref_frac, 3),
                    "low_anchor_fail_frac": round(anc_frac, 3),
                }
            )
        else:
            valid.append(lis)
    return {"valid": valid, "excluded": excluded}


def paired_diffs(
    design: dict, scores: dict, key: dict, valid: list[str], left: str, right: str
) -> dict:
    codec_of = {(m["item"], m["file"]): m["codec"] for m in key["map"]}
    # Per listener, mean (left-right) over MUSHRA items.
    per: dict[str, list[float]] = {lis: [] for lis in valid}
    lookup = {(r["listener"], r["item"], codec_of[(r["item"], r["file"])]): r["score"] for r in scores["rows"]}
    for it in design["items"]:
        if it["method"] != MUSHRA:
            continue
        if left not in it["conditions"] or right not in it["conditions"]:
            continue
        for lis in valid:
            a = lookup.get((lis, it["index"], left))
            b = lookup.get((lis, it["index"], right))
            if a is None or b is None:
                continue
            per[lis].append(a - b)
    xs = [sum(v) / len(v) for v in per.values() if v]
    if len(xs) < 2:
        return {"n": len(xs), "mean": None, "sd": None, "ci95": None, "noninferior": False}
    m, sd = _mean_sd(xs)
    t = t975(len(xs) - 1)
    half = t * sd / math.sqrt(len(xs))
    lo, hi = m - half, m + half
    return {
        "n": len(xs),
        "mean": round(m, 3),
        "sd": round(sd, 3),
        "ci95": [round(lo, 3), round(hi, 3)],
        "margin": MARGIN_MUSHRA,
        "noninferior": lo > -MARGIN_MUSHRA,
        "superior": lo > 0.0,
    }


def analyze(design: dict, scores: dict, key: dict) -> dict:
    scr = screen(design, scores, key)
    mushra_items = [it for it in design["items"] if it["method"] == MUSHRA]
    bs_items = [it for it in design["items"] if it["method"] == BS1116]
    pair = paired_diffs(
        design, scores, key, scr["valid"], "syom-lc", "lavc9-native-aac"
    )
    n_qual = n_noninferiority(PLANNING_SD_MUSHRA, MARGIN_MUSHRA)
    burden_min = 5.0 + len(mushra_items) * 1.5 + len(bs_items) * 2.0
    return {
        "synthetic": True,
        "label": SYNTH_TAG,
        "screening": scr,
        "mushra_items": len(mushra_items),
        "bs1116_items": len(bs_items),
        "syom_minus_lavc": pair,
        "pilot_sd": pair["sd"],
        "planning_sd_mushra": PLANNING_SD_MUSHRA,
        "n_qualification": n_qual,
        "n_exploratory": N_EXPLORATORY,
        "session_burden_min": round(burden_min, 1),
        "session_ok": burden_min <= SESSION_MAX_MIN and len(design["items"]) <= ITEMS_MAX,
        "not_listening_evidence": True,
    }


def verify_blinding(design: dict, out: Path) -> list[str]:
    errors: list[str] = []
    packet = out / "packet"
    if not packet.is_dir():
        return ["missing packet/"]
    # Filenames and packet JSON must not name codecs.
    for p in packet.rglob("*"):
        if not p.is_file():
            continue
        rel = str(p.relative_to(packet)).lower()
        for tok in ("syom", "lavc", "ffmpeg", "fdk", "faac", "glint", "apple"):
            if tok in rel:
                errors.append(f"filename leaks {tok}: {rel}")
        if p.suffix == ".json":
            text = p.read_text(encoding="utf-8").lower()
            for tok in ("syom", "lavc", "ffmpeg", "fdk", "faac", "glint", "apple"):
                if tok in text:
                    errors.append(f"packet json leaks {tok}: {rel}")
    for it in design["items"]:
        ns = {f["n_samples"] for f in it["files"]}
        if len(ns) != 1:
            errors.append(f"item {it['index']}: durations differ {ns}")
        names = [f["file"].split("/")[-1] for f in it["files"]]
        file_order = list(range(len(names)))
        if it["ui_order"] == file_order and len(file_order) > 2:
            # One identity shuffle is allowed; we require some trial to differ.
            pass
        for f in it["files"]:
            bid = f["blind_id"].lower()
            for tok in ("syom", "lavc", "aac"):
                if tok in bid:
                    errors.append(f"blind_id leaks {tok}")
    differed = any(it["ui_order"] != list(range(len(it["conditions"]))) for it in design["items"])
    if not differed:
        errors.append("every UI order matches filename order")
    scores = json.loads((out / "scores.synthetic.json").read_text(encoding="utf-8"))
    if not scores.get("synthetic") or scores.get("label") != SYNTH_TAG:
        errors.append("scores missing SYNTHETIC label")
    if len(design["items"]) > ITEMS_MAX:
        errors.append(f"too many items {len(design['items'])}")
    hold_ids = {h["recording_id"] for h in design["holdout"]}
    if len(hold_ids) != 8:
        errors.append(f"expected 8 holdout recordings, got {len(hold_ids)}")
    return errors


def run(out: Path, seed: str, manifest: Path) -> dict:
    holdout = load_holdout(manifest)
    if len(holdout) != 8:
        raise SystemExit(f"holdout must be 8 recordings, got {len(holdout)}")
    design = build_design(holdout, seed)
    out.mkdir(parents=True, exist_ok=True)
    (out / "design.json").write_text(
        json.dumps(design, indent=2) + "\n", encoding="utf-8"
    )
    key = write_packet(design, out)
    scores = plant_scores(design, seed)
    (out / "scores.synthetic.json").write_text(
        json.dumps(scores, indent=2) + "\n", encoding="utf-8"
    )
    analysis = analyze(design, scores, key)
    (out / "analysis.json").write_text(
        json.dumps(analysis, indent=2) + "\n", encoding="utf-8"
    )
    errors = verify_blinding(design, out)
    (out / "VERIFY.txt").write_text(
        ("OK\n" if not errors else "FAIL\n") + "\n".join(errors) + "\n",
        encoding="utf-8",
    )
    result = {
        "ok": not errors,
        "errors": errors,
        "label": SYNTH_TAG,
        "items": len(design["items"]),
        "mushra_items": analysis["mushra_items"],
        "bs1116_items": analysis["bs1116_items"],
        "valid_listeners": len(analysis["screening"]["valid"]),
        "excluded_listeners": len(analysis["screening"]["excluded"]),
        "n_qualification": analysis["n_qualification"],
        "n_exploratory": N_EXPLORATORY,
        "session_burden_min": analysis["session_burden_min"],
        "noninferior_mushra": analysis["syom_minus_lavc"]["noninferior"],
        "syom_minus_lavc": analysis["syom_minus_lavc"],
        "not_listening_evidence": True,
    }
    (out / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    return result


def verify_dir(out: Path) -> dict:
    design = json.loads((out / "design.json").read_text(encoding="utf-8"))
    errors = verify_blinding(design, out)
    return {"ok": not errors, "errors": errors, "label": SYNTH_TAG}


def main() -> int:
    ap = argparse.ArgumentParser(description="TASK-14 listening protocol dry-run")
    ap.add_argument("cmd", nargs="?", default="dry-run", choices=("dry-run", "verify"))
    ap.add_argument("--out", type=Path, default=ROOT / "lab" / "listen" / "dryrun")
    ap.add_argument("--seed", default="task-14")
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    args = ap.parse_args()
    if args.cmd == "verify":
        result = verify_dir(args.out)
    else:
        result = run(args.out, args.seed, args.manifest)
    json.dump(result, sys.stdout, indent=2)
    sys.stdout.write("\n")
    if result["ok"]:
        print(
            f"OK: dry-run {SYNTH_TAG} items={result.get('items', '?')} "
            f"valid={result.get('valid_listeners', '?')} "
            f"excluded={result.get('excluded_listeners', '?')}",
            file=sys.stderr,
        )
        return 0
    print("FAIL: " + "; ".join(result["errors"]), file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
