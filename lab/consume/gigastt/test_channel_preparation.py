"""Preparation must work from committed sources and enforce them under -O."""

import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SCRIPT = HERE / "profile/prepare_channel_selection.py"
SNAPSHOT = ROOT / "lab/release-audit/runs/task145-20261105/candidate5/reproduction/consumer"
AUDIO = Path("crates/gigastt-core/src/inference/audio")


class ChannelPreparationTests(unittest.TestCase):
    def test_default_preparation_needs_no_ignored_consumer_checkout_or_build(self):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp) / "proof"
            result = subprocess.run([sys.executable, str(SCRIPT), "--output", str(output)],
                                    cwd=ROOT, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            provenance = json.loads((output / "provenance.json").read_text())
            self.assertEqual(Path(provenance["consumer"]), SNAPSHOT)
            self.assertEqual(len(provenance["qualified_source_sha256"]), 5)
            self.assertNotIn("@@", (output / "src/main.rs").read_text())
            self.assertFalse((output / "result.json").exists())
            self.assertFalse((output / "fixtures").exists())

    def test_optimized_python_cannot_bypass_qualified_source_hashes(self):
        with tempfile.TemporaryDirectory() as temp:
            consumer = Path(temp) / "consumer"
            shutil.copytree(SNAPSHOT / AUDIO, consumer / AUDIO)
            source = consumer / AUDIO / "aac.rs"
            source.write_text(source.read_text() + "\n// changed source\n")
            result = subprocess.run([sys.executable, "-O", str(SCRIPT), "--consumer", str(consumer),
                                     "--output", str(Path(temp) / "proof")],
                                    cwd=ROOT, capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Qualified source mismatch", result.stderr)


if __name__ == "__main__":
    unittest.main()
