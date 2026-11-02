#!/usr/bin/env python3
"""Focused offline holdout-readiness regressions; fixtures are synthetic bytes."""

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import verify_corpus


class HoldoutReadinessTests(unittest.TestCase):
    def setUp(self):
        self.manifest = copy.deepcopy(verify_corpus.load_manifest(verify_corpus.MANIFEST))
        self.holdouts = [
            row for row in self.manifest["cases"]
            if row.get("kind") == "external_natural" and row.get("split") == "holdout"
        ]
        self.cache = tempfile.TemporaryDirectory()
        self.addCleanup(self.cache.cleanup)
        for index, row in enumerate(self.holdouts):
            payload = f"synthetic test payload {index}".encode()
            path = Path(self.cache.name) / f"fixture-{index}.bin"
            path.write_bytes(payload)
            row["cache_path"] = str(path)
            row["sha256"] = hashlib.sha256(payload).hexdigest()

    def errors(self):
        return verify_corpus.validate(self.manifest, require_holdouts=True)

    def assert_error(self, fragment):
        self.assertTrue(any(fragment in error for error in self.errors()), self.errors())

    def test_ready_fixture(self):
        self.assertEqual(self.errors(), [])

    def test_optional_absent_assets_still_pass_default(self):
        self.assertEqual(verify_corpus.validate(verify_corpus.load_manifest(verify_corpus.MANIFEST)), [])
        Path(self.holdouts[0]["cache_path"]).unlink()
        self.assertEqual(verify_corpus.validate(self.manifest), [])

    def test_missing_cache_bytes_fail_strict_mode(self):
        Path(self.holdouts[0]["cache_path"]).unlink()
        self.assert_error("missing cache file")

    def test_modified_cache_bytes_fail(self):
        Path(self.holdouts[0]["cache_path"]).write_bytes(b"modified")
        self.assert_error("cache sha256 mismatch")

    def test_missing_and_invalid_hashes_fail(self):
        for value in (None, "", "0" * 63, "z" * 64, 123):
            with self.subTest(value=value):
                self.holdouts[0]["sha256"] = value
                self.assert_error("64-digit hexadecimal SHA-256")

    def test_missing_and_invalid_paths_fail(self):
        for value in (None, "", "  ", 123, "bad\x00path"):
            with self.subTest(value=value):
                self.holdouts[0]["cache_path"] = value
                self.assert_error("nonempty cache_path")

    def test_directory_is_not_a_cache_file(self):
        self.holdouts[0]["cache_path"] = self.cache.name
        self.assert_error("missing cache file")

    def test_exactly_eight_rows_required(self):
        self.manifest["cases"].remove(self.holdouts.pop())
        self.assert_error("exactly 8 natural holdout rows")

    def test_extra_row_with_existing_recording_is_rejected(self):
        extra = copy.deepcopy(self.holdouts[0])
        extra["id"] = "nat-extra-test-only"
        self.manifest["cases"].append(extra)
        self.assert_error("got 9 rows / 8 identities")

    def test_eight_distinct_recording_identities_required(self):
        self.holdouts[0]["recording_id"] = self.holdouts[1]["recording_id"]
        self.assert_error("got 8 rows / 7 identities")

    def test_train_holdout_overlap_remains_rejected(self):
        dev = next(row for row in self.manifest["cases"] if row.get("kind") == "external_natural" and row.get("split") == "dev")
        dev["recording_id"] = self.holdouts[0]["recording_id"]
        self.assert_error("overlaps train/holdout")

    def test_cli_reports_ready_fixture_and_rejects_missing_bytes(self):
        manifest_path = Path(self.cache.name) / "manifest.json"
        manifest_path.write_text(json.dumps(self.manifest), encoding="utf-8")
        command = [
            sys.executable, str(Path(verify_corpus.__file__)),
            "--manifest", str(manifest_path), "--require-holdouts",
        ]
        ready = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(ready.returncode, 0, ready.stdout + ready.stderr)
        self.assertIn("holdouts: 8 registered / 8 hash-verified files", ready.stdout)
        Path(self.holdouts[0]["cache_path"]).unlink()
        missing = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(missing.returncode, 1, missing.stdout + missing.stderr)
        self.assertIn("holdouts: 8 registered / 7 hash-verified files", missing.stdout)
        self.assertIn("missing cache file", missing.stdout)


if __name__ == "__main__":
    unittest.main()
