#!/usr/bin/env python3
"""Collect a bounded, isolated B-105 cache-on/cache-off build comparison.

The local WebDAV forwarder gives each Actions run a unique key prefix, records
application payload bytes, and keeps the exact key inventory needed for cleanup.
It never logs credentials, cache objects, or cache keys.
"""

from __future__ import annotations

import hashlib
import http.client
import http.server
from html import escape as xml_escape
import argparse
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import threading
import time
import uuid
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit


COMMAND = ("cargo", "test", "--package", "corelink-reapi", "--release", "--no-run")
TENANT = "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3"
KEY_RE = re.compile(r"^[A-Za-z0-9_.-]{1,64}$")
MAX_BODY_BYTES = 128 * 1024 * 1024
TIMEOUT_SECONDS = 900


def stats(env: dict[str, str]) -> dict[str, int]:
    result = subprocess.run(
        ("sccache", "--show-stats"), env=env, capture_output=True, text=True,
        timeout=15, check=True,
    )
    raw = result.stdout + "\n" + result.stderr
    labels = {
        "Cache hits": "hits",
        "Cache misses": "misses",
        "Cache read errors": "read_errors",
        "Cache write errors": "write_errors",
    }
    parsed: dict[str, int] = {}
    for line in raw.splitlines():
        for label, key in labels.items():
            if line.strip().startswith(label):
                value = line.split()[-1].replace(",", "")
                if value.isdigit():
                    parsed[key] = int(value)
    if set(parsed) != set(labels.values()):
        raise RuntimeError("sccache omitted a required hit/miss/error counter")
    return parsed


class Meter:
    def __init__(self, path: Path, prefix: str) -> None:
        self.path = path
        self.prefix = prefix
        self.lock = threading.Lock()
        self.phase = "setup"
        self.keys: set[str] = set()
        self.counters: dict[str, dict[str, int]] = {}
        self.put_sizes: dict[str, int] = {}

    def set_phase(self, phase: str) -> None:
        with self.lock:
            self.phase = phase
            self.counters.setdefault(phase, {"get_bytes": 0, "put_bytes": 0, "control_get_bytes": 0, "control_put_bytes": 0, "gets": 0, "puts": 0})

    def key(self, value: str) -> str:
        with self.lock:
            if value not in self.keys:
                self.keys.add(value)
                data = {
                    "schema": "corelink.b105.namespace.v1",
                    "tenant_id": TENANT,
                    "prefix": self.prefix,
                    "keys": sorted(self.keys),
                }
                self.path.parent.mkdir(parents=True, exist_ok=True)
                temporary = self.path.with_suffix(".tmp")
                temporary.write_text(json.dumps(data, sort_keys=True) + "\n", encoding="utf-8")
                temporary.chmod(0o600)
                temporary.replace(self.path)
        return self.prefix + "-" + value

    def request(self, method: str, key: str, request_bytes: int, status: int, response_bytes: int) -> None:
        with self.lock:
            row = self.counters.setdefault(self.phase, {"get_bytes": 0, "put_bytes": 0, "control_get_bytes": 0, "control_put_bytes": 0, "gets": 0, "puts": 0})
            if method == "GET":
                row["gets"] += 1
                if 200 <= status < 300:
                    metric = "control_get_bytes" if key == ".sccache_check" else "get_bytes"
                    row[metric] += response_bytes
            elif method == "PUT":
                row["puts"] += 1
                if 200 <= status < 300:
                    metric = "control_put_bytes" if key == ".sccache_check" else "put_bytes"
                    row[metric] += request_bytes
                    if metric == "put_bytes":
                        self.put_sizes[key] = request_bytes
            elif method == "DELETE" and 200 <= status < 300:
                self.put_sizes.pop(key, None)


