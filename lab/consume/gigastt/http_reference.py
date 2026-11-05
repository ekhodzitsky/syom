#!/usr/bin/env python3
"""Qualified per-backend HTTP expectations, preserving the failed parity canary.

This permits only the pinned, independently explained presentation correction.
It does not change numeric timing/RSS/admission/fault acceptance thresholds.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from fractions import Fraction
from pathlib import Path
from mp4_clocks import CLOCK_CASES, valid_clock_evidence

MODEL_SHA = '1d5a6f580b692e38ba35ed98d0f54648808f27b0a147dc11bd8978c15dc1a18f'
ORT_SHA = '0bb8a9982b44df690195c2c34b75ca791c3b9f20070b8cecbd8f50c6264dd2e2'
HUMAN_SHA = hashlib.sha256('ищи телеканал про бизнес на тиви'.encode()).hexdigest()
TOLERANCE = Fraction(1, 16000)
BASELINE_G04_SHA = hashlib.sha256('ищи телеканал про бизнес на тв'.encode()).hexdigest()
CASE_IDS = {f'golos_{i:02d}-{kind}' for i in range(15) for kind in ['wav', 'aac']} | {'golos_00-he1', 'golos_00-he2'}


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def close(a, b):
    if type(a) not in (float, int) or type(b) not in (float, int):
        return False
    if any(type(value) is float and not math.isfinite(value) for value in [a, b]):
        return False
    # Compare the recorded decimal durations exactly. Binary subtraction can
    # otherwise reject an exact one-sample difference at this inclusive bound.
    return abs(Fraction(str(a)) - Fraction(str(b))) <= TOLERANCE


def seconds(samples):
    return samples / 16000 if type(samples) is int and samples >= 0 else None


def full_ffmpeg_covers_presentation(proof):
    bounded, full = proof.get('ffmpeg_samples'), proof.get('ffmpeg_full_samples')
    hashes = [proof.get(key) for key in ['ffmpeg_presentation_pcm_sha256', 'ffmpeg_full_pcm_sha256']]
    return (type(bounded) is int and bounded > 0 and type(full) is int and full >= bounded
            and type(proof.get('ffmpeg_surplus_samples')) is int
            and proof['ffmpeg_surplus_samples'] == full - bounded
            and proof.get('ffmpeg_presentation_is_full_prefix') is True
            and close(proof.get('ffmpeg_applied_presentation_seconds'), proof.get('presentation_seconds'))
            and all(isinstance(value, str) and len(value) == 64
                    and all(c in '0123456789abcdef' for c in value) for value in hashes)
            and (full != bounded or hashes[0] == hashes[1]))


def qualify(canary, independent):
    """Pure policy: unknown differences or missing independent evidence fail."""
    errors = []
    if canary.get('thresholds') != dict(max_latency_ratio=1.5, max_rss_ratio=1.25, rss_slack_mib=32):
        errors.append('canary numeric thresholds differ from the unchanged baseline')
    if canary.get('static_ort_archive_sha256') != ORT_SHA:
        errors.append('canary ORT archive is not the pinned runtime')
    model_files = independent.get('model_files', {})
    if set(model_files) != {'v3_rnnt_encoder_int8.onnx', 'v3_rnnt_decoder.onnx', 'v3_rnnt_joint.onnx', 'v3_vocab.txt'} or model_files.get('v3_rnnt_encoder_int8.onnx') != MODEL_SHA:
        errors.append('complete model-file identities are missing or inconsistent')
    if independent.get('model_encoder_sha256') != MODEL_SHA:
        errors.append('independent references are not the active 305 MiB model')
    if independent.get('static_ort_archive_sha256') != ORT_SHA:
        errors.append('independent references use a different runtime')
    for gate in ['health_after', 'p95', 'p99', 'peak_rss']:
        if canary.get('gates', {}).get(gate) is not True:
            errors.append(f'canary prerequisite {gate} failed')
    negatives = canary.get('negative', [])
    if {r.get('id') for r in negatives} != {'garbage', 'truncated-m4a'} or not all(r.get('pass') is True for r in negatives):
        errors.append('invalid/truncated-input canary checks are missing or failed')
    cases = canary.get('cases', [])
    ids = [row.get('id') for row in cases]
    if len(cases) != 32 or set(ids) != CASE_IDS:
        errors.append('expected exactly 32 distinct pinned corpus cases')
    references = independent.get('cases', {})
    by_id = {row.get('id'): row for row in cases}
    if set(canary.get('instances', {})) != {'baseline', 'candidate'}:
        errors.append('paired server identity is missing')
    expected = {'baseline': {}, 'candidate': {}}
    explained = []
    for row in cases:
        cid = row['id']; responses = row.get('responses', {})
        expected_kind = 'wav' if cid.endswith('-wav') else 'm4a' if cid.endswith('-aac') else 'adts'
        if row.get('kind') != expected_kind:
            errors.append(f'{cid}: kind differs from the pinned corpus'); continue
        if set(responses) != {'baseline', 'candidate'}:
            errors.append(f'{cid}: missing paired response'); continue
        base, candidate = responses['baseline'], responses['candidate']
        for name, response in responses.items():
            if response.get('status') != 200 or response.get('word_count', 0) <= 0:
                errors.append(f'{cid}: {name} did not successfully recognize speech')
            expected[name][cid] = response
        if base.get('text_sha256') != candidate.get('text_sha256'):
            proof = independent.get('transcript_references', {}).get(cid, {})
            if not (cid == 'golos_04-aac' and proof.get('input_sha256') == row['sha256']
                    and proof.get('human_text_sha256') == HUMAN_SHA
                    and proof.get('wav_text_sha256') == HUMAN_SHA
                    and proof.get('ffmpeg_text_sha256') == HUMAN_SHA
                    and candidate.get('text_sha256') == HUMAN_SHA
                    and base.get('text_sha256') == BASELINE_G04_SHA
                    and proof.get('source_wav_sha256') == by_id.get('golos_04-wav', {}).get('sha256')
                    and all(r.get('text_sha256') == HUMAN_SHA for r in by_id.get('golos_04-wav', {}).get('responses', {}).values())):
                errors.append(f'{cid}: unexplained transcript difference')
            else:
                explained.append(dict(id=cid, kind='human/WAV/FFmpeg presentation correction'))
        proof = references.get(cid, {})
        if row['kind'] in ('m4a', 'wav'):
            wav = by_id.get(cid.replace('-aac', '-wav'), {})
            if proof.get('source_wav_sha256') != wav.get('sha256'):
                errors.append(f'{cid}: source WAV identity differs')
            if proof.get('input_sha256') != row['sha256']:
                errors.append(f'{cid}: no input-bound independent duration evidence'); continue
            duration = proof.get('presentation_seconds')
            clock = proof.get('clock_rounding')
            if cid in CLOCK_CASES:
                if not (valid_clock_evidence(cid, clock)
                        and clock['input_sha256'] == row['sha256']
                        and clock['source_wav_sha256'] == wav.get('sha256')
                        and proof.get('source_wav_frames') == clock['source_wav_frames']
                        and proof.get('source_wav_rate') == clock['source_wav_rate']
                        and proof.get('source_wav_seconds') == clock['source_wav_frames'] / clock['source_wav_rate']
                        and close(duration, clock['edit_duration'] / clock['movie_timescale'])):
                    errors.append(f'{cid}: source WAV/movie clock rounding is not independently explained')
                else:
                    explained.append(dict(id=cid, kind='pinned source WAV/movie clock ceiling',
                                          surplus_samples=clock['surplus_samples']))
            elif clock is not None or not close(duration, proof.get('source_wav_seconds')):
                errors.append(f'{cid}: unexplained source WAV/presentation duration difference')
            if not full_ffmpeg_covers_presentation(proof):
                errors.append(f'{cid}: full FFmpeg decode does not prove coverage of bounded presentation')
            if not all(close(duration, value) for value in [seconds(proof.get('ffmpeg_samples')),
                        seconds(proof.get('candidate_samples')),
                        candidate.get('duration')]):
                errors.append(f'{cid}: candidate disagrees with independent presentation')
            if not close(base.get('duration'), seconds(proof.get('baseline_samples'))):
                errors.append(f'{cid}: baseline PCM count does not explain its duration')
            if row['kind'] == 'wav' and not close(base.get('duration'), duration):
                errors.append(f'{cid}: WAV control changed duration')
            if not close(base.get('duration'), candidate.get('duration')):
                if row['kind'] != 'm4a' or not (type(base.get('duration')) in (int, float) and type(candidate.get('duration')) in (int, float) and base['duration'] > candidate['duration']):
                    errors.append(f'{cid}: unexplained duration difference')
                else:
                    explained.append(dict(id=cid, kind='baseline duration exceeds independently validated presentation',
                                          excess_samples=proof.get('baseline_samples', 0) - proof.get('candidate_samples', 0)))
        elif not close(base.get('duration'), candidate.get('duration')):
            errors.append(f'{cid}: unsupported duration difference')
    return dict(version=1, policy='independently-qualified-presentation-v1',
                qualified_for_load=not errors, errors=errors, expected=expected,
                inputs=[{k: r[k] for k in ['id', 'kind', 'sha256']} for r in cases],
                binary_sha256={n: v['binary_sha256'] for n, v in canary.get('instances', {}).items()},
                model_encoder_sha256=independent.get('model_encoder_sha256'),
                model_files=independent.get('model_files'),
                static_ort_archive_sha256=canary.get('static_ort_archive_sha256'),
                explained_differences=explained, original_canary_pass=canary.get('pass'))


def validate_binding(reference, args, cases):
    if reference.get('version') != 1 or reference.get('policy') != 'independently-qualified-presentation-v1' or reference.get('qualified_for_load') is not True:
        raise ValueError('reference is not independently qualified for load')
    if digest(args.ort) != reference.get('static_ort_archive_sha256') or digest(args.model_dir / 'v3_rnnt_encoder_int8.onnx') != reference.get('model_encoder_sha256'):
        raise ValueError('load model/runtime differs from qualified reference')
    files = reference.get('model_files')
    names = {'v3_rnnt_encoder_int8.onnx', 'v3_rnnt_decoder.onnx', 'v3_rnnt_joint.onnx', 'v3_vocab.txt'}
    if not isinstance(files, dict) or set(files) != names or any(digest(args.model_dir / name) != value for name, value in files.items()):
        raise ValueError('load model files differ from qualified reference')
    if any(digest(getattr(args, name)) != reference['binary_sha256'].get(name) for name in ['baseline', 'candidate']):
        raise ValueError('load binaries differ from qualified reference')
    rows = {r['id']: r for r in reference['inputs']}
    if len(cases) != len(rows) or any(c['id'] not in rows or digest(c['path']) != rows[c['id']]['sha256'] or c['kind'] != rows[c['id']]['kind'] for c in cases):
        raise ValueError('load inputs differ from qualified reference')
    for key in ['canary_path', 'independent_path']:
        if digest(reference[key]) != reference[key.replace('_path', '_sha256')]:
            raise ValueError('qualified reference evidence changed')
    independent = json.loads(Path(reference['independent_path']).read_text())
    if independent.get('input_canary_sha256') != digest(reference['canary_path']):
        raise ValueError('independent presentation evidence belongs to another canary')
    recomputed = qualify(json.loads(Path(reference['canary_path']).read_text()), independent)
    if any(reference.get(key) != value for key, value in recomputed.items()):
        raise ValueError('qualified reference policy does not reproduce from evidence')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['canary', 'independent', 'out']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():parser.error('choose a new output path; existing reference is retained')
    independent = json.loads(args.independent.read_text())
    if independent.get('input_canary_sha256') != digest(args.canary):
        parser.error('independent presentation evidence belongs to another canary')
    record = qualify(json.loads(args.canary.read_text()), independent)
    record.update(canary_path=str(args.canary.resolve()), independent_path=str(args.independent.resolve()),
                  canary_sha256=digest(args.canary), independent_sha256=digest(args.independent))
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(record, indent=2) + '\n')
    print(json.dumps({k: record[k] for k in ['qualified_for_load', 'errors', 'original_canary_pass']}))
    return int(not record['qualified_for_load'])


if __name__ == '__main__':
    raise SystemExit(main())
