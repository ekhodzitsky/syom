#!/usr/bin/env python3
"""Offline receipt/CLI regressions; acquired fixture bytes are not audio."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from corpus_acquisition import apply_acquisitions, load_acquisitions
import verify_corpus


class AcquisitionTests(unittest.TestCase):
    def setUp(self):
        self.manifest = verify_corpus.load_manifest(verify_corpus.MANIFEST)
        self.holdouts = [row for row in self.manifest["cases"]
                         if row.get("kind") == "external_natural" and row.get("split") == "holdout"]
        self.cache = tempfile.TemporaryDirectory()
        self.addCleanup(self.cache.cleanup)
        self.receipt = {"version": 1, "records": []}
        for index, row in enumerate(self.holdouts):
            path = Path(self.cache.name) / f"fixture-{index}.bin"
            path.write_bytes(f"synthetic receipt bytes {index}".encode())
            self.receipt["records"].append({
                "id": row["id"], "recording_id": row["recording_id"],
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "cache_path": str(path), "provenance": {"test": True},
            })

    def apply(self):
        return apply_acquisitions(self.manifest, self.receipt)

    def assert_invalid(self, fragment):
        with self.assertRaisesRegex(ValueError, fragment):
            self.apply()

    def run_cli(self, receipt, *args):
        path = Path(self.cache.name) / "acquisitions.json"
        path.write_text(json.dumps(receipt), encoding="utf-8")
        return subprocess.run([
            sys.executable, str(verify_corpus.ROOT / "scripts/verify_corpus.py"),
            "--acquisitions", str(path), *args,
        ], capture_output=True, text=True)

    def test_valid_overlay_does_not_mutate_inventory_or_receipt(self):
        before = copy.deepcopy(self.manifest)
        receipt_before = copy.deepcopy(self.receipt)
        output = self.apply()
        self.assertEqual(self.manifest, before)
        self.assertEqual(self.receipt, receipt_before)
        self.assertEqual(verify_corpus.validate(output, require_holdouts=True), [])
        output["cases"][0]["split"] = "changed-test-only"
        self.assertEqual(self.manifest, before)

    def test_only_availability_fields_are_overlaid(self):
        record = self.receipt["records"][0]
        record.update(split="dev", rate=1, license="invented", origin_url="invented",
                      layout="stereo", valid_samples=1, presence="invented")
        case = next(row for row in self.apply()["cases"] if row["id"] == record["id"])
        registered = self.holdouts[0]
        for key in ["split", "rate", "license", "origin_url", "layout", "valid_samples"]:
            self.assertEqual(case[key], registered[key])
        self.assertEqual(case["presence"], "acquired-local")
        self.assertNotIn("provenance", case)

    def test_unknown_id_rejected(self):
        self.receipt["records"][0]["id"] = "nat-not-registered"
        self.assert_invalid("exactly one registered case")

    def test_non_natural_id_rejected(self):
        self.receipt["records"][0]["id"] = self.manifest["cases"][0]["id"]
        self.assert_invalid("only apply to external_natural")

    def test_recording_identity_mismatch_rejected(self):
        self.receipt["records"][0]["recording_id"] = self.holdouts[1]["recording_id"]
        self.assert_invalid("recording_id does not match")

    def test_duplicate_receipt_id_rejected(self):
        self.receipt["records"].append(copy.deepcopy(self.receipt["records"][0]))
        self.assert_invalid("duplicate acquisition record")

    def test_ambiguous_inventory_id_rejected(self):
        self.manifest["cases"].append(copy.deepcopy(self.holdouts[0]))
        self.assert_invalid("exactly one registered case")

    def test_malformed_receipt_schema_rejected(self):
        for receipt in [None, [], {}, {"version": True, "records": []},
                        {"version": 2, "records": []}, {"version": 1, "records": {}},
                        {"version": 1, "records": [None]},
                        {"version": 1, "records": [{"id": []}]}]:
            with self.subTest(receipt=receipt), self.assertRaises(ValueError):
                apply_acquisitions(self.manifest, receipt)

    def test_bad_hash_and_path_rejected(self):
        record = self.receipt["records"][0]
        valid = record.copy()
        for key, values in {"sha256": [None, "0" * 63, "g" * 64, 1],
                            "cache_path": [None, "", "  ", "x\x00y", 1]}.items():
            for value in values:
                with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                    record.update(valid)
                    record[key] = value
                    self.apply()

    def test_partial_three_byte_receipt_remains_incomplete(self):
        self.receipt["records"] = self.receipt["records"][:3]
        result = self.run_cli(self.receipt)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("holdouts: 8 registered / 3 hash-verified files", result.stdout)
        result = self.run_cli(self.receipt, "--require-holdouts")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("needs a nonempty cache_path", result.stdout)

    def test_all_eight_byte_receipts_pass_strict(self):
        result = self.run_cli(self.receipt, "--require-holdouts")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("holdouts: 8 registered / 8 hash-verified files", result.stdout)

    def test_explicit_receipt_requires_bytes_even_without_strict(self):
        record = self.receipt["records"][0]
        path = Path(record["cache_path"])
        path.unlink()
        missing = self.run_cli(self.receipt)
        self.assertEqual(missing.returncode, 1, missing.stdout + missing.stderr)
        self.assertIn("missing cache file", missing.stdout)
        path.write_bytes(b"changed")
        changed = self.run_cli(self.receipt)
        self.assertEqual(changed.returncode, 1, changed.stdout + changed.stderr)
        self.assertIn("cache sha256 mismatch", changed.stdout)

    def test_receipt_loader_and_cli_errors_are_clean(self):
        path = Path(self.cache.name) / "receipt.json"
        path.write_text(json.dumps(self.receipt), encoding="utf-8")
        self.assertEqual(load_acquisitions(path), self.receipt)
        path.write_text("bad JSON", encoding="utf-8")
        command = [sys.executable, str(verify_corpus.ROOT / "scripts/verify_corpus.py"),
                   "--acquisitions", str(path)]
        malformed = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(malformed.returncode, 1)
        self.assertIn("FAIL: corpus metadata", malformed.stderr)
        self.assertNotIn("Traceback", malformed.stderr)
        path.unlink()
        missing = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(missing.returncode, 1)
        self.assertIn("FAIL: corpus metadata", missing.stderr)

    def test_two_disjoint_receipts_combine_for_strict_readiness(self):
        second_path = Path(self.cache.name) / "second.json"
        second_path.write_text(json.dumps({"version": 1, "records": self.receipt["records"][3:]}),
                               encoding="utf-8")
        first = {"version": 1, "records": self.receipt["records"][:3]}
        result = self.run_cli(first, "--acquisitions", str(second_path), "--require-holdouts")
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("holdouts: 8 registered / 8 hash-verified files", result.stdout)

    def test_duplicate_identity_across_receipts_fails(self):
        second_path = Path(self.cache.name) / "second.json"
        second_path.write_text(json.dumps({"version": 1, "records": self.receipt["records"][:1]}),
                               encoding="utf-8")
        result = self.run_cli(self.receipt, "--acquisitions", str(second_path))
        self.assertEqual(result.returncode, 1)
        self.assertIn("duplicate acquisition record across receipts", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_uppercase_hash_normalized(self):
        self.receipt["records"][0]["sha256"] = self.receipt["records"][0]["sha256"].upper()
        self.assertEqual(verify_corpus.validate(self.apply(), require_holdouts=True), [])


if __name__ == "__main__":
    unittest.main()
