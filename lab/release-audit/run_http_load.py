#!/usr/bin/env python3
"""Offline paired load/fault qualification of actual gigastt HTTP binaries.

Only creates loopback servers owned by this process. Media/caches/logs stay in
the explicitly selected output directory; responses retain hashes, never text.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import http.client
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import threading
import time

from run_http_canary import digest, normalized, percentile, request, rss


def post(port, payload, timeout=90):
    start = time.monotonic()
    conn = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        conn.request("POST", "/v1/transcribe", payload,
                     {"Content-Type": "application/octet-stream"})
        response = conn.getresponse()
        body = json.loads(response.read())
        text = normalized(body.get("text", ""))
        return {"status": response.status, "code": body.get("code"),
                "retry_after": response.getheader("Retry-After"),
                "text_sha256": hashlib.sha256(text.encode()).hexdigest(),
                "word_count": len(text.split()), "duration": body.get("duration"),
                "wall_ms": (time.monotonic() - start) * 1000}
    except (OSError, ValueError, http.client.HTTPException) as error:
        return {"status": None, "error": type(error).__name__,
                "wall_ms": (time.monotonic() - start) * 1000}
    finally:
        conn.close()


def memory_kib(memory, key="VmRSS"):
    return int(memory[key].split()[0])


def matches(response, expected):
    return (response["status"] == 200 and response.get("word_count", 0) > 0
            and response.get("text_sha256") == expected["text_sha256"]
            and isinstance(response.get("duration"), (int, float))
            and abs(response["duration"] - expected["duration"]) <= 1 / 16000)


class Server:
    def __init__(self, name, binary, args, profile):
        self.out = args.out.resolve() / f"{profile}-{name}"
        self.out.mkdir(parents=True, exist_ok=True)
        ports = []
        for _ in range(2):
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                ports.append(sock.getsockname()[1])
        if ports[0] == ports[1]:
            raise RuntimeError("port collision; rerun")
        self.port, self.metrics_port = ports
        self.pool = 2 if profile == "load" else 1
        cmd = [str(binary.resolve()), "serve", "--offline", "--host", "127.0.0.1",
               "--port", str(self.port), "--model-dir", str(args.model_dir.resolve()),
               "--model-variant", "rnnt", "--pool-size", str(self.pool),
               "--encoder-intra-threads", "2" if profile == "load" else "1",
               "--punctuation", "off", "--itn", "off", "--execution-provider", "cpu",
               "--metrics", "--metrics-listen", f"127.0.0.1:{self.metrics_port}",
               "--shutdown-drain-secs", "1", "--pool-checkout-timeout-secs",
               "30" if profile == "load" else "1", "--inference-timeout-secs",
               "1" if profile == "watchdog" else "600"]
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIGASTT_")}
        env.update(GIGASTT_OFFLINE="1", ORT_DYLIB_PATH=str(args.ort.resolve()),
                   GIGASTT_OPTIMIZED_CACHE_DIR=str(self.out / "cache"), RAYON_NUM_THREADS="2")
        self.log = (self.out / "server.log").open("w")
        self.proc = subprocess.Popen(cmd, stdout=self.log, stderr=subprocess.STDOUT,
                                     env=env, start_new_session=True)
        self.record = {"command": cmd, "binary_sha256": digest(binary),
                       "memory_samples": [], "profile": profile}
        self.stop_event = threading.Event()
        self.sampler = threading.Thread(target=self.sample, daemon=True)
        self.sampler.start()

    def sample(self):
        while not self.stop_event.is_set():
            memory = rss(self.proc.pid)
            if "VmRSS" in memory:
                self.record["memory_samples"].append(
                    {"monotonic": time.monotonic(), **memory})
            self.stop_event.wait(0.05)

    def health(self):
        status, body, _ = request(f"http://127.0.0.1:{self.port}/health", timeout=2)
        if status != 200:
            raise RuntimeError(f"health returned {status}")
        status, ready, _ = request(f"http://127.0.0.1:{self.port}/ready", timeout=2)
        if status not in (200, 503):
            raise RuntimeError(f"readiness returned {status}")
        body.update(readiness=ready.get("status"), pool_available=ready.get("pool_available"),
                    pool_total=ready.get("pool_total"), reason=ready.get("reason"))
        return body

    def wait(self, predicate, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if self.proc.poll() is not None:
                raise RuntimeError(f"server exited {self.proc.returncode}")
            try:
                value = self.health()
                if predicate(value):
                    return value
            except (OSError, ConnectionError):
                pass
            time.sleep(0.02)
        raise TimeoutError("health/pool condition did not recover")

    def ready(self):
        return self.wait(lambda h: h.get("model") not in (None, "loading")
                         and h.get("pool_available") == self.pool, 180)

    def recovered(self):
        start = time.monotonic()
        health = self.wait(lambda h: h.get("model") not in (None, "loading")
                           and h.get("pool_available") == self.pool, 30)
        return {"wall_ms": (time.monotonic() - start) * 1000, "health": health}

    def metrics(self, label):
        conn = http.client.HTTPConnection("127.0.0.1", self.metrics_port, timeout=5)
        try:
            conn.request("GET", "/metrics")
            response = conn.getresponse()
            if response.status != 200:
                raise RuntimeError(f"metrics returned {response.status}")
            text = response.read().decode()
            (self.out / f"{label}.prom").write_text(text)
            return text
        finally:
            conn.close()

    def close(self):
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait()
        self.stop_event.set()
        self.sampler.join(timeout=2)
        self.log.close()
        self.record["exit_code"] = self.proc.returncode
        self.record["clean_shutdown"] = self.proc.returncode == 0


def burst(server, cases, expected, concurrency):
    start = time.monotonic()
    def one(case):
        response = post(server.port, Path(case["path"]).read_bytes())
        return {"id": case["id"], "response": response,
                "pass": matches(response, expected[case["id"]])}
    with ThreadPoolExecutor(max_workers=concurrency) as executor:
        rows = list(executor.map(one, cases))
    values = [row["response"]["wall_ms"] for row in rows]
    return {"concurrency": concurrency, "rows": rows, "pass": all(r["pass"] for r in rows),
            "elapsed_ms": (time.monotonic() - start) * 1000,
            "p95_ms": percentile(values, .95), "p99_ms": percentile(values, .99),
            "recovery": server.recovered(), "memory_after": rss(server.proc.pid)}


def disconnect(server, payload, mode):
    conn = http.client.HTTPConnection("127.0.0.1", server.port, timeout=10)
    try:
        conn.request("POST", "/v1/transcribe", payload,
                     {"Content-Type": "application/octet-stream"})
        busy = server.wait(lambda h: h.get("pool_available") == 0, 5)
        observed = "disconnect"
        if mode == "read-timeout":
            conn.sock.settimeout(.001)
            try:
                response = conn.getresponse()
                observed = f"unexpected-response-{response.status}"
            except TimeoutError:
                observed = "TimeoutError"
        conn.sock.shutdown(socket.SHUT_RDWR)
    finally:
        conn.close()
    return {"mode": mode, "busy_health": busy, "observed": observed,
            "recovery": server.recovered()}


def faults(server, short, long, expected, watchdog):
    rows = {}
    server.record["faults"] = rows
    if watchdog:
        rows["timeout"] = post(server.port, long)
        rows["recovery"] = server.recovered()
        rows["pass"] = (rows["timeout"]["status"] == 504
                        and rows["timeout"].get("code") == "inference_timeout")
    else:
        with ThreadPoolExecutor(max_workers=9) as executor:
            held = executor.submit(post, server.port, long)
            rows["busy_health"] = server.wait(lambda h: h.get("pool_available") == 0, 5)
            responses = list(executor.map(lambda _: post(server.port, short), range(8)))
            rows["queue_timeouts"] = responses
            rows["held_response"] = held.result(timeout=90)
        rows["queue_recovery"] = server.recovered()
        rows["cancellations"] = []
        for _ in range(3):
            for mode in ["disconnect", "read-timeout"]:
                row = disconnect(server, long, mode)
                row["probe"] = post(server.port, short)
                row["pass"] = (matches(row["probe"], expected)
                               and row["recovery"]["wall_ms"] <= 30000
                               and row["observed"] == ("TimeoutError" if mode == "read-timeout"
                                                       else "disconnect"))
                rows["cancellations"].append(row)
        rows["pass"] = (all(r["status"] == 503 and r.get("code") == "timeout"
                            and r.get("retry_after") is not None for r in responses)
                        and rows["held_response"]["status"] == 200
                        and all(r["pass"] for r in rows["cancellations"]))
    rows["probes"] = [post(server.port, short) for _ in range(10)]
    rows["pass"] &= all(matches(r, expected) for r in rows["probes"])
    rows["final_health"] = server.ready()
    metrics = server.metrics("final")
    counter = "gigastt_inference_timeouts_total" if watchdog else "gigastt_pool_timeouts_total"
    rows["timeout_counter"] = sum(float(line.split()[-1]) for line in metrics.splitlines()
                                  if line.startswith(counter + " "))
    rows["pass"] &= rows["timeout_counter"] >= (1 if watchdog else 8)
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ["baseline", "candidate", "model-dir", "ort", "manifest", "reference",
                 "long-input", "out"]:
        parser.add_argument("--" + flag, type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    if (args.out / "result.json").exists():
        parser.error("output already contains results; choose a fresh directory")
    cases = json.loads(args.manifest.read_text())
    reference = json.loads(args.reference.read_text())
    if not reference["pass"]:
        parser.error("reference canary did not pass")
    expected = {r["id"]: r["responses"]["baseline"] for r in reference["cases"]}
    inputs = [{**c, "sha256": digest(Path(c["path"]))} for c in cases]
    ref_hashes = {r["id"]: r["sha256"] for r in reference["cases"]}
    if any(c["sha256"] != ref_hashes.get(c["id"]) for c in inputs):
        parser.error("manifest inputs differ from the paired reference")
    short_case = next(c for c in cases if c["kind"] == "m4a")
    short = Path(short_case["path"]).read_bytes()
    long = args.long_input.read_bytes()
    result = {"pass": False, "instances": {}, "inputs": inputs,
              "long_input": {"path": str(args.long_input), "sha256": digest(args.long_input)},
              "script_sha256": digest(Path(__file__)), "runtime_sha256": digest(args.ort),
              "reference_sha256": digest(args.reference),
              "load_before": Path("/proc/loadavg").read_text().strip(),
              "thresholds": {"latency_ratio": 1.5, "peak_rss_ratio": 1.25,
                             "rss_slack_mib": 32, "steady_growth_mib": 64,
                             "cancel_recovery_secs": 30}}
    all_servers = []
    try:
        servers = {}
        for name, binary in [("baseline", args.baseline), ("candidate", args.candidate)]:
            server = Server(name, binary, args, "load")
            all_servers.append(server)
            result["instances"][f"load-{name}"] = server.record
            server.record["health_before"] = server.ready()
            warmup = [post(server.port, short) for _ in range(4)]
            if not all(matches(r, expected[short_case["id"]]) for r in warmup):
                raise RuntimeError(f"{name} warmup differs from reference")
            server.record["warmup"] = warmup
            server.record["bursts"] = []
            servers[name] = server
        for wave, concurrency in enumerate([1, 2, 4, 8, 8, 8, 8]):
            order = ["baseline", "candidate"] if wave % 2 == 0 else ["candidate", "baseline"]
            for name in order:
                row = burst(servers[name], cases, expected, concurrency)
                servers[name].record["bursts"].append(row)
                print("load", name, wave, concurrency, row["pass"], flush=True)
        gates = {}
        for name, server in servers.items():
            server.metrics("final")
            bursts = server.record["bursts"]
            steady = [memory_kib(b["memory_after"]) for b in bursts[3:]]
            gates[name + "_responses"] = all(b["pass"] for b in bursts)
            gates[name + "_steady_memory"] = max(steady) - steady[0] <= 64 * 1024
            server.record["peak_rss_kib"] = max(memory_kib(m) for m in server.record["memory_samples"])
            server.close()
        base, cand = (servers[n].record for n in ["baseline", "candidate"])
        gates["latency"] = all(b[metric] <= a[metric] * 1.5
                               for a, b in zip(base["bursts"], cand["bursts"])
                               for metric in ["p95_ms", "p99_ms"])
        gates["peak_rss"] = cand["peak_rss_kib"] <= base["peak_rss_kib"] * 1.25 + 32 * 1024
        for profile in ["queue-cancel", "watchdog"]:
            for name, binary in [("baseline", args.baseline), ("candidate", args.candidate)]:
                server = Server(name, binary, args, profile)
                all_servers.append(server)
                result["instances"][f"{profile}-{name}"] = server.record
                server.record["health_before"] = server.ready()
                warmup = post(server.port, short)
                if not matches(warmup, expected[short_case["id"]]):
                    raise RuntimeError(f"{profile}-{name} warmup failed")
                server.record["faults"] = faults(server, short, long, expected[short_case["id"]],
                                                profile == "watchdog")
                gates[f"{profile}-{name}"] = server.record["faults"]["pass"]
                print(profile, name, gates[f"{profile}-{name}"], flush=True)
                server.close()
        result["gates"] = gates
        result["pass"] = all(gates.values())
    except Exception as error:
        result["failure"] = f"{type(error).__name__}: {error}"
    finally:
        for server in all_servers:
            server.close()
        result["pass"] &= all(s.record["clean_shutdown"] for s in all_servers)
        result["load_after"] = Path("/proc/loadavg").read_text().strip()
        (args.out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return int(not result["pass"])


if __name__ == "__main__":
    raise SystemExit(main())
