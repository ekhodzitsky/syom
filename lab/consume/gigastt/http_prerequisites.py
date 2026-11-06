"""Recompute canary prerequisites from recorded observations, not gate flags."""

import re

from http_contract import MODEL_FILES, MODEL_SHA, ORT_SHA, THRESHOLDS, finite_number
from run_http_canary import percentile


def _peak_kib(instance):
    value = instance["final_memory"]["VmHWM"]
    if not isinstance(value, str) or not re.fullmatch(r"[1-9][0-9]* kB", value):
        raise ValueError("invalid positive peak RSS measurement")
    return int(value.split()[0])


def _observed_numeric_gates(canary):
    instances = canary["instances"]
    timings = {}
    for name in ("baseline", "candidate"):
        instance = instances[name]
        values = [row["responses"][name]["wall_ms"] for row in canary["cases"]]
        if any(not finite_number(value) or value <= 0 for value in values):
            raise ValueError("invalid positive request latency measurement")
        observed = {"count": len(values)}
        observed.update({key: percentile(values, q) for key, q in
                         (("median", 0.5), ("p95", 0.95), ("p99", 0.99))})
        if instance["timing_ms"] != observed:
            raise ValueError(f"{name} timing summary differs from recorded requests")
        timings[name] = observed
    base, candidate = instances["baseline"], instances["candidate"]
    return {
        "p95": timings["candidate"]["p95"] <= timings["baseline"]["p95"] * THRESHOLDS["max_latency_ratio"],
        "p99": timings["candidate"]["p99"] <= timings["baseline"]["p99"] * THRESHOLDS["max_latency_ratio"],
        "peak_rss": _peak_kib(candidate) <= _peak_kib(base) * THRESHOLDS["max_rss_ratio"]
        + THRESHOLDS["rss_slack_mib"] * 1024,
    }


def prerequisite_errors(canary, independent):
    errors = []
    if canary.get("thresholds") != THRESHOLDS:
        errors.append("canary numeric thresholds differ from the unchanged baseline")
    if canary.get("static_ort_archive_sha256") != ORT_SHA:
        errors.append("canary ORT archive is not the pinned runtime")
    if independent.get("model_files") != MODEL_FILES:
        errors.append("model-file identities differ from the pinned current model")
    if independent.get("model_encoder_sha256") != MODEL_SHA:
        errors.append("independent references are not the active 305 MiB model")
    if independent.get("static_ort_archive_sha256") != ORT_SHA:
        errors.append("independent references use a different runtime")

    try:
        gates = _observed_numeric_gates(canary)
        for gate, passed in gates.items():
            if not passed:
                errors.append(f"recorded canary prerequisite {gate} failed")
        for name in ("baseline", "candidate"):
            instance = canary["instances"][name]
            if (instance.get("healthy_after") is not True
                    or instance["health_after"].get("status") != "ok"
                    or instance["health_after"].get("model") != "gigaam-v3-rnnt"):
                errors.append(f"{name} recorded health prerequisite failed")
        negative = canary["negative"]
        if len(negative) != 2 or {row["id"] for row in negative} != {"garbage", "truncated-m4a"}:
            raise ValueError("invalid/truncated-input observations are missing or duplicated")
        for row in negative:
            responses = row["responses"]
            a, b = (responses[name]["status"] for name in ("baseline", "candidate"))
            if (set(responses) != {"baseline", "candidate"}
                    or type(a) is not int or type(b) is not int or not 400 <= a == b < 500
                    or row.get("pass") is not True):
                errors.append(f"{row['id']}: recorded invalid-input prerequisite failed")
    except (KeyError, TypeError, ValueError, AttributeError, IndexError) as error:
        errors.append(f"invalid canary prerequisite observations: {error}")

    for gate in ("health_after", "p95", "p99", "peak_rss"):
        if canary.get("gates", {}).get(gate) is not True:
            errors.append(f"canary prerequisite {gate} failed")
    return errors
