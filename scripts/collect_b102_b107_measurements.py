#!/usr/bin/env python3
"""Run the bounded authenticated B-102..B-107 production measurements.

Tokens are read only from the process environment and never serialized.  A
request failure is retained as raw status evidence, never converted to a
success flag.  The resulting JSON is an input to the v2 verifier after the
deployment context is joined by the workflow.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

UUID = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
PHASE = re.compile(r"(?:^|[, ])([a-z][a-z0-9_]*);dur=([0-9]+(?:\.[0-9]+)?)")
AUTH_PHASE = re.compile(r'(?:^|[, ])auth;dur=[0-9]+(?:\.[0-9]+)?;desc="(l1|kv|d1)"(?:[, ]|$)')
# Keep the evidence field bound to the deployed Worker contract.  The cold
# proof intentionally remains stricter than this TTL: a token must be idle for
# at least 61 seconds, even though the current L2 entry expires after 30 s.
KV_PAT_ROW_TTL_SECONDS = 30
COLD_IDLE_SECONDS = 61


def mint_binding_sha256(attestation: dict[str, object]) -> str:
    material = json.dumps(
        {
            "expires_ms": attestation.get("expires_ms"),
            "pat_id": attestation.get("pat_id"),
            "tenant_id": attestation.get("tenant_id"),
            "token_fingerprint": attestation.get("token_fingerprint"),
            "token_id": attestation.get("token_id"),
            "mint_operation_id": attestation.get("mint_operation_id"),
            "mint_request_id": attestation.get("mint_request_id"),
            "mint_response_sha256": attestation.get("mint_response_sha256"),
            "minted_at_epoch": attestation.get("minted_at_epoch"),
            "unused_since_epoch": attestation.get("unused_since_epoch"),
            "observed_at_epoch": attestation.get("observed_at_epoch"),
            "attestation_source": attestation.get("attestation_source"),
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode()
    return "sha256:" + hashlib.sha256(material).hexdigest()


def op(prefix: str) -> str:
    return f"{prefix}-{uuid.uuid4().hex}"


def wire_auth_source(timing: str) -> str | None:
    match = AUTH_PHASE.search(timing)
    return match.group(1) if match else None


def wire_colo(headers: dict[str, str]) -> str | None:
    ray = next((value for key, value in headers.items() if key.lower() == "cf-ray"), "")
    if "-" not in ray:
        return None
    colo = ray.rsplit("-", 1)[1].strip().upper()
    return colo or None


def call(base: str, tenant: str, token: str, method: str, operation: str, key: str, body: bytes = b"") -> dict:
    request = Request(f"{base.rstrip('/')}/cargo/{tenant}/{key}", data=body if method != "GET" else None, method=method,
                     headers={"Authorization": f"Bearer {token}", "Content-Length": str(len(body)), "X-Corelink-Operation": operation})
    started = time.monotonic()
    try:
        with urlopen(request, timeout=10) as response:
            payload = response.read()
            status = response.status
            headers = dict(response.headers.items())
            error = None
    except HTTPError as exc:
        payload = exc.read()
        status = exc.code
        headers = dict(exc.headers.items())
        error = "http_error"
    except (URLError, TimeoutError, OSError) as exc:
        payload = b""
        status = 599
        headers = {}
        error = type(exc).__name__
    elapsed = (time.monotonic() - started) * 1000
    timing = next((value for key, value in headers.items() if key.lower() == "server-timing"), "")
    auth_source = wire_auth_source(timing)
    response_request_id = next((value for key, value in headers.items() if key.lower() == "x-request-id"), "")
    return {"tenant_id": tenant, "operation_id": operation, "method": method, "status": status,
            "payload_bytes": len(body),
            "elapsed_ms": round(elapsed, 3), "server_timing": timing,
            "raw_output_sha256": "sha256:" + hashlib.sha256(payload + timing.encode()).hexdigest(), "error": error,
            "authenticated": auth_source is not None, "auth_source": auth_source,
            "colo": wire_colo(headers), "response_request_id": response_request_id}


def mint_fresh(base: str, tenant: str, session: str, internal_auth: str, operation: str) -> dict:
    """Mint the test PAT through the real production token-exchange API.

    A pre-supplied PAT/timestamp is deliberately unsupported: it cannot prove
    that the token was minted for this observation.  The response body is
    consumed in memory, and only content-addressed metadata leaves this step.
    """
    request = Request(
        f"{base.rstrip('/')}/v1/session/exchange",
        data=json.dumps({"audience": tenant, "scope": "cas:rw"}).encode(),
        method="POST",
        headers={
            "Authorization": f"Bearer {session}",
            "Content-Type": "application/json",
            "X-Corelink-Operation": operation,
            "X-Corelink-Internal-Auth": internal_auth,
        },
    )
    started = time.monotonic()
    try:
        with urlopen(request, timeout=20) as response:
            payload = response.read()
            status = response.status
            response_request_id = next((value for key, value in response.headers.items() if key.lower() == "x-request-id"), "")
    except HTTPError as exc:
        detail = exc.read()
        raise RuntimeError(f"fresh PAT mint API returned HTTP {exc.code}: {hashlib.sha256(detail).hexdigest()}") from exc
    except (URLError, TimeoutError, OSError) as exc:
        raise RuntimeError(f"fresh PAT mint API unavailable: {type(exc).__name__}") from exc
    if status != 200:
        raise RuntimeError(f"fresh PAT mint API returned HTTP {status}")
    try:
        body = json.loads(payload)
    except json.JSONDecodeError as exc:
        raise RuntimeError("fresh PAT mint API returned non-JSON") from exc
    if not isinstance(body, dict) or not isinstance(body.get("token_plaintext"), str) or not body["token_plaintext"]:
        raise RuntimeError("fresh PAT mint API did not return a token")
    if body.get("tenant") != tenant:
        raise RuntimeError("fresh PAT mint API returned the wrong tenant")
    if not response_request_id:
        raise RuntimeError("fresh PAT mint API omitted X-Request-Id")
    pat_id, token_id, expires_ms = body.get("pat_id"), body.get("token_id"), body.get("expires_ms")
    if not isinstance(pat_id, str) or not pat_id or not isinstance(token_id, str) or not token_id or not isinstance(expires_ms, (int, float)):
        raise RuntimeError("fresh PAT mint API omitted binding metadata")
    response_digest = "sha256:" + hashlib.sha256(payload).hexdigest()
    token_fingerprint = "sha256:" + hashlib.sha256(body["token_plaintext"].encode()).hexdigest()
    minted_at = time.time()
    binding = mint_binding_sha256({"expires_ms": expires_ms, "pat_id": pat_id, "tenant_id": tenant,
                                   "token_fingerprint": token_fingerprint, "token_id": token_id,
                                   "mint_operation_id": operation, "mint_request_id": response_request_id,
                                   "mint_response_sha256": response_digest, "minted_at_epoch": minted_at,
                                   "unused_since_epoch": None, "observed_at_epoch": None})
    return {
        "token": body["token_plaintext"],
        "token_fingerprint": token_fingerprint,
        "mint_operation_id": operation,
        "mint_request_id": response_request_id,
        "mint_response_sha256": response_digest,
        "mint_response_binding_sha256": binding,
        "minted_at_epoch": minted_at,
        "mint_elapsed_ms": round((time.monotonic() - started) * 1000, 3),
        "pat_id": pat_id,
        "token_id": token_id,
        "expires_ms": expires_ms,
    }


def phase(timing: str, name: str) -> float:
    for found, duration in PHASE.findall(timing):
        if found == name:
            return float(duration)
    raise RuntimeError(f"missing required Server-Timing phase {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", default=os.environ.get("CORELINK_PERF_BASE", ""))
    parser.add_argument("--tenant", default=os.environ.get("CORELINK_PERF_TENANT", ""))
    parser.add_argument("--token", default=os.environ.get("CORELINK_PERF_PAT", ""))
    parser.add_argument("--mint-url", default=os.environ.get("CORELINK_FRESH_MINT_URL", ""))
    parser.add_argument("--mint-session", default=os.environ.get("CORELINK_FRESH_SESSION", ""))
    parser.add_argument("--mint-auth", default=os.environ.get("CORELINK_INTERNAL_AUTH_KEY", ""))
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if not args.base or not args.token or not UUID.fullmatch(args.tenant):
        raise SystemExit("base, tenant UUID and CORELINK_PERF_PAT are required")
    mint_url = args.mint_url or args.base
    if not args.mint_session or not args.mint_auth:
        raise SystemExit("B-106 owner blocker: CORELINK_FRESH_SESSION and CORELINK_INTERNAL_AUTH_KEY are required for the real mint API")
    mint = mint_fresh(mint_url, args.tenant, args.mint_session, args.mint_auth, op("b106-mint"))
    token_fp = mint["token_fingerprint"]
    result: dict[str, object] = {"captured_at_epoch": time.time(), "tenant_id": args.tenant, "items": {}}
    # B-102: three authenticated 1 KiB writes in one warm sequence.
    result["items"]["B-102"] = {"tenant_id": args.tenant, "token_fingerprint": "sha256:" + hashlib.sha256(args.token.encode()).hexdigest(),
                                  "sequence_window_seconds": 0, "requests": []}
    started = time.monotonic()
    for _ in range(3):
        result["items"]["B-102"]["requests"].append(call(args.base, args.tenant, args.token, "PUT", op("b102"), uuid.uuid4().hex, b"x" * 1024))
    result["items"]["B-102"]["sequence_window_seconds"] = round(time.monotonic() - started, 3)
    # B-103: raw counts at three levels; all levels are retained, including 429s.
    runs = []
    for concurrency in (4, 16, 64):
        started = time.monotonic()
        with ThreadPoolExecutor(max_workers=concurrency) as pool:
            rows = list(pool.map(lambda _: call(args.base, args.tenant, args.token, "PUT", op("b103"), uuid.uuid4().hex, b"x" * 1024), range(concurrency)))
        wall = (time.monotonic() - started) * 1000
        failures: dict[str, int] = {}
        for row in rows:
            if row["status"] != 200:
                failures[str(row["status"])] = failures.get(str(row["status"]), 0) + 1
        runs.append({"tenant_id": args.tenant, "operation_id": op(f"b103-level-{concurrency}"), "concurrency": concurrency,
                     "requests": len(rows), "successes": sum(row["status"] == 200 for row in rows), "failures": failures,
                     "wall_ms": round(wall, 3), "throughput_rps": round(sum(row["status"] == 200 for row in rows) / max(wall / 1000, .001), 6),
                     "raw_output_sha256": "sha256:" + hashlib.sha256(json.dumps(rows, sort_keys=True).encode()).hexdigest()})
    result["items"]["B-103"] = {"tenant_id": args.tenant, "runs": runs}
    # B-104: ten authenticated 404s on random keys.
    result["items"]["B-104"] = {"tenant_id": args.tenant, "samples": [call(args.base, args.tenant, args.token, "GET", op("b104"), uuid.uuid4().hex) for _ in range(10)]}
    # B-106: the real minted token must remain idle for a full revocation-cache
    # window before the cold request and same-colo warm control are observed.
    idle_started = time.time()
    time.sleep(COLD_IDLE_SECONDS)
    observed = time.time()
    cold = call(args.base, args.tenant, mint["token"], "GET", op("b106-cold"), uuid.uuid4().hex)
    warm = call(args.base, args.tenant, mint["token"], "GET", op("b106-warm"), uuid.uuid4().hex)
    if cold["status"] != 404 or cold["auth_source"] != "d1" or cold["authenticated"] is not True:
        raise RuntimeError("B-106 cold request did not prove authenticated d1 lookup on the wire")
    if warm["status"] != 404 or warm["auth_source"] not in {"l1", "kv"} or warm["authenticated"] is not True:
        raise RuntimeError("B-106 warm request did not prove authenticated cache lookup on the wire")
    if not cold["colo"] or cold["colo"] != warm["colo"] or not cold["response_request_id"] or not warm["response_request_id"] or cold["response_request_id"] == warm["response_request_id"]:
        raise RuntimeError("B-106 requests did not prove same-colo wire request identities")
    cold["token_fingerprint"] = warm["token_fingerprint"] = token_fp
    result["items"]["B-106"] = {"tenant_id": args.tenant, "kv_ttl_seconds": KV_PAT_ROW_TTL_SECONDS,
                                  "cold_attestation": {"tenant_id": args.tenant, "operation_id": mint["mint_operation_id"], "mint_operation_id": mint["mint_operation_id"], "minted_at_epoch": mint["minted_at_epoch"],
                                                        "unused_since_epoch": idle_started, "observed_at_epoch": observed,
                                                        "token_fingerprint": token_fp, "mint_request_id": mint["mint_request_id"],
                                                        "mint_raw_output_sha256": mint["mint_response_sha256"],
                                                        "mint_response_sha256": mint["mint_response_sha256"],
                                                        "mint_response_binding_sha256": mint["mint_response_binding_sha256"],
                                                        "pat_id": mint["pat_id"], "token_id": mint["token_id"], "expires_ms": mint["expires_ms"]},
                                  "cold": cold, "warm_control": warm}
    result["items"]["B-106"]["cold_attestation"]["mint_response_binding_sha256"] = mint_binding_sha256(
        result["items"]["B-106"]["cold_attestation"]
    )
    # B-107: `ostore` (R2) and `oaccounting` (D1) are distinct wire phases.
    # Never synthesize the old `or2`/`oaccounting` pair from one aggregate:
    # missing either phase means the deployed container is not the instrumented
    # version and this owner lane must fail closed.
    samples = []
    for _ in range(10):
        row = call(args.base, args.tenant, args.token, "PUT", op("b107"), uuid.uuid4().hex, b"x" * 1024)
        if row["status"] != 200 or row["method"] != "PUT" or row.get("error"):
            raise RuntimeError(f"B-107 request did not succeed: status={row['status']} method={row['method']}")
        row["r2_ms"], row["accounting_ms"] = phase(row["server_timing"], "ostore"), phase(row["server_timing"], "oaccounting")
        row["ostore_ms"] = row["r2_ms"]
        row["storage_total_ms"] = row["r2_ms"] + row["accounting_ms"]
        row["r2_raw_output_sha256"] = row["raw_output_sha256"]
        row["accounting_raw_output_sha256"] = row["raw_output_sha256"]
        samples.append(row)
    result["items"]["B-107"] = {"tenant_id": args.tenant, "samples": samples}
    args.output.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
