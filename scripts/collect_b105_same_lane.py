#!/usr/bin/env python3
"""Run six alternating, paired cache-off/cache-on build measurements."""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path

from b105_isolated_lane import Forwarder, Handler, Meter, cleanup_exact, paired_summary


COMMAND = ("cargo", "test", "--package", "corelink-reapi", "--release", "--no-run")
MAX_PAIRS = 6


def operation(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex}"


def stats(env: dict[str, str]) -> tuple[dict[str, int], str]:
    result = subprocess.run(("sccache", "--show-stats"), env=env, capture_output=True, text=True, timeout=10, check=True)
    raw = result.stdout + "\n" + result.stderr
    labels = {"Cache hits": "hits", "Cache misses": "misses", "Cache read errors": "read_errors", "Cache write errors": "write_errors"}
    parsed: dict[str, int] = {}
    for line in raw.splitlines():
        for label, key in labels.items():
            if line.strip().startswith(label):
                value = line.split()[-1].replace(",", "")
                if not value.isdigit():
                    raise RuntimeError(f"unparseable sccache counter {label}")
                parsed[key] = int(value)
    if set(parsed) != set(labels.values()):
        raise RuntimeError("sccache output omitted a required hit/miss/error counter")
    return parsed, "sha256:" + hashlib.sha256(raw.encode()).hexdigest()


def run(mode: str, pair: int, tenant: str, revision: str) -> dict[str, object]:
    if shutil.which("sccache") is None:
        raise RuntimeError("B-105 requires sccache on the runner")
    env = os.environ.copy()
    env["CORELINK_SCCACHE_PILOT"] = "on" if mode == "enabled" else "off"
    env["RUSTC_WRAPPER"] = "sccache" if mode == "enabled" else ""
    env["CARGO_TARGET_DIR"] = str(Path(os.environ.get("RUNNER_TEMP", "/tmp")) / f"b105-target-{pair}-{mode}")
    if mode == "enabled":
        base = os.environ.get("CORELINK_PERF_BASE", "").rstrip("/")
        token = os.environ.get("CORELINK_PERF_PAT", "")
        if not base or not token:
            raise RuntimeError("B-105 cache-on arm requires CORELINK_PERF_BASE and CORELINK_PERF_PAT")
        env["SCCACHE_WEBDAV_ENDPOINT"] = f"{base}/cargo/{tenant}"
        env["SCCACHE_WEBDAV_TOKEN"] = token
        env["SCCACHE_IGNORE_SERVER_IO_ERROR"] = "0"
        env["CARGO_INCREMENTAL"] = "0"
    subprocess.run(("sccache", "--zero-stats"), env=env, capture_output=True, text=True, timeout=10, check=True)
    before = time.monotonic()
    process = subprocess.run(COMMAND, env=env, capture_output=True, text=True, timeout=360, check=False)
    output = process.stdout + "\n" + process.stderr
    if process.returncode != 0:
        raise RuntimeError(f"B-105 {mode} subprocess failed with exit {process.returncode}")
    counters, stats_digest = stats(env)
    return {
        "tenant_id": tenant,
        "operation_id": operation(f"b105-{mode}"),
        "cache_mode": mode,
        "status": "complete",
        "returncode": process.returncode,
        "duration_seconds": time.monotonic() - before,
        "raw_output_sha256": "sha256:" + hashlib.sha256(output.encode()).hexdigest(),
        "sccache_stats_raw_sha256": stats_digest,
        "sccache": counters,
        "revision": revision,
        "runner": os.environ.get("RUNNER_NAME", ""),
        "machine": os.uname().machine,
        "toolchain": "rust-toolchain.toml",
        "command": list(COMMAND),
    }


