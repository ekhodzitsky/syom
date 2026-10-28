#!/usr/bin/env python3
"""Offline, paired HTTP ASR canary in temporary loopback server processes."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import socket
import subprocess
import time
import urllib.error
import urllib.request

OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as f:
        for part in iter(lambda: f.read(1 << 20), b""):
            h.update(part)
    return h.hexdigest()


def request(url, data=None, timeout=120):
    started = time.monotonic()
    req = urllib.request.Request(url, data=data,
                                 headers={"Content-Type": "application/octet-stream"})
    try:
        with OPENER.open(req, timeout=timeout) as response:
            status, body = response.status, response.read()
    except urllib.error.HTTPError as error:
        status, body = error.code, error.read()
    try:
        value = json.loads(body)
    except json.JSONDecodeError:
        value = {"body": body.decode(errors="replace")}
    return status, value, (time.monotonic() - started) * 1000


def normalized(text):
    return " ".join(re.findall(r"\w+", text.casefold().replace("ё", "е")))


def rss(pid):
    try:
        return {line.split(":")[0]: line.split(":", 1)[1].strip()
                for line in Path(f"/proc/{pid}/status").read_text().splitlines()
                if line.startswith(("VmRSS:", "VmHWM:"))}
    except FileNotFoundError:
        return {}


def percentile(values, q):
    values = sorted(values)
    return values[min(len(values)-1, int((len(values)-1)*q + 0.999999))]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--baseline", type=Path, required=True)
    p.add_argument("--candidate", type=Path, required=True)
    p.add_argument("--model-dir", type=Path, required=True)
    p.add_argument("--ort", type=Path, required=True)
    p.add_argument("--manifest", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--max-latency-ratio", type=float, default=1.5)
    p.add_argument("--max-rss-ratio", type=float, default=1.25)
    p.add_argument("--rss-slack-mib", type=int, default=32)
    args = p.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    used = set()

    def port():
        while True:
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                value = sock.getsockname()[1]
            if value not in used:
                used.add(value)
                return value

    instances = {}
    result = {"cases": [], "load_before": Path("/proc/loadavg").read_text().strip(),
              "runtime_sha256": digest(args.ort), "instances": {},
              "thresholds": {"max_latency_ratio": args.max_latency_ratio,
                             "max_rss_ratio": args.max_rss_ratio,
                             "rss_slack_mib": args.rss_slack_mib}}
    try:
        for name, binary in [("baseline", args.baseline), ("candidate", args.candidate)]:
            http, metrics = port(), port()
            cache = out / "cache" / name
            cache.mkdir(parents=True, exist_ok=True)
            cmd = [str(binary.resolve()), "serve", "--offline", "--host", "127.0.0.1",
                   "--port", str(http), "--model-dir", str(args.model_dir.resolve()),
                   "--model-variant", "rnnt", "--pool-size", "1",
                   "--encoder-intra-threads", "2", "--punctuation", "off", "--itn", "off",
                   "--execution-provider", "cpu", "--metrics", "--metrics-listen",
                   f"127.0.0.1:{metrics}", "--shutdown-drain-secs", "1"]
            env = {k: v for k, v in os.environ.items() if not k.startswith("GIGASTT_")}
            env.update(GIGASTT_OFFLINE="1", ORT_DYLIB_PATH=str(args.ort.resolve()),
                       GIGASTT_OPTIMIZED_CACHE_DIR=str(cache), RAYON_NUM_THREADS="2")
            log = (out / f"{name}.log").open("w")
            proc = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT,
                                    env=env, start_new_session=True)
            instances[name] = (proc, log, http, metrics)
            result["instances"][name] = {"binary_sha256": digest(binary), "command": cmd}
            deadline = time.monotonic() + 180
            while time.monotonic() < deadline:
                if proc.poll() is not None:
                    raise RuntimeError(f"{name} exited {proc.returncode}; see its log")
                try:
                    status, health, _ = request(f"http://127.0.0.1:{http}/health", timeout=2)
                    if status == 200 and health.get("model") not in (None, "loading"):
                        result["instances"][name]["health"] = health
                        break
                except (urllib.error.URLError, TimeoutError, ConnectionError):
                    pass
                time.sleep(0.2)
            else:
                raise TimeoutError(f"{name} readiness timed out")
        cases = json.loads(args.manifest.read_text())
        # One warmup per server is recorded separately from paired requests.
        warmup = Path(cases[0]["path"]).read_bytes()
        for name, (_, _, http, _) in instances.items():
            status, _, ms = request(f"http://127.0.0.1:{http}/v1/transcribe", warmup)
            if status != 200:
                raise RuntimeError(f"{name} warmup returned {status}")
            result["instances"][name]["warmup_ms"] = ms
        for index, case in enumerate(cases):
            payload = Path(case["path"]).read_bytes()
            row = {"id": case["id"], "kind": case["kind"],
                   "sha256": hashlib.sha256(payload).hexdigest(), "responses": {}}
            order = ["baseline", "candidate"] if index % 2 == 0 else ["candidate", "baseline"]
            for name in order:
                proc, _, http, _ = instances[name]
                status, body, ms = request(f"http://127.0.0.1:{http}/v1/transcribe", payload)
                text = normalized(body.get("text", ""))
                row["responses"][name] = {"status": status, "wall_ms": ms,
                    "text_sha256": hashlib.sha256(text.encode()).hexdigest(),
                    "word_count": len(text.split()), "duration": body.get("duration"),
                    "process_memory": rss(proc.pid)}
                if status >= 500:
                    row["responses"][name]["error"] = body
            a, b = row["responses"]["baseline"], row["responses"]["candidate"]
            row["pass"] = (a["status"] == b["status"] == 200
                           and a["word_count"] > 0 and a["text_sha256"] == b["text_sha256"])
            result["cases"].append(row)
            print(row["id"], row["pass"], flush=True)
        result["negative"] = []
        m4a = next(c for c in cases if c["kind"] == "m4a")
        data = Path(m4a["path"]).read_bytes()
        for label, payload in [("garbage", b"not an audio recording"), ("truncated-m4a", data[:-32])]:
            row = {"id": label, "responses": {}}
            for name, (_, _, http, _) in instances.items():
                status, _, ms = request(f"http://127.0.0.1:{http}/v1/transcribe", payload)
                row["responses"][name] = {"status": status, "wall_ms": ms}
            a, b = (row["responses"][n]["status"] for n in ["baseline", "candidate"])
            row["pass"] = a == b and 400 <= a < 500
            result["negative"].append(row)
        for name, (proc, _, http, metrics) in instances.items():
            with OPENER.open(f"http://127.0.0.1:{metrics}/metrics", timeout=5) as response:
                (out / f"{name}.prom").write_bytes(response.read())
            values = [c["responses"][name]["wall_ms"] for c in result["cases"]]
            result["instances"][name]["timing_ms"] = {
                "count": len(values), "median": percentile(values, 0.5),
                "p95": percentile(values, 0.95), "p99": percentile(values, 0.99)}
            result["instances"][name]["final_memory"] = rss(proc.pid)
            status, health, _ = request(f"http://127.0.0.1:{http}/health", timeout=5)
            result["instances"][name]["health_after"] = health
            result["instances"][name]["healthy_after"] = (
                status == 200 and health.get("model") not in (None, "loading"))
        base = result["instances"]["baseline"]
        cand = result["instances"]["candidate"]
        base_peak = int(base["final_memory"]["VmHWM"].split()[0])
        cand_peak = int(cand["final_memory"]["VmHWM"].split()[0])
        result["gates"] = {
            "responses_and_transcripts": all(c["pass"] for c in result["cases"] + result["negative"]),
            "duration_parity": all(
                c["responses"]["baseline"]["duration"] is not None
                and c["responses"]["candidate"]["duration"] is not None
                and abs(c["responses"]["baseline"]["duration"]
                        - c["responses"]["candidate"]["duration"]) <= 1/16000
                for c in result["cases"]),
            "health_after": base["healthy_after"] and cand["healthy_after"],
            "p95": cand["timing_ms"]["p95"] <= base["timing_ms"]["p95"] * args.max_latency_ratio,
            "p99": cand["timing_ms"]["p99"] <= base["timing_ms"]["p99"] * args.max_latency_ratio,
            "peak_rss": cand_peak <= base_peak * args.max_rss_ratio + args.rss_slack_mib * 1024}
        result["pass"] = all(result["gates"].values())
    except Exception as error:
        result["pass"] = False
        result["failure"] = str(error)
    finally:
        for name, (proc, log, _, _) in instances.items():
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
            log.close()
            result["instances"][name]["exit_code"] = proc.returncode
        result["load_after"] = Path("/proc/loadavg").read_text().strip()
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return int(not result["pass"])


if __name__ == "__main__":
    raise SystemExit(main())
