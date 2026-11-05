from pathlib import Path
import datetime
import hashlib
import json
import os
import subprocess
import time

ROOT = Path('/home/ekhodzitsky/Projects/syom')
OUT = ROOT / 'target/task145/candidate5'
HARNESS = Path('/home/ekhodzitsky/.cache/gigastt-worktrees/syom-0.8.1/scripts/compare-aac-decoders.py')
FIXTURES = Path('/home/ekhodzitsky/.cache/gigastt-syom-081/fixtures')
BASE = ROOT / 'target/task145/base-bin/aac_probe'
CANDIDATE = ROOT / 'target/task145/candidate5-bin/aac_probe'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot():
    output = subprocess.check_output(['ps', '-eo', 'pid,ppid,psr,pcpu,comm,args', '--sort=-pcpu'], text=True)
    lines = output.splitlines()[:21]
    affinities = {}
    for line in lines[1:]:
        try:
            pid = int(line.split()[0])
            affinities[str(pid)] = sorted(os.sched_getaffinity(pid))
        except (ValueError, ProcessLookupError, PermissionError):
            pass
    return {'loadavg': list(os.getloadavg()), 'top_processes': lines, 'affinities': affinities}


validation = json.loads((OUT / 'validation.json').read_text())
assert validation.get('source_stable') and validation.get('finished_utc'), 'Consumer checks not complete'
assert all(row['exit_code'] == 0 for row in validation['commands']), 'Consumer check failed'
assert sha(CANDIDATE) == validation['final_binaries'][str(CANDIDATE)]['sha256']
assert 0 in os.sched_getaffinity(0)

for name in ['campaign1', 'campaign2']:
    dest = OUT / name
    dest.mkdir(exist_ok=False)
    command = ['taskset', '-c', '0', 'python3', str(HARNESS), '--base-probe', str(BASE),
               '--candidate-probe', str(CANDIDATE), '--fixtures', str(FIXTURES),
               '--output', str(dest / 'decoder-timing.json')]
    manifest = {'command': command, 'timestamp_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                'cpu': 0, 'variant': 'exact Git-pinned syom0.10.1 FFT stages plus synchronous bare ADTS AU-framed pull, deduplicated classification and synchronous M4A pull; unchanged six-case harness',
                'binary_sha256': {'base': sha(BASE), 'candidate': sha(CANDIDATE)},
                'harness_sha256': sha(HARNESS),
                'sdk_git_revision': validation['sdk_git_revision'],
                'qualification_manifest_sha256': sha(OUT / 'validation.json'),
                'fixtures_sha256': {path.name: sha(path) for path in sorted(FIXTURES.iterdir()) if path.is_file()},
                'before': snapshot()}
    (dest / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    start = time.monotonic()
    with (dest / 'decoder-timing.log').open('w') as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
    manifest.update(exit_code=result.returncode, seconds=round(time.monotonic() - start, 2),
                    after=snapshot(), finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat())
    (dest / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps({'campaign': name, 'exit_code': result.returncode, 'seconds': manifest['seconds'],
                      'report': str(dest / 'decoder-timing.json')}), flush=True)
