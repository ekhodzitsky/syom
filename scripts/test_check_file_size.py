#!/usr/bin/env python3
"""Synthetic policy tests; never depend on production source line counts."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "check_file_size", Path(__file__).with_name("check-file-size.py")
)
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)


class FileSizePolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.config = self.root / "exceptions.json"
        self.write_config([])

    def source(self, name, lines):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("// source\n" * lines, encoding="utf-8")
        return path

    def entry(self, name="src/engine/sbr_existing.rs", cap=410):
        return {"path": name, "max_lines": cap, "reason": "Existing DSP state; freeze growth."}

    def write_config(self, entries):
        self.config.write_text(json.dumps({"schema_version": 1, "exceptions": entries}))

    def check(self):
        return guard.check(self.root, self.config)

    def test_ordinary_implementation_boundary(self):
        path = self.source("src/normal.rs", 400)
        self.assertEqual(self.check(), [])
        path.write_text(path.read_text() + "// extra\n")
        self.assertIn("max 400", self.check()[0])

    def test_prefixes_never_create_exceptions(self):
        for name in ("sbr_new.rs", "ps_new.rs", "crc_new.rs", "extension_payload_new.rs", "isomp4.rs"):
            self.source("src/engine/" + name, 401)
        self.assertEqual(len(self.check()), 5)

    def test_tests_use_500_cap_including_old_exempt_prefixes(self):
        self.source("src/engine/sbr_new_tests.rs", 500)
        self.source("src/tests/helper.rs", 500)
        self.assertEqual(self.check(), [])
        self.source("src/engine/sbr_new_tests.rs", 501)
        self.source("src/tests/helper.rs", 501)
        self.assertEqual(len(self.check()), 2)
        self.assertTrue(all("max 500" in failure for failure in self.check()))

    def test_exception_cannot_override_test_cap(self):
        self.source("src/engine/ps_new_tests.rs", 501)
        self.write_config([self.entry("src/engine/ps_new_tests.rs", 501)])
        with self.assertRaisesRegex(ValueError, "test files"):
            self.check()

    def test_exact_ceiling_freezes_growth(self):
        name = self.entry()["path"]
        self.source(name, 410)
        self.write_config([self.entry()])
        self.assertEqual(self.check(), [])
        self.source(name, 411)
        self.assertIn("max 410", self.check()[0])

    def test_exception_does_not_cover_neighbor(self):
        self.source(self.entry()["path"], 410)
        self.source("src/engine/sbr_neighbor.rs", 401)
        self.write_config([self.entry()])
        failures = self.check()
        self.assertEqual(len(failures), 1)
        self.assertIn("sbr_neighbor.rs", failures[0])

    def test_stale_and_missing_entries_fail(self):
        self.write_config([self.entry()])
        with self.assertRaisesRegex(ValueError, "missing"):
            self.check()
        self.source(self.entry()["path"], 400)
        with self.assertRaisesRegex(ValueError, "stale"):
            self.check()

    def test_duplicate_entries_fail(self):
        self.source(self.entry()["path"], 410)
        self.write_config([self.entry(), self.entry()])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.check()

    def test_malformed_entry_fields_fail(self):
        self.source(self.entry()["path"], 410)
        for update in ({"max_lines": True}, {"max_lines": 400}, {"max_lines": 410.0},
                       {"reason": " "}, {"reason": None}, {"path": 1}, {"unknown": 1}):
            with self.subTest(update=update):
                self.write_config([self.entry() | update])
                with self.assertRaises(ValueError):
                    self.check()

    def test_unsafe_or_noncanonical_paths_fail(self):
        for name in ("/src/a.rs", "src/../a.rs", "src//a.rs", "other/a.rs", "src/a.py", "./src/a.rs"):
            with self.subTest(path=name):
                self.write_config([self.entry(name)])
                with self.assertRaises(ValueError):
                    self.check()

    def test_external_symlink_fails(self):
        outside = self.root / "outside.rs"
        outside.write_text("// source\n" * 410)
        # Another root makes the resolved target genuinely external.
        inner = self.root / "project"
        (inner / "src").mkdir(parents=True)
        try:
            (inner / "src/link.rs").symlink_to(outside)
        except OSError as error:
            self.skipTest(f"symlink creation unavailable on this host: {error}")
        self.write_config([self.entry("src/link.rs")])
        with self.assertRaisesRegex(ValueError, "external"):
            guard.check(inner, self.config)

    def test_malformed_document_fails(self):
        for data in (None, [], {}, {"schema_version": True, "exceptions": []},
                     {"schema_version": 2, "exceptions": []},
                     {"schema_version": 1, "exceptions": {}},
                     {"schema_version": 1, "exceptions": [None]}):
            with self.subTest(data=data):
                self.config.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    self.check()
        self.config.write_text("not JSON")
        with self.assertRaises(ValueError):
            self.check()

    def test_line_count_handles_empty_and_unterminated_line(self):
        path = self.source("src/normal.rs", 0)
        self.assertEqual(guard.line_count(path), 0)
        path.write_text("// line")
        self.assertEqual(guard.line_count(path), 1)
        path.write_text("// line\n// second")
        self.assertEqual(guard.line_count(path), 2)


if __name__ == "__main__":
    unittest.main()