def isolated_main(output: Path, tenant: str, revision: str) -> int:
    """Run the future owner-triggered hosted lane behind a local key boundary."""
    origin = os.environ.get("CORELINK_PERF_BASE", "").rstrip("/")
    token = os.environ.get("CORELINK_PERF_PAT", "")
    if not origin or not token or not re.fullmatch(r"[0-9a-f-]{36}", tenant):
        raise RuntimeError("isolated B-105 requires origin, PAT, and a tenant UUID")
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    if not run_id.isdigit():
        raise RuntimeError("isolated B-105 requires a GitHub run id")
    meter = Meter(f"b105-{run_id}-{uuid.uuid4().hex[:12]}")
    handler = type("B105Handler", (Handler,), {"meter": meter, "origin": origin, "tenant": tenant, "token": token})
    server = Forwarder(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    endpoint = f"http://127.0.0.1:{server.server_port}"
    prior_base = os.environ.get("CORELINK_PERF_BASE")
    os.environ["CORELINK_PERF_BASE"] = endpoint
    pairs: list[dict[str, object]] = []
    failure: str | None = None
    try:
        meter.set_phase("seed")
        seed = run("enabled", -1, tenant, revision)
        for index in range(MAX_PAIRS):
            control_first = index % 2 == 0
            arms: dict[str, dict[str, object]] = {}
            for mode in (("disabled", "enabled") if control_first else ("enabled", "disabled")):
                meter.set_phase(f"pair-{index}-{mode}")
                arms[mode] = run(mode, index, tenant, revision)
            pairs.append({"index": index, "control_first": control_first,
                          "control": arms["disabled"], "treatment": arms["enabled"]})
        summary = paired_summary(pairs)
        if any(int(pair["treatment"]["sccache"]["read_errors"]) or int(pair["treatment"]["sccache"]["write_errors"])  # type: ignore[index]
               for pair in pairs):
            failure = "cache I/O error in treatment"
        elif not all(int(pair["treatment"]["sccache"]["hits"]) > 0 for pair in pairs):  # type: ignore[index]
            failure = "a treatment arm had no cache hits"
    except Exception as exc:
        seed = None
        summary = {"result": "indeterminate"}
        failure = f"{type(exc).__name__}: {str(exc)[:160]}"
    finally:
        meter.set_phase("cleanup")
        deleted, cleanup_failures = cleanup_exact(origin, token, tenant, meter)
        server.shutdown()
        server.server_close()
        if prior_base is None:
            os.environ.pop("CORELINK_PERF_BASE", None)
        else:
            os.environ["CORELINK_PERF_BASE"] = prior_base
    result = {"schema": "corelink.b105-lane.v3", "pairs": pairs, "seed": seed,
              "application_payload": dict(meter.counters),
              "namespace": {"touched_keys": len(meter.keys), "retained_indexed_payload_bytes": meter.retained_payload_bytes()},
              "cleanup": {"attempted": len(meter.keys), "delete_successes": deleted,
                          "failures": cleanup_failures, "verified_absent": cleanup_failures == 0},
              "failure": failure, **summary}
    output.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"result": result["result"], "cleanup_verified": result["cleanup"]["verified_absent"], "failure": failure}, sort_keys=True))
    return 1 if failure or cleanup_failures else 0


def main() -> int:
    if not os.environ.get("CI"):
        print("B-105 must run in the owner-triggered CI lane", file=sys.stderr)
        return 2
    isolated = "--isolated" in sys.argv[1:]
    output_args = [arg for arg in sys.argv[1:] if arg != "--isolated"]
    if len(output_args) != 1:
        print("usage: collect_b105_same_lane.py [--isolated] OUTPUT", file=sys.stderr)
        return 2
    tenant = os.environ.get("CORELINK_PERF_TENANT", "")
    if not tenant:
        print("B-105 requires CORELINK_PERF_TENANT", file=sys.stderr)
        return 2
    revision = os.environ.get("GITHUB_SHA", "")
    if isolated:
        return isolated_main(Path(output_args[0]), tenant, revision)
    pairs = []
    for index in range(MAX_PAIRS):
        control_first = index % 2 == 0
        modes = ("disabled", "enabled") if control_first else ("enabled", "disabled")
        arms = {mode: run(mode, index, tenant, revision) for mode in modes}
        pairs.append({
            "tenant_id": tenant,
            "operation_id": operation(f"b105-pair-{index}"),
            "control_first": control_first,
            "control": arms["disabled"],
            "treatment": arms["enabled"],
        })
    Path(output_args[0]).write_text(json.dumps({"schema": "corelink.b105-lane.v2", "pairs": pairs}, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
