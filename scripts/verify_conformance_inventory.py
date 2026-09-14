#!/usr/bin/env python3
"""Offline TASK-17 inventory checks. Does not spawn ffmpeg/FDK or fetch ISO."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CONF = ROOT / "corpus" / "conformance"
REQUIRED_FILES = (
    "README.md",
    "clauses.json",
    "matrix.json",
    "vectors.json",
    "go-nogo.md",
)
LAYERS = {"asc", "adts", "latm", "pce"}
AGREE = {"match", "match-reject", "diverge", "partial", "local"}
FINDINGS = {f"F{n:02d}" for n in range(9, 20)}


def load(name: str):
    return json.loads((CONF / name).read_text(encoding="utf-8"))


def main() -> int:
    errors: list[str] = []
    for fn in REQUIRED_FILES:
        if not (CONF / fn).is_file():
            errors.append(f"missing {fn}")
    if errors:
        print("FAIL: conformance inventory")
        for e in errors:
            print(f"  {e}")
        return 1

    clauses = load("clauses.json")
    matrix = load("matrix.json")
    vectors = load("vectors.json")
    gonogo = (CONF / "go-nogo.md").read_text(encoding="utf-8")
    readme = (CONF / "README.md").read_text(encoding="utf-8")

    for blob, label in (
        (clauses, "clauses.json"),
        (matrix, "matrix.json"),
        (vectors, "vectors.json"),
    ):
        if blob.get("no_conformance_certificate") is not True:
            errors.append(f"{label}: no_conformance_certificate must be true")
    if clauses.get("iso_14496_26_obtained") is not False:
        errors.append("clauses.json: ISO 14496-26 must stay not-obtained")
    if clauses.get("iso_14496_3_full_text_obtained") is not False:
        errors.append("clauses.json: full 14496-3 must stay not-obtained")
    if "No conformance certificate" not in gonogo and "no conformance certificate" not in gonogo.lower():
        errors.append("go-nogo.md must refuse a conformance certificate")
    if "not a conformance certificate" not in readme.lower():
        errors.append("README.md must refuse a conformance certificate")

    editions = {e.get("id") for e in clauses.get("editions") or []}
    for need in (
        "iso-14496-3",
        "iso-13818-7",
        "iso-11172-3",
        "iso-14496-26-2024",
        "iso-14496-12",
    ):
        if need not in editions:
            errors.append(f"clauses.json missing edition {need}")

    topics = clauses.get("topics") or []
    topic_ids = {t.get("id") for t in topics}
    for need in (
        "asc-aot-rates",
        "asc-pce-in-ga",
        "adts-multi-block",
        "adts-crc",
        "latm-grammar",
        "channel-map-pce",
        "frame-length-960",
        "buffer-limits",
    ):
        if need not in topic_ids:
            errors.append(f"clauses.json missing topic {need}")

    found = set(re.findall(r"\bF1[0-9]\b|\bF09\b", json.dumps(clauses) + json.dumps(matrix) + gonogo))
    missing_f = FINDINGS - found
    if missing_f:
        errors.append(f"DOC-1 findings not mapped: {sorted(missing_f)}")

    rows = matrix.get("rows") or []
    if len(rows) < 10:
        errors.append("matrix.json: expected advertised-tool rows")
    for r in rows:
        rid = r.get("id")
        if not r.get("tests") and not r.get("independent_vectors"):
            errors.append(f"matrix {rid}: no tests and no authored vectors")

    vecs = vectors.get("vectors") or []
    ids = []
    hex_ok = 0
    layers_seen = set()
    for v in vecs:
        vid = v.get("id")
        if not vid or vid in ids:
            errors.append(f"bad vector id {vid!r}")
            continue
        ids.append(vid)
        layer = v.get("layer")
        if layer not in LAYERS:
            errors.append(f"{vid}: bad layer {layer}")
        layers_seen.add(layer)
        if v.get("agreement") not in AGREE:
            errors.append(f"{vid}: bad agreement")
        hx = v.get("hex") or ""
        if not re.fullmatch(r"[0-9a-f]+", hx) or len(hx) % 2:
            errors.append(f"{vid}: hex must be even lowercase")
        else:
            hex_ok += 1
            if len(hx) // 2 > 32:
                errors.append(f"{vid}: authored example too large")
        kind = v.get("kind")
        if kind == "malformed" and v.get("agreement") != "match-reject":
            errors.append(f"{vid}: malformed must be match-reject")
        if kind == "unsupported" and v.get("agreement") != "match-reject":
            errors.append(f"{vid}: unsupported must be match-reject (not success)")
        if v.get("agreement") in ("diverge", "partial") and not v.get("follow_on"):
            errors.append(f"{vid}: gap vector needs follow_on task")
        if v.get("agreement") != "local" and not v.get("independent"):
            errors.append(f"{vid}: missing independent expected fields/error")
    for need in ("asc", "adts", "latm"):
        if need not in layers_seen:
            errors.append(f"vectors.json missing {need} examples")
    if hex_ok < 12:
        errors.append(f"too few authored hex examples ({hex_ok})")

    anchors = {
        "asc-lc-48k-mono": "1188",
        "asc-explicit-sbr-two-rate-24-48": "2b098800",
        "adts-lc-48k-mono-no-crc": "fff14c4000fffc",
        "latm-mux-v1-latmgetvalue-2bit": "80080001011881fe00",
    }
    by_id = {v["id"]: v for v in vecs}
    for aid, hx in anchors.items():
        if by_id.get(aid, {}).get("hex") != hx:
            errors.append(f"anchor {aid} hex changed")

    if "TASK-26" not in gonogo or "TASK-30" not in gonogo:
        errors.append("go-nogo.md must name TASK-26 and TASK-30")
    if "latm_get_value" not in gonogo and "latmGetValue" not in gonogo:
        errors.append("go-nogo.md must cite independent latmGetValue")

    spawned = []
    for p in ROOT.joinpath("src").rglob("*.rs"):
        text = p.read_text(encoding="utf-8")
        if 'Command::new("ffmpeg")' in text or 'Command::new("fdk' in text:
            spawned.append(str(p.relative_to(ROOT)))
    if spawned:
        errors.append(f"src must not spawn ffmpeg/FDK: {spawned}")

    if errors:
        print("FAIL: conformance inventory")
        for e in errors:
            print(f"  {e}")
        return 1
    print(
        f"OK: {len(vecs)} authored vectors, {len(topics)} clause topics, "
        f"ISO 14496-26 not obtained, no ffmpeg invoked"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
