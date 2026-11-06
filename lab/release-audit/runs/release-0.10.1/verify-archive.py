"""Verify a release archive against an immutable clean Git source commit."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument('--archive', type=Path, required=True)
parser.add_argument('--commit', required=True)
args = parser.parse_args()
version = '0.10.1'
with tarfile.open(args.archive) as tar:
    members = tar.getmembers()
    if not all(m.isfile() and m.name.startswith(f'syom-{version}/') for m in members):
        raise ValueError('unexpected archive member')
    files = {m.name.removeprefix(f'syom-{version}/'): tar.extractfile(m).read() for m in members}
if len(files) != len(members):
    raise ValueError('duplicate archive member')
allowed = {'.cargo_vcs_info.json', 'Cargo.lock', 'Cargo.toml', 'Cargo.toml.orig',
           'README.md', 'CHANGELOG.md', 'LICENSE', 'NOTICE'}
rust = [name for name in files if name.startswith('src/') and name.endswith('.rs')]
if any(name not in allowed and name not in rust for name in files):
    raise ValueError('unexpected shipped file')
if any(name.endswith('_tests.rs') for name in rust):
    raise ValueError('test source ships')
vcs = json.loads(files['.cargo_vcs_info.json'])
if vcs['git']['sha1'] != args.commit or vcs['git'].get('dirty', False) or vcs['path_in_vcs']:
    raise ValueError('archive does not identify expected clean source commit')
packed = tomllib.loads(files['Cargo.toml'].decode())
if (packed['package']['version'] != version or packed['package']['rust-version'] != '1.89'
        or packed['package']['license'] != 'MIT' or packed.get('dependencies')
        or packed.get('build-dependencies') or packed.get('target')
        or packed['package'].get('build') or packed['package'].get('links')):
    raise ValueError('package policy mismatch')
if len(files) != 187 or len(rust) != 179 or args.archive.stat().st_size > 512 * 1024:
    raise ValueError('unexpected package shape/size')
for data in files.values():
    data.decode('utf-8')
if b'MIT License' not in files['LICENSE'] or b'patent' not in files['NOTICE'].lower():
    raise ValueError('missing required legal notices')
source_hashes = {}
for name in rust + ['README.md', 'CHANGELOG.md', 'LICENSE', 'NOTICE', 'Cargo.toml.orig']:
    original = 'Cargo.toml' if name == 'Cargo.toml.orig' else name
    data = subprocess.check_output(['git', 'show', f'{args.commit}:{original}'])
    if data != files[name]:
        raise ValueError(f'package/source mismatch: {name}')
    source_hashes[name] = hashlib.sha256(data).hexdigest()
lock = tomllib.loads(files['Cargo.lock'].decode())
if not any(p['name'] == 'syom' and p['version'] == version for p in lock['package']):
    raise ValueError('archive lock does not identify release')
print(json.dumps({'pass': True, 'version': version, 'source_commit': args.commit,
                  'vcs': vcs, 'archive_bytes': args.archive.stat().st_size,
                  'archive_sha256': hashlib.sha256(args.archive.read_bytes()).hexdigest(),
                  'files': len(files), 'runtime_rust_files': len(rust), 'rust_version': '1.89',
                  'runtime_dependencies': 0, 'build_dependencies': 0,
                  'source_sha256': source_hashes}, indent=2))
