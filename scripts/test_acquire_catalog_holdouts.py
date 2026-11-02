#!/usr/bin/env python3
"""Integrity and full native-duration acquisition regressions; synthetic bytes."""
from array import array
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

from acquire_catalog_holdouts import check_pcm, verify_source, verify_and_pin_source


class CatalogAcquisitionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / 'test.bin'

    def pcm(self, values):
        samples = array('f', values)
        if sys.byteorder != 'little':
            samples.byteswap()
        self.path.write_bytes(samples.tobytes())

    def test_exact_native_stereo_duration(self):
        self.pcm([0.125] * (12 * 3 * 2))
        self.assertEqual(check_pcm(self.path, 3, 2), 36)

    def test_short_and_long_excerpts_fail(self):
        for count in [11, 13]:
            with self.subTest(count=count):
                self.pcm([0.0] * count)
                with self.assertRaisesRegex(ValueError, 'exactly twelve'):
                    check_pcm(self.path, 1, 1)

    def test_nonfinite_samples_fail(self):
        for invalid in [float('nan'), float('inf'), float('-inf')]:
            with self.subTest(invalid=invalid):
                self.pcm([0.0] * 11 + [invalid])
                with self.assertRaisesRegex(ValueError, 'nonfinite'):
                    check_pcm(self.path, 1, 1)

    def test_pinned_checksum_and_length(self):
        payload = b'original source'
        self.path.write_bytes(payload)
        record = dict(source_bytes=len(payload), source_md5=hashlib.md5(payload).hexdigest(),
                      source_sha1=hashlib.sha1(payload).hexdigest())
        verify_source(self.path, record)
        self.path.write_bytes(b'modified source')
        with self.assertRaisesRegex(ValueError, 'md5'):
            verify_source(self.path, record)
        self.path.write_bytes(payload + b'x')
        with self.assertRaisesRegex(ValueError, 'length'):
            verify_source(self.path, record)

    def test_unpinned_cache_still_checks_prior_receipt_hash(self):
        self.path.write_bytes(b'first acquisition')
        prior = hashlib.sha256(self.path.read_bytes()).hexdigest()
        verify_source(self.path, {}, prior)
        self.path.write_bytes(b'changed cache')
        with self.assertRaisesRegex(ValueError, 'previous SHA-256'):
            verify_source(self.path, {}, prior)


    def test_failed_upgrade_retains_prior_hash_after_run_receipt_loses_record(self):
        original_sha = hashlib.sha256(b'first acquisition').hexdigest()
        self.path.write_bytes(b'changed cache')
        with self.assertRaisesRegex(ValueError, 'previous SHA-256'):
            verify_and_pin_source(self.path, {}, original_sha)
        # Simulate the next main() call loading a receipt without the failed row.
        with self.assertRaisesRegex(ValueError, 'previous SHA-256'):
            verify_and_pin_source(self.path, {})
        self.assertEqual((self.path.parent / 'source-sha256.txt').read_text().strip(), original_sha)

    def test_first_download_pin_survives_repeated_cache_failures(self):
        self.path.write_bytes(b'first acquisition')
        first = verify_and_pin_source(self.path, {}, downloaded=True)
        self.path.write_bytes(b'changed cache')
        for _ in range(2):
            with self.assertRaisesRegex(ValueError, 'previous SHA-256'):
                verify_and_pin_source(self.path, {})
        self.assertEqual((self.path.parent / 'source-sha256.txt').read_text().strip(), first)

    def test_unverified_unpinned_cache_is_never_adopted(self):
        self.path.write_bytes(b'cached with no provenance')
        with self.assertRaisesRegex(ValueError, 'refusing to adopt'):
            verify_and_pin_source(self.path, {})

    def test_invalid_pin_is_rejected(self):
        self.path.write_bytes(b'cache')
        (self.path.parent / 'source-sha256.txt').write_text('invalid')
        with self.assertRaisesRegex(ValueError, 'invalid source SHA-256 pin'):
            verify_and_pin_source(self.path, {})


if __name__ == '__main__':
    unittest.main()
