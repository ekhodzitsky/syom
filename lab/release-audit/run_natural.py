#!/usr/bin/env python3
"""Local recordings only; preserve oracle, timing and presentation alignment metadata."""
import argparse
import json
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("out", type=Path)
    parser.add_argument("--reps", type=int, default=101)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    binary = ROOT / "lab/release-audit/target/release"
    rows = []
    for case in json.loads(args.manifest.read_text()):
        folder = args.out / case["id"]
        folder.mkdir(exist_ok=True)
        started = time.time()
        with (folder / "driver.log").open("w") as log:
            run = subprocess.run(["taskset", "-c", "0", str(binary / "natural"),
                                  case["path"], str(folder), str(args.reps)],
                                 cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=300)
        row = {"id": case["id"], "recording_id": case["recording_id"],
               "kind": case["kind"], "sha256": case["sha256"], "driver_exit": run.returncode}
        if run.returncode:
            rows.append(row)
            continue
        info = json.loads((folder / "syom.json").read_text())
        probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "a:0",
                                "-show_entries", "stream=sample_rate,channels,duration_ts,time_base",
                                "-of", "json", case["path"]], capture_output=True, text=True, check=True)
        stream = json.loads(probe.stdout)["streams"][0]
        reference = folder / "reference.f32"
        subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-y", "-i", case["path"],
                        "-map", "0:a:0", "-c:a", "pcm_f32le", "-f", "f32le", str(reference)],
                       check=True, capture_output=True)
        rate, channels = int(stream["sample_rate"]), int(stream["channels"])
        raw_samples = reference.stat().st_size // (4 * channels)
        v = info["validation"]
        shape = rate == v["rate"] and channels == v["channels"]
        compared_samples = raw_samples
        # FFmpeg can emit a padded final AAC frame beyond an M4A edit duration.
        # Trim only to independently probed presentation duration, recording both lengths.
        presentation = None
        if Path(case["path"]).suffix.lower() in (".m4a", ".mp4") and "duration_ts" in stream:
            from fractions import Fraction
            presentation = round(int(stream["duration_ts"]) * Fraction(stream["time_base"]) * rate)
            if raw_samples >= presentation and v["samples"] == presentation:
                compared_samples = presentation
                with reference.open("r+b") as f:
                    f.truncate(presentation * channels * 4)
        compare = subprocess.run([str(binary / "compare_pcm"), str(reference),
                                  str(folder / "syom.f32"), str(channels)], capture_output=True, text=True)
        score = json.loads(compare.stdout)
        row.update(shape_match=shape, rate=rate, channels=channels, samples=v["samples"],
                   reference_raw_samples=raw_samples, reference_compared_samples=compared_samples,
                   presentation_samples=presentation, oracle=score,
                   seek_exact=all(s["exact_remaining_pcm"] for s in v["seek"]),
                   timing_ms={k: x for k, x in info["one_shot_decode_ms"].items() if k != "raw_ms"},
                   elapsed_seconds=time.time()-started)
        row["pass"] = (shape and score.get("comparable", False) and row["seek_exact"]
                       and all(c["max_abs_s16_lsb"] <= 2 for c in score.get("channels", [])))
        (folder / "comparison.json").write_text(json.dumps(row, indent=2) + "\n")
        rows.append(row)
        print(case["id"], row["pass"], flush=True)
        # Audio is private input/derived data, not evidence to commit. Retain statistics.
        reference.unlink()
        (folder / "syom.f32").unlink()
    (args.out / "summary.json").write_text(json.dumps(rows, indent=2) + "\n")
    return int(any(not r.get("pass", False) for r in rows))


if __name__ == "__main__":
    raise SystemExit(main())
