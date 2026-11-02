#!/usr/bin/env python3
"""Rebuild pinned offline HE development comparison; retain no audio in git.

Needs the existing FDK 2.0.3 static prefix, ffmpeg/ffprobe, and Rust. This
is same-requested-rate evidence, not a listening or matched-rate quality gate.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def capture(command):
    return subprocess.check_output(command, cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--fdk-prefix', type=Path,
                        default=ROOT / 'target/lc-speed/native/fdk-prefix')
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if (out / 'report.json').exists():
        parser.error('report exists; choose a new output directory to preserve it')
    prefix = args.fdk_prefix.resolve()
    library = prefix / 'lib/libfdk-aac.a'
    if not library.is_file():
        parser.error('build the pinned FDK prefix first; see lab/fdk/PIN.md')
    temp_root = ROOT / 'target/he-requalify-work'
    temp_root.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, FDK_PREFIX=str(prefix), CARGO_TARGET_DIR=str(ROOT / 'target'),
               TMPDIR=str(temp_root))
    commands = []

    def run(command, name):
        with (out / name).open('w') as log:
            result = subprocess.run(command, cwd=ROOT, env=env,
                                    stdout=log, stderr=subprocess.STDOUT)
        commands.append(dict(command=command, exit_code=result.returncode, log=name))
        result.check_returncode()

    run(['sh', 'lab/fdk/build_driver.sh'], 'fdk-build.log')
    run(['cargo', 'build', '--release', '--locked', '--offline', '--manifest-path',
         'lab/quality/Cargo.toml', '--bin', 'syom_tune133'], 'he-build.log')
    run(['taskset', '-c', '0', 'target/release/syom_tune133', 'speech2'], 'results.log')
    keys = ['clip', 'engine', 'requested_kbps', 'adts_source_duration_kbps',
            'alignment_samples', 'source_samples_per_channel',
            'decoded_samples_per_channel', 'scored_samples_per_channel',
            'waveform_snr_db', 'lf_magnitude_snr_db', 'hf_absolute_level_error_db', 'note']
    rows = []
    for line in (out / 'results.log').read_text().splitlines():
        if any('| ' + engine + ' |' in line for engine in ['syom-he1', 'fdk-he1']):
            values = [part.strip() for part in line.strip('|').split('|')]
            if len(values) != len(keys):
                raise ValueError('unexpected score table shape')
            row = dict(zip(keys, values))
            if row['source_samples_per_channel'] != row['scored_samples_per_channel']:
                raise ValueError('incomplete reference scoring')
            rows.append(row)
    if len(rows) != 8:
        raise ValueError('expected all eight development comparison rows')
    artifacts = {}
    work = temp_root / 'syom-tune133-speech2'
    for path in sorted(work.glob('*')):
        if path.suffix not in ('.adts', '.f32'):
            continue
        item = dict(sha256=sha(path), bytes=path.stat().st_size)
        if path.name.endswith('.scored.adts'):
            item['streams'] = json.loads(capture([
                'ffprobe', '-v', 'error', '-show_entries',
                'stream=codec_name,profile,sample_rate,channels', '-of', 'json', str(path)
            ]))['streams']
            if len(item['streams']) != 1 or item['streams'][0]['sample_rate'] != '48000':
                raise ValueError('unexpected stream rate/shape')
        artifacts[path.name] = item
    expected_streams = {f"{row['clip']}-{row['engine']}-24.scored.adts" for row in rows}
    actual_streams = {name for name in artifacts if name.endswith('.scored.adts')}
    if actual_streams != expected_streams:
        raise ValueError('missing or unexpected scored stream artifacts')
    sources = capture(['git', 'ls-files', 'src', 'Cargo.toml', 'Cargo.lock']).splitlines()
    sources += ['lab/quality/src/tune133.rs', 'lab/quality/src/complete_alignment.rs',
                'lab/quality/Cargo.toml', 'lab/quality/Cargo.lock', 'lab/fdk/fdk_adapt.c',
                'lab/fdk/fdk_driver.c', 'lab/fdk/fdk_adapt.h', 'lab/fdk/build_driver.sh',
                'lab/score/Cargo.toml', 'lab/quality/requalify_he.py']
    for folder in ['lab/quality/src', 'lab/score/src']:
        sources += [str(path.relative_to(ROOT)) for path in (ROOT / folder).rglob('*.rs')]
    (out / 'source-manifest.json').write_text(json.dumps(
        {name: sha(ROOT / name) for name in sources if (ROOT / name).is_file()}, indent=2) + '\n')
    report = dict(
        timestamp_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
        base_commit=capture(['git', 'rev-parse', 'HEAD']),
        modified_tracked_files=capture(['git', 'diff', '--name-only']).splitlines(),
        host=platform.platform(), cpu=capture(['lscpu']),
        rustc=capture(['rustc', '--version']),
        ffmpeg=capture(['ffmpeg', '-version']).splitlines()[0],
        environment={key: env[key] for key in ['FDK_PREFIX', 'CARGO_TARGET_DIR', 'TMPDIR']},
        commands=commands,
        hashes={str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path): sha(path)
                for path in [ROOT / 'target/release/syom_tune133',
                             ROOT / 'lab/fdk/fdk_driver', library]},
        scope='Same requested 24 kbps, not matched actual rate; development fixtures only. '
              'No listening, timing, or quality superiority qualification. '
              'Mono may be inferred as HE-AACv2 stereo; scorer uses first decoded channel. '
              'Spectral metrics use complete 2048-sample blocks only.',
        rows=rows, temporary_artifacts=artifacts)
    (out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'Recorded {len(rows)} rows and {len(artifacts)} temporary artifact hashes in {out}')


if __name__ == '__main__':
    main()
