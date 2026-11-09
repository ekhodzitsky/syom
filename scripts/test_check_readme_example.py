"""README extraction must describe what is executed and what is compile-only."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("readme_examples", Path(__file__).with_name("check-readme-example.py"))
README = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(README)


class ReadmeExampleTests(unittest.TestCase):
    def text(self, blocks):
        return 'cargo add syom@0.10.1\n' + blocks + '\nExpected output: `ok`'

    def test_keeps_order_and_no_run_contract(self):
        version, examples, output = README.examples_from_readme(self.text(
            '```rust\nfn main() {}\n```\n'
            '```rust,no_run\nfn main() {}\n```\n'
            '```rust\nfn main() {}\n```'))
        self.assertEqual(version, "0.10.1")
        self.assertEqual([execute for _, execute in examples], [True, False, True])
        self.assertEqual(output, "ok")

    def test_rejects_ignored_examples_and_a_nonrunning_first_result(self):
        for language in ["rust,ignore", "rust,no_run"]:
            with self.subTest(language=language), self.assertRaises(ValueError):
                README.examples_from_readme(self.text(f'```{language}\nfn main() {{}}\n```'))

    def test_registry_provenance_rejects_path_or_different_release(self):
        package = {"name": "syom", "version": "0.10.1", "checksum": "archive-hash",
                   "source": "registry+https://github.com/rust-lang/crates.io-index"}
        self.assertEqual(README.registry_dependency({"package": [package]}, "0.10.1"), package)
        for broken in [dict(package, version="0.10.0"),
                       {k: v for k, v in package.items() if k != "source"},
                       {k: v for k, v in package.items() if k != "checksum"}]:
            with self.subTest(broken=broken), self.assertRaises(ValueError):
                README.registry_dependency({"package": [broken]}, "0.10.1")

    def test_rejects_an_unclosed_additional_example(self):
        text = self.text('```rust\nfn main() {}\n```')
        with self.assertRaisesRegex(ValueError, "unclosed"):
            README.examples_from_readme(text + '\n```rust\nfn main() {}')

    def test_rejects_ambiguous_version_or_missing_output(self):

        text = self.text('```rust\nfn main() {}\n```')
        with self.assertRaises(ValueError):
            README.examples_from_readme(text + '\ncargo add syom@0.10.2')
        with self.assertRaises(ValueError):
            README.examples_from_readme(text.replace('Expected output: `ok`', ''))


if __name__ == "__main__":
    unittest.main()
