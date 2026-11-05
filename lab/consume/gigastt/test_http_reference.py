#!/usr/bin/env python3
"""Offline acceptance-policy controls using synthetic observations only."""
import copy
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
import unittest

from http_reference import BASELINE_G04_SHA, CASE_IDS, HUMAN_SHA, MODEL_SHA, ORT_SHA, qualify, validate_binding, digest


def fixture():
    cases = []
    evidence = dict(model_encoder_sha256=MODEL_SHA, static_ort_archive_sha256=ORT_SHA,
                    model_files={'v3_rnnt_encoder_int8.onnx':MODEL_SHA, 'v3_rnnt_decoder.onnx':'decoder',
                                 'v3_rnnt_joint.onnx':'joint', 'v3_vocab.txt':'vocab'},
                    cases={}, transcript_references={})
    for cid in sorted(CASE_IDS):
        kind = 'wav' if cid.endswith('-wav') else 'm4a' if cid.endswith('-aac') else 'adts'
        response = dict(status=200, word_count=8, text_sha256=HUMAN_SHA, duration=5.0)
        cases.append(dict(id=cid, kind=kind, sha256=cid,
                          responses=dict(baseline=copy.deepcopy(response), candidate=copy.deepcopy(response))))
        if kind != 'adts':
            evidence['cases'][cid] = dict(input_sha256=cid, source_wav_sha256=cid.replace('-aac', '-wav'),
                                         presentation_seconds=5.0, source_wav_seconds=5.0,
                                         ffmpeg_applied_presentation_seconds=5.0,
                                         ffmpeg_presentation_pcm_sha256='a' * 64,
                                         ffmpeg_full_pcm_sha256='a' * 64,
                                         ffmpeg_full_samples=80000, ffmpeg_surplus_samples=0,
                                         ffmpeg_presentation_is_full_prefix=True,
                                         ffmpeg_samples=80000, candidate_samples=80000, baseline_samples=80000)
    canary = dict(pass_=False, cases=cases, static_ort_archive_sha256=ORT_SHA,
                  thresholds=dict(max_latency_ratio=1.5, max_rss_ratio=1.25, rss_slack_mib=32),
                  negative=[dict(id=name, pass_=True) for name in ['garbage', 'truncated-m4a']],
                  gates={name: True for name in ['health_after', 'p95', 'p99', 'peak_rss']},
                  instances={name: dict(binary_sha256=name) for name in ['baseline', 'candidate']})
    canary['pass'] = canary.pop('pass_')
    for row in canary['negative']:row['pass'] = row.pop('pass_')
    return canary, evidence


class ReferencePolicyTests(unittest.TestCase):
    def test_complete_equal_observations_qualify_without_mutating_input(self):
        canary, evidence = fixture();before = copy.deepcopy(canary)
        self.assertTrue(qualify(canary, evidence)['qualified_for_load'])
        self.assertEqual(canary, before)
        self.assertFalse(canary['pass'])

    def test_explained_case_preserves_backend_specific_durations_and_original_failure(self):
        canary, evidence = fixture();cid = 'golos_04-aac'
        row = next(row for row in canary['cases'] if row['id'] == cid)
        row['responses']['baseline'].update(text_sha256=BASELINE_G04_SHA, duration=5.092)
        evidence['cases'][cid]['baseline_samples'] = 81472
        evidence['transcript_references'][cid] = dict(input_sha256=cid, source_wav_sha256='golos_04-wav',
                                                    human_text_sha256=HUMAN_SHA, wav_text_sha256=HUMAN_SHA,
                                                    ffmpeg_text_sha256=HUMAN_SHA)
        result = qualify(canary, evidence)
        self.assertTrue(result['qualified_for_load'], result['errors'])
        self.assertFalse(result['original_canary_pass'])
        self.assertNotEqual(result['expected']['baseline'][cid]['duration'], result['expected']['candidate'][cid]['duration'])
        evidence['transcript_references'][cid]['ffmpeg_text_sha256'] = 'unknown'
        self.assertFalse(qualify(canary, evidence)['qualified_for_load'])

    def test_unknown_transcript_duration_or_source_difference_fails(self):
        for mutation in ['text', 'duration', 'source', 'nan']:
            canary, evidence = fixture();row = canary['cases'][0]
            if mutation == 'text':row['responses']['candidate']['text_sha256'] = 'unknown'
            if mutation == 'duration':row['responses']['candidate']['duration'] = 4.0
            if mutation == 'source':evidence['cases'][row['id']]['input_sha256'] = 'wrong'
            if mutation == 'nan':row['responses']['candidate']['duration'] = float('nan')
            self.assertFalse(qualify(canary, evidence)['qualified_for_load'], mutation)

    def test_relabeling_m4a_cannot_bypass_independent_duration_checks(self):
        canary, evidence = fixture()
        row = next(row for row in canary['cases'] if row['id'] == 'golos_00-aac')
        row['kind'] = 'adts'
        evidence['cases'].pop(row['id'])
        self.assertFalse(qualify(canary, evidence)['qualified_for_load'])

    def test_bounded_ffmpeg_count_requires_explicit_full_decode_coverage(self):
        for mutation in ['short', 'missing', 'duration', 'surplus', 'prefix', 'hash']:
            canary, evidence = fixture(); proof = evidence['cases']['golos_00-aac']
            if mutation == 'short':proof['ffmpeg_full_samples'] = 79999
            if mutation == 'missing':proof.pop('ffmpeg_full_samples')
            if mutation == 'duration':proof['ffmpeg_applied_presentation_seconds'] = 4.0
            if mutation == 'surplus':proof['ffmpeg_surplus_samples'] = 1024
            if mutation == 'prefix':proof['ffmpeg_presentation_is_full_prefix'] = False
            if mutation == 'hash':proof['ffmpeg_full_pcm_sha256'] = 'b' * 64
            self.assertFalse(qualify(canary, evidence)['qualified_for_load'], mutation)

    def test_full_ffmpeg_surplus_is_retained_without_redefining_presentation(self):
        canary, evidence = fixture(); proof = evidence['cases']['golos_00-aac']
        proof.update(ffmpeg_full_samples=81024, ffmpeg_surplus_samples=1024,
                     ffmpeg_full_pcm_sha256='b' * 64)
        self.assertTrue(qualify(canary, evidence)['qualified_for_load'])
        self.assertEqual(proof['ffmpeg_samples'], 80000)

    def test_runtime_model_numeric_threshold_or_required_checks_cannot_be_bypassed(self):
        for mutation in ['model', 'ort', 'threshold', 'gate', 'negative', 'corpus']:
            canary, evidence = fixture()
            if mutation == 'model':evidence['model_encoder_sha256'] = '215MiB-model'
            if mutation == 'ort':canary['static_ort_archive_sha256'] = 'other'
            if mutation == 'threshold':canary['thresholds']['max_latency_ratio'] = 2.0
            if mutation == 'gate':canary['gates']['p95'] = False
            if mutation == 'negative':canary['negative'][0]['pass'] = False
            if mutation == 'corpus':canary['cases'].pop()
            self.assertFalse(qualify(canary, evidence)['qualified_for_load'], mutation)


class ReferenceBindingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.canary, self.independent = fixture()
        self.cpath = self.root / 'canary.json'; self.ipath = self.root / 'independent.json'
        self.cpath.write_text(json.dumps(self.canary))
        self.independent['input_canary_sha256'] = digest(self.cpath)
        self.ipath.write_text(json.dumps(self.independent))
        self.reference = qualify(self.canary, self.independent)
        self.reference.update(canary_path=str(self.cpath), independent_path=str(self.ipath),
                              canary_sha256=digest(self.cpath), independent_sha256=digest(self.ipath))
        self.args = SimpleNamespace(ort=self.root/'ort', model_dir=self.root/'model',
                                    baseline=self.root/'baseline', candidate=self.root/'candidate')
        self.hashes = {str(self.args.ort): ORT_SHA,
                       str(self.args.baseline): 'baseline', str(self.args.candidate): 'candidate'}
        for name, value in self.independent['model_files'].items():
            self.hashes[str(self.args.model_dir / name)] = value
        self.cases = []
        for row in self.canary['cases']:
            path = self.root / row['id']; self.hashes[str(path)] = row['sha256']
            self.cases.append(dict(id=row['id'], kind=row['kind'], path=str(path)))
        self.mock = patch('http_reference.digest', side_effect=lambda p: self.hashes[str(p)] if str(p) in self.hashes else digest(p))
        self.mock.start(); self.addCleanup(self.mock.stop)

    def test_reference_recomputes_with_bound_models_binaries_inputs_and_failed_canary(self):
        validate_binding(self.reference, self.args, self.cases)
        self.assertFalse(self.reference['original_canary_pass'])

    def test_flipped_qualification_or_edited_expectation_cannot_bypass_evidence(self):
        self.reference['expected']['candidate']['golos_00-aac']['duration'] = 9.0
        with self.assertRaisesRegex(ValueError, 'does not reproduce'):
            validate_binding(self.reference, self.args, self.cases)

    def test_flipped_old_pass_does_not_override_a_failed_numeric_gate(self):
        self.canary['pass'] = True
        self.canary['gates']['p95'] = False
        self.cpath.write_text(json.dumps(self.canary))
        self.independent['input_canary_sha256'] = digest(self.cpath)
        self.ipath.write_text(json.dumps(self.independent))
        self.reference['canary_sha256'] = digest(self.cpath)
        self.reference['independent_sha256'] = digest(self.ipath)
        with self.assertRaisesRegex(ValueError, 'does not reproduce'):
            validate_binding(self.reference, self.args, self.cases)

    def test_independent_presentation_from_another_canary_is_rejected(self):
        self.independent['input_canary_sha256'] = 'stale-canary'
        self.ipath.write_text(json.dumps(self.independent))
        self.reference['independent_sha256'] = digest(self.ipath)
        with self.assertRaisesRegex(ValueError, 'another canary'):
            validate_binding(self.reference, self.args, self.cases)

    def test_changed_decoder_model_or_input_bytes_fail(self):
        self.hashes[str(self.args.model_dir/'v3_rnnt_decoder.onnx')] = 'different'
        with self.assertRaisesRegex(ValueError, 'model files differ'):
            validate_binding(self.reference, self.args, self.cases)


if __name__ == '__main__':
    unittest.main()
