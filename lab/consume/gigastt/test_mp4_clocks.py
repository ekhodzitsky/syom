"""Small structural parser controls; synthetic boxes contain no audio."""
import unittest

from mp4_clocks import parse_clocks


def box(name, payload):
    return (8 + len(payload)).to_bytes(4, 'big') + name.encode() + payload


def clocks(mutation=''):
    def payload(size, fields):
        data = bytearray(size)
        for offset, value in fields.items():data[offset:offset + 4] = value.to_bytes(4, 'big')
        return bytes(data)
    mvhd = payload(100, {12:1000, 16:3344})
    if mutation == 'version':mvhd = b'\1' + mvhd[1:]
    header = box('mvhd', mvhd)
    if mutation == 'duplicate_clock':header += header
    tkhd = box('tkhd', payload(84, {12:1, 20:3344}))
    edit = box('edts', box('elst', payload(20, {4:1, 8:3344, 12:1024, 16:65536})))
    table = payload(24, {4:2, 8:53, 12:1024, 16:1, 20:244})
    if mutation == 'table_count':table = table[:4] + (3).to_bytes(4, 'big') + table[8:]
    media = box('mdhd', payload(24, {12:16000, 16:54516}))
    media += box('minf', box('stbl', box('stts', table)))
    track = box('trak', tkhd + edit + box('mdia', media))
    if mutation == 'duplicate_track':track += track
    result = box('moov', header + track)
    if mutation == 'truncated':result = result[:-1]
    if mutation == 'wide_box':result = b'\0\0\0\1' + result[4:]
    return result


class ClockParserTests(unittest.TestCase):
    def test_clock_parser_records_exact_movie_edit_media_sample_table(self):
        result = parse_clocks(clocks())
        self.assertEqual(result['movie_duration'], result['edit_duration'])
        self.assertEqual(result['media_duration'], 54516)
        self.assertEqual(result['stts_duration'], 54516)
        self.assertEqual(result['stts_entries'], [[53, 1024], [1, 244]])
        self.assertEqual(result['media_time'], 1024)

    def test_clock_parser_rejects_unsupported_duplicate_or_truncated_shapes(self):
        for mutation in ['version', 'duplicate_clock', 'duplicate_track', 'table_count', 'truncated', 'wide_box']:
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                parse_clocks(clocks(mutation))


if __name__ == '__main__':
    unittest.main()
