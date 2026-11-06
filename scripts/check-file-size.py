#!/usr/bin/env python3
"""Enforce Rust line caps with reviewed, exact-path implementation ceilings."""

from __future__ import annotations

import json
import sys
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[1]
IMPL_MAX = 400
TEST_MAX = 500
EXCEPTIONS = Path("scripts/file-size-exceptions.json")


def is_test(path: Path) -> bool:
    return path.name.endswith("_tests.rs") or "tests" in path.parts


def line_count(path: Path) -> int:
    text = path.read_text(encoding="utf-8")
    return text.count("\n") + bool(text and not text.endswith("\n"))


def load_exceptions(root: Path, config: Path) -> dict[str, int]:
    """Reject malformed, duplicate, missing and no-longer-needed exceptions."""
    data = json.loads(config.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or set(data) != {"schema_version", "exceptions"}:
        raise ValueError("expected schema_version and exceptions")
    if type(data["schema_version"]) is not int or data["schema_version"] != 1:
        raise ValueError("unsupported schema_version")
    if not isinstance(data["exceptions"], list):
        raise ValueError("exceptions must be a list")
    ceilings: dict[str, int] = {}
    for entry in data["exceptions"]:
        if not isinstance(entry, dict) or set(entry) != {"path", "max_lines", "reason"}:
            raise ValueError("each exception needs path, max_lines and reason")
        name, cap, reason = entry["path"], entry["max_lines"], entry["reason"]
        if not isinstance(name, str):
            raise ValueError("exception path must be a string")
        rel = PurePosixPath(name)
        if (rel.is_absolute() or ".." in rel.parts or rel.as_posix() != name
                or not rel.parts or rel.parts[0] != "src" or rel.suffix != ".rs"):
            raise ValueError(f"invalid source path: {name!r}")
        path = root / name
        if not path.resolve().is_relative_to(root.resolve()) or not path.is_file():
            raise ValueError(f"missing or external source: {name}")
        if is_test(Path(name)):
            raise ValueError(f"test files cannot have exceptions: {name}")
        if name in ceilings:
            raise ValueError(f"duplicate exception: {name}")
        if type(cap) is not int or cap <= IMPL_MAX:
            raise ValueError(f"max_lines must be an integer above {IMPL_MAX}: {name}")
        if not isinstance(reason, str) or not reason.strip():
            raise ValueError(f"reason must be nonempty: {name}")
        if line_count(path) <= IMPL_MAX:
            raise ValueError(f"stale exception; source fits ordinary cap: {name}")
        ceilings[name] = cap
    return ceilings


def check(root: Path, config: Path) -> list[str]:
    ceilings = load_exceptions(root, config)
    failures = []
    for path in sorted(root.glob("src/**/*.rs")):
        rel = path.relative_to(root)
        # Tests always use their cap, regardless of implementation naming.
        cap = TEST_MAX if is_test(rel) else ceilings.get(rel.as_posix(), IMPL_MAX)
        n = line_count(path)
        if n > cap:
            failures.append(f"FAIL: {rel} has {n} lines (max {cap})")
    return failures


def main() -> int:
    try:
        failures = check(ROOT, ROOT / EXCEPTIONS)
    except (OSError, ValueError) as exc:
        print(f"FAIL: invalid file-size exception configuration: {exc}")
        return 1
    if failures:
        print("\n".join(failures))
        return 1
    print("OK: all Rust sources within line caps and exact-path exception ceilings")
    return 0


if __name__ == "__main__":
    sys.exit(main())
