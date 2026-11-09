"""Behavioural regressions for checkout-local documentation verification."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("doc_links", Path(__file__).with_name("check-doc-links.py"))
LINKS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LINKS)


class DocumentationLinkTests(unittest.TestCase):
    def test_heading_punctuation_duplicates_and_explicit_aliases(self):
        text = '# A 0.10.1\n## A 0.10.1\n<a id="quick-start"></a>\n'
        self.assertEqual(LINKS.anchors(text), {"a-0101", "a-0101-1", "quick-start"})

    def test_explicit_alias_does_not_invent_an_extra_heading_slug(self):
        text = '<a id="a"></a>\n# A\n'
        self.assertEqual(LINKS.anchors(text), {"a"})

    def test_code_is_not_a_navigation_link_and_unclosed_fences_fail(self):
        text = '[real](a.md)\n```rust\n[not a link](missing.md)\n```\n`[inline](missing.md)`'
        self.assertEqual(list(LINKS.links(text)), ["a.md"])
        with self.assertRaisesRegex(ValueError, "unclosed"):
            list(LINKS.links('```rust\nnot closed'))

    def test_linked_image_checks_both_asset_and_destination(self):
        self.assertEqual(list(LINKS.links("[![label](badge.svg)](guide.md#top)")),
                         ["badge.svg", "guide.md#top"])

    def test_references_and_encoded_file_names_resolve(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "root.md").write_text('[guide][g]\n[g]: <a%20b.md#heading>\n')
            (root / "a b.md").write_text('# Heading\n')
            self.assertEqual(LINKS.check(root, ["root.md"])["errors"], [])

    def test_repository_urls_check_this_checkout_and_bad_anchors_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a.md").write_text('# Examples\n')
            (root / "root.md").write_text(
                '[a](https://github.com/ekhodzitsky/syom/blob/main/a.md#examples)\n'
                '[b](a.md#missing)\n[web](https://example.com)\n')
            report = LINKS.check(root, ["root.md"])
            self.assertEqual(report["local_links_checked"], 2)
            self.assertEqual(report["external_links_not_checked"], 1)
            self.assertEqual(len(report["errors"]), 1)
            self.assertIn("missing anchor", report["errors"][0])

    def test_missing_targets_and_outside_paths_are_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "root.md").write_text('[a](missing.md)\n[b](../outside.md)\n')
            self.assertEqual(len(LINKS.check(root, ["root.md"])["errors"]), 2)


if __name__ == "__main__":
    unittest.main()
