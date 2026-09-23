"""Bounded, tenant-root-safe accounting for the B-105 cache lane.

This module is deliberately stdlib-only so the hosted contract can exercise the
same forwarding and cleanup code with a fake origin.  It never discovers keys:
the only remote object operations are for keys recorded from successful cache
traffic in this process.
"""

from __future__ import annotations

import http.client
import http.server
import json
import re
import statistics
import threading
import time
from collections import defaultdict
from html import escape as xml_escape
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit


MAX_BODY_BYTES = 128 * 1024 * 1024
MAX_TRACKED_KEYS = 4096
MAX_CLEANUP_SECONDS = 900
KEY_RE = re.compile(r"^[A-Za-z0-9_.-]{1,64}$")


def blank_counter() -> dict[str, int]:
    return {"get_bytes": 0, "put_bytes": 0, "gets": 0, "puts": 0}


class Meter:
    """Records exact cache keys and successful application payload bytes."""

    def __init__(self, prefix: str) -> None:
        if not re.fullmatch(r"b105-[0-9]{1,20}-[0-9a-f]{12}", prefix):
            raise ValueError("unsafe B-105 namespace prefix")
        self.prefix = prefix
        self.phase = "setup"
        self.keys: set[str] = set()
        self.counters: dict[str, dict[str, int]] = defaultdict(blank_counter)
        self.indexed_payloads: dict[str, int] = {}
        self.lock = threading.Lock()

    def set_phase(self, phase: str) -> None:
        if not re.fullmatch(r"(?:seed|pair-[0-5]-(?:enabled|disabled)|cleanup|cleanup_verify)", phase):
            raise ValueError("unsafe B-105 meter phase")
        with self.lock:
            self.phase = phase
            self.counters[phase]

    def namespace_key(self, key: str) -> str:
        if not KEY_RE.fullmatch(key):
            raise ValueError("unsafe B-105 cache key")
        with self.lock:
            if key not in self.keys and len(self.keys) >= MAX_TRACKED_KEYS:
                raise RuntimeError("B-105 exact-key inventory limit exceeded")
            self.keys.add(key)
        return f"{self.prefix}-{key}"

    def record(self, method: str, key: str, request_bytes: int, status: int, response_bytes: int) -> None:
        """Count only successful GET bodies and successful PUT request bodies."""
        with self.lock:
            row = self.counters[self.phase]
            if method == "GET":
                row["gets"] += 1
                if 200 <= status < 300:
                    row["get_bytes"] += response_bytes
            elif method == "PUT":
                row["puts"] += 1
                if 200 <= status < 300:
                    row["put_bytes"] += request_bytes
                    self.indexed_payloads[key] = request_bytes
            elif method == "DELETE" and 200 <= status < 300:
                self.indexed_payloads.pop(key, None)

    def retained_payload_bytes(self) -> int:
        with self.lock:
            return sum(self.indexed_payloads.values())

    def write_manifest(self, path: Path, tenant: str) -> None:
        if not self.keys:
            return
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_suffix(".tmp")
        temporary.write_text(
            json.dumps({"schema": "corelink.b105.namespace.v2", "tenant_id": tenant,
                        "prefix": self.prefix, "keys": sorted(self.keys)}, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        temporary.chmod(0o600)
        temporary.replace(path)


class Forwarder(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = False


class Handler(http.server.BaseHTTPRequestHandler):
    """Local WebDAV boundary: collection roots are virtual; keys are forwarded."""

    protocol_version = "HTTP/1.1"
    meter: Meter
    origin: str
    tenant: str
    token: str

    def log_message(self, _format: str, *_args: object) -> None:
        # Paths carry cache keys and must never reach Actions logs.
        return

    do_GET = lambda self: self.forward()  # noqa: E731,N815
    do_PUT = lambda self: self.forward()  # noqa: E731,N815
    do_HEAD = lambda self: self.forward()  # noqa: E731,N815
    do_DELETE = lambda self: self.forward()  # noqa: E731,N815
    do_PROPFIND = lambda self: self.forward()  # noqa: E731,N815
    do_MKCOL = lambda self: self.forward()  # noqa: E731,N815

    def _root(self, parsed_path: str) -> bool:
        return parsed_path == f"/cargo/{self.tenant}/"

    def _key(self, parsed_path: str) -> str | None:
        prefix = f"/cargo/{self.tenant}/"
        if not parsed_path.startswith(prefix):
            return None
        raw = parsed_path[len(prefix):]
        # Refuse encoded and nested forms so cleanup cannot be tricked into a
        # second path spelling for an otherwise valid key.
        if not raw or "/" in raw or "%" in raw:
            return None
        key = unquote(raw)
        return key if KEY_RE.fullmatch(key) else None

    def forward(self) -> None:
        parsed = urlsplit(self.path)
        if self._root(parsed.path):
            if self.command in ("PROPFIND", "MKCOL"):
                self.local_collection_root(parsed.path)
            else:
                self.send_error(400)
            return
        key = self._key(parsed.path)
        if key is None:
            self.send_error(400)
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self.send_error(400)
            return
        if length < 0 or length > MAX_BODY_BYTES:
            self.send_error(413)
            return
        body = self.rfile.read(length) if length else b""
        try:
            status, headers, response = origin_request(
                self.origin, self.token, self.command,
                f"/cargo/{self.tenant}/{quote(self.meter.namespace_key(key), safe='._-')}", body,
            )
            self.meter.record(self.command, key, len(body), status, len(response))
            self.send_response(status)
            for name in ("Content-Type", "Content-Length", "Last-Modified", "ETag", "DAV"):
                if name in headers:
                    self.send_header(name, headers[name])
            if "Content-Length" not in headers and self.command != "HEAD":
                self.send_header("Content-Length", str(len(response)))
            self.send_header("Connection", "close")
            self.end_headers()
            if self.command != "HEAD" and response:
                self.wfile.write(response)
        except Exception:
            self.meter.record(self.command, key, len(body), 599, 0)
            self.send_error(502)
        self.close_connection = True

    def local_collection_root(self, path: str) -> None:
        if self.command == "MKCOL":
            self.send_response(201)
            self.send_header("Content-Length", "0")
            self.end_headers()
            self.close_connection = True
            return
        body = (
            '<?xml version="1.0" encoding="utf-8"?><D:multistatus xmlns:D="DAV:"><D:response>'
            f"<D:href>{xml_escape(path)}</D:href><D:propstat><D:prop><D:resourcetype>"
            "<D:collection/></D:resourcetype></D:prop><D:status>HTTP/1.1 200 OK</D:status>"
            "</D:propstat></D:response></D:multistatus>"
        ).encode()
        self.send_response(207)
        self.send_header("Content-Type", "application/xml")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True


def origin_request(origin: str, token: str, method: str, path: str, body: bytes = b"", timeout_seconds: int = 60) -> tuple[int, dict[str, str], bytes]:
    target = urlsplit(origin)
    if target.scheme != "https" or not target.hostname or target.query or target.fragment:
        raise ValueError("B-105 origin must be a plain HTTPS origin")
    request_path = target.path.rstrip("/") + path
    connection = http.client.HTTPSConnection(target.hostname, target.port or 443, timeout=timeout_seconds)
    try:
        connection.request(method, request_path, body=body or None, headers={
            "Authorization": "Bearer " + token,
            "Content-Length": str(len(body)), "Connection": "close",
        })
        response = connection.getresponse()
        payload = response.read(MAX_BODY_BYTES + 1)
        if len(payload) > MAX_BODY_BYTES:
            raise RuntimeError("B-105 origin response exceeded bound")
        return response.status, {name: value for name, value in response.getheaders()}, payload
    finally:
        connection.close()


def cleanup_exact(origin: str, token: str, tenant: str, meter: Meter) -> tuple[int, int]:
    """Delete then PROPFIND every recorded key; no prefix or collection operation exists."""
    deleted = failures = 0
    keys = sorted(meter.keys)
    deadline = time.monotonic() + MAX_CLEANUP_SECONDS
    for position, key in enumerate(keys):
        remaining = int(deadline - time.monotonic())
        if remaining <= 0:
            failures += len(keys) - position
            break
        scoped = f"/cargo/{tenant}/{quote(meter.namespace_key(key), safe='._-')}"
        try:
            status, _, _ = origin_request(origin, token, "DELETE", scoped, timeout_seconds=min(60, remaining))
            meter.record("DELETE", key, 0, status, 0)
            deleted += int(200 <= status < 300)
            remaining = int(deadline - time.monotonic())
            if remaining <= 0:
                failures += 1
                continue
            verify, _, _ = origin_request(origin, token, "PROPFIND", scoped, timeout_seconds=min(60, remaining))
            failures += int(verify != 404)
        except Exception:
            failures += 1
    return deleted, failures


def paired_summary(pairs: list[dict[str, object]]) -> dict[str, object]:
    """Validate and summarize exactly six alternating disabled/enabled pairs."""
    if len(pairs) != 6:
        raise ValueError("B-105 requires exactly six paired observations")
    deltas: list[float] = []
    for index, pair in enumerate(pairs):
        if pair.get("index") != index or pair.get("control_first") is not (index % 2 == 0):
            raise ValueError("B-105 pair order is not alternating")
        control = pair.get("control")
        treatment = pair.get("treatment")
        if not isinstance(control, dict) or not isinstance(treatment, dict):
            raise ValueError("B-105 pair is incomplete")
        if control.get("cache_mode") != "disabled" or treatment.get("cache_mode") != "enabled":
            raise ValueError("B-105 pair arms are mislabeled")
        control_duration = control.get("duration_seconds")
        treatment_duration = treatment.get("duration_seconds")
        if not isinstance(control_duration, (int, float)) or not isinstance(treatment_duration, (int, float)):
            raise ValueError("B-105 pair duration is missing")
        deltas.append(float(treatment_duration) - float(control_duration))
    mean = statistics.mean(deltas)
    deviation = statistics.stdev(deltas)
    half_width = 2.571 * deviation / (len(deltas) ** 0.5)
    interval = [round(mean - half_width, 3), round(mean + half_width, 3)]
    result = "faster" if interval[1] < 0 else "slower" if interval[0] > 0 else "indeterminate"
    return {"pair_deltas_seconds": [round(delta, 3) for delta in deltas],
            "mean_delta_seconds": round(mean, 3),
            "delta_95_percent_confidence_interval_seconds": interval, "result": result}
