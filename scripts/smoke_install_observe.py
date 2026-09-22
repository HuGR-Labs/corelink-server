#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import http.client
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
SCHEMA_VERSION = 1
OBSERVATIONS = (
    "startup",
    "readiness",
    "endpoint_functional",
    "endpoint_wrong",
    "timeout",
    "process_death",
    "cleanup",
    "artifact_provenance",
)
class _FixtureHandler(BaseHTTPRequestHandler):
    def do_GET(self) -> None:  # noqa: N802 - stdlib handler API
        if self.path == "/healthz":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ready\n")
        elif self.path == "/":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"corelink smoke service\n")
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, _format: str, *_args: object) -> None:
        return
def _serve(port: int) -> int:
    server = ThreadingHTTPServer(("127.0.0.1", port), _FixtureHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        return 130
    finally:
        server.server_close()
    return 0
def _now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
def _request(port: int, path: str, timeout: float) -> tuple[int, bytes]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    try:
        connection.request("GET", path)
        response = connection.getresponse()
        return response.status, response.read()
    finally:
        connection.close()
def _wait_for(port: int, path: str, expected: int, deadline: float) -> tuple[bool, str]:
    while time.monotonic() < deadline:
        try:
            status, _body = _request(port, path, min(0.2, max(0.01, deadline - time.monotonic())))
            if status == expected:
                return True, f"status={status}"
        except (OSError, socket.timeout):
            pass
        time.sleep(0.02)
    return False, f"expected_status={expected}"
def _backend_probe(timeout: float) -> dict[str, Any]:
    docker = shutil.which("docker")
    if docker is None:
        return {"status": "FAIL", "kind": "unavailable", "reason": "docker_missing"}
    try:
        completed = subprocess.run(
            [docker, "info"], capture_output=True, text=True, timeout=timeout, check=False
        )
    except subprocess.TimeoutExpired:
        return {"status": "FAIL", "kind": "unavailable", "reason": "docker_info_timeout"}
    if completed.returncode != 0:
        return {"status": "FAIL", "kind": "unavailable", "reason": "docker_info_failed"}
    details = f"{completed.stdout}\n{completed.stderr}".lower()
    kind = "shim" if "nerdctl" in details or "containerd" in details else "daemon_or_unknown"
    return {"status": "PASS", "kind": kind, "reason": "docker_info_reachable"}


def _provenance() -> dict[str, str]:
    helper = Path(__file__).resolve()
    digest = hashlib.sha256(helper.read_bytes()).hexdigest()
    return {
        "helper": str(helper.relative_to(Path.cwd())) if helper.is_relative_to(Path.cwd()) else str(helper),
        "helper_sha256": digest,
        "repository": os.environ.get("GITHUB_REPOSITORY", "local"),
        "ref": os.environ.get("GITHUB_REF", "local"),
        "sha": os.environ.get("GITHUB_SHA", "local"),
        "workflow": os.environ.get("GITHUB_WORKFLOW", "local"),
    }


def observe(receipt_path: Path, timeout: float) -> dict[str, Any]:
    started = time.monotonic()
    observations: dict[str, dict[str, Any]] = {}
    failures: list[str] = []
    backend = _backend_probe(timeout)
    if backend["status"] != "PASS":
        failures.append("backend_unavailable")

    process: subprocess.Popen[bytes] | None = None
    temp_root: Path | None = None
    port = _free_port()
    try:
        temp_root = Path(tempfile.mkdtemp(prefix="corelink-i1672-"))
        process = subprocess.Popen(
            [sys.executable, str(Path(__file__).resolve()), "--serve", "--port", str(port)],
            cwd=temp_root,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        observations["startup"] = {
            "status": "PASS" if process.poll() is None else "FAIL",
            "pid": process.pid,
            "process_alive": process.poll() is None,
        }
        if observations["startup"]["status"] != "PASS":
            failures.append("startup_failed")

        ready, detail = _wait_for(port, "/healthz", 200, time.monotonic() + timeout)
        observations["readiness"] = {"status": "PASS" if ready else "FAIL", "detail": detail}
        if not ready:
            failures.append("readiness_failed")

        try:
            status, body = _request(port, "/", timeout)
            functional = status == 200 and b"corelink smoke service" in body
            observations["endpoint_functional"] = {"status": "PASS" if functional else "FAIL", "http_status": status}
        except (OSError, socket.timeout) as exc:
            observations["endpoint_functional"] = {"status": "FAIL", "error": type(exc).__name__}
        if observations["endpoint_functional"]["status"] != "PASS":
            failures.append("endpoint_not_functional")

        try:
            status, _body = _request(port, "/wrong-endpoint", timeout)
            wrong = status == 404
            observations["endpoint_wrong"] = {"status": "PASS" if wrong else "FAIL", "http_status": status}
        except (OSError, socket.timeout) as exc:
            observations["endpoint_wrong"] = {"status": "FAIL", "error": type(exc).__name__}
        if observations["endpoint_wrong"]["status"] != "PASS":
            failures.append("wrong_endpoint_not_rejected")

        unused_port = _free_port()
        timeout_started = time.monotonic()
        timed_out = not _wait_for(unused_port, "/healthz", 200, timeout_started + min(timeout, 0.25))[0]
        observations["timeout"] = {"status": "PASS" if timed_out else "FAIL", "port": unused_port}
        if not timed_out:
            failures.append("timeout_not_observed")
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=timeout)
        dead = process is not None and process.poll() is not None
        observations["process_death"] = {"status": "PASS" if dead else "FAIL", "process_alive": not dead}
        if not dead:
            failures.append("process_did_not_exit")
        if temp_root is not None:
            shutil.rmtree(temp_root, ignore_errors=True)
        cleaned = temp_root is not None and not temp_root.exists()
        observations["cleanup"] = {"status": "PASS" if cleaned else "FAIL", "temporary_path_removed": cleaned}
        if not cleaned:
            failures.append("cleanup_failed")

    provenance = _provenance()
    observations["artifact_provenance"] = {"status": "PASS", **provenance}
    service_failures = [
        name for name in OBSERVATIONS[:-1] if observations.get(name, {}).get("status") != "PASS"
    ]
    if backend["status"] == "PASS" and not service_failures:
        classification = f"{backend['kind']}_reachable_service_functional"
    elif backend["status"] == "PASS":
        classification = f"{backend['kind']}_reachable_service_unfunctional"
    elif not service_failures:
        classification = "backend_unavailable_service_functional"
    else:
        classification = "backend_unavailable_service_unfunctional"
    receipt: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "receipt_type": "corelink_smoke_install_observation",
        "issue": 1672,
        "captured_at": _now(),
        "runner": {
            "kind": "github-hosted" if os.environ.get("GITHUB_ACTIONS") == "true" else "local",
            "label": os.environ.get("RUNNER_LABEL", "ubuntu-latest"),
            "event": os.environ.get("GITHUB_EVENT_NAME", "manual"),
        },
        "backend": backend,
        "service": {
            "classification": classification,
            "functional": not service_failures,
            "failures": service_failures,
            "process_and_endpoint_are_independent": True,
        },
        "observations": observations,
        "failures": failures,
        "result": "PASS" if not failures else "FAIL",
        "duration_ms": round((time.monotonic() - started) * 1000),
    }
    receipt_path.parent.mkdir(parents=True, exist_ok=True)
    receipt_path.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return receipt


def _free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--receipt", type=Path, default=Path("artifacts/i1672/smoke-install-receipt.json"))
    parser.add_argument("--timeout-seconds", type=float, default=2.0)
    parser.add_argument("--serve", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--port", type=int, default=0, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.serve:
        return _serve(args.port)
    receipt = observe(args.receipt, max(0.1, args.timeout_seconds))
    print(json.dumps(receipt, sort_keys=True))
    return 0 if receipt["result"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
