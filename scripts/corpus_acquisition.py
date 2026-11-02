#!/usr/bin/env python3
"""Apply local byte-acquisition receipts without changing the corpus inventory.

Receipts contain availability metadata only. They cannot change source identity,
split, license, layout or any other registered experimental condition.
"""
from __future__ import annotations

import copy
import json
from pathlib import Path
import re


def load_acquisitions(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def apply_acquisitions(manifest: dict, receipt: dict) -> dict:
    """Return an independent inventory copy or raise ValueError on bad metadata.

    File existence/integrity is checked by verify_corpus; receipt parsing does
    not claim audio, license or experimental-protocol qualification.
    """
    if not isinstance(receipt, dict) or type(receipt.get("version")) is not int or receipt["version"] != 1:
        raise ValueError("acquisition receipt needs integer version 1")
    records = receipt.get("records")
    if not isinstance(records, list):
        raise ValueError("acquisition receipt records must be a list")
    if not isinstance(manifest, dict) or not isinstance(manifest.get("cases"), list):
        raise ValueError("corpus manifest needs a cases list")
    result = copy.deepcopy(manifest)
    rows: dict[str, list[dict]] = {}
    for case in result["cases"]:
        if not isinstance(case, dict):
            raise ValueError("corpus cases must be objects")
        cid = case.get("id")
        if isinstance(cid, str):
            rows.setdefault(cid, []).append(case)
    seen: set[str] = set()
    for index, record in enumerate(records):
        if not isinstance(record, dict):
            raise ValueError(f"acquisition record[{index}] must be an object")
        cid = record.get("id")
        if not isinstance(cid, str) or not cid.strip():
            raise ValueError(f"acquisition record[{index}] needs a nonempty id")
        if cid in seen:
            raise ValueError(f"{cid}: duplicate acquisition record")
        seen.add(cid)
        matches = rows.get(cid, [])
        if len(matches) != 1:
            raise ValueError(f"{cid}: acquisition id must identify exactly one registered case")
        case = matches[0]
        if case.get("kind") != "external_natural":
            raise ValueError(f"{cid}: acquisitions can only apply to external_natural cases")
        recording = record.get("recording_id")
        if not isinstance(recording, str) or not recording or recording != case.get("recording_id"):
            raise ValueError(f"{cid}: acquisition recording_id does not match registered identity")
        sha = record.get("sha256")
        if not isinstance(sha, str) or re.fullmatch(r"[0-9a-fA-F]{64}", sha) is None:
            raise ValueError(f"{cid}: acquisition needs a 64-digit hexadecimal SHA-256")
        cache = record.get("cache_path")
        if not isinstance(cache, str) or not cache.strip() or "\x00" in cache:
            raise ValueError(f"{cid}: acquisition needs a nonempty cache_path")
        case.update(sha256=sha.lower(), cache_path=cache, presence="acquired-local")
    return result
