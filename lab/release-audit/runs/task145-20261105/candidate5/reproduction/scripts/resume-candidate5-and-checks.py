from pathlib import Path
import argparse
import datetime
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import time
import tomllib

ROOT = Path('/home/ekhodzitsky/Projects/syom')
WORK = ROOT / 'target/task145/gigastt'
OUT = ROOT / 'target/task145/candidate5'
BIN = ROOT / 'target/task145/candidate5-bin'
MSRV_TARGET = ROOT / 'target/task145/consumer-msrv-target'
PLAN = json.loads((ROOT / 'target/task145/consumer-checks/plan.json').read_text())
ENV = os.environ.copy()
for key in list(ENV):
    if key.startswith('CARGO_PROFILE_RELEASE_') or key == 'CARGO_ENCODED_RUSTFLAGS':
        ENV.pop(key)
ENV.update(PLAN['environment'])
FIXTURES = Path('/home/ekhodzitsky/.cache/gigastt-syom-081/fixtures')
PCM_MANIFEST = FIXTURES.parent / 'pcm-manifest.json'
OLD_PROBE = Path('/home/ekhodzitsky/.cache/gigastt/aac-090-target/release/examples/aac_probe')
SCRIPTS = Path('/home/ekhodzitsky/.cache/gigastt-worktrees/syom-0.8.1/scripts')
BUILD = ['cargo', '+stable', 'build', '--release', '--locked', '--offline', '-p', 'gigastt',
         '-p', 'gigastt-core', '--bin', 'gigastt', '--example', 'aac_probe', '--features',
         'gigastt-core/__internals,gigastt/extended-aac']


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def digest(path):
    value = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            value.update(chunk)
    return value.hexdigest()


def source_map(directory):
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others', '--exclude-standard'],
                                    cwd=directory, text=True).splitlines()
    suffixes = {'.rs', '.toml', '.lock', '.proto', '.c', '.cc', '.cpp', '.h', '.hpp', '.inc'}
    return {name: digest(directory / name) for name in sorted(set(names))
            if (directory / name).is_file() and Path(name).suffix in suffixes}


def artifact(path):
    return {'path': str(path), 'resolved_path': str(path.resolve()),
            'bytes': path.stat().st_size, 'sha256': digest(path)}


parser = argparse.ArgumentParser()
parser.add_argument('--git-rev', required=True)
args = parser.parse_args()
assert re.fullmatch('[0-9a-f]{40}', args.git_rev), 'Require full exact committed SDK SHA'
assert subprocess.check_output(['rustup', 'run', 'stable', 'rustc', '--version'], text=True).strip() == 'rustc 1.98.1 (48a229cea 2026-09-01)'
manifest = tomllib.loads((WORK / 'crates/gigastt-core/Cargo.toml').read_text())
sdk_dep = manifest['dependencies']['syom']
assert sdk_dep['rev'] == args.git_rev and sdk_dep.get('git', '').rstrip('/').removesuffix('.git') == 'https://github.com/ekhodzitsky/syom', 'Consumer exact Git pin has not been prepared'
expected_tests = json.loads((ROOT / 'target/task145/consumer-checks/candidate5-new-test-names.json').read_text())
assert expected_tests['source_frozen'] and expected_tests['names'], 'Await frozen ADTS pull source and exact new test names before launch'
assert expected_tests.get('adts_pull_names') and set(expected_tests['adts_pull_names']) <= set(expected_tests['names']), 'Require explicit additional ADTS pull tests before launch'
frozen_source = json.loads((OUT / 'source-freeze.json').read_text())
assert all(digest(WORK / name) == expected for name, expected in frozen_source['files'].items()), 'Frozen ADTS source manifest mismatch'
assert (OUT / 'validation.json').exists() and (ROOT / 'target/task145/candidate5-attempt1/validation.json').exists(), 'Require preserved first attempt'
OUT.mkdir(exist_ok=True)
assert BIN.is_dir(), 'Reuse immutable initial production binaries'
before = source_map(WORK)
(OUT / 'source-manifest-recovery-before.json').write_text(json.dumps(before, indent=2) + '\n')
meta = json.loads((OUT / 'validation.json').read_text())
assert 'failure' in meta and 'finished_utc' not in meta
meta.pop('failure', None)
meta.pop('failed_utc', None)
meta['commands'] = []
meta['recovery_started_utc'] = now()
meta['preserved_first_attempt'] = 'target/task145/candidate5-attempt1'
meta['source_manifest_recovery_before'] = 'source-manifest-recovery-before.json'
meta['test_only_recovery_proof'] = 'clippy-test-recovery/proof.json'



