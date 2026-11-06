#!/usr/bin/env python3
"""Pure semantic package-boundary controls; no Cargo or compiler subprocesses."""
import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location("check_package", Path(__file__).with_name("check-package.py"))
CHECK_PACKAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK_PACKAGE)
manifest_errors = CHECK_PACKAGE.manifest_errors

PACKAGE = '[package]\nname = "product"\nversion = "1.0.0"\n'


class PackagePolicyTests(unittest.TestCase):
    def test_original_and_normalized_empty_dependencies_are_allowed(self):
        self.assertEqual(manifest_errors(PACKAGE + '\n[dependencies]\n[build-dependencies]\n'), [])
        self.assertEqual(manifest_errors(PACKAGE + 'build = false\n\n[dependencies]\n'), [])

    def test_current_root_manifest_obeys_policy(self):
        root = Path(__file__).resolve().parents[1]
        self.assertEqual(manifest_errors((root / 'Cargo.toml').read_text(),
                                        build_rs_present=(root / 'build.rs').exists()), [])

    def test_comment_cannot_mask_an_arbitrary_runtime_dependency(self):
        text = PACKAGE + '\n# Intentionally empty\n[dependencies]\nnot_on_any_blacklist = "1"\n'
        errors = manifest_errors(text)
        self.assertTrue(any('[dependencies]' in error and 'not_on_any_blacklist' in error for error in errors))

    def test_runtime_and_build_dependencies_cannot_hide_in_optional_or_renamed_specs(self):
        for table in ('dependencies', 'build-dependencies', 'build_dependencies'):
            for value in ('"1"', '{ version = "1", optional = true }',
                          '{ package = "ffmpeg-sys", path = "lab/native" }'):
                with self.subTest(table=table, value=value):
                    self.assertTrue(manifest_errors(PACKAGE + f'\n[{table}]\narbitrary = {value}\n'))

    def test_dev_dependencies_and_irrelevant_comments_are_allowed(self):
        text = (PACKAGE + '# Intentionally empty is not required; lab/native and ffmpeg-sys are comments.\n'
                '\n[dev-dependencies]\ncriterion = "0.8"\n'
                'comparison = { package = "ffmpeg-sys", path = "lab/native" }\n')
        self.assertEqual(manifest_errors(text), [])
        self.assertEqual(manifest_errors(text.replace('[dev-dependencies]', '[dev_dependencies]')), [])

    def test_native_build_configuration_and_root_build_file_are_refused(self):
        for value in ('true', '"custom-build.rs"', '""', '0'):
            with self.subTest(build=value):
                self.assertTrue(manifest_errors(PACKAGE + f'build = {value}\n'))
        self.assertTrue(manifest_errors(PACKAGE, build_rs_present=True))
        self.assertTrue(manifest_errors(PACKAGE + 'build = false\n', build_rs_present=True))

    def test_links_key_is_refused_even_when_empty(self):
        for value in ('"native-library"', '""'):
            with self.subTest(links=value):
                self.assertTrue(manifest_errors(PACKAGE + f'links = {value}\n'))

    def test_target_tables_are_refused_for_dependency_and_dev_configurations(self):
        for configuration in ('\n[target]\n',
                              '\n[target.\'cfg(unix)\'.dependencies]\narbitrary = "1"\n',
                              '\n[target.\'cfg(unix)\'.dev-dependencies]\nhelper = "1"\n'):
            with self.subTest(configuration=configuration):
                self.assertTrue(any('target-specific' in error for error in manifest_errors(PACKAGE + configuration)))
        self.assertEqual(manifest_errors(PACKAGE + '# [target.unix.dependencies] is only a comment.\n'), [])

    def test_invalid_toml_and_non_table_dependencies_fail_closed(self):
        self.assertTrue(manifest_errors('[package\n'))
        self.assertTrue(manifest_errors('package = "not a table"\n'))
        for table in ('dependencies', 'build-dependencies', 'build_dependencies'):
            self.assertTrue(manifest_errors(f'{table} = false\n' + PACKAGE))

    def test_same_semantic_policy_labels_root_and_packed_failures(self):
        text = PACKAGE + '\n[dependencies]\narbitrary = "1"\n'
        for label in ('root manifest', 'packaged manifest'):
            self.assertTrue(all(error.startswith(label) for error in manifest_errors(text, label=label)))


if __name__ == '__main__':
    unittest.main()
