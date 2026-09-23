"""Hosted behavioral contract for the isolated B-105 forwarding boundary."""

from __future__ import annotations

import http.client
import unittest

from scripts import b105_isolated_lane as lane


TENANT = "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3"
PREFIX = "b105-42-0123456789ab"


class FakeResponse:
    def __init__(self, status: int, payload: bytes = b"") -> None:
        self.status = status
        self.payload = payload

    def read(self, _limit: int) -> bytes:
        return self.payload

    def getheaders(self) -> list[tuple[str, str]]:
        return [("Content-Length", str(len(self.payload)))]


class FakeHttpsConnection:
    requests: list[tuple[str, str, bytes]] = []

    def __init__(self, _host: str, _port: int, timeout: int) -> None:
        self.request_data: tuple[str, str, bytes] | None = None

    def request(self, method: str, path: str, body: bytes | None, headers: dict[str, str]) -> None:
        assert headers["Authorization"] == "Bearer redacted-test-token"
        self.request_data = (method, path, body or b"")
        self.requests.append(self.request_data)

    def getresponse(self) -> FakeResponse:
        assert self.request_data is not None
        method, path, _ = self.request_data
        if method == "GET" and path.endswith("-missing"):
            return FakeResponse(500, b"must-not-count")
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
        self.meter = lane.Meter(PREFIX)
        handler = type("TestHandler", (lane.Handler,), {
            "meter": self.meter, "origin": "https://cache.example.invalid", "tenant": TENANT,
            "token": "redacted-test-token",
        })
        self.server = lane.Forwarder(("127.0.0.1", 0), handler)
        self.thread = __import__("threading").Thread(target=self.server.serve_forever, daemon=True)
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

    def test_tenant_root_is_local_and_all_remote_paths_are_single_exact_namespace(self) -> None:
        root = f"/cargo/{TENANT}/"
        self.assertEqual(self.request("PROPFIND", root)[0], 207)
        self.assertEqual(self.request("MKCOL", root)[0], 201)
        self.assertEqual(FakeHttpsConnection.requests, [])
        self.assertEqual(self.request("GET", f"{root}key-a")[0], 200)
        self.assertEqual(self.request("GET", f"{root}key%2Da")[0], 400)
        self.assertEqual(self.request("GET", f"/cargo/{TENANT}")[0], 400)
        self.assertEqual(len(FakeHttpsConnection.requests), 1)
        _, path, _ = FakeHttpsConnection.requests[0]
        self.assertEqual(path, f"/cargo/{TENANT}/{PREFIX}-key-a")

    def test_success_only_byte_accounting_separates_seed_and_exact_cleanup(self) -> None:
        root = f"/cargo/{TENANT}/"
        self.meter.set_phase("seed")
        self.assertEqual(self.request("PUT", f"{root}seed", b"seed")[0], 201)
        self.meter.set_phase("pair-0-enabled")
        self.assertEqual(self.request("GET", f"{root}hit")[0], 200)
        self.assertEqual(self.request("PUT", f"{root}pair", b"pair")[0], 201)
        self.assertEqual(self.request("GET", f"{root}missing")[0], 500)
        self.assertEqual(self.request("PUT", f"{root}failed", b"no-count")[0], 500)
        self.assertEqual(self.meter.counters["seed"]["put_bytes"], 4)
        self.assertEqual(self.meter.counters["pair-0-enabled"], {
            "get_bytes": len(b"read-payload"), "put_bytes": 4, "gets": 2, "puts": 2,
        })
        self.assertEqual(self.meter.retained_payload_bytes(), 8)
        self.meter.set_phase("cleanup")
        deleted, failures = lane.cleanup_exact("https://cache.example.invalid", "redacted-test-token", TENANT, self.meter)
        self.assertEqual((deleted, failures), (4, 0))
        self.assertEqual(self.meter.retained_payload_bytes(), 0)
        cleanup_calls = [(method, path) for method, path, _ in FakeHttpsConnection.requests if method in {"DELETE", "PROPFIND"}]
        expected = {f"/cargo/{TENANT}/{PREFIX}-{key}" for key in {"seed", "hit", "pair", "missing", "failed"}}
        self.assertEqual({path for _, path in cleanup_calls}, expected)
        self.assertEqual({method for method, _ in cleanup_calls}, {"DELETE", "PROPFIND"})
        self.assertTrue(all(path != f"/cargo/{TENANT}/" and "/" not in path.removeprefix(f"/cargo/{TENANT}/") for _, path in cleanup_calls))

    def test_six_alternating_pairs_calculate_treatment_minus_control_ci(self) -> None:
        pairs = []
        for index in range(6):
            pairs.append({"index": index, "control_first": index % 2 == 0,
                          "control": {"cache_mode": "disabled", "duration_seconds": 10 + index},
                          "treatment": {"cache_mode": "enabled", "duration_seconds": 8 + index}})
        summary = lane.paired_summary(pairs)
        self.assertEqual(summary["pair_deltas_seconds"], [-2.0] * 6)
        self.assertEqual(summary["delta_95_percent_confidence_interval_seconds"], [-2.0, -2.0])
        self.assertEqual(summary["result"], "faster")
        pairs[1]["control_first"] = True
        with self.assertRaisesRegex(ValueError, "alternating"):
            lane.paired_summary(pairs)

    def test_key_inventory_is_bounded_before_any_unbounded_cleanup(self) -> None:
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
