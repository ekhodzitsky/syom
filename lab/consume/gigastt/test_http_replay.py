"""Replay immutable TASK-145 observations without servers, models or media."""

import json
import math
from pathlib import Path
import unittest

from http_reference import qualify
from run_http_load import matches

ARCHIVE = Path(__file__).resolve().parents[2] / "release-audit/runs/task145-20261105/http-candidate5"


def archived(name):
    return json.loads((ARCHIVE / name).read_text())


class HistoricalReplayTests(unittest.TestCase):
    def test_fresh_policy_preserves_every_archived_qualification_field(self):
        canary = archived("canary/result.json")
        independent = archived("independent/independent.json")
        saved = archived("reference-attempt2/reference.json")
        result = qualify(canary, independent)
        self.assertTrue(result["qualified_for_load"], result["errors"])
        self.assertEqual(result, {key: saved[key] for key in result})
        self.assertIs(canary["pass"], False)

    def test_every_recorded_load_response_keeps_its_original_verdict(self):
        reference = archived("reference-attempt2/reference.json")
        load = archived("load/result.json")
        count = 0
        for backend in ("baseline", "candidate"):
            for burst in load["instances"][f"load-{backend}"]["bursts"]:
                for row in burst["rows"]:
                    with self.subTest(backend=backend, case=row["id"]):
                        self.assertEqual(matches(row["response"], reference["expected"][backend][row["id"]]),
                                         row["pass"])
                    count += 1
        self.assertEqual(count, 448)

    def test_load_uses_the_same_inclusive_one_sample_boundary_as_reference(self):
        response = dict(status=200, word_count=1, text_sha256="same", duration=3.204)
        expected = dict(text_sha256="same", duration=3.2039375)
        self.assertTrue(matches(response, expected))
        response["duration"] = math.nextafter(3.204, math.inf)
        self.assertFalse(matches(response, expected))


if __name__ == "__main__":
    unittest.main()
