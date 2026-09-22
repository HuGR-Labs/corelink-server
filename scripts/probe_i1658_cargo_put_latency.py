#!/usr/bin/env python3
"""Bounded staging probe for issue #1658 / backlog B-102.

The probe is intentionally staging-only and dispatch-only. It records enough
wire evidence to separate authentication, Worker D1, container D1, R2, and
framework work. It never retries a request and never writes a credential or
body to the artifact.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import datetime as dt
import hashlib
import json
import math
import os
import re
import sys
import time
import uuid
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


TARGET = "https://staging.corelink.humangr.com"
UUID_V4 = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
REQUIRED_PHASES = (
    "auth", "wdb", "qtier", "qbatch", "qresid", "total", "origin", "ohop",
    "opat", "oquota", "ostore", "oaccounting", "ohandler",
)
OPTIONAL_PHASES = ("qdo", "qcontrol", "oargon", "opermit", "ortier", "oaudit", "oratelimit")
KNOWN_PHASES = REQUIRED_PHASES + OPTIONAL_PHASES
ORIGIN_SUBPHASES = ("opat", "oquota", "ostore", "oaccounting", "ohandler", "oargon", "opermit", "ortier", "oaudit", "oratelimit")
RETRYABLE = {429, 500, 502, 503, 504}
AUTH_DESC = re.compile(r'\bauth;[^,]*\bdesc="([^"]+)"')
TIMING = re.compile(r"(?P<name>[A-Za-z][A-Za-z0-9_-]*);(?P<params>[^,]*)")
DURATION = re.compile(r"(?:^|;)\s*dur=([0-9]+(?:\.[0-9]+)?)\s*(?:;|$)")


class ProbeError(RuntimeError):
    def __init__(self, message: str, observation: dict[str, object] | None = None) -> None:
        super().__init__(message)
        self.observation = observation


def sha(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def now() -> str:
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def percentile(values: list[float], rank: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil(len(ordered) * rank) - 1))
    return ordered[index]


def stats(rows: list[dict[str, object]], key: str) -> dict[str, float | int]:
    values = [float(row[key]) for row in rows]
    ordered = sorted(values)
    median = (ordered[(len(ordered) - 1) // 2] + ordered[len(ordered) // 2]) / 2
    return {"n": len(values), "p50_ms": round(median, 3), "p90_ms": round(percentile(values, 0.90), 3)}


def header(headers: dict[str, str], name: str) -> str | None:
    wanted = name.lower()
    return next((value for key, value in headers.items() if key.lower() == wanted), None)


def parse_timing(raw: str) -> tuple[dict[str, float], str]:
    values: dict[str, float] = {}
    for match in TIMING.finditer(raw):
        name = match.group("name")
        params = match.group("params")
        duration = DURATION.search(params)
        if duration is None:
            raise ProbeError(f"malformed Server-Timing phase: {name}")
        value = float(duration.group(1))
        if name == "oother":
            if "ohandler" in values and values["ohandler"] != value:
                raise ProbeError("oother and ohandler disagree")
            values.setdefault("ohandler", value)
            continue
        if name in values:
            raise ProbeError(f"duplicate Server-Timing phase: {name}")
        values[name] = value
    unknown = set(values) - set(KNOWN_PHASES)
    if unknown:
        raise ProbeError(f"unknown Server-Timing phase(s): {sorted(unknown)}")
    missing = [phase for phase in REQUIRED_PHASES if phase not in values]
    if missing:
        raise ProbeError(f"required Server-Timing phase(s) missing: {', '.join(missing)}")
    origin_sum = values["ohop"] + sum(values[name] for name in ORIGIN_SUBPHASES)
    if abs(origin_sum - values["origin"]) > 1.0:
        raise ProbeError(f"origin phase budget mismatch: {origin_sum:g} != {values['origin']:g}")
    auth_match = AUTH_DESC.search(raw)
    auth_source = auth_match.group(1) if auth_match else "unknown"
    if auth_source not in {"d1", "l1", "kv"}:
        raise ProbeError(f"auth cache tier is missing or unknown: {auth_source}")
    return values, auth_source


def request(
    base: str,
    tenant: str,
    token: str | None,
    method: str,
    key: str,
    body: bytes = b"",
    path: str | None = None,
) -> dict[str, object]:
    headers = {
        "Content-Length": str(len(body)),
        "X-Corelink-Operation": f"i1658-{uuid.uuid4().hex}",
    }
    if token is not None:
        headers["Authorization"] = f"Bearer {token}"
    if method == "PUT":
        headers["Content-Type"] = "application/octet-stream"
    started = time.monotonic()
    status: int | None = None
    response_headers: dict[str, str] = {}
    response_body = b""
    transport_error: str | None = None
    try:
        request_path = path or f"/cargo/{tenant}/{key}"
        req = Request(f"{base.rstrip('/')}{request_path}", data=body if method == "PUT" else None, method=method, headers=headers)
        with urlopen(req, timeout=20) as response:
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
    row: dict[str, object] = {
        "method": method,
        "status": status,
        "attempts": 1,
        "elapsed_ms": elapsed_ms,
        "request_body_bytes": len(body),
        "request_body_sha256": sha(body),
        "response_body_bytes": len(response_body),
        "response_body_sha256": sha(response_body),
        "retry_after": header(response_headers, "retry-after"),
        "server_timing": header(response_headers, "server-timing"),
        "response_request_id": header(response_headers, "x-request-id"),
        "cf_ray": header(response_headers, "cf-ray"),
        "transport_error": transport_error,
    }
    return row


def observed_put(base: str, tenant: str, token: str, ordinal: str) -> dict[str, object]:
    key = f"i1658-{ordinal}-{uuid.uuid4().hex}"
    body = hashlib.sha256(key.encode()).digest() * 32
    body = body[:1024]
    row = request(base, tenant, token, "PUT", key, body)
    row["key_sha256"] = sha(key.encode())
    if row["status"] != 200:
        raise ProbeError(f"authenticated PUT returned HTTP {row['status']}", row)
    if row["request_body_bytes"] != 1024:
        raise ProbeError("PUT body is not exactly 1 KiB")
    timing = row["server_timing"]
    if not isinstance(timing, str) or not timing:
        raise ProbeError("authenticated PUT did not return Server-Timing")
    phases, auth_source = parse_timing(timing)
    row["auth_source"] = auth_source
    for name, value in phases.items():
        row[name] = value
    row["hashing_phase"] = "ohandler"
    # DELETE removes the map row; the R2 blob remains subject to staging GC.
    cleanup = request(base, tenant, token, "DELETE", key)
    row["cleanup"] = cleanup
    row["cleanup_status"] = cleanup["status"]
    if cleanup["status"] != 204:
        raise ProbeError(f"PUT cleanup returned HTTP {cleanup['status']}", row)
    return row


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default=os.environ.get("B102_TARGET_HOST", ""))
    parser.add_argument("--tenant", default=os.environ.get("B102_TENANT_ID", ""))
    parser.add_argument("--token", default=os.environ.get("B102_PAT", ""))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.base.rstrip("/") != TARGET:
        raise SystemExit(f"refusing non-canonical target; expected {TARGET}")
    if not UUID_V4.fullmatch(args.tenant):
        raise SystemExit("B102_TENANT_ID must be a lowercase UUID v4")
    if not args.token:
        raise SystemExit("B102_PAT is required")
    try:
        health = request(args.base, args.tenant, None, "GET", "health", path="/health")
        if health["status"] != 200:
            raise ProbeError(f"health control returned HTTP {health['status']}")
        unauth = request(args.base, args.tenant, None, "PUT", f"i1658-unauth-{uuid.uuid4().hex}", b"x" * 1024)
        if unauth["status"] not in {401, 403}:
            raise ProbeError(f"unauthenticated PUT control returned HTTP {unauth['status']}")
        serial = [observed_put(args.base, args.tenant, args.token, f"serial-{i}") for i in range(10)]
        concurrent_rows: dict[str, list[dict[str, object]]] = {}
        for level in (1, 4):
            started = time.monotonic()
            with concurrent.futures.ThreadPoolExecutor(max_workers=level) as pool:
                rows = list(pool.map(lambda i: observed_put(args.base, args.tenant, args.token, f"concurrent-{level}-{i}"), range(level)))
            for row in rows:
                row["concurrency"] = level
            concurrent_rows[str(level)] = rows
            concurrent_rows[f"{level}_wall_ms"] = [{"wall_ms": round((time.monotonic() - started) * 1000, 3)}]
        all_rows = serial + concurrent_rows["1"] + concurrent_rows["4"]
        cold = [row for row in serial if row["auth_source"] == "d1"]
        warm = [row for row in serial if row["auth_source"] in {"l1", "kv"}]
        if not cold or not warm:
            raise ProbeError("serial population did not contain both cold d1 and warm l1/kv auth")
        phases = {name: stats(all_rows, name) for name in ("elapsed_ms", *REQUIRED_PHASES, *OPTIONAL_PHASES) if all(name in row for row in all_rows)}
        output = {
            "schema": "corelink.hot-cargo-put.evidence.v1",
            "issue": 1658,
            "backlog_id": "B-102",
            "environment": "staging",
            "target": TARGET,
            "tenant_id": args.tenant,
            "captured_at": now(),
            "sample_contract": {"serial": 10, "concurrency": [1, 4], "body_bytes": 1024, "attempts": 1},
            "controls": {"health_status": health["status"], "unauthenticated_put_status": unauth["status"]},
            "phase_metadata": {"hashing": "ohandler", "cold_auth": "d1", "warm_auth": ["l1", "kv"]},
            "serial": serial,
            "concurrency": concurrent_rows,
            "phase_stats": phases,
            "cold_stats": {name: stats(cold, name) for name in ("elapsed_ms", *REQUIRED_PHASES, *OPTIONAL_PHASES) if all(name in row for row in cold)},
            "warm_stats": {name: stats(warm, name) for name in ("elapsed_ms", *REQUIRED_PHASES, *OPTIONAL_PHASES) if all(name in row for row in warm)},
        }
        args.output.write_text(json.dumps(output, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        return 0
    except (OSError, ProbeError, ValueError) as exc:
        observation = exc.observation if isinstance(exc, ProbeError) else None
        failure = {"schema": "corelink.hot-cargo-put.evidence.v1", "status": "INDETERMINATE", "error": str(exc), "captured_at": now()}
        if observation is not None:
            failure["observation"] = observation
        args.output.write_text(json.dumps(failure, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(f"B-102 INDETERMINATE: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
