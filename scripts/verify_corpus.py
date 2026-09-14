#!/usr/bin/env python3
"""Offline AAC evaluation-corpus verifier.

No network. Fails on bit-flipped committed files, missing required
assets, train/holdout recording-id overlap, and inconsistent metadata.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from corpus_synth import sha256_of  # noqa: E402

MANIFEST = ROOT / "corpus" / "manifest.json"
REQUIRED_NATURAL = 24
REQUIRED_CASES = 48


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest()


def load_manifest(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _err(errors: list[str], msg: str) -> None:
    errors.append(msg)


def validate(manifest: dict, *, flip_path: Path | None = None) -> list[str]:
    errors: list[str] = []
    cases = manifest.get("cases")
    if not isinstance(cases, list) or len(cases) < REQUIRED_CASES:
        _err(errors, f"need >= {REQUIRED_CASES} cases, got {0 if not isinstance(cases, list) else len(cases)}")
        return errors

    ids: set[str] = set()
    natural_recordings: dict[str, str] = {}
    split_recordings: dict[str, set[str]] = defaultdict(set)
    natural = 0

    for i, c in enumerate(cases):
        cid = c.get("id")
        if not cid or cid in ids:
            _err(errors, f"case[{i}] missing or duplicate id {cid!r}")
            continue
        ids.add(cid)
        kind = c.get("kind")
        split = c.get("split")
        rec = c.get("recording_id")
        rate = c.get("rate")
        layout = c.get("layout")
        samples = c.get("valid_samples")
        origin = c.get("origin")
        redist = c.get("redistribution")
        sha = c.get("sha256")
        if split not in ("dev", "holdout"):
            _err(errors, f"{cid}: split must be dev|holdout")
        if not rec:
            _err(errors, f"{cid}: recording_id required")
        if not origin or not redist:
            _err(errors, f"{cid}: origin and redistribution required")
        if layout not in (
            "mono",
            "stereo",
            "3.0",
            "4.0",
            "5.0",
            "5.1",
            "7.1",
            "none",
        ):
            _err(errors, f"{cid}: invalid layout {layout!r}")
        if rate is not None and (not isinstance(rate, int) or rate < 0):
            _err(errors, f"{cid}: invalid rate")
        if samples is not None and (not isinstance(samples, int) or samples < 0):
            _err(errors, f"{cid}: invalid valid_samples")
        if kind == "external_natural":
            natural += 1
            if rec:
                prev = natural_recordings.get(rec)
                if prev and prev != split:
                    _err(
                        errors,
                        f"recording {rec} overlaps train/holdout ({prev} vs {split})",
                    )
                natural_recordings[rec] = split
                split_recordings[split].add(rec)
            if sha not in (None, ""):
                # Optional local cache: if present, must match.
                rel = c.get("cache_path")
                if rel:
                    p = ROOT / rel
                    if p.is_file() and sha256_file(p) != sha:
                        _err(errors, f"{cid}: cache sha256 mismatch")
            # Missing cache is an explicit gap, not a hard fail.
            continue
        if not sha or len(sha) != 64:
            _err(errors, f"{cid}: committed/generated asset needs SHA-256")
            continue
        if kind == "bitstream":
            rel = c.get("path")
            if not rel:
                _err(errors, f"{cid}: path required")
                continue
            p = ROOT / rel
            if flip_path is not None and p.resolve() == flip_path.resolve():
                p = flip_path
            if not p.is_file():
                _err(errors, f"{cid}: missing required asset {rel}")
                continue
            got = sha256_file(p)
            if got != sha:
                _err(errors, f"{cid}: sha256 mismatch (modified bytes)")
        elif kind == "generated_pcm":
            spec = c.get("generator")
            if not isinstance(spec, dict):
                _err(errors, f"{cid}: generator spec required")
                continue
            got = sha256_of(spec)
            if got != sha:
                _err(errors, f"{cid}: generated pcm sha256 mismatch")
            if samples == 0 and spec.get("fn") not in ("empty",):
                _err(errors, f"{cid}: valid_samples 0 but generator is not empty")
        else:
            _err(errors, f"{cid}: unknown kind {kind!r}")

    if natural < REQUIRED_NATURAL:
        _err(errors, f"need >= {REQUIRED_NATURAL} natural excerpts, got {natural}")
    nrec = len(natural_recordings)
    nhold = len(split_recordings.get("holdout", ()))
    if nrec and nhold * 3 < nrec:
        _err(
            errors,
            f"holdout recording identities {nhold} < one-third of {nrec} natural sources",
        )
    # Overlap already flagged per recording; also catch same id in both sets.
    both = split_recordings["dev"] & split_recordings["holdout"]
    for rec in sorted(both):
        _err(errors, f"recording {rec} in both dev and holdout")
    return errors


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--manifest", type=Path, default=MANIFEST)
    ap.add_argument(
        "--flip",
        type=Path,
        help="verify this path instead of the manifest path (corruption test)",
    )
    args = ap.parse_args()
    if not args.manifest.is_file():
        print(f"FAIL: missing manifest {args.manifest}", file=sys.stderr)
        return 1
    man = load_manifest(args.manifest)
    errors = validate(man, flip_path=args.flip)
    if errors:
        print("FAIL: corpus verification")
        for e in errors:
            print(f"  - {e}")
        return 1
    n = len(man["cases"])
    nat = sum(1 for c in man["cases"] if c.get("kind") == "external_natural")
    print(f"OK: {n} cases, {nat} natural excerpts, offline checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
