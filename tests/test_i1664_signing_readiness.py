"""Adversarial regressions for the signing preflight and post-run gate."""

from __future__ import annotations

import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from io import StringIO
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from scripts import verify_i1664_signing_readiness as readiness  # noqa: E402


PACKET_PATH = ROOT / "docs/handoff/2026-09-22-i1664-signing-readiness.json"
VALID_FINGERPRINT = "9a3b" * 16
BOUNDED_SCOPES = {
    "windows": "release-cli Windows signer workflow only",
    "apple": "release-cli macOS notarization workflow only",
}


def packet_with_preflight_inputs() -> dict:
    packet = json.loads(PACKET_PATH.read_text(encoding="utf-8"))
    packet["status"] = "preflight_ready"
    for name, lane in packet["lanes"].items():
        lane["status"] = "preflight_ready"
        lane["identity"].update(
            {
                "fingerprint": VALID_FINGERPRINT,
                "issuer": f"{name} issuer",
                "subject_or_team": f"{name} subject",
                "chain_or_profile": f"{name} signing chain",
            }
        )
        if name in BOUNDED_SCOPES:
            lane["access"]["scope"] = BOUNDED_SCOPES[name]
        lane["access"]["access_status"] = "verified"
        lane["expiry"] = {"expires_at": "2099-01-01T00:00:00Z", "status": "valid"}
    return packet


class SigningReadinessTest(unittest.TestCase):
    def test_credentialless_packet_stays_blocked_for_preflight(self) -> None:
        packet = json.loads(PACKET_PATH.read_text(encoding="utf-8"))
        self.assertFalse(readiness.validate(packet))
        self.assertFalse(readiness.validate_preflight(packet))

    def test_preflight_accepts_bounded_identity_without_post_run_receipts(self) -> None:
        packet = packet_with_preflight_inputs()
        self.assertFalse(readiness.validate(packet))
        self.assertTrue(readiness.validate_preflight(packet))
        for lane in packet["lanes"].values():
            self.assertEqual(lane["verification"]["status"], "missing")

    def test_preflight_cli_emits_machine_readable_state(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            packet_path = Path(directory) / "packet.json"
            packet_path.write_text(json.dumps(packet_with_preflight_inputs()), encoding="utf-8")
            stdout = StringIO()
            argv = ["verify_i1664_signing_readiness.py", "--packet", str(packet_path), "--emit-preflight-ready"]
            with patch.object(sys, "argv", argv), redirect_stdout(stdout):
                self.assertEqual(readiness.main(), 0)
        self.assertEqual(stdout.getvalue(), "true\n")

    def test_preflight_rejects_unscoped_or_overbroad_apple_and_windows_access(self) -> None:
        rejected_scopes = ("", "unscoped", "*", "all workflows", "release-cli signing workflow")
        for lane_name in BOUNDED_SCOPES:
            for scope in rejected_scopes:
                with self.subTest(lane=lane_name, scope=scope):
                    packet = packet_with_preflight_inputs()
                    packet["lanes"][lane_name]["access"]["scope"] = scope
                    with self.assertRaises(readiness.ContractError):
                        readiness.validate_preflight(packet)

    def test_preflight_rejects_malformed_placeholder_and_wildcard_fingerprints(self) -> None:
        rejected_fingerprints = (
            "",
            "not-a-fingerprint",
            "a" * 40,
            "TBD",
            "*" * 64,
            "0" * 64,
            "F" * 64,
            "0123456789abcdef" * 4,
        )
        for lane_name in BOUNDED_SCOPES:
            for fingerprint in rejected_fingerprints:
                with self.subTest(lane=lane_name, fingerprint=fingerprint[:16]):
                    packet = packet_with_preflight_inputs()
                    packet["lanes"][lane_name]["identity"]["fingerprint"] = fingerprint
                    with self.assertRaises(readiness.ContractError):
                        readiness.validate_preflight(packet)

    def test_preflight_rejects_expiry_or_missing_access_review(self) -> None:
        packet = packet_with_preflight_inputs()
        packet["lanes"]["apple"]["access"]["access_status"] = "missing"
        with self.assertRaises(readiness.ContractError):
            readiness.validate_preflight(packet)

        packet = packet_with_preflight_inputs()
        packet["lanes"]["windows"]["expiry"] = {
            "expires_at": "2000-01-01T00:00:00Z",
            "status": "valid",
        }
        with self.assertRaises(readiness.ContractError):
            readiness.validate_preflight(packet)

    def test_full_ready_still_requires_every_post_run_receipt(self) -> None:
        packet = packet_with_preflight_inputs()
        packet["status"] = "ready"
        for lane in packet["lanes"].values():
            lane["status"] = "ready"
            lane["blockers"] = []
            lane["verification"] = {
                "status": "verified",
                "receipt_reference": "https://github.com/HuGR-dev/corelink-server/actions/runs/123456",
                "verified_at": "2026-09-25T00:00:00Z",
                "method": "hosted signer verification",
            }
        self.assertTrue(readiness.validate(packet))
        self.assertTrue(readiness.validate_preflight(packet))


if __name__ == "__main__":
    unittest.main()
