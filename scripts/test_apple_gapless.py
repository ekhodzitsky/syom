"""Offline checks for the independent native interoperability evidence reader."""
import importlib.util
from pathlib import Path
import struct
import unittest
from unittest.mock import patch
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("apple_gapless", ROOT / "lab/apple/gapless.py")
APPLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(APPLE)


def box(typ, body, large=False):
    if large:
        return struct.pack(">I4sQ", 1, typ, len(body) + 16) + body
    return struct.pack(">I4s", len(body) + 8, typ) + body


def fixture(data_type=1, duplicate=False):
    item = box(b"mean", bytes(4) + b"com.apple.iTunes")
    item += box(b"name", bytes(4) + b"iTunSMPB")
    item += box(b"data", struct.pack(">II", data_type, 0) +
                b" 00000000 00000840 000001A0 0000000000056220")
    items = box(b"----", item) * (2 if duplicate else 1)
    meta = box(b"meta", bytes(4) + box(b"ilst", items))
    edit = box(b"trak", box(b"edts", box(b"elst", bytes(20))))
    return box(b"ftyp", b"M4A " + bytes(4)) + box(b"moov", edit + box(b"udta", meta), True)


class NativeGaplessEvidenceTests(unittest.TestCase):
    def test_native_help_exit_two_is_explicit_and_does_not_relax_conversion(self):
        result = subprocess.CompletedProcess(["afconvert", "-hf"], 2, "",
                                             "Audio file and data formats:\n")
        report = {"commands": []}
        with patch.object(APPLE.subprocess, "run", return_value=result):
            listing = APPLE.run(["afconvert", "-hf"], report,
                                allowed=(0, 2), include_stderr=True)
            self.assertIn("Audio file and data formats:", listing)
            with self.assertRaises(RuntimeError):
                APPLE.run(["afconvert", "-f", "m4af", "in.wav", "out.m4a"], report)
        self.assertEqual([c["exit_code"] for c in report["commands"]], [2, 2])

    def test_extracts_native_counts_and_only_renames_edit_type(self):
        original = fixture()
        tag, patched, edits = APPLE.inspect(original)
        self.assertEqual((tag["priming"], tag["remainder"], tag["samples"]), (2112, 416, 352800))
        self.assertEqual(edits, 1)
        self.assertEqual(len(patched), len(original))
        at = original.index(b"elst")
        self.assertEqual(patched, original[:at] + b"free" + original[at + 4:])
        self.assertEqual(APPLE.inspect(patched)[0], tag)

    def test_records_data_type_without_normalising_native_bytes(self):
        self.assertEqual(APPLE.inspect(fixture(0))[0]["data_type"], 0)
        self.assertEqual(APPLE.inspect(fixture(1))[0]["data_type"], 1)

    def test_rejects_duplicate_native_items(self):
        with self.assertRaisesRegex(ValueError, "expected one"):
            APPLE.inspect(fixture(duplicate=True))

    def test_never_uses_text_outside_an_item(self):
        with self.assertRaisesRegex(ValueError, "expected one"):
            APPLE.inspect(box(b"moov", box(b"free",
                b"iTunSMPB 00000000 00000840 000001A0 0000000000056220")))

    def test_rejects_truncated_or_parent_crossing_box_headers(self):
        for data in [struct.pack(">I4s", 7, b"moov"),
                     struct.pack(">I4s", 20, b"moov"),
                     struct.pack(">I4s", 1, b"moov"),
                     struct.pack(">I4sQ", 1, b"moov", 2**63)]:
            with self.subTest(data=data), self.assertRaises(ValueError):
                list(APPLE.boxes(data))


if __name__ == "__main__":
    unittest.main()
