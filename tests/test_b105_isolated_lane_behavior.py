"""GitHub-hosted behavioral proof for the isolated B-105 collector boundary."""

from __future__ import annotations

import http.client
import threading
import unittest

from scripts import b105_isolated_lane as lane


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
        if method == "PUT" and path.endswith("-failed"):
            return FakeResponse(500, b"must-not-count")
        if method == "GET":
            return FakeResponse(200, b"read-payload")
        if method == "PROPFIND":
            return FakeResponse(404)
        return FakeResponse(204 if method == "DELETE" else 201)

    def close(self) -> None:
        return


class IsolatedLaneBehaviorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.original_connection = lane.http.client.HTTPSConnection
        lane.http.client.HTTPSConnection = FakeHttpsConnection
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
        lane.http.client.HTTPSConnection = self.original_connection

    def request(self, method: str, path: str, body: bytes = b"") -> tuple[int, bytes]:
        connection = http.client.HTTPConnection("127.0.0.1", self.server.server_port, timeout=5)
        connection.request(method, path, body=body, headers={"Content-Length": str(len(body))})
        response = connection.getresponse()
        result = response.status, response.read()
        connection.close()
        return result

    @staticmethod
    def pairs(deltas: list[float]) -> list[dict[str, object]]:
        return [
            {"index": index, "control_first": index % 2 == 0,
             "control": {"cache_mode": "disabled", "duration_seconds": 10.0},
             "treatment": {"cache_mode": "enabled", "duration_seconds": 10.0 + delta}}
            for index, delta in enumerate(deltas)
        ]

    def test_exact_six_pairs_require_complete_alternating_arms_and_all_ci_outcomes(self) -> None:
        for deltas, result in (([-2.0] * 6, "faster"), ([2.0] * 6, "slower"), ([-3, -1, 1, 3, -2, 2], "indeterminate")):
            self.assertEqual(lane.paired_summary(self.pairs(deltas))["result"], result)
        with self.assertRaisesRegex(ValueError, "exactly six"):
            lane.paired_summary(self.pairs([-1.0] * 5))
        with self.assertRaisesRegex(ValueError, "exactly six"):
            lane.paired_summary(self.pairs([-1.0] * 7))
        malformed = self.pairs([-1.0] * 6)
        malformed[1]["control_first"] = True
        with self.assertRaisesRegex(ValueError, "alternating"):
            lane.paired_summary(malformed)
        incomplete = self.pairs([-1.0] * 6)
        incomplete[0].pop("treatment")
        with self.assertRaisesRegex(ValueError, "incomplete"):
            lane.paired_summary(incomplete)

    def test_generated_namespace_binds_every_remote_key_while_tenant_root_stays_local(self) -> None:
        root = f"/cargo/{TENANT}/"
        self.assertEqual(self.request("PROPFIND", root)[0], 207)
        self.assertEqual(self.request("MKCOL", root)[0], 201)
        self.assertEqual(FakeHttpsConnection.requests, [])
        self.assertEqual(self.request("GET", f"{root}key-a")[0], 200)
        self.assertEqual(FakeHttpsConnection.requests[0][1], f"/cargo/{TENANT}/{PREFIX}-key-a")
        self.assertTrue(PREFIX.startswith("b105-42-"))

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
        self.assertEqual(self.meter.counters["seed"], {"get_bytes": 0, "put_bytes": 4, "gets": 0, "puts": 1})
        self.assertEqual(self.meter.counters["pair-0-enabled"], {
            "get_bytes": len(b"read-payload") * 2, "put_bytes": 4, "gets": 2, "puts": 1,
        })
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
            lane.origin_request("https://cache.example.invalid", "redacted-test-token", "GET", f"/cargo/{TENANT}/{PREFIX}-hit", timeout_seconds=61)
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

    def test_key_inventory_rejects_the_boundary_before_a_broad_cleanup_can_exist(self) -> None:
        original_limit = lane.MAX_TRACKED_KEYS
        lane.MAX_TRACKED_KEYS = 1
        try:
            self.meter.namespace_key("first")
            with self.assertRaisesRegex(RuntimeError, "inventory limit"):
                self.meter.namespace_key("second")
        finally:
            lane.MAX_TRACKED_KEYS = original_limit


if __name__ == "__main__":
    unittest.main()
