#!/usr/bin/env python3
"""Measure independent native presentation geometry and one fresh ASR reference.

Uses local fixtures, ffprobe/ffmpeg and pinned model files only. Do not run
concurrently with decoder latency campaigns. No files are downloaded.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import wave

from http_contract import CORPUS_SHA256, MODEL_FILES, MODEL_SHA, ORT_SHA, HUMAN_SHA, digest, index_cases
from run_http_canary import normalized
from mp4_clocks import CLOCK_CASES, clock_evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ['baseline-probe', 'candidate-probe', 'candidate-server', 'model-dir',
                 'ort', 'canary', 'manifest', 'out']:
        parser.add_argument('--' + flag, type=Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        parser.error('choose a fresh output directory; prior attempts are retained')
    args.out.mkdir(parents=True)
    dest = args.out / 'independent.json'
    model_files = {name: digest(args.model_dir / name) for name in MODEL_FILES}
    if model_files != MODEL_FILES or digest(args.ort) != ORT_SHA:
        parser.error('model/runtime differs from the pinned current 305 MiB campaign')
    canary = json.loads(args.canary.read_text())
    cases = json.loads(args.manifest.read_text())
    try:
        by_id = index_cases(cases, 'manifest')
        observed = index_cases(canary['cases'], 'canary')
    except ValueError as error:
        parser.error(str(error))
    for row in cases:
        if (digest(row['path']) != CORPUS_SHA256[row['id']]
                or observed[row['id']]['sha256'] != CORPUS_SHA256[row['id']]):
            parser.error('input changed after canary')
    for name in ['baseline', 'candidate']:
        command = canary['instances'][name]['command']
        if Path(command[command.index('--model-dir') + 1]).resolve() != args.model_dir.resolve():
            parser.error('canary used another model directory')
    if digest(args.candidate_server) != canary['instances']['candidate']['binary_sha256']:
        parser.error('ASR reference binary differs from fresh candidate canary')
    commands = []

    def run(command, log):
        result = subprocess.run(command, capture_output=True)
        (args.out / log).write_bytes(result.stderr)
        commands.append(dict(command=command, exit_code=result.returncode, stderr_log=log))
        result.check_returncode()
        return result.stdout

    record = dict(version=1, model_encoder_sha256=MODEL_SHA, model_files=model_files,
                  static_ort_archive_sha256=ORT_SHA, cases={}, transcript_references={},
                  commands=commands, input_canary_sha256=digest(args.canary),
                  probe_binary_sha256={name: digest(getattr(args, name + '_probe'))
                                       for name in ['baseline', 'candidate']})
    for row in cases:
        cid = row['id']
        if row['kind'] not in ['wav', 'm4a']:continue
        source = Path(row['path']).resolve()
        wav = Path(by_id[cid.replace('-aac', '-wav')]['path']).resolve()
        with wave.open(str(wav)) as original:
            wav_frames, wav_rate = original.getnframes(), original.getframerate()
            wav_seconds = wav_frames / wav_rate
        probe_cmd = ['ffprobe', '-v', 'error', '-show_entries', 'format=duration', '-of', 'json', str(source)]
        probe_raw = run(probe_cmd, cid + '-ffprobe.log')
        (args.out / (cid + '-ffprobe.json')).write_bytes(probe_raw)
        presentation = float(json.loads(probe_raw)['format']['duration'])
        ffmpeg_base = ['ffmpeg', '-nostdin', '-v', 'error', '-i', str(source)]
        ffmpeg_output = ['-ar', '16000', '-ac', '1', '-f', 'f32le', 'pipe:1']
        full_pcm = run(ffmpeg_base + ffmpeg_output, cid + '-ffmpeg-full.log')
        ffmpeg_cmd = ffmpeg_base + ['-t', str(presentation)] + ffmpeg_output
        pcm = run(ffmpeg_cmd, cid + '-ffmpeg.log')
        if len(pcm) % 4 or len(full_pcm) % 4:
            raise ValueError('partial float sample in FFmpeg decode')
        if full_pcm[:len(pcm)] != pcm:
            raise ValueError('bounded presentation is not a prefix of the full FFmpeg decode')
        frames = {}
        for name in ['baseline', 'candidate']:
            probe = getattr(args, name + '_probe')
            response_raw = run([str(probe.resolve()), 'file', str(source), '0'], cid + '-' + name + '-probe.log')
            (args.out / (cid + '-' + name + '-probe.json')).write_bytes(response_raw)
            response = json.loads(response_raw)
            if 'error' in response or len(response['planes']) != 1 or not response['planes'][0]['finite']:
                raise ValueError('native decoder probe failed')
            frames[name] = response['planes'][0]['samples']
        record['cases'][cid] = dict(input_sha256=digest(source), source_wav_sha256=digest(wav),
                                    presentation_seconds=presentation, source_wav_seconds=wav_seconds,
                                    source_wav_frames=wav_frames, source_wav_rate=wav_rate,
                                    ffmpeg_samples=len(pcm) // 4,
                                    ffmpeg_applied_presentation_seconds=presentation,
                                    ffmpeg_presentation_pcm_sha256=hashlib.sha256(pcm).hexdigest(),
                                    ffmpeg_full_samples=len(full_pcm) // 4,
                                    ffmpeg_full_pcm_sha256=hashlib.sha256(full_pcm).hexdigest(),
                                    ffmpeg_surplus_samples=(len(full_pcm) - len(pcm)) // 4,
                                    ffmpeg_presentation_is_full_prefix=True,
                                    candidate_samples=frames['candidate'], baseline_samples=frames['baseline'])
        if cid in CLOCK_CASES:
            record['cases'][cid]['clock_rounding'] = clock_evidence(cid, source, wav, wav_frames, wav_rate)
        # Only this one independent presentation is recognized, after native
        # geometry was collected. The source corpus and timed canary stay intact.
        if cid == 'golos_04-aac':
            ref = args.out / 'golos04-ffmpeg-presentation.wav'
            raw = args.out / 'golos04-ffmpeg-presentation.f32'
            raw.write_bytes(pcm)
            run(['ffmpeg', '-nostdin', '-v', 'error', '-y', '-f', 'f32le', '-ar', '16000', '-ac', '1',
                 '-i', str(raw), '-c:a', 'pcm_f32le', str(ref)], 'reference-wave.log')
            result_path = args.out / 'golos04-ffmpeg-asr.json'
            command = [str(args.candidate_server.resolve()), '--offline', 'transcribe', str(ref),
                       '--model-dir', str(args.model_dir.resolve()), '--model-variant', 'rnnt',
                       '--encoder-intra-threads', '2', '--punctuation', 'off', '--itn', 'off',
                       '--format', 'json', '--output', str(result_path)]
            env = {k: v for k, v in os.environ.items() if not k.startswith('GIGASTT_')}
            env.update(GIGASTT_OFFLINE='1', GIGASTT_OPTIMIZED_CACHE_DIR=str(args.out / 'reference-cache'),
                       RAYON_NUM_THREADS='2')
            result = subprocess.run(command, env=env, capture_output=True)
            (args.out / 'reference-asr.log').write_bytes(result.stdout + result.stderr)
            commands.append(dict(command=command, exit_code=result.returncode, log='reference-asr.log'))
            result.check_returncode()
            text = normalized(json.loads(result_path.read_text())['text'])
            record['transcript_references'][cid] = dict(
                input_sha256=digest(source), source_wav_sha256=digest(wav), human_text_sha256=HUMAN_SHA,
                wav_text_sha256=observed['golos_04-wav']['responses']['candidate']['text_sha256'],
                ffmpeg_text_sha256=hashlib.sha256(text.encode()).hexdigest(),
                ffmpeg_input_sha256=digest(ref), asr_result_sha256=digest(result_path))
        print(cid, frames, 'ffmpeg_samples', len(pcm) // 4, flush=True)
    record['ffprobe'] = subprocess.check_output(['ffprobe', '-version'], text=True).splitlines()[0]
    record['ffmpeg'] = subprocess.check_output(['ffmpeg', '-version'], text=True).splitlines()[0]
    dest.write_text(json.dumps(record, indent=2) + '\n')
    print('Independent evidence:', dest)


if __name__ == '__main__':
    main()
