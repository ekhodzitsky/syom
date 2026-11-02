#!/usr/bin/env python3
"""Acquire three preregistered catalog files; no listening or codec evaluation.

Audio and the detached receipt stay under ignored target/. Requires ffmpeg and
ffprobe. Only the exact fixed_file_excerpts plan URLs are downloaded.
"""
from __future__ import annotations

import argparse
from array import array
import datetime
import hashlib
import json
import math
import re
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PLAN = ROOT / 'corpus/acquisition-plan-v1.json'
IDS = {'nat-librivox-war', 'nat-musopen-chopin', 'nat-fma-percussion'}


def digest(path: Path, algorithm: str = 'sha256') -> str:
    value = hashlib.new(algorithm)
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b''):
            value.update(chunk)
    return value.hexdigest()


def verify_source(path: Path, record: dict, prior_sha: str | None = None) -> None:
    if record.get('source_bytes') is not None and path.stat().st_size != record['source_bytes']:
        raise ValueError('source byte length differs from registered plan')
    for algorithm in ['md5', 'sha1']:
        expected = record.get('source_' + algorithm)
        if expected and digest(path, algorithm) != expected:
            raise ValueError(f'source {algorithm} differs from registered plan')
    if prior_sha and digest(path) != prior_sha:
        raise ValueError('cached source differs from previous SHA-256 receipt')


def verify_and_pin_source(path: Path, record: dict, prior_sha: str | None = None,
                          *, downloaded: bool = False) -> str:
    """Keep the first source hash independently of replaceable run receipts."""
    pin = path.parent / 'source-sha256.txt'
    pinned = pin.read_text().strip() if pin.exists() else None
    for value in [pinned, prior_sha]:
        if value is not None and re.fullmatch(r'[0-9a-f]{64}', value) is None:
            raise ValueError('invalid source SHA-256 pin')
    if pinned and prior_sha and pinned != prior_sha:
        raise ValueError('source pin conflicts with previous receipt')
    if not pinned and prior_sha:
        # Migrate the trusted previous receipt BEFORE checking possibly damaged
        # bytes, so a failure cannot erase the only known-good source hash.
        with pin.open('x') as stream:
            stream.write(prior_sha + '\n')
        pinned = prior_sha
    if not pinned and not downloaded and not any(
            record.get('source_' + algorithm) for algorithm in ['md5', 'sha1']):
        raise ValueError('cached source has no acquisition pin; refusing to adopt it')
    verify_source(path, record, pinned)
    actual = digest(path)
    if not pinned:
        with pin.open('x') as stream:
            stream.write(actual + '\n')
    return actual


def check_pcm(path: Path, rate: int, channels: int) -> int:
    count = rate * 12
    if path.stat().st_size != count * channels * 4:
        raise ValueError('decoded excerpt is not exactly twelve native seconds')
    values = array('f')
    with path.open('rb') as stream:
        values.fromfile(stream, count * channels)
    if sys.byteorder != 'little':
        values.byteswap()
    if not all(math.isfinite(value) for value in values):
        raise ValueError('decoded excerpt contains nonfinite samples')
    return count


