"""Bounded, run-scoped B-105 cache forwarding and accounting.

The future credentialed measurement calls this code through a local WebDAV
boundary.  The boundary accepts only one generated namespace below one tenant,
and cleanup can address only the exact keys this process recorded.
"""

from __future__ import annotations

import http.client
import http.server
import math
import re
import socket
import statistics
import threading
import time
from collections import defaultdict
from html import escape as xml_escape
from urllib.parse import quote, unquote, urlsplit


MAX_BODY_BYTES = 128 * 1024 * 1024
MAX_TRACKED_KEYS = 4096
MAX_CLEANUP_SECONDS = 900
MAX_REQUEST_SECONDS = 60
KEY_RE = re.compile(r"^[A-Za-z0-9_.-]{1,64}$")
TENANT_RE = re.compile(r"^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$")
PREFIX_RE = re.compile(r"^b105-[0-9]{1,20}-[0-9a-f]{12}$")
REMOTE_PATH_RE = re.compile(
    r"^/cargo/([0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12})/"
    r"(b105-[0-9]{1,20}-[0-9a-f]{12})-([A-Za-z0-9_.-]{1,64})$"
)
REMOTE_METHODS = frozenset(("GET", "PUT", "HEAD", "DELETE", "PROPFIND"))


def generated_prefix(run_id: str, nonce: str) -> str:
    """Bind the generated namespace to this Actions run and one fresh nonce."""
    if not run_id.isdigit() or not re.fullmatch(r"[0-9a-f]{12}", nonce):
        raise ValueError("unsafe B-105 generated namespace")
    return f"b105-{run_id}-{nonce}"


def _blank_counter() -> dict[str, int]:
    return {"get_bytes": 0, "put_bytes": 0, "gets": 0, "puts": 0}


class Meter:
    """Records one exact run namespace and successful application payloads."""

    def __init__(self, prefix: str) -> None:
        if not PREFIX_RE.fullmatch(prefix):
            raise ValueError("unsafe B-105 namespace prefix")
        self.prefix = prefix
        self.phase = "setup"
        self.keys: set[str] = set()
        self.counters: dict[str, dict[str, int]] = defaultdict(_blank_counter)
        self.indexed_payloads: dict[str, int] = {}
        self.successes: set[tuple[str, str, str]] = set()
        self.cleanup_inventory: tuple[str, ...] | None = None
        self.accepting = True
        self.active_forwards = 0
        self.lock = threading.Condition()

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
            if not self.accepting or self.cleanup_inventory is not None:
                raise RuntimeError("B-105 key inventory is frozen for cleanup")
            if key not in self.keys and len(self.keys) >= MAX_TRACKED_KEYS:
                raise RuntimeError("B-105 exact-key inventory limit exceeded")
            self.keys.add(key)
        return f"{self.prefix}-{key}"

    def begin_forward(self, key: str) -> tuple[str, str]:
        """Reserve a key and freeze the accounting phase before remote I/O."""
        if not KEY_RE.fullmatch(key):
            raise ValueError("unsafe B-105 cache key")
        with self.lock:
            if not self.accepting or self.cleanup_inventory is not None:
                raise RuntimeError("B-105 key inventory is frozen for cleanup")
            if key not in self.keys and len(self.keys) >= MAX_TRACKED_KEYS:
                raise RuntimeError("B-105 exact-key inventory limit exceeded")
            self.keys.add(key)
            self.active_forwards += 1
            return f"{self.prefix}-{key}", self.phase

    def end_forward(self) -> None:
        with self.lock:
            self.active_forwards -= 1
            self.lock.notify_all()

    def record(
        self, method: str, key: str, request_bytes: int, status: int, response_bytes: int,
        *, phase: str | None = None,
    ) -> None:
        """Count only a first complete success; errors and retry noise are absent."""
        complete = (method == "GET" and status == 200) or (method == "PUT" and status in (200, 201, 204))
        if not complete:
            return
        with self.lock:
            request_phase = self.phase if phase is None else phase
            identity = (request_phase, method, key)
            if identity in self.successes:
                return
            self.successes.add(identity)
            row = self.counters[request_phase]
            if method == "GET":
                row["gets"] += 1
                row["get_bytes"] += response_bytes
            else:
                row["puts"] += 1
                row["put_bytes"] += request_bytes
                self.indexed_payloads[key] = request_bytes

    def record_delete(self, key: str, status: int) -> None:
        if 200 <= status < 300:
            with self.lock:
                self.indexed_payloads.pop(key, None)

    def retained_payload_bytes(self) -> int:
        with self.lock:
            return sum(self.indexed_payloads.values())

    def freeze_for_cleanup(self, deadline: float, monotonic: object = time.monotonic) -> tuple[str, ...] | None:
        """Stop new forwarding, drain in-flight requests, then freeze exact keys."""
        clock = monotonic  # type: ignore[assignment]
        with self.lock:
            self.accepting = False
            while self.active_forwards:
                remaining = deadline - clock()
                if remaining <= 0:
                    return None
                self.lock.wait(remaining)
            if self.cleanup_inventory is None:
                self.cleanup_inventory = tuple(sorted(self.keys))
            return self.cleanup_inventory


