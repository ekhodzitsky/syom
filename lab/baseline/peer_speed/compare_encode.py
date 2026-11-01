#!/usr/bin/env python3
"""Balanced one-core, in-process LC comparisons; never run by cargo test.

Each binary times complete one-shot encoding/free internally. This script
retains every repetition, verifies untimed output and records host load.
No process wall time is used as codec time; no outlier is removed.
"""
import argparse
import hashlib
import itertools
import json
import math
from pathlib import Path
import statistics
import subprocess
import tempfile


def host():
    return {
        "loadavg": Path("/proc/loadavg").read_text().strip(),
        "cpu0_khz": Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
        .read_text().strip(),
    }


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(path, channels, source_samples, delay):
    probe = subprocess.run([
        "ffprobe", "-v", "error", "-select_streams", "a:0", "-show_entries",
        "stream=profile,sample_rate,channels", "-of", "json", str(path),
    ], check=True, capture_output=True, text=True)
    stream, = json.loads(probe.stdout)["streams"]
    assert stream["profile"] == "LC" and int(stream["sample_rate"]) == 48000
    assert stream["channels"] == channels
    pcm = path.with_suffix(".f32")
    decoded = subprocess.run([
        "ffmpeg", "-v", "error", "-xerror", "-y", "-i", str(path),
        "-f", "f32le", "-acodec", "pcm_f32le", str(pcm),
    ], check=True, capture_output=True)
    assert not decoded.stderr
    raw = pcm.read_bytes()
    assert len(raw) % (4 * channels) == 0
    import array
    samples = array.array("f", raw)
    assert all(math.isfinite(x) for x in samples)
    assert any(abs(x) > 1e-6 for x in samples), "silent decoded PCM"
    count = len(samples) // channels
    # The pinned FFmpeg ADTS lane returns untrimmed frames for both engines.
    required = source_samples + delay
    assert count >= required, (count, required)
    pcm.unlink()
    return count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["baseline", "candidate", "fdk", "fixtures", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--rounds", type=int, default=6)
    parser.add_argument("--warm", type=int, default=4)
    parser.add_argument("--reps", type=int, default=31)
    parser.add_argument("--cells", nargs="+", choices=["voice128", "voice24", "lecture24", "lecture-m24"])
    args = parser.parse_args()
    assert args.rounds > 0 and args.warm >= 0 and args.reps > 0
    args.output.mkdir(parents=True, exist_ok=True)
    binaries = {name: getattr(args, name).resolve() for name in ["baseline", "candidate", "fdk"]}
    cells = [
        ("voice128", "voice-like.f32", 2, 128000),
        ("voice24", "voice-like.f32", 2, 24000),
        ("lecture24", "lecture.f32", 2, 24000),
        ("lecture-m24", "lecture-m.f32", 1, 24000),
    ]
    if args.cells:
        cells = [cell for cell in cells if cell[0] in args.cells]
    rows = []
    orders = list(itertools.permutations(binaries))
    hashes = {}
    pcm_hashes = {}
    pcm_fnv = {}
    with tempfile.TemporaryDirectory(prefix="syom-lc-compare-") as temporary:
        work = Path(temporary)
        for block in range(args.rounds):
            order = orders[block % len(orders)]
            for cell, fixture, channels, bps in cells:
                pcm = (args.fixtures / fixture).resolve()
                source_samples = pcm.stat().st_size // (4 * channels)
                for engine in order:
                    adts = work / f"{engine}-{cell}.adts"
                    binary = str(binaries[engine])
                    common = ["48000", str(channels), str(bps)]
                    if engine == "fdk":
                        command = [binary, "bench-encode", *common, "2", str(pcm),
                                   str(args.warm), str(args.reps), str(adts)]
                    else:
                        command = [binary, "encode-pcm", *common, str(pcm),
                                   str(args.warm), str(args.reps), str(adts)]
                    command = ["taskset", "-c", "0", *command]
                    before = host()
                    result = subprocess.run(command, check=True, capture_output=True, text=True)
                    after = host()
                    raw = args.output / f"{block}-{cell}-{engine}.log"
                    raw.write_text(result.stdout + result.stderr)
                    timed, = [json.loads(line) for line in result.stdout.splitlines()
                              if line.startswith("{") and json.loads(line).get("op") == "encode"]
                    assert len(timed["samples_ms"]) == args.reps
                    assert all(math.isfinite(x) and x > 0 for x in timed["samples_ms"])
                    assert timed["rate"] == 48000 and timed["channels"] == channels
                    assert timed["samples"] == source_samples
                    assert timed["bitrate_bps"] == bps
                    assert timed["warm"] == args.warm and timed["reps"] == args.reps
                    pcm_digest = sha(pcm)
                    assert pcm_hashes.setdefault(cell, pcm_digest) == pcm_digest, "PCM source changed"
                    assert pcm_fnv.setdefault(cell, timed["pcm_fnv1a"]) == timed["pcm_fnv1a"], "different PCM read"
                    digest = sha(adts)
                    key = (cell, engine)
                    assert hashes.setdefault(key, digest) == digest, "nondeterministic encode"
                    decoded = None
                    if block == 0:
                        decoded = validate(adts, channels, source_samples,
                                           timed.get("delay", 1024))
                    row = dict(block=block, cell=cell, variant=engine, command=command,
                               host_before=before, host_after=after, pcm_sha256=pcm_digest,
                               adts_sha256=digest, decoded_samples=decoded, measurement=timed)
                    rows.append(row)
                    print(f"block={block} {cell} {engine}: {timed['median_ms']:.4f} ms", flush=True)
        for cell, *_ in cells:
            assert hashes[cell, "baseline"] == hashes[cell, "candidate"], cell + " bytes changed"
    summaries = []
    for cell, *_ in cells:
        medians = {}
        for engine in binaries:
            values = [time for row in rows if row["cell"] == cell and row["variant"] == engine
                      for time in row["measurement"]["samples_ms"]]
            medians[engine] = statistics.median(values)
        summaries.append(dict(cell=cell, pooled_median_ms=medians,
                              candidate_vs_baseline=medians["candidate"] / medians["baseline"],
                              at_or_below_fdk=medians["candidate"] <= medians["fdk"]))
    report = dict(protocol="cycle the six engine orders on CPU0; complete in-process encode/free; no outlier removal",
                  warm=args.warm, reps=args.reps, rounds=args.rounds,
                  binary_sha256={name: sha(path) for name, path in binaries.items()},
                  summaries=summaries, rows=rows)
    (args.output / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summaries, indent=2))


if __name__ == "__main__":
    main()
