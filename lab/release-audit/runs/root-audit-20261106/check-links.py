from pathlib import Path
import json
import re
from urllib.parse import unquote, urlsplit

ROOT = Path.cwd()
FILES = ['README.md', 'CONTRIBUTING.md', 'docs/README.md',
         'docs/usage-and-validation.md', 'docs/persistent-checkpoints.md',
         'docs/comparison.md', 'docs/documentation-audit-2026-11-06.md', 'lab/README.md', 'AGENTS.md', 'docs/repository-layout.md', 'docs/repository-audit-2026-11-06.md']
PREFIX = 'https://github.com/ekhodzitsky/syom/blob/main/'


def prose(path):
    return re.sub(r'^```[^\n]*\n.*?^```\s*$', '', path.read_text(), flags=re.M | re.S)


def anchors(path):
    text = prose(path)
    seen = {}
    result = set(re.findall(r'<a\s+(?:id|name)=["\']([^"\']+)', text))
    for heading in re.findall(r'^#{1,6}\s+(.+?)\s*#*$', text, flags=re.M):
        label = re.sub(r'\[([^]]+)\]\([^)]*\)', r'\1', heading)
        label = re.sub(r'<[^>]+>', '', label).lower()
        slug = re.sub(r'[^\w\- ]', '', label).replace(' ', '-')
        count = seen.get(slug, 0)
        seen[slug] = count + 1
        result.add(slug if count == 0 else f'{slug}-{count}')
    return result


checked = []
failures = []
for name in FILES:
    source = ROOT / name
    text = prose(source)
    targets = re.findall(r'\]\(([^\s)]+)\)', text)
    targets += re.findall(r'^\[[^]]+\]:\s+(\S+)', text, flags=re.M)
    for raw in targets:
        if raw.startswith(PREFIX):
            relative = raw[len(PREFIX):]
            parent = ROOT
        elif urlsplit(raw).scheme:
            continue
        else:
            relative, parent = raw, source.parent
        parsed = urlsplit(relative)
        path = (parent / unquote(parsed.path)).resolve() if parsed.path else source
        error = None
        if not path.exists():error = 'missing target'
        elif parsed.fragment and path.suffix == '.md' and unquote(parsed.fragment) not in anchors(path):
            error = 'missing heading/explicit anchor'
        row = dict(source=name, target=raw, error=error)
        checked.append(row)
        if error:failures.append(row)

result = dict(pass_=not failures, checked_links=len(checked), failures=failures,
              scope='Active documentation, local paths/headings and own-repository main links; no external availability claim')
result['pass'] = result.pop('pass_')
(ROOT/'target/root-audit-20261106/links.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
raise SystemExit(bool(failures))
