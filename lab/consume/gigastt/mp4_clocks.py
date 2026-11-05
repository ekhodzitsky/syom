"""Strict clock evidence for two pinned AAC fixtures; no general MP4 parser."""
import hashlib
from pathlib import Path

CLOCK_CASES = {
    'golos_09-aac': dict(input_sha256='51b7b6d7c8d39a79356eec609180575749d4e4dbcbfd684987966544d31589ac',
        source_wav_sha256='a23daa8fa6529d02a7e931fb507ac3be998de6c3e10475c693d4b7fc2f086b68',
        source_wav_frames=53492, surplus_samples=12, stts_entries=[[53, 1024], [1, 244]]),
    'golos_10-aac': dict(input_sha256='ee8b94dde3d282459849af00038043976989706011cfd5479a687fb247565eb3',
        source_wav_sha256='fd681926b2d8087d1772246d6c95110695f253190b0cef581d25c80977477d81',
        source_wav_frames=62499, surplus_samples=13, stts_entries=[[62, 1024], [1, 35]]),
}


def parse_clocks(data):
    """Reject unsupported box widths/versions, duplicate paths and truncation."""
    found = {}
    containers = {'moov', 'trak', 'edts', 'mdia', 'minf', 'stbl'}
    wanted = {'mvhd', 'tkhd', 'mdhd', 'elst', 'stts'}

    def walk(start, end, parent=''):
        at = start
        while at < end:
            if at + 8 > end:raise ValueError('truncated box header')
            size = int.from_bytes(data[at:at + 4], 'big')
            name = data[at + 4:at + 8].decode('latin1')
            if size < 8 or at + size > end:raise ValueError('unsupported or truncated box size')
            path = parent + '/' + name
            if name in containers | wanted:
                if path in found:raise ValueError('duplicate clock/container path')
                found[path] = data[at + 8:at + size]
            if name in containers:walk(at + 8, at + size, path)
            at += size

    walk(0, len(data))
    expected_containers = {'/moov', '/moov/trak', '/moov/trak/edts', '/moov/trak/mdia',
                           '/moov/trak/mdia/minf', '/moov/trak/mdia/minf/stbl'}
    if {p for p in found if p.rsplit('/', 1)[-1] in containers} != expected_containers:
        raise ValueError('unsupported container shape')
    paths = {'mvhd': '/moov/mvhd', 'tkhd': '/moov/trak/tkhd',
             'mdhd': '/moov/trak/mdia/mdhd', 'elst': '/moov/trak/edts/elst',
             'stts': '/moov/trak/mdia/minf/stbl/stts'}
    if {p for p in found if p.rsplit('/', 1)[-1] in wanted} != set(paths.values()):
        raise ValueError('unsupported clock box shape')
    try:parts = {name: found[path] for name, path in paths.items()}
    except KeyError as error:raise ValueError('missing clock box') from error
    for name, payload in parts.items():
        if len(payload) < 4 or payload[0] != 0:raise ValueError('unsupported clock version')
        if name in {'mvhd', 'tkhd', 'mdhd', 'elst'} and len(payload) != {'mvhd':100, 'tkhd':84, 'mdhd':24, 'elst':20}[name]:
            raise ValueError('unsupported clock payload shape')
    def u32(name, offset):return int.from_bytes(parts[name][offset:offset + 4], 'big')
    if u32('elst', 4) != 1 or parts['elst'][1:4] != b'\0\0\0':
        raise ValueError('unsupported edit list')
    count = u32('stts', 4)
    if not count or len(parts['stts']) != 8 + count * 8:raise ValueError('truncated sample-time table')
    entries = [[u32('stts', 8 + i * 8), u32('stts', 12 + i * 8)] for i in range(count)]
    return dict(movie_timescale=u32('mvhd', 12), movie_duration=u32('mvhd', 16),
        track_duration=u32('tkhd', 20), track_id=u32('tkhd', 12),
        media_timescale=u32('mdhd', 12), media_duration=u32('mdhd', 16),
        edit_duration=u32('elst', 8), media_time=int.from_bytes(parts['elst'][12:16], 'big', signed=True),
        media_rate_integer=int.from_bytes(parts['elst'][16:18], 'big', signed=True),
        media_rate_fraction=int.from_bytes(parts['elst'][18:20], 'big'), stts_entries=entries,
        stts_duration=sum(n * delta for n, delta in entries))


def clock_evidence(cid, path, wav, frames, rate):
    pin = CLOCK_CASES[cid]
    data = Path(path).read_bytes()
    identity = dict(input_sha256=hashlib.sha256(data).hexdigest(),
                    source_wav_sha256=hashlib.sha256(Path(wav).read_bytes()).hexdigest())
    if any(identity[key] != pin[key] for key in identity):raise ValueError('clock input identity changed')
    proof = dict(**identity, source_wav_frames=frames, source_wav_rate=rate,
                 surplus_samples=pin['surplus_samples'], **parse_clocks(data))
    if not valid_clock_evidence(cid, proof):raise ValueError('unexplained source/container clocks')
    return proof


def valid_clock_evidence(cid, proof):
    pin = CLOCK_CASES.get(cid)
    if pin is None or not isinstance(proof, dict):return False
    integers = ['source_wav_frames', 'source_wav_rate', 'surplus_samples', 'movie_timescale',
                'movie_duration', 'track_duration', 'track_id', 'media_timescale', 'media_duration',
                'edit_duration', 'media_time', 'media_rate_integer', 'media_rate_fraction', 'stts_duration']
    if any(type(proof.get(key)) is not int for key in integers):return False
    if any(proof.get(key) != pin[key] for key in ['input_sha256', 'source_wav_sha256', 'source_wav_frames', 'surplus_samples', 'stts_entries']):return False
    frames = proof['source_wav_frames']; ticks = (frames * 1000 + 15999) // 16000
    return (proof['source_wav_rate'] == proof['media_timescale'] == 16000
            and proof['movie_timescale'] == 1000 and proof['track_id'] == 1
            and proof['movie_duration'] == proof['track_duration'] == proof['edit_duration'] == ticks
            and proof['media_time'] == 1024 and proof['media_rate_integer'] == 1
            and proof['media_rate_fraction'] == 0
            and proof['media_duration'] == proof['stts_duration'] == frames + 1024
            and ticks * 16 - frames == proof['surplus_samples'])
