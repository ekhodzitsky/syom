#!/usr/bin/env python3
"""Verify a consumer patch on temporary BASE files; only --apply edits consumer."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tempfile


class HandoffError(ValueError):
    """The handoff or consumer does not match its declared provenance."""


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def safe_path(value):
    if (not isinstance(value, str) or not value or '\\' in value or ':' in value
            or any(ord(c) < 32 for c in value)):
        raise HandoffError('invalid manifest path')
    path = PurePosixPath(value)
    if path.is_absolute() or any(p.lower() in ('', '.', '..', '.git') for p in value.split('/')):
        raise HandoffError(f'unsafe manifest path: {value}')
    return value


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise HandoffError(f'duplicate manifest key: {key}')
        result[key] = value
    return result


def git(repo, *args):
    # Explicit -C must not accidentally use an inherited alternate worktree/index.
    env = {k: v for k, v in os.environ.items() if k not in {
        'GIT_DIR', 'GIT_WORK_TREE', 'GIT_INDEX_FILE', 'GIT_COMMON_DIR',
        'GIT_OBJECT_DIRECTORY', 'GIT_ALTERNATE_OBJECT_DIRECTORIES', 'GIT_NAMESPACE'}}
    result = subprocess.run(['git', '-C', str(repo), *args], env=env,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if result.returncode:
        raise HandoffError(result.stderr.decode(errors='replace').strip() or 'git command failed')
    return result.stdout


def load_manifest(path):
    manifest = json.loads(Path(path).read_text(), object_pairs_hook=unique_object)
    if not isinstance(manifest, dict):
        raise HandoffError('manifest must be an object')
    for key, length in [('consumer_base', 40), ('sha256', 64)]:
        if not isinstance(manifest.get(key), str) or not re.fullmatch('[0-9a-f]{' + str(length) + '}', manifest[key]):
            raise HandoffError(f'invalid {key}')
    if type(manifest.get('bytes')) is not int or manifest['bytes'] <= 0:
        raise HandoffError('invalid patch byte count')
    safe_path(manifest.get('patch'))
    files = manifest.get('files')
    if not isinstance(files, dict) or not files:
        raise HandoffError('manifest needs resulting file hashes')
    for name, sha in files.items():
        safe_path(name)
        if not isinstance(sha, str) or not re.fullmatch('[0-9a-f]{64}', sha):
            raise HandoffError(f'invalid file hash: {name}')
    return manifest


def check_base(consumer, base):
    top = Path(os.fsdecode(git(consumer, 'rev-parse', '--show-toplevel')).strip()).resolve()
    if top != consumer:
        raise HandoffError('--consumer must be the repository root')
    if git(consumer, 'rev-parse', 'HEAD').decode().strip() != base:
        raise HandoffError('consumer HEAD differs from manifest base')


def require_clean(consumer):
    if git(consumer, 'status', '--porcelain=v1', '--untracked-files=all', '-z'):
        raise HandoffError('--apply requires a clean tracked and untracked working tree')


def verify_files(directory, files):
    for name, expected in files.items():
        path = directory / name
        if path.is_symlink() or not path.is_file() or digest(path) != expected:
            raise HandoffError(f'resulting file differs from manifest: {name}')


def check_handoff(consumer, manifest_path, patch_path=None, apply=False):
    consumer = Path(consumer).resolve()
    manifest_path = Path(manifest_path).resolve()
    manifest = load_manifest(manifest_path)
    patch = Path(patch_path).resolve() if patch_path else manifest_path.parent / manifest['patch']
    patch_bytes = patch.read_bytes()
    if len(patch_bytes) != manifest['bytes'] or hashlib.sha256(patch_bytes).hexdigest() != manifest['sha256']:
        raise HandoffError('patch SHA-256 or byte count differs from manifest')
    check_base(consumer, manifest['consumer_base'])
    if apply:
        require_clean(consumer)

    with tempfile.TemporaryDirectory(prefix='gigastt-handoff-') as temp:
        snapshot = Path(temp) / 'verified.patch'
        snapshot.write_bytes(patch_bytes)
        scratch = Path(temp) / 'checkout'
        scratch.mkdir()
        git(scratch, 'init', '--quiet')
        parents = {str(parent) for name in manifest['files']
                   for parent in PurePosixPath(name).parents if str(parent) != '.'}
        # Read immutable Git blobs, never the consumer's possibly dirty files.
        entries = git(consumer, 'ls-tree', '-r', '-t', '-z', manifest['consumer_base'])
        for entry in entries.split(b'\0'):
            if not entry:
                continue
            metadata, raw_name = entry.split(b'\t', 1)
            mode, kind, oid = metadata.split()
            name = os.fsdecode(raw_name)
            if name in parents and kind != b'tree':
                raise HandoffError('manifest path parent is not a directory in base')
            if name not in manifest['files']:
                continue
            if kind != b'blob' or mode not in (b'100644', b'100755'):
                raise HandoffError('unsupported base file kind')
            destination = scratch / name
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(git(consumer, 'cat-file', 'blob', oid.decode()))
            destination.chmod(0o755 if mode == b'100755' else 0o644)
        names = []
        for row in git(scratch, 'apply', '--numstat', '-z', '--', str(snapshot)).split(b'\0'):
            if not row:
                continue
            fields = row.split(b'\t', 2)
            if len(fields) != 3 or not fields[2]:
                raise HandoffError('unsupported patch path encoding or rename')
            names.append(safe_path(fields[2].decode()))
        if len(names) != len(set(names)) or set(names) != set(manifest['files']):
            raise HandoffError('patch file set differs from manifest')
        git(scratch, 'apply', '--check', '--', str(snapshot))
        git(scratch, 'apply', '--', str(snapshot))
        verify_files(scratch, manifest['files'])

        if apply:
            check_base(consumer, manifest['consumer_base'])
            require_clean(consumer)
            git(consumer, 'apply', '--check', '--', str(snapshot))
            git(consumer, 'apply', '--', str(snapshot))
            verify_files(consumer, manifest['files'])
    return {'pass': True, 'mode': 'apply' if apply else 'check',
            'consumer_base': manifest['consumer_base'], 'patch_sha256': manifest['sha256'],
            'verified_files': len(manifest['files'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--consumer', type=Path, required=True)
    parser.add_argument('--manifest', type=Path, default=Path(__file__).with_name('adapter-manifest.json'))
    parser.add_argument('--patch', type=Path)
    parser.add_argument('--apply', action='store_true', help='apply to a clean consumer after verification')
    args = parser.parse_args()
    try:
        result = check_handoff(args.consumer, args.manifest, args.patch, args.apply)
    except (OSError, ValueError) as error:
        print(json.dumps(dict(error=str(error))), file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
