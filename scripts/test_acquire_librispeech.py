#!/usr/bin/env python3
"""Offline selection/prefix tests with synthetic bytes, not holdout audio."""
import math
import struct
import tarfile
import unittest

from acquire_librispeech_holdouts import ordered_members, take_prefix


def member(name, kind=tarfile.REGTYPE):
    item = tarfile.TarInfo(name)
    item.type = kind
    return item


class AcquisitionSelectionTests(unittest.TestCase):
    def test_numeric_order_and_reserved_speaker(self):
        paths = [member(f'LibriSpeech/test-clean/1580/{chapter}/1580-{chapter}-{utterance}.flac')
                 for chapter, utterance in [(10, 1), (2, 10), (2, 2)]]
        paths.append(member('LibriSpeech/test-clean/1995/1/1995-1-1.flac'))
        names = [p.name for p in ordered_members(paths, 1580)]
        self.assertEqual(names, [paths[2].name, paths[1].name, paths[0].name])

    def test_unsafe_or_inconsistent_member_rejected(self):
        for path in [member('LibriSpeech/test-clean/1580/2/1995-2-1.flac'),
                     member('LibriSpeech/test-clean/1580/2/1580-2-1.flac', tarfile.SYMTYPE)]:
            with self.assertRaises(ValueError):
                ordered_members([path], 1580)

    def test_exact_prefix_across_members_without_padding(self):
        chunks = [struct.pack('<2f', 1, 2), struct.pack('<3f', 3, 4, 5)]
        self.assertEqual(take_prefix(iter(chunks), 4), struct.pack('<4f', 1, 2, 3, 4))
        with self.assertRaisesRegex(ValueError, 'insufficient'):
            take_prefix(iter(chunks), 6)

    def test_reject_nonfinite_and_partial_samples(self):
        for chunk in [b'123', struct.pack('<f', math.nan), struct.pack('<f', math.inf)]:
            with self.assertRaises(ValueError):
                take_prefix(iter([chunk]), 1)


if __name__ == '__main__':
    unittest.main()
