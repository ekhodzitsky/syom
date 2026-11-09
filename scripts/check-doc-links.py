#!/usr/bin/env python3
"""Check maintained documentation links against this checkout, without network."""
from __future__ import annotations

import argparse
import html
import json
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
PAGES = [
    "README.md", "CONTRIBUTING.md", "docs/README.md",
    "docs/usage-and-validation.md", "docs/verification.md",
    "docs/comparison.md", "docs/persistent-checkpoints.md",
    "docs/repository-layout.md", "lab/README.md",
    "lab/apple/PIN.md", "lab/apple/REPORT.md",
]
REPOSITORY_PREFIXES = (
    "https://github.com/ekhodzitsky/syom/blob/main/",
    "https://github.com/ekhodzitsky/syom/tree/main/",
)


def prose(text: str) -> str:
    result = []
    fence = None
    for line in text.splitlines():
        match = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if fence is None:
            if match:
                fence = match[1]
            else:
                result.append(line)
        elif (match and match[1][0] == fence[0]
              and len(match[1]) >= len(fence) and not match[2].strip()):
            fence = None
    if fence is not None:
        raise ValueError("unclosed fenced code block")
    return "\n".join(result)


def anchors(text: str) -> set[str]:
    text = prose(text)
    explicit = set(re.findall(r"<a\s+[^>]*?(?:id|name)=[\"']([^\"']+)[\"']", text))
    result = set(explicit)
    # Explicit HTML IDs do not affect GitHub's heading-slug counter.
    used = set()
    for line in text.splitlines():
        match = re.match(r"^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$", line)
        if not match:
            continue
        label = re.sub(r"\[([^\]]+)\]\([^)]*\)", r"\1", match[1])
        label = html.unescape(re.sub(r"<[^>]+>", "", label)).lower()
        base = re.sub(r"[^\w\s-]", "", label)
        base = re.sub(r"\s", "-", base)
        candidate, suffix = base, 0
        while candidate in used:
            suffix += 1
            candidate = f"{base}-{suffix}"
        used.add(candidate)
        result.add(candidate)
    return result


def links(text: str):
    text = prose(text)
    # Fences are excluded; inline code may contain examples of link syntax.
    text = re.sub(r"`+([^`\n]*)`+", "", text)
    for match in re.finditer(r"!?\[[^\]\n]*\]\((<[^>]+>|[^\s)]+)(?:\s+[\"'][^\n]*?[\"'])?\)", text):
        yield match[1].strip("<>")
    # Also check the destination of a linked image (for example a badge).
    for match in re.finditer(r"\[!\[[^\]\n]*\]\([^\n]*?\)\]\((<[^>]+>|[^\s)]+)\)", text):
        yield match[1].strip("<>")
    for match in re.finditer(r"^ {0,3}\[[^\]]+\]:\s*(<[^>]+>|\S+)", text, flags=re.M):
        yield match[1].strip("<>")


def check(root: Path, pages: list[str]) -> dict:
    root = root.resolve()
    errors = []
    checked = 0
    external = 0
    cached_anchors = {}
    for name in pages:
        page = (root / name).resolve()
        if not page.is_relative_to(root) or not page.is_file():
            errors.append(f"{name}: missing page or outside checkout")
            continue
        try:
            hrefs = list(links(page.read_text(encoding="utf-8")))
        except ValueError as error:
            errors.append(f"{name}: {error}")
            continue
        for href in hrefs:
            repository_path = next((href[len(prefix):] for prefix in REPOSITORY_PREFIXES
                                    if href.startswith(prefix)), None)
            parsed = urlsplit(repository_path if repository_path is not None else href)
            if repository_path is None and (parsed.scheme or parsed.netloc):
                external += 1
                continue
            base = root if repository_path is not None else page.parent
            target = (base / unquote(parsed.path)).resolve() if parsed.path else page
            checked += 1
            if not target.is_relative_to(root) or not target.exists():
                errors.append(f"{name}: missing target or outside checkout: {href}")
                continue
            if parsed.fragment and target.suffix.lower() in (".md", ".markdown"):
                try:
                    if target not in cached_anchors:
                        cached_anchors[target] = anchors(target.read_text(encoding="utf-8"))
                    if unquote(parsed.fragment) not in cached_anchors[target]:
                        errors.append(f"{name}: missing anchor: {href}")
                except ValueError as error:
                    errors.append(f"{name}: target {href}: {error}")
    return dict(pages=pages, local_links_checked=checked, external_links_not_checked=external,
                errors=errors)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("pages", nargs="*", default=PAGES)
    args = parser.parse_args()
    report = check(ROOT, args.pages)
    print(json.dumps(report, indent=2))
    return int(bool(report["errors"]))


if __name__ == "__main__":
    raise SystemExit(main())