def save():
    (OUT / 'validation.json').write_text(json.dumps(meta, indent=2) + '\n')


def run(command, name, *, cpu='2-7', overrides=None, separate_stderr=False):
    start = time.monotonic()
    env = ENV.copy()
    env.update(overrides or {})
    full = ['taskset', '-c', cpu] + command
    with (OUT / name).open('w') as log:
        if separate_stderr:
            with (OUT / (name + '.stderr.log')).open('w') as errors:
                result = subprocess.run(full, cwd=WORK, env=env, stdout=log, stderr=errors)
        else:
            result = subprocess.run(full, cwd=WORK, env=env, stdout=log, stderr=subprocess.STDOUT)
    row = {'command': full, 'exit_code': result.returncode,
           'seconds': round(time.monotonic() - start, 2), 'log': name,
           'environment_override': overrides or {}}
    meta['commands'].append(row)
    save()
    print(json.dumps(row), flush=True)
    if result.returncode:
        raise SystemExit(result.returncode)


def copy_bins(label):
    outputs = {}
    for relative, name in [('release/gigastt', 'gigastt'), ('release/examples/aac_probe', 'aac_probe')]:
        source = Path(ENV['CARGO_TARGET_DIR']) / relative
        destination = BIN / name
        if destination.exists() and digest(source) != digest(destination):
            previous = OUT / ('initial-' + name)
            shutil.copy2(destination, previous)
        shutil.copy2(source, destination)
        outputs[str(destination)] = artifact(destination)
    meta[label] = outputs
    save()


def pcm_parity(name):
    run(['python3', str(SCRIPTS / 'compare-aac-pcm.py'), '--base-probe', str(OLD_PROBE),
         '--candidate-probe', str(BIN / 'aac_probe'), '--manifest', str(PCM_MANIFEST),
         '--output', str(OUT / (name + '.json'))], name + '.log', cpu='1')
    data = json.loads((OUT / (name + '.json')).read_text())
    assert data['pass'] and len(data['cases']) == 29, 'Require all 29 exact original syom0.10 PCM cells'


def host_snapshot():
    lines = subprocess.check_output(['ps', '-eo', 'pid,ppid,psr,pcpu,comm', '--sort=-pcpu'], text=True).splitlines()[:21]
    affinity = {}
    for line in lines[1:]:
        try:
            pid = int(line.split()[0])
            affinity[str(pid)] = sorted(os.sched_getaffinity(pid))
        except (ValueError, ProcessLookupError, PermissionError):
            pass
    return {'loadavg': list(os.getloadavg()), 'processes': lines, 'affinities': affinity}


def preliminary_gate():
    cold_release = json.loads((ROOT / 'target/task145/consumer-checks/candidate5-cold-stack-release.json').read_text())
    assert cold_release['released'], 'Independent cold-stack owner must release before early CPU0 timing'
    destination = OUT / 'early-diagnostic'
    destination.mkdir(exist_ok=False)
    command = ['python3', str(SCRIPTS / 'compare-aac-decoders.py'), '--base-probe',
               str(ROOT / 'target/task145/base-bin/aac_probe'), '--candidate-probe',
               str(BIN / 'aac_probe'), '--fixtures', str(FIXTURES), '--output',
               str(destination / 'decoder-timing.json')]
    data = {'qualification': False, 'reason': 'Root-requested preliminary unchanged six-case diagnostic before expensive workspace LTO tests',
            'cpu': 0, 'before': host_snapshot(), 'sdk_git_revision': args.git_rev,
            'binary_sha256': {'base': digest(ROOT / 'target/task145/base-bin/aac_probe'),
                              'candidate': digest(BIN / 'aac_probe')},
            'harness_sha256': digest(SCRIPTS / 'compare-aac-decoders.py'),
            'command': ['taskset', '-c', '0'] + command}
    start = time.monotonic()
    with (destination / 'decoder-timing.log').open('w') as log:
        result = subprocess.run(data['command'], cwd=WORK, env=ENV, stdout=log, stderr=subprocess.STDOUT)
    data.update(exit_code=result.returncode, seconds=round(time.monotonic() - start, 2), after=host_snapshot())
    if (destination / 'decoder-timing.json').exists():
        data['summary'] = json.loads((destination / 'decoder-timing.json').read_text())['summary']
    (destination / 'manifest.json').write_text(json.dumps(data, indent=2) + '\n')
    meta['preliminary_diagnostic'] = data
    save()
    print(json.dumps({'preliminary_six_case_exit_code': result.returncode, 'summary': data.get('summary')}), flush=True)
    if result.returncode:
        raise SystemExit('Preliminary gate failed; preserve raw outcome and stop before expensive full checks for root review')


