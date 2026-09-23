#!/usr/bin/env python3
"""Capture a bounded /cargo concurrency matrix without hiding failed writes.

This is an owner-dispatched diagnostic, not a closure claim.  It compares the
canonical PUT-only burst with the six WebDAV requests that sccache/opendal
uses around a write.  Every response is retained as redacted wire evidence:
status, body digest/shape, retry hint, Server-Timing, and request identity.
Response bodies are never copied into the artifact and the PAT is only used in
memory. Each operation also uses a payload derived from its unique key; that
keeps independent writes on independent content-hash accounting locks instead
of making the harness serialize them on one synthetic blob.

The three sequential warm PUTs run first so the burst is measured on the hot
authenticated path. The fixed concurrency levels and method order are
intentional. A run with a 429 or another unexpected response is written to the
artifact and exits 2 so CI cannot turn the observation into a green result by
ignoring failures.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import os
import re
import threading
import time
import uuid
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


# PUT-only reaches the required 220 independent writes.  The WebDAV sequence
# remains bounded at 64 because each operation emits six requests.
CONCURRENCIES = (4, 16, 64, 220)
TARGET = "https://staging.corelink.humangr.com"
UUID_V4 = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")

# The expected status of a request against a key that is unique to this run.
# A 429 is deliberately absent from every set: it is evidence of the defect,
# never a tolerated success.
EXPECTED = {
    "GET": {404},
    "HEAD": {404},
    "PROPFIND": {404},
    "MKCOL": {201},
    "PUT": {200},
    "DELETE": {204},
}
WEBDAV_METHODS = ("GET", "HEAD", "PROPFIND", "MKCOL", "PUT", "DELETE")


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def sha256(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def header(headers: dict[str, str], name: str) -> str | None:
    wanted = name.lower()
    return next((value for key, value in headers.items() if key.lower() == wanted), None)


def error_code(body: bytes) -> str | None:
    """Extract only a non-secret machine error marker from a JSON body."""
    try:
        parsed = json.loads(body)
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None
    if not isinstance(parsed, dict):
        return None
    for key in ("error", "code", "type"):
        value = parsed.get(key)
        if isinstance(value, str) and 0 < len(value) <= 80 and re.fullmatch(r"[A-Za-z0-9_.:-]+", value):
            return value
    return None


def request(
    base: str,
    tenant: str,
    token: str,
    method: str,
    key: str,
    body: bytes = b"",
) -> dict[str, object]:
    operation_id = f"b103-{uuid.uuid4().hex}"
    url = f"{base.rstrip('/')}/cargo/{tenant}/{key}"
    headers = {
        "Authorization": f"Bearer {token}",
        "Content-Length": str(len(body)),
        "X-Corelink-Operation": operation_id,
    }
    if method == "PROPFIND":
        headers["Depth"] = "0"
    if method == "PUT":
        headers["Content-Type"] = "application/octet-stream"
    started = time.monotonic()
    status: int | None = None
    response_headers: dict[str, str] = {}
    response_body = b""
    transport_error: str | None = None
    try:
        req = Request(url, data=body if method in {"PUT", "MKCOL"} else None, method=method, headers=headers)
        with urlopen(req, timeout=30) as response:
            status = response.status
            response_headers = dict(response.headers.items())
            response_body = response.read()
    except HTTPError as exc:
        status = exc.code
        response_headers = dict(exc.headers.items())
        response_body = exc.read()
        transport_error = "http_error"
    except (OSError, TimeoutError, URLError) as exc:
        transport_error = type(exc).__name__
    elapsed_ms = round((time.monotonic() - started) * 1000, 3)
    return {
        # This is intentionally retained in the redacted wire artifact.  The
        # same non-secret value is forwarded to the root Worker, so it binds a
        # response to its unfiltered same-window tail event.
        "operation_id": operation_id,
        "method": method,
        "key": key,
        "request_body_sha256": sha256(body) if method == "PUT" else None,
        "status": status,
        "expected_statuses": sorted(EXPECTED[method]),
        "ok": status in EXPECTED[method],
        "elapsed_ms": elapsed_ms,
        "response_body_bytes": len(response_body),
        "response_body_sha256": sha256(response_body),
        "response_error_code": error_code(response_body),
        "transport_error": transport_error,
        "retry_after": header(response_headers, "retry-after"),
        "server_timing": header(response_headers, "server-timing"),
        "response_request_id": header(response_headers, "x-request-id"),
        "cf_ray": header(response_headers, "cf-ray"),
    }


def run_operation(base: str, tenant: str, token: str, mode: str) -> list[dict[str, object]]:
    key = uuid.uuid4().hex * 2
    if mode == "put_only":
        methods = ("PUT",)
    else:
        methods = WEBDAV_METHODS
    rows: list[dict[str, object]] = []
    for method in methods:
        # Unique keys model independent cache artifacts.  Keep their payloads
        # unique as well: AccountingCasHandler shards its reserve/write lock
        # by content hash, and one shared body would turn this matrix into a
        # same-hash idempotency/serialization test instead of a parallel
        # independent-write test.
        body = f"b103-diagnostic:{key}".encode("ascii") if method == "PUT" else b""
        rows.append(request(base, tenant, token, method, key, body))
    return rows


def run_arm(base: str, tenant: str, token: str, mode: str, concurrency: int) -> dict[str, object]:
    started = time.monotonic()
    barrier = threading.Barrier(concurrency + 1)

    def synchronized_operation() -> list[dict[str, object]]:
        # Every operation is ready before the coordinator opens the arm.  This
        # prevents executor scheduling from turning the 220-write arm into a
        # gradual ramp, while the timeout guarantees a broken harness is
        # recorded as a red result instead of waiting indefinitely.
        barrier.wait(timeout=30)
        return run_operation(base, tenant, token, mode)

    responses: list[dict[str, object]] = []
    harness_errors: list[str] = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        futures = [pool.submit(synchronized_operation) for _ in range(concurrency)]
        try:
            barrier.wait(timeout=30)
        except threading.BrokenBarrierError:
            harness_errors.append("coordinator_barrier_broken")
        for future in futures:
            try:
                responses.extend(future.result())
            except (threading.BrokenBarrierError, TimeoutError) as exc:
                harness_errors.append(f"operation_{type(exc).__name__}")
            except Exception as exc:  # retain a red diagnostic artifact
                harness_errors.append(f"operation_{type(exc).__name__}")
    wall_ms = round((time.monotonic() - started) * 1000, 3)
    failures = [row for row in responses if row["ok"] is not True]
    status_counts: dict[str, int] = {}
    for row in responses:
        status = str(row["status"])
        status_counts[status] = status_counts.get(status, 0) + 1
    return {
        "mode": mode,
        "concurrency": concurrency,
        "operations": concurrency,
        "requests": len(responses),
        "successful_requests": len(responses) - len(failures),
        "failed_requests": len(failures) + len(harness_errors),
        "harness_errors": harness_errors,
        "status_counts": status_counts,
        "wall_ms": wall_ms,
        "throughput_rps": round((len(responses) - len(failures)) / max(wall_ms / 1000, 0.001), 6),
        "responses_sha256": sha256(json.dumps(responses, sort_keys=True, separators=(",", ":")).encode()),
        "responses": responses,
    }


def run_warm_sequence(base: str, tenant: str, token: str) -> dict[str, object]:
    """Capture three sequential writes before opening the concurrency arms.

    The sequence warms the same tenant/PAT path that the burst exercises while
    keeping every payload independent.  Reusing one content hash would measure
    CAS idempotency, not the hot authenticated write path.
    """
    responses: list[dict[str, object]] = []
    for _ in range(3):
        key = uuid.uuid4().hex * 2
        body = f"b103-warm:{key}".encode("ascii")
        responses.append(request(base, tenant, token, "PUT", key, body))
    failures = [row for row in responses if row["ok"] is not True]
    return {
        "method": "PUT",
        "requests": len(responses),
        "successful_requests": len(responses) - len(failures),
        "failed_requests": len(failures),
        "responses_sha256": sha256(json.dumps(responses, sort_keys=True, separators=(",", ":")).encode()),
        "responses": responses,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default=os.environ.get("B103_TARGET_HOST", ""))
    parser.add_argument("--tenant", default=os.environ.get("B103_TENANT_ID", ""))
    parser.add_argument("--deployment-sha", default=os.environ.get("B103_DEPLOYMENT_SHA", ""))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    token = os.environ.get("B103_PAT", "")
    if args.base.rstrip("/") != TARGET:
        raise SystemExit(f"refusing non-canonical target; expected {TARGET}")
    if not UUID_V4.fullmatch(args.tenant):
        raise SystemExit("B103_TENANT_ID must be a lowercase UUID v4")
    if not token:
        raise SystemExit("B103_PAT is required")
    if not re.fullmatch(r"[0-9a-f]{40}", args.deployment_sha):
        raise SystemExit("B103_DEPLOYMENT_SHA must be the target receipt's 40-character SHA")

    warm_sequence = run_warm_sequence(args.base.rstrip("/"), args.tenant, token)
    arms: list[dict[str, object]] = []
    for mode in ("put_only", "webdav_sequence"):
        levels = CONCURRENCIES if mode == "put_only" else CONCURRENCIES[:-1]
        for concurrency in levels:
            arms.append(run_arm(args.base.rstrip("/"), args.tenant, token, mode, concurrency))
    result = {
        "schema": "corelink.b103-cargo-write-reproducer.v1",
        "environment": "staging",
        "target": TARGET,
        "deployment_sha": args.deployment_sha,
        "tenant_id": "[REDACTED]",
        "tenant_id_sha256": sha256(args.tenant.encode("ascii")),
        "captured_at": now(),
        "concurrency_levels": list(CONCURRENCIES),
        "put_only_max_concurrency": max(CONCURRENCIES),
        "webdav_max_concurrency": max(CONCURRENCIES[:-1]),
        "method_contract": {
            "warm_sequence": ["PUT", "PUT", "PUT"],
            "put_only": ["PUT"],
            "webdav_sequence": list(WEBDAV_METHODS),
            "expected_statuses": {method: sorted(statuses) for method, statuses in EXPECTED.items()},
        },
        "warm_sequence": warm_sequence,
        "arms": arms,
        "failed_assertions": int(warm_sequence["failed_requests"]) + sum(
            int(arm["failed_requests"]) for arm in arms
        ),
    }
    args.output.write_text(json.dumps(result, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    # A diagnostic artifact is useful whether the system is healthy or broken;
    # the exit status keeps CI honest about which one was observed.
    return 0 if result["failed_assertions"] == 0 else 2


if __name__ == "__main__":
    raise SystemExit(main())
