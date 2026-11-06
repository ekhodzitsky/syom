"""Reproduce pre-refactor counterexamples against the immutable Git revision."""
from pathlib import Path
import copy
import hashlib
import json
import subprocess
import sys

ROOT = Path.cwd()
sys.path.insert(0, str(ROOT / 'lab/consume/gigastt'))
from http_reference import qualify, validate_binding
from run_http_load import matches
from test_http_reference import fixture, ReferenceBindingTests

BEFORE = '2d4c491a3cc336b8be5c47e95f461a5390ae5732'


def previous(path):
    raw = subprocess.check_output(['git', 'show', f'{BEFORE}:{path}'])
    namespace = {'__name__': 'audit_previous', '__file__': str(ROOT / path)}
    exec(compile(raw, f'{BEFORE}:{path}', 'exec'), namespace)
    return namespace, hashlib.sha256(raw).hexdigest()


old, old_sha = previous('lab/consume/gigastt/http_reference.py')
cases = []
for name in ['numeric-flags', 'corpus-bytes', 'decoder-model', 'negative-adts-duration']:
    canary, evidence = fixture()
    if name == 'numeric-flags':
        for row in canary['cases']:
            row['responses']['candidate']['wall_ms'] = 1000.0
        canary['instances']['candidate']['timing_ms'].update(median=1000.0, p95=1000.0, p99=1000.0)
    if name == 'corpus-bytes':
        row = next(r for r in canary['cases'] if r['id'] == 'golos_00-aac')
        row['sha256'] = evidence['cases'][row['id']]['input_sha256'] = '0' * 64
    if name == 'decoder-model':
        evidence['model_files']['v3_rnnt_decoder.onnx'] = '0' * 64
    if name == 'negative-adts-duration':
        row = next(r for r in canary['cases'] if r['id'] == 'golos_00-he1')
        for response in row['responses'].values():response['duration'] = -1.0
    before = old['qualify'](canary, evidence)['qualified_for_load']
    after = qualify(canary, evidence)['qualified_for_load']
    assert before is True and after is False, name
    cases.append(dict(name=name, before_accepted=before, after_accepted=after))

binding = ReferenceBindingTests()
binding.setUp()
try:
    binding.cases[-1] = copy.deepcopy(binding.cases[0])
    actual_digest = old['digest']
    old['digest'] = lambda p: binding.hashes[str(p)] if str(p) in binding.hashes else actual_digest(p)
    old['validate_binding'](binding.reference, binding.args, binding.cases)
    try:
        validate_binding(binding.reference, binding.args, binding.cases)
    except ValueError:
        cases.append(dict(name='duplicate-load-input', before_accepted=True, after_accepted=False))
    else:
        raise AssertionError('duplicate load input accepted')
finally:
    binding.doCleanups()

old_load, load_sha = previous('lab/consume/gigastt/run_http_load.py')
response = dict(status=200, word_count=1, text_sha256='same', duration=3.204)
expected = dict(text_sha256='same', duration=3.2039375)
assert old_load['matches'](response, expected) is False
assert matches(response, expected) is True
cases.append(dict(name='inclusive-one-sample-load-boundary', before_accepted=False, after_accepted=True))

print(json.dumps(dict(before_revision=BEFORE, before_policy_sha256=old_sha,
                     before_load_sha256=load_sha, counterexamples=cases,
                     scope='Synthetic offline observations; no model, server, media or timing campaign'), indent=2))
