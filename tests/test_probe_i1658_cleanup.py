"""Fast failure-path tests for the bounded B-102 staging probe."""

from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import probe_i1658_cargo_put_latency as probe


BASE = "https://staging.corelink.humangr.com"
TENANT = "223e4567-e89b-42d3-a456-426614174000"
TOKEN = "synthetic-test-token"


def response(method: str, status: int, **extra: object) -> dict[str, object]:
    return {
        "method": method,
        "status": status,
        "attempts": 1,
        "elapsed_ms": 1.0,
        "request_body_bytes": 1024 if method == "PUT" else 0,
        "request_body_sha256": "sha256:test",
        "response_body_bytes": 0,
        "response_body_sha256": "sha256:test",
        "retry_after": None,
        "server_timing": 'auth;desc="l1";dur=1' if method == "PUT" else None,
        "response_request_id": "request-test",
        "cf_ray": "ray-test",
        "transport_error": None,
        **extra,
    }


class ProbeCleanupTests(unittest.TestCase):
    def test_successful_put_is_cleaned_after_timing_parse(self) -> None:
        put = response("PUT", 200)
        delete = response("DELETE", 204)
        with patch.object(probe, "request", side_effect=[put, delete]) as mocked, patch.object(
            probe, "parse_timing", return_value=({"total": 1.0}, "l1")
        ):
            result = probe.observed_put(BASE, TENANT, TOKEN, "serial-1")

        self.assertEqual([entry.args[3] for entry in mocked.call_args_list], ["PUT", "DELETE"])
        self.assertEqual(result["cleanup_status"], 204)
        self.assertEqual(result["auth_source"], "l1")

    def test_malformed_timing_still_attempts_cleanup_and_fails(self) -> None:
        put = response("PUT", 200)
        delete = response("DELETE", 204)
        with patch.object(probe, "request", side_effect=[put, delete]) as mocked, patch.object(
            probe, "parse_timing", side_effect=probe.ProbeError("malformed Server-Timing")
        ):
            with self.assertRaisesRegex(probe.ProbeError, "malformed Server-Timing; cleanup status 204") as caught:
                probe.observed_put(BASE, TENANT, TOKEN, "serial-2")

        self.assertEqual([entry.args[3] for entry in mocked.call_args_list], ["PUT", "DELETE"])
        self.assertEqual(caught.exception.observation["cleanup_status"], 204)

    def test_missing_timing_still_attempts_cleanup_and_fails(self) -> None:
        put = response("PUT", 200, server_timing=None)
        delete = response("DELETE", 204)
        with patch.object(probe, "request", side_effect=[put, delete]) as mocked:
            with self.assertRaisesRegex(probe.ProbeError, "did not return Server-Timing; cleanup status 204"):
                probe.observed_put(BASE, TENANT, TOKEN, "serial-missing-timing")

        self.assertEqual([entry.args[3] for entry in mocked.call_args_list], ["PUT", "DELETE"])

    def test_non_success_or_ambiguous_put_still_attempts_cleanup(self) -> None:
        put = response("PUT", 503, retry_after="2")
        delete = response("DELETE", 204)
        with patch.object(probe, "request", side_effect=[put, delete]) as mocked:
            with self.assertRaisesRegex(probe.ProbeError, "HTTP 503; cleanup status 204"):
                probe.observed_put(BASE, TENANT, TOKEN, "serial-3")

        self.assertEqual([entry.args[3] for entry in mocked.call_args_list], ["PUT", "DELETE"])

    def test_cleanup_failure_rejects_otherwise_valid_sample(self) -> None:
        put = response("PUT", 200)
        delete = response("DELETE", 500, transport_error="http_error")
        with patch.object(probe, "request", side_effect=[put, delete]), patch.object(
            probe, "parse_timing", return_value=({"total": 1.0}, "l1")
        ):
            with self.assertRaisesRegex(probe.ProbeError, "cleanup returned HTTP 500") as caught:
                probe.observed_put(BASE, TENANT, TOKEN, "serial-4")

        self.assertEqual(caught.exception.observation["cleanup"]["transport_error"], "http_error")

    def test_accepted_unauthenticated_control_is_cleaned_then_fails(self) -> None:
        accepted = response("PUT", 200)
        delete = response("DELETE", 204)
        with patch.object(probe, "request", side_effect=[accepted, delete]) as mocked:
            with self.assertRaisesRegex(probe.ProbeError, "unauthenticated PUT returned HTTP 200") as caught:
                probe.unauthenticated_control(BASE, TENANT, TOKEN)

        self.assertEqual([entry.args[3] for entry in mocked.call_args_list], ["PUT", "DELETE"])
        self.assertIsNone(mocked.call_args_list[0].args[2])
        self.assertEqual(mocked.call_args_list[1].args[2], TOKEN)
        self.assertEqual(caught.exception.observation["cleanup"]["status"], 204)

    def test_expected_unauthenticated_rejection_does_not_delete(self) -> None:
        rejected = response("PUT", 403)
        with patch.object(probe, "request", return_value=rejected) as mocked:
            result = probe.unauthenticated_control(BASE, TENANT, TOKEN)

        self.assertEqual(result["status"], 403)
        mocked.assert_called_once()


if __name__ == "__main__":
    unittest.main()