class Forwarder(http.server.ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = False


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    meter: Meter
    origin: str
    tenant: str
    token: str

    def log_message(self, _format: str, *_args: object) -> None:
        # Request paths contain cache keys and must not enter Actions logs.
        return

    def do_GET(self) -> None:  # noqa: N802
        self.forward()

    def do_PUT(self) -> None:  # noqa: N802
        self.forward()

    def do_HEAD(self) -> None:  # noqa: N802
        self.forward()

    def do_DELETE(self) -> None:  # noqa: N802
        self.forward()

    def do_PROPFIND(self) -> None:  # noqa: N802
        self.forward()

    def do_MKCOL(self) -> None:  # noqa: N802
        self.forward()

    def forward(self) -> None:
        parsed = urlsplit(self.path)
        parts = parsed.path.split("/")
        if len(parts) not in (3, 4) or parts[1] != "cargo" or parts[2] != self.tenant:
            self.send_error(400)
            return
        key = unquote(parts[3]) if len(parts) == 4 else ""
        if key:
            if not KEY_RE.fullmatch(key):
                self.send_error(400)
                return
            upstream_key = self.meter.key(key)
        elif self.command in ("PROPFIND", "MKCOL") and parsed.path.endswith("/"):
            # The storage backend may stat/create its collection root. Present
            # a virtual local root; never forward a tenant-root call.
            self.send_collection_root(parsed.path)
            return
        else:
            self.send_error(400)
            return

        try:
            content_length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            self.send_error(400)
            return
        if content_length < 0 or content_length > MAX_BODY_BYTES:
            self.send_error(413)
            return
        request_body = self.rfile.read(content_length) if content_length else b""
        target = urlsplit(self.origin)
        request_path = (
            target.path.rstrip("/")
            + f"/cargo/{self.tenant}"
            + (f"/{quote(upstream_key, safe='._-')}" if upstream_key else "/")
        )
        if parsed.query:
            request_path += "?" + parsed.query
        headers = {
            "Authorization": "Bearer " + self.token,
            "Content-Length": str(len(request_body)),
            "Connection": "close",
        }
        for name in ("Content-Type", "Depth", "If-Match", "If-None-Match"):
            value = self.headers.get(name)
            if value:
                headers[name] = value
        connection = http.client.HTTPSConnection(target.hostname, target.port or 443, timeout=60)
        try:
            connection.request(self.command, request_path, body=request_body or None, headers=headers)
            response = connection.getresponse()
            response_body = response.read(MAX_BODY_BYTES + 1)
            if len(response_body) > MAX_BODY_BYTES:
                raise RuntimeError("cache response exceeded the bounded body limit")
            self.meter.request(self.command, key, len(request_body), response.status, len(response_body))
            self.send_response(response.status)
            for name in ("Content-Type", "Content-Length", "Last-Modified", "ETag", "DAV"):
                value = response.getheader(name)
                if value is not None:
                    self.send_header(name, value)
            if response.getheader("Content-Length") is None and self.command != "HEAD":
                self.send_header("Content-Length", str(len(response_body)))
            self.send_header("Connection", "close")
            self.end_headers()
            if self.command != "HEAD" and response_body:
                self.wfile.write(response_body)
            self.close_connection = True
        except Exception:
            self.meter.request(self.command, key, len(request_body), 599, 0)
            self.send_error(502)
        finally:
            connection.close()

    def send_collection_root(self, path: str) -> None:
        if self.command == "MKCOL":
            self.send_response(201)
            self.send_header("Content-Length", "0")
            self.send_header("Connection", "close")
            self.end_headers()
            self.close_connection = True
            return
        href = xml_escape(path)
        body = (
            '<?xml version="1.0" encoding="utf-8"?>\n'
            '<D:multistatus xmlns:D="DAV:"><D:response><D:href>'
            f"{href}</D:href><D:propstat><D:prop>"
            "<D:resourcetype><D:collection/></D:resourcetype>"
            "<D:getlastmodified>Thu, 01 Jan 1970 00:00:00 GMT</D:getlastmodified>"
            "</D:prop><D:status>HTTP/1.1 200 OK</D:status>"
            "</D:propstat></D:response></D:multistatus>"
        ).encode("utf-8")
        self.send_response(207)
        self.send_header("Content-Type", "application/xml")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(body)
        self.close_connection = True


def run_arm(mode: str, pair: int | None, env: dict[str, str], meter: Meter, target: Path) -> dict[str, object]:
    shutil.rmtree(target, ignore_errors=True)
    env = env.copy()
    env["CARGO_TARGET_DIR"] = str(target)
    env["CARGO_INCREMENTAL"] = "0"
    env["CORELINK_SCCACHE_PILOT"] = "on" if mode == "enabled" else "off"
    if mode == "enabled":
        subprocess.run(("sccache", "--stop-server"), env=env, capture_output=True, timeout=15, check=False)
        env["RUSTC_WRAPPER"] = "sccache"
        env["SCCACHE_IGNORE_SERVER_IO_ERROR"] = "0"
        subprocess.run(("sccache", "--zero-stats"), env=env, capture_output=True, timeout=15, check=True)
    else:
        env.pop("RUSTC_WRAPPER", None)
        env.pop("RUSTC_WORKSPACE_WRAPPER", None)
        env.pop("SCCACHE_WEBDAV_ENDPOINT", None)
        env.pop("SCCACHE_WEBDAV_TOKEN", None)
        env.pop("SCCACHE_IGNORE_SERVER_IO_ERROR", None)

    before = time.monotonic()
    process = subprocess.run(COMMAND, env=env, capture_output=True, timeout=TIMEOUT_SECONDS, check=False)
    elapsed = time.monotonic() - before
    output = process.stdout + b"\n" + process.stderr
    if process.returncode != 0:
        raise RuntimeError(f"{mode} build failed with exit {process.returncode}; output sha256={hashlib.sha256(output).hexdigest()}")
    counters = stats(env) if mode == "enabled" else {"hits": 0, "misses": 0, "read_errors": 0, "write_errors": 0}
    return {
        "mode": mode,
        "pair": pair,
        "duration_seconds": round(elapsed, 3),
        "cache": counters,
        "output_sha256": "sha256:" + hashlib.sha256(output).hexdigest(),
        "returncode": process.returncode,
    }


def validated_origin() -> tuple[str, str]:
    base = os.environ.get("CORELINK_PERF_BASE", "").rstrip("/")
    token = os.environ.get("CORELINK_SCCACHE_TOKEN", "")
    origin = urlsplit(base)
    if (
        not token
        or origin.scheme != "https"
        or origin.hostname != "corelink-api.humangr.com"
        or origin.username is not None
        or origin.password is not None
        or origin.path not in ("", "/")
        or origin.query
        or origin.fragment
    ):
        raise RuntimeError("B-105 requires the canonical HTTPS origin and dogfood cache credential")
    return base, token


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--cleanup-only", action="store_true")
    parser.add_argument("--namespace-manifest", type=Path)
    args = parser.parse_args()
    if args.cleanup_only:
        return cleanup_manifest(args.namespace_manifest or Path(os.environ["RUNNER_TEMP"]) / "b105-namespace.json")
    if os.environ.get("GITHUB_REF") != "refs/heads/main" or os.environ.get("GITHUB_REF_PROTECTED") != "true":
        print("B-105 production measurement is restricted to protected main", file=sys.stderr)
        return 2
    try:
        base, token = validated_origin()
    except RuntimeError as exc:
        print(str(exc), file=sys.stderr)
        return 2
    run_id = os.environ.get("GITHUB_RUN_ID", "")
    revision = os.environ.get("GITHUB_SHA", "")
    runner = os.environ.get("RUNNER_NAME", "")
    if not run_id.isdigit() or not re.fullmatch(r"[0-9a-f]{40}", revision) or not runner:
        print("B-105 GitHub run identity is incomplete", file=sys.stderr)
        return 2

    runner_temp = Path(os.environ["RUNNER_TEMP"])
    namespace_path = runner_temp / "b105-namespace.json"
    target_dir = runner_temp / "b105-target"
    prefix = f"b105-{run_id}-{uuid.uuid4().hex[:12]}"
    meter = Meter(namespace_path, prefix)
    handler = type("B105Handler", (Handler,), {"meter": meter, "origin": base, "tenant": TENANT, "token": token})
    server = Forwarder(("127.0.0.1", 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    endpoint = f"http://127.0.0.1:{server.server_port}/cargo/{TENANT}"
    common_env = os.environ.copy()
    common_env["SCCACHE_WEBDAV_ENDPOINT"] = endpoint
    common_env["SCCACHE_WEBDAV_TOKEN"] = token
    common_env["SCCACHE_IGNORE_SERVER_IO_ERROR"] = "0"
    result: dict[str, object] = {
        "schema": "corelink.b105.lane.v3",
        "run_id": run_id,
        "revision": revision,
        "runner": runner,
        "runner_os": os.environ.get("RUNNER_OS", ""),
        "machine": os.uname().machine,
        "toolchain": subprocess.run(("rustc", "-vV"), capture_output=True, text=True, timeout=15, check=True).stdout.splitlines()[0],
        "target": subprocess.run(("rustc", "--print", "host-tuple"), capture_output=True, text=True, timeout=15, check=True).stdout.strip(),
        "workload": list(COMMAND),
        "namespace_prefix_sha256": "sha256:" + hashlib.sha256(prefix.encode()).hexdigest(),
        "cache_namespace_isolated": True,
        "result": "incomplete",
    }
    pairs: list[dict[str, object]] = []
    failure: str | None = None
    try:
        # Seed writes are measured and reported separately from the six pairs.
        meter.set_phase("seed")
        seed = run_arm("enabled", None, common_env, meter, target_dir)
        meter.set_phase("pair-0-control")
        pairs.append({"index": 0, "control_first": True, "control": run_arm("disabled", 0, common_env, meter, target_dir)})
        meter.set_phase("pair-0-treatment")
        pairs[0]["treatment"] = run_arm("enabled", 0, common_env, meter, target_dir)
        for index in range(1, 6):
            control_first = index % 2 == 0
            entry: dict[str, object] = {"index": index, "control_first": control_first}
            first, second = (("disabled", "enabled") if control_first else ("enabled", "disabled"))
            for mode in (first, second):
                meter.set_phase(f"pair-{index}-{mode}")
                entry["control" if mode == "disabled" else "treatment"] = run_arm(mode, index, common_env, meter, target_dir)
            pairs.append(entry)
        result["seed"] = seed
        result["pairs"] = pairs
        deltas = [float(p["treatment"]["duration_seconds"]) - float(p["control"]["duration_seconds"]) for p in pairs]  # type: ignore[index]
        ordered = sorted(deltas)
        median = (ordered[2] + ordered[3]) / 2
        mean_delta = statistics.mean(deltas)
        deviation = statistics.stdev(deltas)
        confidence_half_width = 2.571 * deviation / (len(deltas) ** 0.5)
        confidence_interval = [round(mean_delta - confidence_half_width, 3), round(mean_delta + confidence_half_width, 3)]
        result["pair_deltas_seconds"] = deltas
        result["median_delta_seconds"] = round(median, 3)
        result["mean_delta_seconds"] = round(mean_delta, 3)
        result["delta_95_percent_confidence_interval_seconds"] = confidence_interval
        result["result"] = (
            "faster" if deviation > 0 and confidence_interval[1] < 0
            else "slower" if deviation > 0 and confidence_interval[0] > 0
            else "indeterminate"
        )
        if any(int(p["treatment"]["cache"]["read_errors"]) or int(p["treatment"]["cache"]["write_errors"]) for p in pairs):  # type: ignore[index]
            failure = "cache IO error in a measured treatment"
        elif not all(int(p["treatment"]["cache"]["hits"]) > 0 for p in pairs):  # type: ignore[index]
            failure = "at least one treatment had no cache hits"
    except Exception as exc:
        # Do not print child process output or request paths; only the bounded
        # exception class and an explicitly scrubbed message are retained.
        failure = f"{type(exc).__name__}: {str(exc)[:180]}"
    finally:
        retained_indexed_bytes = sum(meter.put_sizes.values())
        meter.set_phase("cleanup")
        cleaned, cleanup_failures = cleanup(base, token, prefix, meter.keys)
        meter.set_phase("cleanup_verify")
        for key in sorted(meter.keys):
            try:
                status, _ = propfind(base, token, prefix + "-" + key)
                if status != 404:
                    cleanup_failures += 1
            except Exception:
                cleanup_failures += 1
        result["namespace"] = {"touched_keys": len(meter.keys), "retained_indexed_objects": len(meter.put_sizes), "retained_indexed_payload_bytes": retained_indexed_bytes}
        result["cleanup"] = {"attempted": len(meter.keys), "delete_successes": cleaned, "failures": cleanup_failures, "verified_absent": cleanup_failures == 0}
        result["application_payload"] = meter.counters
        result["currency_cost"] = None
        result["currency_cost_basis"] = "physical quantities only; this lane does not claim a provider invoice rate"
        result["failure"] = failure
        if failure:
            result["result"] = "indeterminate"
        server.shutdown()
        server.server_close()
        shutil.rmtree(target_dir, ignore_errors=True)

    output_path = runner_temp / "b105-lane.json"
    output_path.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    output_path.chmod(0o600)
    print(json.dumps({"result": result["result"], "median_delta_seconds": result.get("median_delta_seconds"), "cleanup_verified": result.get("cleanup", {}).get("verified_absent"), "failure": failure}, sort_keys=True))
    if failure or int(result["cleanup"]["failures"]):  # type: ignore[index]
        return 1
    return 0


def cleanup_manifest(path: Path) -> int:
    try:
        base, token = validated_origin()
    except RuntimeError:
        print("B-105 cleanup unavailable: protected origin or credential is absent", file=sys.stderr)
        return 2
    try:
        manifest = json.loads(path.read_text(encoding="utf-8"))
        prefix = manifest["prefix"]
        keys = manifest["keys"]
        if manifest.get("schema") != "corelink.b105.namespace.v1" or manifest.get("tenant_id") != TENANT:
            raise ValueError("unexpected namespace manifest")
        if not re.fullmatch(r"b105-[0-9]{1,20}-[0-9a-f]{12}", prefix):
            raise ValueError("unexpected namespace prefix")
        if not isinstance(keys, list) or any(not isinstance(key, str) or not KEY_RE.fullmatch(key) for key in keys):
            raise ValueError("unsafe key inventory")
    except Exception as exc:
        print(f"B-105 cleanup manifest invalid: {type(exc).__name__}", file=sys.stderr)
        return 2
    deleted, failed = cleanup(base, token, prefix, set(keys))
    verify_failures = 0
    for key in keys:
        try:
            status, _ = propfind(base, token, prefix + "-" + key)
            if status != 404:
                verify_failures += 1
        except Exception:
            verify_failures += 1
    print(json.dumps({"keys": len(keys), "delete_successes": deleted, "delete_failures": failed, "verification_failures": verify_failures}, sort_keys=True))
    return 1 if failed or verify_failures else 0


def propfind(base: str, token: str, key: str) -> tuple[int, int | None]:
    import urllib.error
    import urllib.request
    from xml.etree import ElementTree

    request = urllib.request.Request(
        base.rstrip("/") + f"/cargo/{TENANT}/{quote(key, safe='._-')}",
        data=b"<?xml version='1.0'?><D:propfind xmlns:D='DAV:'><D:prop><D:getcontentlength/></D:prop></D:propfind>",
        headers={"Authorization": "Bearer " + token, "Depth": "0", "Content-Type": "application/xml"},
        method="PROPFIND",
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            body = response.read(1024 * 1024)
            size = None
            if response.status == 207:
                tree = ElementTree.fromstring(body)
                node = tree.find(".//{DAV:}getcontentlength")
                if node is not None and node.text and node.text.isdigit():
                    size = int(node.text)
            return response.status, size
    except urllib.error.HTTPError as exc:
        return exc.code, None


def cleanup(base: str, token: str, prefix: str, keys: set[str]) -> tuple[int, int]:
    import urllib.error
    import urllib.request

    succeeded = 0
    failed = 0
    for key in sorted(keys):
        request = urllib.request.Request(
            base.rstrip("/") + f"/cargo/{TENANT}/{quote(prefix + '-' + key, safe='._-')}",
            headers={"Authorization": "Bearer " + token},
            method="DELETE",
        )
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                if response.status in (200, 202, 204):
                    succeeded += 1
                else:
                    failed += 1
        except urllib.error.HTTPError as exc:
            # DELETE is idempotent; preserve exact-key cleanup if the backend
            # reports that an already absent key is not-found.
            if exc.code == 404:
                succeeded += 1
            else:
                failed += 1
        except Exception:
            failed += 1
    return succeeded, failed


if __name__ == "__main__":
    raise SystemExit(main())
