from pathlib import Path
import datetime
import hashlib
import json
import os
import statistics
import subprocess
import time

ROOT = Path('/home/ekhodzitsky/Projects/syom')
OUT = ROOT / 'target/task145/candidate5'
REG = ROOT / 'target/task145/quiet-core-selection/same-class-preregistered-selection.json'
SELECTION = json.loads(REG.read_text())
CPU = SELECTION['selected_logical_cpu']
assert CPU == 1 and SELECTION['selected_smt_siblings'] == [13]
OBSERVER = 8
assert OBSERVER not in [CPU, *SELECTION['selected_smt_siblings']]
HARNESS = Path('/home/ekhodzitsky/.cache/gigastt-worktrees/syom-0.8.1/scripts/compare-aac-decoders.py')
FIXTURES = Path('/home/ekhodzitsky/.cache/gigastt-syom-081/fixtures')
BASE = ROOT / 'target/task145/base-bin/aac_probe'
CANDIDATE = ROOT / 'target/task145/candidate5-bin/aac_probe'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_text(path):
    try:
        return path.read_text().strip()
    except OSError:
        return None


def cpu_stat():
    rows = {}
    for line in Path('/proc/stat').read_text().splitlines():
        fields = line.split()
        if fields[0] in [f'cpu{CPU}', 'cpu13']:
            rows[fields[0]] = [int(value) for value in fields[1:9]]
    return rows


def owned_children(parent):
    result = []
    names = read_text(Path(f'/proc/{parent}/task/{parent}/children')) or ''
    for name in names.split():
        pid = int(name)
        try:
            fields = Path(f'/proc/{pid}/stat').read_text().rsplit(') ', 1)[1].split()
            result.append({'pid': pid, 'state': fields[0], 'utime_ticks': int(fields[11]),
                           'stime_ticks': int(fields[12]), 'threads': int(fields[17]),
                           'last_cpu': int(fields[36]), 'allowed_cpus': sorted(os.sched_getaffinity(pid)),
                           'command': Path(f'/proc/{pid}/cmdline').read_bytes().replace(b'\0', b' ').decode(errors='replace')})
        except (OSError, IndexError, ValueError):
            pass
    return result


validation = json.loads((OUT / 'validation.json').read_text())
assert validation['source_stable'] and validation['finished_utc']
assert all(command['exit_code'] == 0 for command in validation['commands'])
assert sha(CANDIDATE) == validation['final_binaries'][str(CANDIDATE)]['sha256']
os.sched_setaffinity(0, {OBSERVER})
for number in [1, 2]:
    destination = OUT / f'preregistered-cpu1-campaign{number}'
    destination.mkdir(exist_ok=False)
    command = ['taskset', '-c', str(CPU), 'python3', str(HARNESS), '--base-probe', str(BASE),
               '--candidate-probe', str(CANDIDATE), '--fixtures', str(FIXTURES),
               '--output', str(destination / 'decoder-timing.json')]
    meta = {'command': command, 'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'cpu': CPU, 'smt_siblings': [13], 'observer_cpu': OBSERVER,
            'preregistration': {'path': str(REG), 'sha256': sha(REG), 'registered_utc': SELECTION['preregistered_utc']},
            'scope': 'Exactly two preregistered confirmations after independent idle/topology selection; original CPU0 failure retained; no benchmark-based CPU selection or threshold edits',
            'sdk_git_revision': validation['sdk_git_revision'],
            'binary_sha256': {'base': sha(BASE), 'candidate': sha(CANDIDATE)},
            'harness_sha256': sha(HARNESS), 'qualification_manifest_sha256': sha(OUT / 'validation.json'),
            'fixture_sha256': {name: sha(FIXTURES / name) for name in ['speech0.m4a', 'lc.adts', 'long-stereo.m4a']},
            'loadavg_before': list(os.getloadavg()), 'passive_observer': {
                'interval_seconds': 0.1, 'affinity': [OBSERVER],
                'metrics': '/proc/stat CPU1/13 ticks, reported scaling_cur_freq, owned probe PID/args/CPU/affinity/ticks',
                'limits': 'Reported cpufreq is not APERF/MPERF effective frequency; proc ticks quantized; short probes may end between samples. No PMU claims. Sampler only reads files; no loads/governor/process changes.'}}
    (destination / 'manifest.json').write_text(json.dumps(meta, indent=2) + '\n')
    samples = []
    start = time.monotonic()
    observer_start = time.process_time()
    with (destination / 'decoder-timing.log').open('w') as log:
        process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
        while process.poll() is None:
            samples.append({'seconds': time.monotonic() - start, 'cpu_ticks': cpu_stat(),
                            'frequency_khz': {str(cpu): read_text(Path(f'/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq')) for cpu in [CPU, 13]},
                            'owned_probes': owned_children(process.pid)})
            time.sleep(0.1)
        code = process.wait()
    duration = time.monotonic() - start
    observer_seconds = time.process_time() - observer_start
    (destination / 'passive-observer.json').write_text(json.dumps(samples, indent=2) + '\n')
    utilization = {}
    for cpu in [f'cpu{CPU}', 'cpu13']:
        intervals = []
        for previous, current in zip(samples, samples[1:]):
            a = previous['cpu_ticks'][cpu]
            b = current['cpu_ticks'][cpu]
            total = sum(b) - sum(a)
            idle = (b[3] + b[4]) - (a[3] + a[4])
            if total:
                intervals.append(100 * (total - idle) / total)
        utilization[cpu] = {'mean_interval_busy_percent': statistics.mean(intervals),
                            'median_interval_busy_percent': statistics.median(intervals),
                            'max_interval_busy_percent': max(intervals)}
    meta.update(exit_code=code, seconds=duration, loadavg_after=list(os.getloadavg()),
                finished_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                observer_cpu_seconds=observer_seconds, observer_one_core_percent=100 * observer_seconds / duration,
                observer_samples=len(samples), utilization=utilization)
    if (destination / 'decoder-timing.json').exists():
        meta['summary'] = json.loads((destination / 'decoder-timing.json').read_text())['summary']
    (destination / 'manifest.json').write_text(json.dumps(meta, indent=2) + '\n')
    print(json.dumps({'campaign': number, 'exit_code': code, 'ratios': {case: data['median_paired_time_ratio'] for case, data in meta.get('summary', {}).items()},
                      'observer_cpu_seconds': observer_seconds, 'observer_one_core_percent': meta['observer_one_core_percent']}), flush=True)
    if code:
        raise SystemExit('Preregistered confirmation failed; stop with no further CPU selection or reruns')
