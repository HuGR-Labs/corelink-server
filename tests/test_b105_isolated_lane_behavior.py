"""GitHub-hosted behavioral proof for the isolated B-105 collector boundary."""

from __future__ import annotations

import http.client
import json
import os
import socket
import tempfile
import threading
import time
import unittest
from pathlib import Path
from unittest import mock
from urllib.parse import urlsplit

from scripts import b105_isolated_lane as lane
from scripts import collect_b105_same_lane as collector


TENANT = "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3"
PREFIX = lane.generated_prefix("42", "0123456789ab")


class FakeResponse:
    def __init__(self, status: int, payload: bytes = b"") -> None:
        self.status = status
        self.payload = payload

    def read(self, _limit: int) -> bytes:
        return self.payload

    def getheaders(self) -> list[tuple[str, str]]:
        return [("Content-Length", str(len(self.payload)))]


class TruncatedResponse:
    status = 200

    def read(self, _limit: int) -> bytes:
        raise http.client.IncompleteRead(b"partial", 8)

    def getheaders(self) -> list[tuple[str, str]]:
        return [("Content-Length", "10")]


class FakeHttpsConnection:
    requests: list[tuple[str, str, bytes, float]] = []
    attempts: dict[tuple[str, str], int] = {}

    def __init__(self, _host: str, _port: int, timeout: float) -> None:
        self.timeout = timeout
        self.request_data: tuple[str, str, bytes] | None = None

    def request(self, method: str, path: str, body: bytes | None, headers: dict[str, str]) -> None:
        assert headers["Authorization"] == "Bearer redacted-test-token"
        self.request_data = (method, path, body or b"")
        self.requests.append((*self.request_data, self.timeout))

    def getresponse(self) -> FakeResponse:
        assert self.request_data is not None
        method, path, _ = self.request_data
        identity = (method, path)
        self.attempts[identity] = self.attempts.get(identity, 0) + 1
        if method == "GET" and path.endswith("-failed"):
            return FakeResponse(500, b"must-not-count")
        if method == "GET" and path.endswith("-partial"):
            return FakeResponse(206, b"must-not-count")
        if method == "GET" and path.endswith("-redirect"):
            return FakeResponse(302, b"must-not-count")
        if method == "GET" and path.endswith("-retry") and self.attempts[identity] == 1:
            return FakeResponse(500, b"must-not-count")
        if method == "GET" and path.endswith("-oversize"):
            return FakeResponse(200, b"0123456789abcdef")
        if method == "GET" and path.endswith("-truncated"):
            return TruncatedResponse()
        if method == "PUT" and path.endswith("-failed"):
            return FakeResponse(500, b"must-not-count")
        if method == "PUT" and path.endswith("-truncated"):
            return TruncatedResponse()
        if method == "DELETE" and path.endswith("-delete-failed"):
            return FakeResponse(500)
        if method == "DELETE" and path.endswith("-delete-transport-failed"):
            raise OSError("fake DELETE transport failure")
        if method == "GET":
            return FakeResponse(200, b"read-payload")
        if method == "PROPFIND":
            return FakeResponse(404)
        return FakeResponse(204 if method == "DELETE" else 201)

    def close(self) -> None:
        return


class IsolatedLaneBehaviorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.original_connection_factory = lane._new_origin_connection
        lane._new_origin_connection = lambda host, port, timeout, _deadline, _monotonic: FakeHttpsConnection(host, port, timeout)
        FakeHttpsConnection.requests = []
        FakeHttpsConnection.attempts = {}
        self.meter = lane.Meter(PREFIX)
        handler = type("TestHandler", (lane.Handler,), {
            "meter": self.meter, "origin": "https://cache.example.invalid", "tenant": TENANT,
            "token": "redacted-test-token",
        })
        self.server = lane.Forwarder(("127.0.0.1", 0), handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        lane._new_origin_connection = self.original_connection_factory

    def request(self, method: str, path: str, body: bytes = b"") -> tuple[int, bytes]:
        connection = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=5)
        connection.request(method, path, body=body, headers={"Content-Length": str(len(body))})
        response = connection.getresponse()
        result = response.status, response.read()
        connection.close()
        with self.meter.lock:
            if not self.meter.lock.wait_for(lambda: self.meter.active_forwards == 0, timeout=5):
                raise AssertionError("forwarding request did not finish after its response")
        return result

    @staticmethod
    def pairs(deltas: list[float]) -> list[dict[str, object]]:
        return [
            {"index": index, "control_first": index % 2 == 0,
             "control": {"cache_mode": "disabled", "status": "complete", "returncode": 0, "duration_seconds": 10.0},
             "treatment": {"cache_mode": "enabled", "status": "complete", "returncode": 0, "duration_seconds": 10.0 + delta}}
            for index, delta in enumerate(deltas)
        ]

    def test_exact_six_pairs_require_complete_alternating_arms_and_all_ci_outcomes(self) -> None:
        for deltas, result in (
            ([-2.0] * 6, "faster"), ([2.0] * 6, "slower"),
            ([-3, -1, 1, 3, -2, 2], "indeterminate"),
            ([0, 3, -2, 1, -1, 1], "indeterminate"),
        ):
            self.assertEqual(lane.paired_summary(self.pairs(deltas))["result"], result)
        with self.assertRaisesRegex(ValueError, "exactly six"):
            lane.paired_summary(self.pairs([-1.0] * 5))
        with self.assertRaisesRegex(ValueError, "exactly six"):
            lane.paired_summary(self.pairs([-1.0] * 7))
        malformed = self.pairs([-1.0] * 6)
        malformed[1]["control_first"] = True
        with self.assertRaisesRegex(ValueError, "alternating"):
            lane.paired_summary(malformed)
        failed_arm = self.pairs([-1.0] * 6)
        failed_arm[2]["treatment"]["status"] = "failed"
        failed_arm[2]["treatment"]["returncode"] = 1
        with self.assertRaisesRegex(ValueError, "did not complete successfully"):
            lane.paired_summary(failed_arm)
        incomplete = self.pairs([-1.0] * 6)
        incomplete[0].pop("treatment")
        with self.assertRaisesRegex(ValueError, "incomplete"):
            lane.paired_summary(incomplete)
        nonfinite = self.pairs([-1.0] * 6)
        nonfinite[0]["treatment"]["duration_seconds"] = float("nan")
        with self.assertRaisesRegex(ValueError, "duration"):
            lane.paired_summary(nonfinite)

    def test_generated_namespace_binds_every_remote_key_while_tenant_root_stays_local(self) -> None:
        root = f"/cargo/{TENANT}/"
        self.assertEqual(self.request("PROPFIND", root)[0], 207)
        self.assertEqual(self.request("MKCOL", root)[0], 201)
        self.assertEqual(FakeHttpsConnection.requests, [])
        self.assertEqual(self.request("GET", f"{root}key-a")[0], 200)
        self.assertEqual(FakeHttpsConnection.requests[0][1], f"/cargo/{TENANT}/{PREFIX}-key-a")
        self.assertTrue(PREFIX.startswith("b105-42-"))

    def test_request_accounting_uses_the_phase_captured_before_remote_io(self) -> None:
        self.meter.set_phase("seed")
        _, phase = self.meter.begin_forward("phase-race")
        self.meter.set_phase("pair-0-enabled")
        self.meter.record("PUT", "phase-race", 4, 201, 0, phase=phase)
        self.meter.end_forward()
        self.assertEqual(self.meter.counters["seed"]["put_bytes"], 4)
        self.assertEqual(self.meter.counters["pair-0-enabled"]["put_bytes"], 0)

    def test_dns_setup_cannot_extend_an_origin_request_past_its_deadline(self) -> None:
        release = threading.Event()

        def blocked_resolution(*_args: object, **_kwargs: object) -> list[tuple[object, ...]]:
            release.wait(2)
            return []

        started = time.monotonic()
        try:
            with mock.patch.object(lane.socket, "getaddrinfo", side_effect=blocked_resolution), mock.patch.object(
                lane.socket, "socket", side_effect=AssertionError("TCP started before DNS completed"),
            ):
                connection = lane.DeadlineHTTPSConnection(
                    "cache.example.invalid", 443, 0.05, started + 0.05, time.monotonic,
                )
                with self.assertRaisesRegex(TimeoutError, "name resolution"):
                    connection.connect()
            self.assertLess(time.monotonic() - started, 0.5)
        finally:
            release.set()

    def test_rejects_root_prefix_listing_encoded_malformed_and_broad_paths_before_the_origin(self) -> None:
        root = f"/cargo/{TENANT}/"
        rejected = (
            f"/cargo/{TENANT}", root, f"{root}prefix/child", f"{root}key%2Da",
            f"{root}%2e%2e", f"{root}key?listing=1", f"/cargo/not-a-tenant/key", "/cargo/",
        )
        for path in rejected:
            self.assertEqual(self.request("DELETE", path)[0], 400, path)
        self.assertEqual(FakeHttpsConnection.requests, [])
        with self.assertRaisesRegex(ValueError, "unrecorded"):
            lane.recorded_key_path(TENANT, self.meter, "never-recorded")

    def test_payload_limits_and_success_only_bytes_exclude_errors_partials_redirects_duplicates_and_retries(self) -> None:
        root = f"/cargo/{TENANT}/"
        self.meter.set_phase("seed")
        self.assertEqual(self.request("PUT", f"{root}seed", b"seed")[0], 201)
        self.meter.set_phase("pair-0-enabled")
        self.assertEqual(self.request("GET", f"{root}hit")[0], 200)
        self.assertEqual(self.request("GET", f"{root}hit")[0], 200)
        self.assertEqual(self.request("PUT", f"{root}pair", b"pair")[0], 201)
        self.assertEqual(self.request("PUT", f"{root}pair", b"pair")[0], 201)
        self.assertEqual(self.request("GET", f"{root}failed")[0], 500)
        self.assertEqual(self.request("GET", f"{root}partial")[0], 206)
        self.assertEqual(self.request("GET", f"{root}redirect")[0], 302)
        self.assertEqual(self.request("GET", f"{root}retry")[0], 500)
        self.assertEqual(self.request("GET", f"{root}retry")[0], 200)
        self.assertEqual(self.request("PUT", f"{root}failed", b"no-count")[0], 500)
        self.assertEqual(self.request("GET", f"{root}truncated")[0], 502)
        self.assertEqual(self.request("PUT", f"{root}truncated", b"no-count")[0], 502)
        short_upload = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=5)
        short_upload.putrequest("PUT", f"{root}short")
        short_upload.putheader("Content-Length", "4")
        short_upload.endheaders()
        short_upload.send(b"xx")
        short_upload.sock.shutdown(socket.SHUT_WR)
        self.assertEqual(short_upload.getresponse().status, 400)
        short_upload.close()
        self.assertEqual(self.meter.counters["seed"], {"get_bytes": 0, "put_bytes": 4, "gets": 0, "puts": 1})
        self.assertEqual(self.meter.counters["pair-0-enabled"], {
            "get_bytes": len(b"read-payload") * 2, "put_bytes": 4, "gets": 2, "puts": 1,
        })
        self.assertEqual(
            sum(phase == "pair-0-enabled" and method == "GET" for phase, method, _ in self.meter.successes), 2,
        )
        self.assertEqual(self.meter.retained_payload_bytes(), 8)
        original_limit = lane.MAX_BODY_BYTES
        lane.MAX_BODY_BYTES = 5
        try:
            self.assertEqual(self.request("PUT", f"{root}limit", b"12345")[0], 201)
            self.assertEqual(self.request("PUT", f"{root}too-large", b"123456")[0], 413)
            self.assertEqual(self.request("GET", f"{root}oversize")[0], 502)
        finally:
            lane.MAX_BODY_BYTES = original_limit
        with self.assertRaisesRegex(ValueError, "request timeout"):
            lane._origin_request("https://cache.example.invalid", "redacted-test-token", "GET", f"/cargo/{TENANT}/{PREFIX}-hit", timeout_seconds=61)
        with self.assertRaisesRegex(ValueError, "cleanup deadline"):
            lane.cleanup_exact("https://cache.example.invalid", "redacted-test-token", TENANT, self.meter, cleanup_seconds=0)

    def test_each_recorded_key_is_deleted_then_verified_absent_within_the_deadline(self) -> None:
        root = f"/cargo/{TENANT}/"
        self.meter.set_phase("seed")
        self.assertEqual(self.request("PUT", f"{root}seed", b"seed")[0], 201)
        self.meter.set_phase("pair-0-enabled")
        self.assertEqual(self.request("GET", f"{root}hit")[0], 200)
        self.meter.set_phase("cleanup")
        deleted, failures = lane.cleanup_exact("https://cache.example.invalid", "redacted-test-token", TENANT, self.meter)
        self.assertEqual((deleted, failures), (2, 0))
        cleanup = [(method, path) for method, path, _, _ in FakeHttpsConnection.requests if method in {"DELETE", "PROPFIND"}]
        expected = [f"/cargo/{TENANT}/{PREFIX}-hit", f"/cargo/{TENANT}/{PREFIX}-seed"]
        self.assertEqual(cleanup, [("DELETE", expected[0]), ("PROPFIND", expected[0]), ("DELETE", expected[1]), ("PROPFIND", expected[1])])
        self.assertEqual(self.meter.retained_payload_bytes(), 0)
        with self.assertRaisesRegex(RuntimeError, "frozen"):
            self.meter.namespace_key("late-arrival")
        failed_delete = lane.Meter(PREFIX)
        failed_delete.namespace_key("delete-failed")
        before = len(FakeHttpsConnection.requests)
        self.assertEqual(lane.cleanup_exact("https://cache.example.invalid", "redacted-test-token", TENANT, failed_delete), (0, 1))
        self.assertEqual([(method, path) for method, path, _, _ in FakeHttpsConnection.requests[before:]], [
            ("DELETE", f"/cargo/{TENANT}/{PREFIX}-delete-failed"),
            ("PROPFIND", f"/cargo/{TENANT}/{PREFIX}-delete-failed"),
        ])
        deadline_meter = lane.Meter(PREFIX)
        deadline_meter.namespace_key("late")
        before = len(FakeHttpsConnection.requests)
        ticks = iter((0.0, 0.0, 2.0))
        deleted, failures = lane.cleanup_exact(
            "https://cache.example.invalid", "redacted-test-token", TENANT, deadline_meter,
            cleanup_seconds=1, monotonic=lambda: next(ticks),
        )
        self.assertEqual((deleted, failures), (1, 1))
        self.assertEqual([entry[0] for entry in FakeHttpsConnection.requests[before:]], ["DELETE"])

        transport_failure = lane.Meter(PREFIX)
        transport_failure.namespace_key("delete-transport-failed")
        before = len(FakeHttpsConnection.requests)
        self.assertEqual(lane.cleanup_exact("https://cache.example.invalid", "redacted-test-token", TENANT, transport_failure), (0, 1))
        self.assertEqual([(method, path) for method, path, _, _ in FakeHttpsConnection.requests[before:]], [
            ("DELETE", f"/cargo/{TENANT}/{PREFIX}-delete-transport-failed"),
            ("PROPFIND", f"/cargo/{TENANT}/{PREFIX}-delete-transport-failed"),
        ])

    def test_cleanup_waits_for_an_active_meter_request_then_fails_closed_at_its_deadline(self) -> None:
        self.meter.begin_forward("in-flight")
        before = len(FakeHttpsConnection.requests)
        deleted, failures = lane.cleanup_exact(
            "https://cache.example.invalid", "redacted-test-token", TENANT, self.meter, cleanup_seconds=0.05,
        )
        self.assertEqual((deleted, failures), (0, 1))
        self.assertEqual([entry[0] for entry in FakeHttpsConnection.requests[before:]], [])
        with self.assertRaisesRegex(RuntimeError, "frozen"):
            self.meter.begin_forward("after-freeze")
        self.meter.end_forward()
        self.assertEqual(FakeHttpsConnection.requests, [])

    def test_key_inventory_rejects_the_boundary_before_a_broad_cleanup_can_exist(self) -> None:
        original_limit = lane.MAX_TRACKED_KEYS
        lane.MAX_TRACKED_KEYS = 1
        try:
            self.meter.namespace_key("first")
            with self.assertRaisesRegex(RuntimeError, "inventory limit"):
                self.meter.namespace_key("second")
        finally:
            lane.MAX_TRACKED_KEYS = original_limit

    def test_collector_runs_the_isolated_boundary_and_writes_the_bounded_receipt(self) -> None:
        calls: list[tuple[str, int]] = []

        def fake_run(mode: str, pair: int, tenant: str, revision: str) -> dict[str, object]:
            calls.append((mode, pair))
            if mode == "enabled":
                endpoint = urlsplit(os.environ["CORELINK_PERF_BASE"])
                key = "seed" if pair < 0 else f"pair-{pair}"
                path = f"/cargo/{tenant}/{key}"
                def local_request(method: str, body: bytes = b"") -> int:
                    connection = http.client.HTTPConnection(endpoint.hostname, endpoint.port, timeout=5)
                    connection.request(method, path, body=body, headers={"Content-Length": str(len(body))})
                    response = connection.getresponse()
                    status = response.status
                    response.read()
                    connection.close()
                    return status
                self.assertEqual(local_request("PUT", b"body"), 201)
                self.assertEqual(local_request("GET"), 200)
            return {
                "cache_mode": mode, "status": "complete", "returncode": 0,
                "duration_seconds": 8.0 if mode == "enabled" else 10.0,
                "sccache": {"hits": 1 if mode == "enabled" else 0, "read_errors": 0, "write_errors": 0},
                "revision": revision,
            }

        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "receipt.json"
            with mock.patch.dict(collector.os.environ, {
                "CORELINK_PERF_BASE": "https://cache.example.invalid",
                "CORELINK_PERF_PAT": "redacted-test-token",
                "GITHUB_RUN_ID": "4242",
            }, clear=False), mock.patch.object(collector, "run", side_effect=fake_run):
                self.assertEqual(collector.isolated_main(output, TENANT, "a" * 40), 0)
            receipt = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(calls, [("enabled", -1), ("disabled", 0), ("enabled", 0), ("enabled", 1), ("disabled", 1), ("disabled", 2), ("enabled", 2), ("enabled", 3), ("disabled", 3), ("disabled", 4), ("enabled", 4), ("enabled", 5), ("disabled", 5)])
        self.assertEqual(receipt["result"], "faster")
        self.assertEqual(len(receipt["pairs"]), 6)
        self.assertTrue(receipt["namespace"]["prefix"].startswith("b105-4242-"))
        self.assertEqual(receipt["application_payload"]["seed"]["put_bytes"], 4)
        self.assertTrue(all(receipt["application_payload"][f"pair-{index}-enabled"]["get_bytes"] == len(b"read-payload") for index in range(6)))
        self.assertEqual(receipt["cleanup"], {"attempted": 7, "delete_successes": 7, "failures": 0, "verified_absent": True})


if __name__ == "__main__":
    unittest.main()
