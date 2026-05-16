#!/usr/bin/env python3
"""Tiny in-memory HTTP mock target for the wave-25 endurance dress-run.

Purpose
-------
Provide a fast, dependency-free local HTTP server that responds 200/202
on the customer-facing wave-22 routes so the 10-min dress-run profile of
`tests/load/k6/scenarios/endurance-24h-w22.js` can exercise the full
harness pipeline (k6 -> summary -> analyzer -> verdict) on a dev box
without provisioning the real `apps/server` binary.

This is NOT a substitute for the real server. The greenlight verdict
emitted from a run against this mock is informative for harness wiring
only and is explicitly marked as such in the audit trail
(`specs/_audits/2026-05-16-endurance-10min-dressrun.md`).

Run:
    python3 scripts/_dressrun_mock_target.py --port 8787
Stops on SIGTERM/SIGINT.
"""
from __future__ import annotations

import argparse
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


class MockHandler(BaseHTTPRequestHandler):
    def _ok(self, status: int = 200, body: dict | None = None) -> None:
        payload = json.dumps(body or {"status": "ok"}).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def log_message(self, format: str, *args) -> None:  # noqa: A002
        # Silence default logging — k6 emits enough chatter.
        return

    def do_GET(self) -> None:  # noqa: N802
        if self.path == "/healthz":
            self._ok(200, {"status": "ok"})
            return
        if self.path.startswith("/v1/audit/analytics/event-count"):
            self._ok(200, {"count": 12345, "window": "24h"})
            return
        if self.path.startswith("/v1/audit/analytics/timeline"):
            self._ok(200, {"buckets": [{"t": 0, "n": 100}]})
            return
        if self.path.startswith("/v1/admin/diagnostics/memory"):
            self._ok(200, {"rss_bytes": 256 * 1024 * 1024, "cpu_user_pct": 12.5})
            return
        self._ok(404, {"error": "not_found"})

    def do_POST(self) -> None:  # noqa: N802
        length = int(self.headers.get("Content-Length") or 0)
        if length > 0:
            self.rfile.read(length)
        if self.path == "/v1/audit/export":
            self._ok(202, {"job_id": "mock-job"})
            return
        if self.path == "/v1/cas/upload":
            self._ok(200, {"sha256": "mock-digest"})
            return
        if self.path == "/v1/dsr/erasure":
            self._ok(202, {"request_id": "mock-dsr"})
            return
        if self.path == "/v1/clerk/auth":
            self._ok(200, {"session_token": "mock-token"})
            return
        self._ok(404, {"error": "not_found"})


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--host", default="127.0.0.1")
    args = ap.parse_args(argv)

    server = ThreadingHTTPServer((args.host, args.port), MockHandler)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
