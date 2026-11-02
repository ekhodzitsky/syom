#!/usr/bin/env python3
"""Acquire three preregistered LibriSpeech excerpts from a local verified archive.

No network, listening, normalization or codec encoding. Audio and detached
receipts belong under ignored target/, never in the generated inventory.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import platform
import sys
import struct
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]


def hashes(path: Path) -> dict:
    algorithms = {name: hashlib.new(name) for name in ('md5', 'sha256')}
    with path.open('rb') as source:
        while chunk := source.read(1 << 20):
            for digest in algorithms.values():
                digest.update(chunk)
    return {name: digest.hexdigest() for name, digest in algorithms.items()}


def ordered_members(members, speaker: int):
    """Numeric chapter/utterance order; accept only matching regular FLACs."""
    selected = []
    pattern = r'LibriSpeech/test-clean/(\d+)/(\d+)/(\d+)-(\d+)-(\d+)\.flac'
    for member in members:
        match = re.fullmatch(pattern, member.name)
        if not match or int(match[1]) != speaker:
            continue
        if not member.isfile() or match[1] != match[3] or match[2] != match[4]:
            raise ValueError(f'inconsistent source member {member.name}')
        selected.append((int(match[2]), int(match[5]), member.name, member))
    return [row[-1] for row in sorted(selected, key=lambda row: row[:3])]


def take_prefix(chunks, samples: int) -> bytes:
    """Concatenate complete native f32 mono samples and keep an exact prefix."""
    result = bytearray()
    for chunk in chunks:
        if len(chunk) % 4:
            raise ValueError('partial f32 sample')
        if any(not math.isfinite(value[0]) for value in struct.iter_unpack('<f', chunk)):
            raise ValueError('nonfinite decoded source')
        result.extend(chunk[:max(0, samples * 4 - len(result))])
        if len(result) == samples * 4:
            return bytes(result)
    raise ValueError('insufficient source samples; no padding or substitution allowed')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, default=ROOT / 'corpus/acquisition-plan-v1.json')
    parser.add_argument('--archive', type=Path,
                        default=ROOT / 'target/corpus-cache/openslr12/test-clean.tar.gz')
    parser.add_argument('--out', type=Path, default=ROOT / 'target/corpus-cache/openslr12')
    parser.add_argument('--receipt', type=Path,
                        default=ROOT / 'target/corpus-cache/librispeech-acquisitions.json')
    args = parser.parse_args()
    plan = json.loads(args.plan.read_text())['librispeech']
    archive_hashes = hashes(args.archive)
    if archive_hashes['md5'] != plan['archive_md5']:
        raise ValueError('archive does not match preregistered upstream MD5')
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    records = []
    tools = {name: subprocess.check_output([name, "-version"], text=True).splitlines()[0]
             for name in ["ffmpeg", "ffprobe"]}
    tools.update(python=sys.version, host=platform.platform())
    with tarfile.open(args.archive, 'r:gz') as archive:
        members = archive.getmembers()
        metadata = {}
        for name in ['LICENSE.TXT', 'README.TXT', 'SPEAKERS.TXT', 'CHAPTERS.TXT', 'BOOKS.TXT']:
            member = archive.getmember('LibriSpeech/' + name)
            if not member.isfile():
                raise ValueError('non-regular attribution metadata')
            data = archive.extractfile(member).read()
            (out / name).write_bytes(data)
            metadata[name] = hashlib.sha256(data).hexdigest()
        for item in plan['records']:
            selected = ordered_members(members, item['speaker_id'])
            used = []
            remaining = plan['excerpt_samples']

            def chunks():
                nonlocal remaining
                for member in selected:
                    source = out / Path(member.name).name
                    source.write_bytes(archive.extractfile(member).read())
                    probe_command = [
                        'ffprobe', '-v', 'error', '-show_entries',
                        'stream=codec_name,sample_rate,channels', '-of', 'json', str(source)
                    ]
                    probe = json.loads(subprocess.check_output(probe_command, text=True))['streams']
                    if (len(probe) != 1 or probe[0]['codec_name'] != 'flac'
                            or int(probe[0]['sample_rate']) != plan['source_sample_rate']
                            or probe[0]['channels'] != plan['source_channels']):
                        raise ValueError('unexpected source rate/channel/codec')
                    decode_command = [
                        'ffmpeg', '-nostdin', '-v', 'error', '-i', str(source),
                        '-f', 'f32le', '-acodec', 'pcm_f32le', 'pipe:1'
                    ]
                    raw = subprocess.check_output(decode_command)
                    count = len(raw) // 4
                    consumed = min(remaining, count)
                    used.append(dict(member=member.name, sha256=hashes(source)['sha256'],
                                     decoded_samples=count, retained_samples=consumed,
                                     probe=probe, probe_command=probe_command, decode_command=decode_command))
                    remaining -= consumed
                    yield raw

            raw = take_prefix(chunks(), plan['excerpt_samples'])
            dest = out / (item['id'] + '.f32le')
            dest.write_bytes(raw)
            records.append(dict(
                id=item['id'], recording_id=item['recording_id'],
                sha256=hashes(dest)['sha256'], cache_path=str(dest),
                format=dict(encoding='f32le', sample_rate=plan['source_sample_rate'],
                            channels=plan['source_channels'], samples_per_channel=plan['excerpt_samples']),
                source_members=used, archive_hashes=archive_hashes, tools=tools,
                catalog_url=plan['catalog_url'], archive_url=plan['archive_url'],
                license=plan['license'], attribution=plan['attribution'],
                attribution_metadata_sha256=metadata,
                plan_sha256=hashes(args.plan)['sha256'], selection=plan['selection']))
    args.receipt.parent.mkdir(parents=True, exist_ok=True)
    args.receipt.write_text(json.dumps(dict(version=1, records=records), indent=2) + '\n')
    print(f'Acquired {len(records)} sealed source excerpts; receipt: {args.receipt}')


if __name__ == '__main__':
    main()
