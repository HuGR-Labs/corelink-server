"""Regression checks for the preflight versus post-run signing gate."""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

from scripts import verify_i1664_signing_readiness as readiness  # noqa: E402


PACKET_PATH = ROOT / "docs/handoff/2026-09-22-i1664-signing-readiness.json"


def packet_with_preflight_inputs() -> dict:
    packet = json.loads(PACKET_PATH.read_text(encoding="utf-8"))
    packet["status"] = "preflight_ready"
    for name, lane in packet["lanes"].items():
        lane["status"] = "preflight_ready"
        lane["identity"].update(
            {
                "fingerprint": "a" * 64,
                "issuer": f"{name} issuer",
                "subject_or_team": f"{name} subject",
                "chain_or_profile": f"{name} signing chain",
            }
        )
        lane["access"]["access_status"] = "verified"
        lane["expiry"] = {"expires_at": "2099-01-01T00:00:00Z", "status": "valid"}
    return packet


class SigningReadinessTest(unittest.TestCase):
    def test_credentialless_packet_stays_blocked_for_preflight(self) -> None:
        packet = json.loads(PACKET_PATH.read_text(encoding="utf-8"))
        self.assertFalse(readiness.validate(packet))
        self.assertFalse(readiness.validate_preflight(packet))

    def test_preflight_accepts_identity_without_post_run_receipts(self) -> None:
        packet = packet_with_preflight_inputs()
        self.assertFalse(readiness.validate(packet))
        self.assertTrue(readiness.validate_preflight(packet))
        for lane in packet["lanes"].values():
            self.assertEqual(lane["verification"]["status"], "missing")

    def test_preflight_rejects_expired_or_unscoped_identity(self) -> None:
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
