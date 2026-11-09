"""The shared ignore policy protects local state without hiding proof fixtures."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class IgnorePolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith('GIT_')}
        subprocess.run(['git', 'init', '--quiet', str(self.repo)],
                       env=self.env, check=True, capture_output=True)
        shutil.copyfile(ROOT / '.gitignore', self.repo / '.gitignore')

    def ignored(self, name):
        result = subprocess.run(['git', '-C', str(self.repo), '-c', f'core.excludesFile={os.devnull}',
                                 'check-ignore', '--no-index', '--quiet', name],
                                env=self.env, capture_output=True)
        self.assertIn(result.returncode, (0, 1), result.stderr)
        return result.returncode == 0

    def test_credentials_and_machine_state_are_ignored(self):
        for name in ['.env', '.env.local', 'lab/demo/.env.production', 'credentials.json',
                     '.cargo/credentials.toml', 'private.pem', 'keys/client-private.pem',
                     'keys/client.key', 'keys/client.p12', 'keys/id_rsa', 'keys/id_ed25519',
                     'model.onnx', 'cache/optimized.ort', 'weights.safetensors', 'model.gguf',
                     'target/build.o', '.codex/session.json', 'backlog/tasks/private.md']:
            with self.subTest(name=name):
                self.assertTrue(self.ignored(name))

    def test_routine_audit_logs_are_ignored(self):
        for name in ['docs/audit/runs/example.log',
                     'docs/audit/runs/example/00.log',
                     'lab/release-audit/runs/example.log',
                     'lab/release-audit/runs/example/fuzz/worker.log']:
            with self.subTest(name=name):
                self.assertTrue(self.ignored(name))

    def test_safe_templates_and_shared_configuration_remain_visible(self):
        for name in ['.env.example', '.env.sample', 'lab/demo/.env.template',
                     'credentials.example.json', 'public.pem', '.cargo/config.toml',
                     'AGENTS.md', 'Cargo.lock', 'rust-toolchain.toml']:
            with self.subTest(name=name):
                self.assertFalse(self.ignored(name))

    def test_codec_fixtures_and_reviewed_text_evidence_remain_visible(self):
        for name in ['src/goldens/example.m4a', 'src/goldens/example.mp4',
                     'lab/frame960/vectors/example.m4a', 'src/goldens/reference.s16',
                     'corpus/fuzz/truncated.bin', 'lab/release-audit/runs/example.json',
                     'lab/reviewed-failure.log', 'lab/native-reference.f32']:
            with self.subTest(name=name):
                self.assertFalse(self.ignored(name))


if __name__ == '__main__':
    unittest.main()