def acquire(record: dict, cache: Path, offline: bool, prior: dict) -> dict:
    directory = cache / record['id']
    directory.mkdir(parents=True, exist_ok=True)
    name = record['source_filename']
    if Path(name).name != name:
        raise ValueError('source filename must be a basename')
    source = directory / name
    downloaded = False
    if not source.exists():
        if offline:
            raise ValueError('source absent in offline cache')
        partial = directory / (name + '.download')
        try:
            with urllib.request.urlopen(record['source_url'], timeout=45) as response:
                with partial.open('wb') as dest:
                    shutil.copyfileobj(response, dest)
            verify_source(partial, record)
            partial.replace(source)
            downloaded = True
        finally:
            partial.unlink(missing_ok=True)
    source_sha = verify_and_pin_source(
        source, record, prior.get('source_sha256'), downloaded=downloaded)
    probe_command = ['ffprobe', '-v', 'error', '-select_streams', 'a',
                     '-show_entries', 'stream=codec_name,sample_rate,channels,channel_layout:format=duration',
                     '-of', 'json', str(source)]
    probe = json.loads(subprocess.check_output(probe_command, text=True))
    streams = probe.get('streams', [])
    if len(streams) != 1 or streams[0].get('codec_name') != 'mp3':
        raise ValueError('expected exactly one MP3 audio stream')
    rate, channels = int(streams[0]['sample_rate']), int(streams[0]['channels'])
    if rate <= 0 or channels <= 0:
        raise ValueError('invalid native source shape')
    pcm = directory / 'first-12s-native.f32'
    partial = directory / 'first-12s-native.f32.partial'
    decode = ['ffmpeg', '-nostdin', '-v', 'error', '-y', '-i', str(source),
              '-map', '0:a:0', '-t', '12', '-c:a', 'pcm_f32le', '-f', 'f32le', str(partial)]
    try:
        subprocess.run(decode, check=True, capture_output=True)
        samples = check_pcm(partial, rate, channels)
        partial.replace(pcm)
    finally:
        partial.unlink(missing_ok=True)
    return dict(
        id=record['id'], recording_id=record['recording_id'],
        sha256=digest(pcm), cache_path=str(pcm.resolve()),
        acquired_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        catalog_url=record['catalog_url'], metadata_url=record['metadata_url'],
        catalog_attribution=record['catalog_attribution'], license_url=record['license_url'],
        license_scope_note=record.get('license_scope_note'), caveat=record['caveat'],
        source_url=record['source_url'], source_filename=name,
        source_bytes=source.stat().st_size, source_sha256=source_sha,
        source_pin_path=str((directory / 'source-sha256.txt').resolve()),
        source_md5=digest(source, 'md5'), source_sha1=digest(source, 'sha1'),
        source_probe=probe, source_downloaded_this_run=downloaded,
        rate=rate, channels=channels, valid_samples=samples,
        format='interleaved f32le', finite_samples=True,
        selection=record['selection'], probe_command=probe_command, decode_command=decode,
        scope='Source excerpt acquisition only; no audio listening, codec output, '
              'resampling, loudness normalization, quality or high-band qualification.')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--cache', type=Path, default=ROOT / 'target/corpus-cache/catalog')
    parser.add_argument('--receipt', type=Path, default=ROOT / 'target/corpus-cache/catalog-acquisitions.json')
    parser.add_argument('--offline', action='store_true')
    args = parser.parse_args()
    plan = json.loads(PLAN.read_text())
    records = plan['fixed_file_excerpts']
    if {record['id'] for record in records} != IDS or len(records) != len(IDS):
        raise ValueError('plan must contain exactly the three reserved catalog identities')
    old = json.loads(args.receipt.read_text()) if args.receipt.exists() else {}
    prior = {record['id']: record for record in old.get('records', [])}
    result = dict(version=1, plan_path=str(PLAN.relative_to(ROOT)), plan_sha256=digest(PLAN),
                  ffmpeg=subprocess.check_output(['ffmpeg', '-version'], text=True).splitlines()[0],
                  ffprobe=subprocess.check_output(['ffprobe', '-version'], text=True).splitlines()[0],
                  records=[], failures=[])
    for record in records:
        try:
            acquired = acquire(record, args.cache.resolve(), args.offline, prior.get(record['id'], {}))
            result['records'].append(acquired)
            print(f"{record['id']}: acquired {acquired['rate']} Hz / {acquired['channels']} ch / 12 s", flush=True)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            result['failures'].append(dict(id=record['id'], error=str(error)))
            print(f"{record['id']}: GAP: {error}", flush=True)
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        args.receipt.write_text(json.dumps(result, indent=2) + '\n')
    return int(bool(result['failures']))


if __name__ == '__main__':
    raise SystemExit(main())
