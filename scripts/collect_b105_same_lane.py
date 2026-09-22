#!/usr/bin/env python3
"""Run six alternating, paired cache-off/cache-on build measurements."""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


COMMAND = ("cargo", "test", "--package", "corelink-reapi", "--release", "--no-run")


def operation(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex}"


def provider_receipt(url: str, token: str) -> dict[str, object]:
    request = urllib.request.Request(url, headers={"Authorization": f"Bearer {token}", "Accept": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            body = json.loads(response.read().decode("utf-8"))
            request_id = response.headers.get("x-request-id", "")
    except (OSError, urllib.error.HTTPError, json.JSONDecodeError) as exc:
        raise RuntimeError(f"B-105 provider receipt failed: {exc}") from exc
    required = {"bytes_read", "bytes_written", "retained_bytes", "provider_receipt_id"}
    if not isinstance(body, dict) or set(required) - set(body):
        raise RuntimeError("B-105 provider receipt omitted byte quantities or receipt identity")
    if any(not isinstance(body[key], int) or body[key] < 0 for key in ("bytes_read", "bytes_written", "retained_bytes")):
        raise RuntimeError("B-105 provider receipt has invalid byte quantities")
    if not isinstance(body["provider_receipt_id"], str) or not body["provider_receipt_id"]:
        raise RuntimeError("B-105 provider receipt id is missing")
    if not request_id:
        raise RuntimeError("B-105 provider receipt omitted request identity")
    return {**{key: body[key] for key in required if key != "provider_receipt_id"},
            "provider_receipt_id": body["provider_receipt_id"],
            "request_id": request_id}


def cleanup(purge_url: str, namespace: str, token: str) -> dict[str, object]:
    payload = json.dumps({"namespace": namespace}).encode("utf-8")
    request = urllib.request.Request(
        purge_url,
        data=payload,
        method="POST",
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            body = json.loads(response.read().decode("utf-8"))
            request_id = response.headers.get("x-request-id", "")
            if response.status < 200 or response.status >= 300 or not request_id or not isinstance(body, dict):
                raise RuntimeError("B-105 cleanup receipt is incomplete")
            retained = body.get("retained_bytes")
            if not isinstance(retained, int) or retained != 0:
                raise RuntimeError("B-105 cleanup receipt does not prove zero retained bytes")
            return {"status": response.status, "request_id": request_id, "retained_bytes": retained}
    except (OSError, urllib.error.HTTPError, json.JSONDecodeError) as exc:
        raise RuntimeError(f"B-105 namespace cleanup failed: {exc}") from exc


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


def run(mode: str, pair: int, tenant: str, namespace: str, revision: str) -> dict[str, object]:
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
        env["SCCACHE_WEBDAV_ENDPOINT"] = f"{base}/cargo/{tenant}/{namespace}"
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
        "namespace": namespace,
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


def main() -> int:
    if not os.environ.get("CI"):
        print("B-105 must run in the owner-triggered CI lane", file=sys.stderr)
        return 2
    tenant = os.environ.get("CORELINK_PERF_TENANT", "")
    base = os.environ.get("CORELINK_PERF_BASE", "").rstrip("/")
    token = os.environ.get("CORELINK_PERF_PAT", "")
    receipt_url = os.environ.get("CORELINK_B105_RECEIPT_URL", "")
    purge_url = os.environ.get("CORELINK_B105_PURGE_URL", "")
    if not tenant or not base or not token or not receipt_url or not purge_url:
        print("B-105 requires CORELINK_PERF_TENANT", file=sys.stderr)
        return 2
    revision = os.environ.get("GITHUB_SHA", "")
    namespace = f"b105-{os.environ.get('GITHUB_RUN_ID', operation('run'))}-{revision[:12]}"
    pairs = []
    result: dict[str, object] = {"schema": "corelink.b105-lane.v3", "tenant_id": tenant, "namespace": namespace, "pairs": pairs}
    try:
        for index in range(6):
            control_first = index % 2 == 0
            modes = ("disabled", "enabled") if control_first else ("enabled", "disabled")
            arms = {mode: run(mode, index, tenant, namespace, revision) for mode in modes}
            if arms["disabled"]["cache_mode"] != "disabled" or arms["enabled"]["cache_mode"] != "enabled":
                raise RuntimeError("B-105 pair cache modes are incomplete")
            if any(arms[mode]["namespace"] != namespace or arms[mode]["revision"] != revision for mode in modes):
                raise RuntimeError("B-105 pair identity drifted")
            pairs.append({"tenant_id": tenant, "namespace": namespace, "operation_id": operation(f"b105-pair-{index}"), "control_first": control_first, "control": arms["disabled"], "treatment": arms["enabled"]})
        if len(pairs) != 6 or [pair["control_first"] for pair in pairs] != [True, False, True, False, True, False]:
            raise RuntimeError("B-105 pairs are not six alternating control-first measurements")
        result["provider_receipt"] = provider_receipt(receipt_url, token)
    finally:
        result["cleanup"] = cleanup(purge_url, namespace, token)
    Path(sys.argv[1]).write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
