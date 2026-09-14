#!/usr/bin/env python3
"""Offline oracle-provenance checks. Does not spawn ffmpeg/FDK."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from corpus_synth import sha256_of  # noqa: E402

MANIFEST = ROOT / "corpus" / "oracles" / "provenance.json"
REQUIRED = (
    "id",
    "role",
    "engine",
    "pcm_precision",
    "comparison",
    "sha256",
)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    man = json.loads(MANIFEST.read_text(encoding="utf-8"))
    errors: list[str] = []
    recs = man.get("records")
    if not isinstance(recs, list) or not recs:
        print("FAIL: no records")
        return 1
    policies = man.get("policies") or {}
    if "deterministic" not in policies or "pns_stochastic" not in policies:
        errors.append("policies.deterministic and policies.pns_stochastic required")
    ids = set()
    gaps = 0
    for r in recs:
        rid = r.get("id")
        if not rid or rid in ids:
            errors.append(f"bad id {rid!r}")
            continue
        ids.add(rid)
        for k in REQUIRED:
            if not r.get(k):
                errors.append(f"{rid}: missing {k}")
        cmp = r.get("comparison")
        if cmp not in ("deterministic", "pns_stochastic"):
            errors.append(f"{rid}: comparison must be deterministic|pns_stochastic")
        if r.get("engine_build") in (None, "") and not r.get("provenance_gap"):
            if r.get("engine", "").startswith("unknown") or "ffmpeg" in (
                r.get("engine") or ""
            ):
                errors.append(f"{rid}: ffmpeg/unknown engine needs provenance_gap")
        if r.get("provenance_gap"):
            gaps += 1
        path = r.get("path")
        if path:
            p = ROOT / path
            if not p.is_file():
                errors.append(f"{rid}: missing {path}")
            elif sha256_file(p) != r["sha256"]:
                errors.append(f"{rid}: sha256 mismatch (do not silent-remint)")
        elif r.get("settings"):
            got = sha256_of(r["settings"])
            if got != r["sha256"]:
                errors.append(f"{rid}: regenerated synth hash mismatch")
        else:
            errors.append(f"{rid}: need path or generator settings")
    if errors:
        print("FAIL: oracle provenance")
        for e in errors:
            print(f"  - {e}")
        return 1
    print(
        f"OK: {len(recs)} oracle records, {gaps} explicit provenance gaps, "
        "no ffmpeg invoked"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
