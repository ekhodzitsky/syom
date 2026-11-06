#!/usr/bin/env python3
"""Offline acceptance-policy controls using synthetic observations only."""
import copy
import json
import math
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
import unittest

from http_reference import BASELINE_G04_SHA, CASE_IDS, HUMAN_SHA, MODEL_SHA, ORT_SHA, qualify, validate_binding, digest, close
from mp4_clocks import CLOCK_CASES
from http_contract import CORPUS_SHA256, MODEL_FILES


def fixture():
    cases = []
    evidence = dict(model_encoder_sha256=MODEL_SHA, static_ort_archive_sha256=ORT_SHA,
                    model_files=copy.deepcopy(MODEL_FILES),
                    cases={}, transcript_references={})
    for cid in sorted(CASE_IDS):
        kind = 'wav' if cid.endswith('-wav') else 'm4a' if cid.endswith('-aac') else 'adts'
        response = dict(status=200, word_count=8, text_sha256=HUMAN_SHA, duration=5.0, wall_ms=100.0)
        cases.append(dict(id=cid, kind=kind, sha256=CORPUS_SHA256[cid],
                          responses=dict(baseline=copy.deepcopy(response), candidate=copy.deepcopy(response))))
        if kind != 'adts':
            evidence['cases'][cid] = dict(input_sha256=CORPUS_SHA256[cid], source_wav_sha256=CORPUS_SHA256[cid.replace('-aac', '-wav')],
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
    for row in canary['negative']:
        row['pass'] = row.pop('pass_')
        row['responses'] = {name: dict(status=422) for name in ('baseline', 'candidate')}
    for instance in canary['instances'].values():
        instance.update(timing_ms=dict(count=32, median=100.0, p95=100.0, p99=100.0),
                        final_memory=dict(VmHWM='1024 kB'), healthy_after=True,
                        health_after=dict(status='ok', model='gigaam-v3-rnnt'))
    by_id = {row['id']: row for row in cases}
    for cid, pin in copy.deepcopy(CLOCK_CASES).items():
        frames = pin['source_wav_frames']; ticks = (frames * 1000 + 15999) // 16000
        for kind in ['wav', 'aac']:
            rid = cid.replace('-aac', '-' + kind); row = by_id[rid]
            row['sha256'] = pin['source_wav_sha256'] if kind == 'wav' else pin['input_sha256']
            samples = frames if kind == 'wav' else ticks * 16
            for response in row['responses'].values():response['duration'] = samples / 16000
            evidence['cases'][rid].update(input_sha256=row['sha256'], source_wav_sha256=pin['source_wav_sha256'],
                source_wav_frames=frames, source_wav_rate=16000, source_wav_seconds=frames / 16000,
                presentation_seconds=samples / 16000, ffmpeg_samples=samples, candidate_samples=samples,
                baseline_samples=samples, ffmpeg_full_samples=samples, ffmpeg_applied_presentation_seconds=samples / 16000)
        evidence['cases'][cid]['clock_rounding'] = dict(**pin, source_wav_rate=16000,
            movie_timescale=1000, movie_duration=ticks, track_duration=ticks, track_id=1,
            media_timescale=16000, media_duration=frames + 1024, edit_duration=ticks,
            media_time=1024, media_rate_integer=1, media_rate_fraction=0, stts_duration=frames + 1024)
    return canary, evidence


class DurationBoundaryTests(unittest.TestCase):
    def test_actual_golos05_golos12_one_sample_boundary_is_inclusive_both_signs(self):
        for low, high in [(3.2039375, 3.204), (5.9439375, 5.944)]:
            self.assertTrue(close(high, low))
            self.assertTrue(close(low, high))
            self.assertTrue(close(math.nextafter(high, low), low))
            self.assertTrue(close(math.nextafter(low, high), high))
            self.assertFalse(close(math.nextafter(high, math.inf), low))
            self.assertFalse(close(math.nextafter(low, -math.inf), high))

    def test_invalid_types_and_nonfinite_durations_remain_rejected(self):
        for value in [None, True, False, '3.204', [], {}, complex(1), math.nan, math.inf, -math.inf]:
            self.assertFalse(close(value, 3.204))
            self.assertFalse(close(3.204, value))


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
        evidence['transcript_references'][cid] = dict(input_sha256=CORPUS_SHA256[cid], source_wav_sha256=CORPUS_SHA256['golos_04-wav'],
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

    def test_pinned_clock_rounding_rejects_wrong_clock_stts_priming_edit_hash_or_case(self):
        for mutation in ['clock', 'stts', 'priming', 'edit', 'hash', 'wav_hash', 'case', 'missing']:
            canary, evidence = fixture(); cid = 'golos_09-aac'
            proof = evidence['cases'][cid]['clock_rounding']
            if mutation == 'clock':proof['movie_timescale'] = 1001
            if mutation == 'stts':proof['stts_entries'][0][1] = 1023
            if mutation == 'priming':proof['media_time'] = 1023
            if mutation == 'edit':proof['edit_duration'] += 1
            if mutation == 'hash':proof['input_sha256'] = 'other'
            if mutation == 'wav_hash':proof['source_wav_sha256'] = 'other'
            if mutation == 'case':evidence['cases']['golos_08-aac']['clock_rounding'] = proof
            if mutation == 'missing':evidence['cases'][cid].pop('clock_rounding')
            self.assertFalse(qualify(canary, evidence)['qualified_for_load'], mutation)

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

    def test_true_flags_cannot_hide_bad_raw_numeric_health_or_negative_observations(self):
        for mutation in ('latency', 'summary', 'memory', 'nonfinite', 'health', 'negative', 'model'):
            canary, evidence = fixture()
            if mutation == 'latency':
                for row in canary['cases']:
                    row['responses']['candidate']['wall_ms'] = 1000.0
                canary['instances']['candidate']['timing_ms'].update(median=1000.0, p95=1000.0, p99=1000.0)
            if mutation == 'summary':canary['instances']['candidate']['timing_ms']['p95'] = 99.0
            if mutation == 'memory':canary['instances']['candidate']['final_memory']['VmHWM'] = '999999 kB'
            if mutation == 'nonfinite':canary['cases'][0]['responses']['candidate']['wall_ms'] = math.nan
            if mutation == 'health':canary['instances']['candidate']['health_after']['status'] = 'loading'
            if mutation == 'negative':canary['negative'][0]['responses']['candidate']['status'] = 200
            if mutation == 'model':evidence['model_files']['v3_rnnt_decoder.onnx'] = '0' * 64
            self.assertFalse(qualify(canary, evidence)['qualified_for_load'], mutation)

    def test_matching_labels_do_not_allow_replacing_pinned_input_bytes(self):
        canary, evidence = fixture()
        row = next(row for row in canary['cases'] if row['id'] == 'golos_00-aac')
        row['sha256'] = evidence['cases'][row['id']]['input_sha256'] = '0' * 64
        self.assertFalse(qualify(canary, evidence)['qualified_for_load'])

    def test_equal_adts_responses_still_require_real_duration_count_and_text_hash(self):
        for key, value in [('duration', -1.0), ('duration', 0.0), ('word_count', True),
                           ('text_sha256', None), ('text_sha256', 'not-a-hash')]:
            canary, evidence = fixture()
            row = next(row for row in canary['cases'] if row['id'] == 'golos_00-he1')
            for response in row['responses'].values():response[key] = value
            self.assertFalse(qualify(canary, evidence)['qualified_for_load'], (key, value))


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

    def test_duplicate_load_input_cannot_replace_a_required_case(self):
        self.cases[-1] = copy.deepcopy(self.cases[0])
        with self.assertRaisesRegex(ValueError, 'distinct pinned corpus'):
            validate_binding(self.reference, self.args, self.cases)


if __name__ == '__main__':
    unittest.main()
