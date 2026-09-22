#!/usr/bin/env python3
"""Bounded, read-only deployed B-129 diagnostic probe for issue #1671.

The target must explicitly expose the deployed commit and diagnostic flag in
response headers.  This is intentional: without wire identity this receipt
cannot be evidence for the deployed version.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import ssl
import sys
import time
import urllib.parse
import http.client
from datetime import datetime, timezone

SHA = re.compile(r"^[0-9a-fA-F]{40}$")
UUID = re.compile(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")
TOKEN = re.compile(r"^[A-Za-z0-9._~:-]{1,180}$")
PHASE = re.compile(r"(?P<n>[a-z][a-z0-9_-]*);dur=(?P<d>[0-9]+(?:\.[0-9]+)?)")
Q = ("qtier", "qdo", "qbatch", "qresid", "qcontrol")
ORIGIN = ("opat", "oquota", "ostore", "oaccounting", "oargon", "opermit", "ortier", "oaudit", "oratelimit", "ohandler")


def fail(message: str) -> None:
    raise RuntimeError(message)


def timing(raw: str) -> dict[str, float]:
    values: dict[str, float] = {}
    for match in PHASE.finditer(raw):
        name, value = match.group("n"), float(match.group("d"))
        if name in values or not math.isfinite(value):
            fail("malformed or duplicate Server-Timing phase")
        values[name] = value
    if not values or any(";dur=" in part and not PHASE.search(part) for part in raw.split(",")):
        fail("malformed or empty Server-Timing header")
    if "oother" in values:
        if "ohandler" in values and not math.isclose(values["oother"], values["ohandler"], abs_tol=1e-6):
            fail("conflicting ohandler/oother alias")
        values.setdefault("ohandler", values["oother"])
    return values


def request(url: urllib.parse.SplitResult, token: str) -> tuple[int, dict[str, str], float]:
    started = time.monotonic()
    conn_cls = http.client.HTTPSConnection if url.scheme == "https" else http.client.HTTPConnection
    if url.scheme == "https":
        conn = conn_cls(url.hostname, url.port or 443, timeout=20, context=ssl.create_default_context())
    else:
        conn = conn_cls(url.hostname, url.port or 80, timeout=20)
    try:
        conn.request("GET", url.path + ("?" + url.query if url.query else ""), headers={"Authorization": f"Bearer {token}", "Accept": "*/*", "Cache-Control": "no-store"})
        response = conn.getresponse()
        # Never persist or print the response body. Read only a small prefix so
        # the server can finish its response while keeping the lane bounded.
        response.read(4096)
        headers = {key.lower(): value.strip() for key, value in response.getheaders()}
        return response.status, headers, time.monotonic() - started
    finally:
        conn.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--tenant", required=True)
    parser.add_argument("--cargo-key", required=True)
    parser.add_argument("--deployed-sha", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--samples", type=int, default=10)
    args = parser.parse_args()
    if os.environ.get("OWNER_APPROVED_PROBE") != "1":
        fail("OWNER_APPROVED_PROBE must equal 1")
    if os.environ.get("DEPLOYED_SERVER_TIMING_WDB_DETAIL") != "on":
        fail("deployed Server-Timing detail flag is not confirmed on")
    declared_sha = os.environ.get("CORELINK_DEPLOYED_COMMIT", "")
    if declared_sha and declared_sha.lower() != args.deployed_sha.lower():
        fail("declared deployed SHA does not match dispatch SHA")
    if not SHA.fullmatch(args.deployed_sha) or not UUID.fullmatch(args.tenant) or not TOKEN.fullmatch(args.cargo_key):
        fail("invalid target, tenant, cargo key, or deployed SHA")
    token = os.environ.get("CORELINK_DOGFOOD_PAT")
    if not token:
        fail("CORELINK_DOGFOOD_PAT is required")
    target = urllib.parse.urlsplit(args.target)
    if target.scheme != "https" or target.username or target.password or target.path not in ("", "/") or target.query or target.fragment or not target.hostname:
        fail("target must be an HTTPS origin without credentials, query, or fragment")
    base = args.target.rstrip("/") + "/cargo/" + args.tenant + "/" + args.cargo_key
    url = urllib.parse.urlsplit(base)
    if args.samples < 1 or args.samples > 10:
        fail("sample count must be between 1 and 10")
    deadline = time.monotonic() + 240
    rows = []
    for ordinal in range(1, args.samples + 1):
        if time.monotonic() >= deadline:
            fail("probe deadline exceeded")
        status, headers, wall = request(url, token)
        wire_sha = headers.get("x-corelink-deployed-sha") or headers.get("x-corelink-deployed-commit")
        wire_flag = headers.get("x-corelink-server-timing-wdb-detail")
        if not wire_sha or wire_sha.lower() != args.deployed_sha.lower():
            fail("deployed SHA missing or mismatched on wire")
        if wire_flag != "on":
            fail("diagnostic flag missing or mismatched on wire")
        values = timing(headers.get("server-timing", ""))
        required = set(Q) | {"ohop", "wdb", "origin", "total"}
        if not required.issubset(values):
            fail("required B-129 Server-Timing phase missing")
        if not math.isclose(sum(values[n] for n in Q), values["wdb"], abs_tol=1e-6):
            fail("q phase sum does not reconcile to wdb")
        if not math.isclose(values["ohop"] + sum(values.get(n, 0.0) for n in ORIGIN), values["origin"], abs_tol=1e-6):
            fail("origin phase sum does not reconcile")
        if not headers.get("x-request-id") or not headers.get("cf-ray"):
            fail("response is missing request or colo identity")
        if status != 200 or values["total"] <= 0 or values["total"] + 1e-6 < values["wdb"] + values["origin"]:
            fail("non-success or over-counted timing row")
        residual = 100.0 * (values["total"] - values["wdb"] - values["origin"]) / values["total"]
        rows.append({"sample": ordinal, "status": status, "wall_s": round(wall, 6), "phases_ms": values, "residual_pct": round(residual, 6), "request_id": headers.get("x-request-id", ""), "cf_ray": headers.get("cf-ray", "")})
    maximum = max(row["residual_pct"] for row in rows)
    if maximum >= 10:
        fail(f"residual is {maximum:.3f}%, expected <10%")
    receipt = {"issue": 1671, "work_package": "B-129", "kind": "approved_read_only_probe", "target_origin": f"{target.scheme}://{target.netloc}", "tenant": args.tenant, "cargo_key_sha256": hashlib.sha256(args.cargo_key.encode()).hexdigest(), "deployed_sha": args.deployed_sha.lower(), "diagnostic_flag": "on", "captured_at": datetime.now(timezone.utc).isoformat(), "sample_count": args.samples, "residual_max_pct": maximum, "rows": rows}
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(receipt, handle, sort_keys=True, indent=2)
        handle.write("\n")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (RuntimeError, OSError, ValueError) as exc:
        print(f"probe failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
