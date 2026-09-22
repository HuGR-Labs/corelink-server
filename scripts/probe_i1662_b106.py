#!/usr/bin/env python3
"""Bounded live B-106 cold-to-warm PAT evidence lane.

The run subcommand creates exactly one named customer PAT, waits for the
revocation-cache window, and performs two authenticated GETs for nonexistent
keys.  The cleanup subcommand revokes that PAT.  The bearer is kept only in a
0600 runner-temporary state file; it is never printed or written to the
receipt.

This is intentionally not a general performance harness.  Its only data
plane calls are two GETs, so it cannot write B-102/B-103/B-107 state.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import secrets
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path
from typing import Any
from urllib.parse import urlsplit


TENANT_RE = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$",
    re.IGNORECASE,
)
PAT_ID_RE = re.compile(r"^[A-Za-z0-9._:-]{1,160}$")
AUTH_TIMING_RE = re.compile(
    r"(?:^|,\s*)auth;dur=(?P<ms>[0-9]+(?:\.[0-9]+)?);desc=\"(?P<source>l1|kv|d1)\"(?:,|$)"
)
COLO_RE = re.compile(r"^[A-Z]{3}$")
MIN_IDLE_SECONDS = 61
REQUEST_TIMEOUT_SECONDS = 15


class ProbeError(RuntimeError):
    """A fail-closed, non-secret probe failure."""


def fail(message: str) -> None:
    raise ProbeError(message)


def sha256(value: str) -> str:
    return "sha256:" + hashlib.sha256(value.encode("utf-8")).hexdigest()


def origin(value: str) -> str:
    parsed = urlsplit(value)
    if (
        parsed.scheme != "https"
        or not parsed.hostname
        or parsed.username is not None
        or parsed.password is not None
        or parsed.path not in ("", "/")
        or parsed.query
        or parsed.fragment
    ):
        fail("target must be an HTTPS origin without credentials, path, query, or fragment")
    if parsed.port is not None and not 1 <= parsed.port <= 65535:
        fail("target port is invalid")
    return f"https://{parsed.netloc}"


def read_json_response(
    request: urllib.request.Request,
    *,
    expected_status: set[int],
    body_limit: int = 64 * 1024,
) -> tuple[int, dict[str, str], bytes]:
    """Read a bounded response while never exposing its body in diagnostics."""

    try:
        with urllib.request.urlopen(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
            status = int(response.status)
            raw_headers = response.headers
            body = response.read(body_limit + 1)
            headers = {key.lower(): value.strip() for key, value in raw_headers.items()}
    except urllib.error.HTTPError as exc:
        # Error bodies can contain credentials or customer data.  Consume and
        # discard them, retaining only the status for the redacted receipt.
        try:
            exc.read(body_limit)
        finally:
            raise ProbeError(f"HTTP {exc.code} from allowlisted endpoint") from None
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        raise ProbeError(f"transport failure ({type(exc).__name__})") from None
    if len(body) > body_limit:
        fail("response exceeded the bounded body limit")
    if status not in expected_status:
        fail(f"unexpected HTTP status {status}")
    return status, headers, body


def json_response(
    request: urllib.request.Request,
    *,
    expected_status: set[int],
) -> dict[str, Any]:
    _status, _headers, raw = read_json_response(request, expected_status=expected_status)
    try:
        body = json.loads(raw)
    except (UnicodeDecodeError, json.JSONDecodeError):
        fail("endpoint returned non-JSON")
    if not isinstance(body, dict):
        fail("endpoint returned a non-object JSON response")
    return body


def auth_headers(session: str) -> dict[str, str]:
    if not session:
        fail("owner session is missing")
    return {
        "Authorization": f"Bearer {session}",
        "Accept": "application/json",
        "Content-Type": "application/json",
    }


def mint_pat(base: str, session: str, name: str) -> dict[str, str]:
    payload = json.dumps({"name": name, "scopes": ["cas:rw"]}, separators=(",", ":")).encode()
    request = urllib.request.Request(
        f"{base}/v1/customer/keys",
        data=payload,
        method="POST",
        headers=auth_headers(session),
    )
    body = json_response(request, expected_status={201})
    pat = body.get("pat")
    token = body.get("token")
    if not isinstance(pat, dict) or not isinstance(token, str) or not token:
        fail("PAT mint response omitted required fields")
    pat_id = pat.get("pat_id")
    if not isinstance(pat_id, str) or not PAT_ID_RE.fullmatch(pat_id):
        fail("PAT mint response contained an invalid pat_id")
    if pat.get("name") != name:
        fail("PAT mint response name did not match the unique probe name")
    scopes = pat.get("scopes")
    if not isinstance(scopes, list) or "cas:rw" not in scopes:
        fail("PAT mint response did not grant the expected cache-read scope")
    return {"pat_id": pat_id, "token": token, "token_fingerprint": sha256(token), "name": name}


def response_header(headers: dict[str, str], name: str) -> str:
    value = headers.get(name.lower(), "")
    if not value:
        fail(f"response omitted {name}")
    return value


def parse_colo(headers: dict[str, str]) -> str:
    ray = response_header(headers, "cf-ray")
    suffix = ray.rsplit("-", 1)[-1].upper()
    if not COLO_RE.fullmatch(suffix):
        fail("response had an invalid Cloudflare colo")
    return suffix


def parse_auth_timing(headers: dict[str, str]) -> tuple[float, str]:
    raw = response_header(headers, "server-timing")
    matches = list(AUTH_TIMING_RE.finditer(raw))
    if len(matches) != 1:
        fail("response did not contain exactly one parseable auth Server-Timing metric")
    return float(matches[0].group("ms")), matches[0].group("source")


def probe_get_allow_404(base: str, tenant: str, token: str, operation: str) -> dict[str, Any]:
    """GET a random missing key and retain only redacted wire metadata."""

    key = secrets.token_hex(32)
    request = urllib.request.Request(
        f"{base}/cargo/{tenant}/{key}",
        method="GET",
        headers={
            "Authorization": f"Bearer {token}",
            "Accept": "application/octet-stream",
            "X-Corelink-Operation": operation,
        },
    )
    started = time.monotonic()
    try:
        with urllib.request.urlopen(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
            status = int(response.status)
            headers = {key.lower(): value.strip() for key, value in response.headers.items()}
            body = response.read(4097)
    except urllib.error.HTTPError as exc:
        if exc.code != 404:
            try:
                exc.read(4096)
            finally:
                fail(f"authenticated probe returned HTTP {exc.code}")
        status = 404
        headers = {key.lower(): value.strip() for key, value in exc.headers.items()}
        body = exc.read(4097)
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        fail(f"authenticated probe transport failure ({type(exc).__name__})")
    if len(body) > 4096:
        fail("authenticated probe response exceeded the bounded body limit")
    elapsed_ms = round((time.monotonic() - started) * 1000, 3)
    auth_ms, auth_source = parse_auth_timing(headers)
    return {
        "operation_id": operation,
        "status": status,
        "wall_ms": elapsed_ms,
        "auth_ms": round(auth_ms, 3),
        "auth_source": auth_source,
        "colo": parse_colo(headers),
        "response_body_sha256": sha256(body.decode("utf-8", errors="replace")),
        "request_id": response_header(headers, "x-request-id"),
    }


def revoke_pat(base: str, session: str, pat_id: str) -> int:
    if not PAT_ID_RE.fullmatch(pat_id):
        fail("state contained an invalid pat_id")
    request = urllib.request.Request(
        f"{base}/v1/customer/keys/{pat_id}/revoke",
        method="POST",
        headers=auth_headers(session),
    )
    _body = json_response(request, expected_status={200})
    return 200


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    os.chmod(path, 0o600)


def load_state(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        fail("probe state is missing or malformed")
    if not isinstance(value, dict):
        fail("probe state is not a JSON object")
    return value


def run(args: argparse.Namespace) -> int:
    base = origin(args.target)
    tenant = args.tenant.lower()
    if not TENANT_RE.fullmatch(tenant):
        fail("tenant must be a canonical v4 UUID")
    owner_session = os.environ.get("CORELINK_B106_OWNER_SESSION", "")
    if not owner_session:
        fail("owner session is required")
    if not re.fullmatch(r"b106-cold-warm-[0-9]+-[0-9]+", args.name):
        fail("PAT name is not uniquely bound to this run")

    captured = time.time()
    receipt: dict[str, Any] = {
        "schema_version": 1,
        "issue": 1662,
        "backlog_id": "B-106",
        "valid": False,
        "captured_at_epoch": captured,
        "target_origin": base,
        "tenant_id": tenant,
        "pat_name": args.name,
        "requests": [],
        "cleanup": {"attempted": False, "revoked": False},
    }
    write_json(args.receipt, receipt)
    state: dict[str, Any] = {"target": base, "tenant": tenant, "pat_id": None}
    write_json(args.state, state)
    try:
        minted = mint_pat(base, owner_session, args.name)
        # Persist only the opaque row id needed by the unconditional revoke.
        # The bearer remains in this process's memory and never enters state.
        state["pat_id"] = minted["pat_id"]
        write_json(args.state, state)
        receipt["pat_id"] = minted["pat_id"]
        receipt["token_fingerprint"] = minted["token_fingerprint"]
        receipt["minted_at_epoch"] = time.time()
        write_json(args.receipt, receipt)

        idle_started = time.time()
        # This sleep is the only deliberate wait in the lane.  It proves the
        # L2 cache window elapsed before the first authenticated observation.
        time.sleep(MIN_IDLE_SECONDS)
        observed = time.time()
        if observed - idle_started < MIN_IDLE_SECONDS:
            fail("cold idle interval was shorter than 61 seconds")
        cold = probe_get_allow_404(base, tenant, minted["token"], f"b106-cold-{uuid.uuid4().hex}")
        if cold["auth_source"] != "d1":
            fail("cold request did not prove auth;desc=\"d1\"")
        warm = probe_get_allow_404(base, tenant, minted["token"], f"b106-warm-{uuid.uuid4().hex}")
        if warm["auth_source"] not in {"l1", "kv"}:
            fail("immediate warm request did not prove auth;desc=\"l1\" or \"kv\"")
        if warm["colo"] != cold["colo"]:
            fail("cold and warm requests were served by different colos")
        receipt.update(
            {
                "unused_since_epoch": idle_started,
                "observed_at_epoch": observed,
                "cold_interval_seconds": round(observed - idle_started, 3),
                "requests": [cold, warm],
                "valid": True,
            }
        )
        write_json(args.receipt, receipt)
        return 0
    except ProbeError as exc:
        receipt["failure"] = str(exc)
        write_json(args.receipt, receipt)
        raise


def cleanup(args: argparse.Namespace) -> int:
    state = load_state(args.state)
    pat_id = state.get("pat_id")
    if pat_id is None:
        if args.receipt.exists():
            try:
                output = json.loads(args.receipt.read_text(encoding="utf-8"))
            except (OSError, UnicodeDecodeError, json.JSONDecodeError):
                output = {"schema_version": 1, "issue": 1662, "backlog_id": "B-106", "valid": False}
            if not isinstance(output, dict):
                output = {"schema_version": 1, "issue": 1662, "backlog_id": "B-106", "valid": False}
            output["cleanup"] = {"attempted": False, "revoked": True, "result": "no_pat_minted"}
            write_json(args.receipt, output)
        return 0
    owner_session = os.environ.get("CORELINK_B106_OWNER_SESSION", "")
    if not isinstance(pat_id, str) or not owner_session:
        fail("probe state is incomplete; refusing to claim cleanup")
    output: dict[str, Any] = {"schema_version": 1, "issue": 1662, "backlog_id": "B-106", "valid": False}
    try:
        status = revoke_pat(origin(args.target), owner_session, pat_id)
        # Revoke first.  A missing or damaged receipt must never block cleanup.
        try:
            output = json.loads(args.receipt.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            output = {"schema_version": 1, "issue": 1662, "backlog_id": "B-106", "valid": False}
        if not isinstance(output, dict):
            output = {"schema_version": 1, "issue": 1662, "backlog_id": "B-106", "valid": False}
        output["cleanup"] = {"attempted": True, "revoked": True, "status": status}
        # Remove the bearer state as soon as the revoke has succeeded.  The
        # state file is never an uploaded artifact.
        args.state.unlink(missing_ok=True)
        write_json(args.receipt, output)
        return 0
    except ProbeError as exc:
        output["cleanup"] = {"attempted": True, "revoked": False, "result": str(exc)}
        write_json(args.receipt, output)
        raise


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser()
    sub = p.add_subparsers(dest="command", required=True)
    run_parser = sub.add_parser("run")
    run_parser.add_argument("--target", required=True)
    run_parser.add_argument("--tenant", required=True)
    run_parser.add_argument("--name", required=True)
    run_parser.add_argument("--state", type=Path, required=True)
    run_parser.add_argument("--receipt", type=Path, required=True)
    cleanup_parser = sub.add_parser("cleanup")
    cleanup_parser.add_argument("--target", required=True)
    cleanup_parser.add_argument("--state", type=Path, required=True)
    cleanup_parser.add_argument("--receipt", type=Path, required=True)
    return p


def main() -> int:
    args = parser().parse_args()
    try:
        if args.command == "run":
            return run(args)
        return cleanup(args)
    except ProbeError as exc:
        print(f"B-106 lane failed closed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