class Forwarder(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = False


class Handler(http.server.BaseHTTPRequestHandler):
    """Local WebDAV boundary: tenant collections are virtual; keys forward."""

    protocol_version = "HTTP/1.1"
    meter: Meter
    origin: str
    tenant: str
    token: str

    def log_message(self, _format: str, *_args: object) -> None:
        return

    do_GET = lambda self: self.forward()  # noqa: E731,N815
    do_PUT = lambda self: self.forward()  # noqa: E731,N815
    do_HEAD = lambda self: self.forward()  # noqa: E731,N815
    do_DELETE = lambda self: self.forward()  # noqa: E731,N815
    do_PROPFIND = lambda self: self.forward()  # noqa: E731,N815
    do_MKCOL = lambda self: self.forward()  # noqa: E731,N815

    def _root(self, path: str) -> bool:
        return path == f"/cargo/{self.tenant}/"

    def _key(self, path: str) -> str | None:
        prefix = f"/cargo/{self.tenant}/"
        if not path.startswith(prefix):
            return None
        raw = path[len(prefix):]
        if not raw or "/" in raw or "%" in raw:
            return None
        key = unquote(raw)
        return key if KEY_RE.fullmatch(key) else None

    def forward(self) -> None:
        self.connection.settimeout(MAX_REQUEST_SECONDS)
        parsed = urlsplit(self.path)
        if parsed.scheme or parsed.netloc or parsed.query or parsed.fragment:
            self.send_error(400)
            self.close_connection = True
            return
        if self._root(parsed.path):
            if self.command in ("PROPFIND", "MKCOL"):
                self.local_collection_root(parsed.path)
            else:
                self.send_error(400)
            return
        key = self._key(parsed.path)
        if key is None:
            self.send_error(400)
            self.close_connection = True
            return
        if self.command not in REMOTE_METHODS:
            self.send_error(405)
            self.close_connection = True
            return
        lengths = self.headers.get_all("Content-Length", [])
        if len(lengths) > 1 or self.headers.get("Transfer-Encoding") is not None:
            self.send_error(400)
            self.close_connection = True
            return
        try:
            length = int(lengths[0]) if lengths else 0
        except ValueError:
            self.send_error(400)
            self.close_connection = True
            return
        if length < 0 or length > MAX_BODY_BYTES:
            self.send_error(413)
            self.close_connection = True
            return
        body = self.rfile.read(length) if length else b""
        if len(body) != length:
            self.send_error(400)
            self.close_connection = True
            return
        try:
            namespaced, phase = self.meter.begin_forward(key)
        except (ValueError, RuntimeError):
            self.send_error(400)
            self.close_connection = True
            return
        try:
            status, headers, response = _origin_request(
                self.origin, self.token, self.command,
                remote_key_path(self.tenant, self.meter, key, namespaced=namespaced), body,
            )
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
            self.meter.record(self.command, key, len(body), status, len(response), phase=phase)
        except Exception:
            self.send_error(502)
        finally:
            self.meter.end_forward()
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


def remote_key_path(tenant: str, meter: Meter, key: str, *, namespaced: str | None = None) -> str:
    if not TENANT_RE.fullmatch(tenant) or not KEY_RE.fullmatch(key):
        raise ValueError("unsafe B-105 tenant")
    remote_key = namespaced if namespaced is not None else meter.namespace_key(key)
    if remote_key != f"{meter.prefix}-{key}":
        raise ValueError("unsafe B-105 remote key")
    return f"/cargo/{tenant}/{quote(remote_key, safe='._-')}"


def recorded_key_path(tenant: str, meter: Meter, key: str) -> str:
    """Make a cleanup target only for a key recorded by this meter instance."""
    if not TENANT_RE.fullmatch(tenant) or not KEY_RE.fullmatch(key):
        raise ValueError("unsafe B-105 cleanup path")
    with meter.lock:
        if key not in meter.keys:
            raise ValueError("unrecorded B-105 cleanup key")
        namespaced = f"{meter.prefix}-{key}"
    return f"/cargo/{tenant}/{quote(namespaced, safe='._-')}"


def _resolved_before_deadline(host: str, port: int, deadline: float, monotonic: object) -> list[tuple[object, ...]]:
    """Resolve in a daemon thread so DNS cannot hold the request past its bound."""
    clock = monotonic  # type: ignore[assignment]
    completed = threading.Event()
    result: dict[str, object] = {}

    def resolve() -> None:
        try:
            result["addresses"] = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
        except Exception as exc:
            result["error"] = exc
        finally:
            completed.set()

    threading.Thread(target=resolve, daemon=True).start()
    remaining = deadline - clock()
    if remaining <= 0 or not completed.wait(remaining) or clock() >= deadline:
        raise TimeoutError("B-105 origin name resolution exceeded its request deadline")
    if "error" in result:
        raise OSError("B-105 origin name resolution failed") from result["error"]  # type: ignore[arg-type]
    addresses = result.get("addresses")
    if not isinstance(addresses, list) or not addresses:
        raise OSError("B-105 origin name resolution returned no address")
    return addresses  # type: ignore[return-value]


class DeadlineHTTPSConnection(http.client.HTTPSConnection):
    """HTTPSConnection whose DNS, TCP, and TLS setup share one deadline."""

    def __init__(self, host: str, port: int, timeout: float, deadline: float, monotonic: object) -> None:
        super().__init__(host, port, timeout=timeout)
        self.deadline = deadline
        self.monotonic = monotonic
        self._deadline_lock = threading.Lock()
        self._deadline_expired = False

    def expire(self) -> None:
        with self._deadline_lock:
            self._deadline_expired = True
            if self.sock is not None:
                try:
                    self.sock.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            self.close()

    def connect(self) -> None:
        clock = self.monotonic  # type: ignore[assignment]
        with self._deadline_lock:
            if self._deadline_expired or clock() >= self.deadline:
                raise TimeoutError("B-105 origin connection exceeded its request deadline")
        addresses = _resolved_before_deadline(self.host, self.port, self.deadline, clock)
        last_error: OSError | None = None
        for family, kind, protocol, _, address in addresses:
            remaining = min(float(self.timeout), self.deadline - clock())
            if remaining <= 0:
                raise TimeoutError("B-105 origin connection exceeded its request deadline")
            sock = socket.socket(family, kind, protocol)
            try:
                sock.settimeout(remaining)
                sock.connect(address)
                remaining = min(float(self.timeout), self.deadline - clock())
                if remaining <= 0:
                    raise TimeoutError("B-105 origin connection exceeded its request deadline")
                sock.settimeout(remaining)
                wrapped = self._context.wrap_socket(sock, server_hostname=self.host)
                with self._deadline_lock:
                    if self._deadline_expired or clock() >= self.deadline:
                        wrapped.close()
                        raise TimeoutError("B-105 TLS setup exceeded its request deadline")
                    self.sock = wrapped
                if clock() >= self.deadline:
                    raise TimeoutError("B-105 TLS setup exceeded its request deadline")
                return
            except OSError as exc:
                last_error = exc
                sock.close()
                if clock() >= self.deadline:
                    raise TimeoutError("B-105 origin connection exceeded its request deadline") from exc
        if last_error is not None:
            raise last_error
        raise OSError("B-105 origin connection returned no usable address")


def _new_origin_connection(host: str, port: int, timeout: float, deadline: float, monotonic: object) -> http.client.HTTPSConnection:
    return DeadlineHTTPSConnection(host, port, timeout, deadline, monotonic)


def _origin_request(
    origin: str, token: str, method: str, path: str, body: bytes = b"", timeout_seconds: float = MAX_REQUEST_SECONDS,
) -> tuple[int, dict[str, str], bytes]:
    target = urlsplit(origin)
    if (target.scheme != "https" or not target.hostname or target.username is not None or target.password is not None
            or target.path not in ("", "/") or target.query or target.fragment):
        raise ValueError("B-105 origin must be a plain HTTPS origin")
    match = REMOTE_PATH_RE.fullmatch(path)
    if not match or not PREFIX_RE.fullmatch(match.group(2)) or not KEY_RE.fullmatch(match.group(3)):
        raise ValueError("unsafe B-105 remote path")
    if method not in REMOTE_METHODS or len(body) > MAX_BODY_BYTES:
        raise ValueError("unsafe B-105 remote request")
    if timeout_seconds <= 0 or timeout_seconds > MAX_REQUEST_SECONDS:
        raise ValueError("unsafe B-105 request timeout")
    request_path = target.path.rstrip("/") + path
    clock = time.monotonic
    deadline = clock() + timeout_seconds
    connection = _new_origin_connection(target.hostname, target.port or 443, timeout_seconds, deadline, clock)
    expire = getattr(connection, "expire", connection.close)
    watchdog = threading.Timer(timeout_seconds, expire)
    watchdog.daemon = True
    watchdog.start()
    try:
        connection.request(method, request_path, body=body or None, headers={
            "Authorization": "Bearer " + token,
            "Content-Length": str(len(body)), "Connection": "close",
        })
        response = connection.getresponse()
        payload = response.read(MAX_BODY_BYTES + 1)
        if clock() >= deadline:
            raise TimeoutError("B-105 origin response exceeded its request deadline")
        if len(payload) > MAX_BODY_BYTES:
            raise RuntimeError("B-105 origin response exceeded bound")
        return response.status, {name: value for name, value in response.getheaders()}, payload
    finally:
        watchdog.cancel()
        connection.close()


def cleanup_exact(
    origin: str, token: str, tenant: str, meter: Meter, *, cleanup_seconds: float = MAX_CLEANUP_SECONDS,
    monotonic: object = time.monotonic,
) -> tuple[int, int]:
    """For every recorded key, DELETE immediately followed by same-key 404."""
    if cleanup_seconds <= 0 or cleanup_seconds > MAX_CLEANUP_SECONDS:
        raise ValueError("unsafe B-105 cleanup deadline")
    clock = monotonic  # type: ignore[assignment]
    deadline = clock() + cleanup_seconds
    deleted = failures = 0
    keys = meter.freeze_for_cleanup(deadline, clock)
    if keys is None:
        return 0, max(1, len(meter.keys))
    for position, key in enumerate(keys):
        remaining = deadline - clock()
        if remaining <= 0:
            failures += len(keys) - position
            break
        path = recorded_key_path(tenant, meter, key)
        try:
            status, _, _ = _origin_request(origin, token, "DELETE", path, timeout_seconds=min(MAX_REQUEST_SECONDS, remaining))
            meter.record_delete(key, status)
            if 200 <= status < 300:
                deleted += 1
            else:
                failures += 1
        except Exception:
            failures += 1
        remaining = deadline - clock()
        if remaining <= 0:
            failures += 1
            continue
        try:
            verify, _, _ = _origin_request(origin, token, "PROPFIND", path, timeout_seconds=min(MAX_REQUEST_SECONDS, remaining))
            if verify != 404:
                failures += 1
        except Exception:
            failures += 1
    return deleted, failures


def paired_summary(pairs: list[dict[str, object]]) -> dict[str, object]:
    """Validate precisely six complete alternating pairs and a paired 95% CI."""
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
        for arm in (control, treatment):
            returncode = arm.get("returncode")
            if (arm.get("status") != "complete" or not isinstance(returncode, int)
                    or isinstance(returncode, bool) or returncode != 0):
                raise ValueError("B-105 pair arm did not complete successfully")
        if control.get("cache_mode") != "disabled" or treatment.get("cache_mode") != "enabled":
            raise ValueError("B-105 pair arms are mislabeled")
        control_duration = control.get("duration_seconds")
        treatment_duration = treatment.get("duration_seconds")
        if (not isinstance(control_duration, (int, float)) or isinstance(control_duration, bool)
                or not isinstance(treatment_duration, (int, float)) or isinstance(treatment_duration, bool)
                or not math.isfinite(control_duration) or not math.isfinite(treatment_duration)
                or control_duration < 0 or treatment_duration < 0):
            raise ValueError("B-105 pair duration is missing")
        deltas.append(float(treatment_duration) - float(control_duration))
    mean = statistics.mean(deltas)
    deviation = statistics.stdev(deltas)
    half_width = 2.571 * deviation / (len(deltas) ** 0.5)
    interval = [round(mean - half_width, 3), round(mean + half_width, 3)]
    result = "faster" if interval[1] < 0 else "slower" if interval[0] > 0 else "indeterminate"
    return {
        "pair_deltas_seconds": [round(delta, 3) for delta in deltas],
        "mean_delta_seconds": round(mean, 3),
        "delta_95_percent_confidence_interval_seconds": interval,
        "result": result,
    }
