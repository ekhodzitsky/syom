"""Offline handoff checks using real nine-file patches and disposable Git repos."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import check_handoff as handoff
from check_handoff import HandoffError, check_handoff, digest, load_manifest


class HandoffTests(unittest.TestCase):
    def git(self, *args):
        return subprocess.check_output(['git', '-C', str(self.consumer), *args], stderr=subprocess.PIPE)

    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.consumer = self.root / 'consumer'
        self.consumer.mkdir()
        self.git('init', '--quiet')
        self.old = {f'src/existing{i}.rs': f'old {i}\n' for i in range(5)}
        self.new = {**{p: text.replace('old', 'new') for p, text in self.old.items()},
                    **{f'src/new{i}.rs': f'new module {i}\n' for i in range(4)}}
        for name, text in self.old.items():
            path = self.consumer / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        self.git('add', '--all')
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                 'commit', '--quiet', '-m', 'base fixture')
        self.base = self.git('rev-parse', 'HEAD').decode().strip()
        for name, text in self.new.items():
            (self.consumer / name).write_text(text)
        patch = self.git('diff', '--binary', self.base)
        for name in self.new.keys() - self.old.keys():
            result = subprocess.run(['git', '-C', str(self.consumer), 'diff', '--binary',
                                     '--no-index', '--', '/dev/null', name], capture_output=True)
            self.assertEqual(result.returncode, 1)
            patch += result.stdout
        self.patch = self.root / 'adapter.patch'
        self.patch.write_bytes(patch)
        self.manifest = self.root / 'adapter-manifest.json'
        self.data = dict(consumer_base=self.base, patch=self.patch.name,
                         sha256=digest(self.patch), bytes=self.patch.stat().st_size,
                         files={p: digest(self.consumer / p) for p in self.new},
                         apply_check=True, applied_files_identical=True)
        self.save()
        for name, text in self.old.items():
            (self.consumer / name).write_text(text)
        for name in self.new.keys() - self.old.keys():
            (self.consumer / name).unlink()
        self.assertEqual(self.git('status', '--porcelain'), b'')

    def save(self):
        self.manifest.write_text(json.dumps(self.data))

    def state(self):
        return {str(p.relative_to(self.consumer)): p.read_bytes()
                for p in self.consumer.rglob('*') if p.is_file()}

    def test_default_verifies_nine_results_without_mutating_dirty_consumer_or_index(self):
        (self.consumer / 'src/existing0.rs').write_text('unrelated local edit\n')
        (self.consumer / 'untracked.txt').write_text('keep me\n')
        before = self.state()
        result = check_handoff(self.consumer, self.manifest)
        self.assertEqual(result['mode'], 'check')
        self.assertEqual(result['verified_files'], 9)
        self.assertEqual(self.state(), before)

    def test_explicit_apply_changes_all_nine_files_and_keeps_head_at_base(self):
        result = check_handoff(self.consumer, self.manifest, apply=True)
        self.assertTrue(result['pass'])
        self.assertEqual(result['mode'], 'apply')
        self.assertEqual(self.git('rev-parse', 'HEAD').decode().strip(), self.base)
        for path, expected in self.data['files'].items():
            self.assertEqual(digest(self.consumer / path), expected)

    def test_apply_refuses_tracked_and_untracked_changes_without_touching_them(self):
        for name in ['src/existing0.rs', 'untracked.txt']:
            path = self.consumer / name
            path.write_text('keep local change\n')
            before = self.state()
            with self.assertRaisesRegex(HandoffError, 'clean tracked and untracked'):
                check_handoff(self.consumer, self.manifest, apply=True)
            self.assertEqual(self.state(), before)
            if name in self.old:
                path.write_text(self.old[name])
            else:
                path.unlink()

    def test_wrong_patch_hash_or_size_fails_before_consumer_mutation(self):
        for key, value in [('sha256', '0' * 64), ('bytes', self.data['bytes'] + 1)]:
            previous = self.data[key]
            self.data[key] = value
            self.save()
            before = self.state()
            with self.assertRaisesRegex(HandoffError, 'SHA-256 or byte count'):
                check_handoff(self.consumer, self.manifest, apply=True)
            self.assertEqual(self.state(), before)
            self.data[key] = previous

    def test_wrong_base_and_repository_subdirectory_are_refused(self):
        self.data['consumer_base'] = '0' * 40
        self.save()
        with self.assertRaisesRegex(HandoffError, 'HEAD differs'):
            check_handoff(self.consumer, self.manifest)
        self.data['consumer_base'] = self.base
        self.save()
        with self.assertRaisesRegex(HandoffError, 'repository root'):
            check_handoff(self.consumer / 'src', self.manifest)

    def test_missing_or_extra_manifest_path_cannot_hide_patch_changes(self):
        original = copy.deepcopy(self.data['files'])
        for kind in ['missing', 'extra']:
            self.data['files'] = copy.deepcopy(original)
            if kind == 'missing':
                self.data['files'].pop('src/new0.rs')
            else:
                self.data['files']['extra.txt'] = '0' * 64
            self.save()
            with self.assertRaisesRegex(HandoffError, 'file set differs'):
                check_handoff(self.consumer, self.manifest)

    def test_wrong_result_hash_is_checked_even_when_manifest_booleans_claim_success(self):
        self.data['files']['src/new0.rs'] = '0' * 64
        self.save()
        before = self.state()
        with self.assertRaisesRegex(HandoffError, 'resulting file differs'):
            check_handoff(self.consumer, self.manifest, apply=True)
        self.assertEqual(self.state(), before)

    def test_unsafe_manifest_paths_and_duplicate_json_keys_are_refused(self):
        for name in ['../escape', '/absolute', 'src/.GiT/config', 'src\\escape', 'C:escape', 'bad\nname']:
            self.data['files'] = {name: '0' * 64}
            self.save()
            with self.assertRaises(HandoffError):
                load_manifest(self.manifest)
        self.manifest.write_text('{"files":{},"files":{}}')
        with self.assertRaisesRegex(HandoffError, 'duplicate manifest key'):
            load_manifest(self.manifest)

    def test_cli_default_is_read_only_and_reports_verified_file_count(self):
        before = self.state()
        command = ['python3', str(Path(__file__).with_name('check_handoff.py')),
                   '--consumer', str(self.consumer), '--manifest', str(self.manifest)]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['verified_files'], 9)
        self.assertEqual(self.state(), before)

    def test_apply_uses_verified_snapshot_if_original_patch_changes_during_check(self):
        original_git = handoff.git

        def replace_patch(repo, *args):
            if args[:2] == ('apply', '--check') and Path(repo) != self.consumer:
                self.patch.write_text('changed after verification\n')
            return original_git(repo, *args)

        with patch('check_handoff.git', side_effect=replace_patch):
            self.assertTrue(check_handoff(self.consumer, self.manifest, apply=True)['pass'])
        for name, expected in self.data['files'].items():
            self.assertEqual(digest(self.consumer / name), expected)


if __name__ == '__main__':
    unittest.main()