try:
    proof = json.loads((OUT / 'clippy-test-recovery/proof.json').read_text())
    assert proof['production_prefix_identical'] and proof['only_assertion_changed'] and proof['pass']
    old = json.loads((ROOT / 'target/task145/candidate5-attempt1/source-manifest-before.json').read_text())
    assert {name for name in set(old) | set(before) if old.get(name) != before.get(name)} == {'crates/gigastt-core/src/inference/audio/aac/input.rs'}
    for name, data in meta['initial_binaries'].items():
        assert digest(Path(name)) == data['sha256'], 'Immutable initial production binary changed'
    save()
    for index, command in enumerate(PLAN['commands']):
        override = {'CARGO_TARGET_DIR': str(MSRV_TARGET)} if '+1.94.0' in command else None
        run(command, f'recovery-check-{index}.log', overrides=override)
        if index == 2:
            proof = json.loads((ROOT / 'target/task145/consumer-checks/candidate5-aac-coverage-proof.json').read_text())
            expected = json.loads((ROOT / 'target/task145/consumer-checks/candidate5-new-test-names.json').read_text())
            assert expected['source_frozen'] and expected['names'], 'New classification test names must be frozen before qualification'
            passed = set(re.findall(r'^test (\S+) \.\.\. ok$', (OUT / f'recovery-check-{index}.log').read_text(), re.M))
            required = set(proof['names'])
            assert required <= passed, f'Missing prior AAC guard coverage: {required - passed}'
            assert all(any(name.rsplit('::', 1)[-1] == test for name in passed) for test in expected['names']), 'Additional AAC regression tests did not all run successfully'
            meta['aac_guard_coverage'] = {'source': f'recovery-check-{index}.log', 'prior_guard_count': len(required),
                                          'all_prior_guard_names_passed': True,
                                          'total_required_guard_functions': len(required) + len(expected['names']),
                                          'additional_aac_tests_passed': expected['names']}
            save()
    spec = importlib.util.spec_from_file_location('publication', WORK / 'scripts/check-publication.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    names = set(subprocess.check_output(['git', 'diff', '--name-only', 'HEAD'], cwd=WORK, text=True).splitlines())
    names.update(subprocess.check_output(['git', 'ls-files', '--others', '--exclude-standard'], cwd=WORK, text=True).splitlines())
    errors = []
    for name in sorted(names):
        path = WORK / name
        if path.is_file():
            errors.extend(module.check_file(name, path.read_bytes()))
    assert not errors, errors
    meta['working_tree_publication_scan'] = {'pass': True, 'checked_files': sorted(names)}
    assert source_map(WORK) == before, 'Source changed during qualification'
    run(BUILD, 'recovery-final-build.log')
    copy_bins('final_binaries')
    matched = all(data['sha256'] == meta['initial_binaries'][name]['sha256']
                  for name, data in meta['final_binaries'].items())
    meta['final_binaries_match_initial'] = matched
    if not matched:
        pcm_parity('final-pcm-parity')
    run([str(BIN / 'gigastt'), '--version'], 'recovery-final-server-smoke.log')
    assert source_map(WORK) == before, 'Source changed during final freeze'
    meta.update(finished_utc=now(), source_stable=True)
    (OUT / 'source-manifest-recovery-after.json').write_text(json.dumps(source_map(WORK), indent=2) + '\n')
    save()
    meta['source_manifest_recovery_after'] = 'source-manifest-recovery-after.json'
    save()
    print('Candidate5 all consumer checks and binary freeze passed. Timing not started.', flush=True)
except BaseException as error:
    meta['failure'] = repr(error)
    meta['failed_utc'] = now()
    save()
    raise
