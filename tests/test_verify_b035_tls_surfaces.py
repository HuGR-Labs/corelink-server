"""Focused B-035 contract and mutation tests.

These tests are intentionally repository-only.  Cloudflare proof is a separate
read-only ``--live`` invocation and must not receive credentials in this suite.
"""

from __future__ import annotations

import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b035_tls_surfaces as verify


class B035TlsSurfaceTests(unittest.TestCase):
    def test_inventory_enumerates_exactly_eight_promises(self) -> None:
        report = verify.inventory()
        self.assertEqual(report["instrument_count"], 8)
        self.assertEqual(report["status"], "open_exact_inventory")
        self.assertTrue(
            all(row["claim_count"] == 1 for row in report["instruments"])
        )
        self.assertTrue(
            all(row["claim_matches_expected"] for row in report["instruments"])
        )
        self.assertEqual(report["external_surface"]["protocol_floor"], "TLS 1.2")
        self.assertIn("no cipher-suite floor", report["external_surface"]["cipher_policy"])

    def test_each_instrument_is_required_and_missing_file_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for instrument in verify.INSTRUMENTS:
                source = verify.ROOT / instrument.path
                target = root / instrument.path
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(source, target)

            removed = root / verify.INSTRUMENTS[0].path
            removed.unlink()
            code, report = verify.verify(root, live=False, expect_open=True)

        self.assertEqual(code, 1)
        self.assertEqual(report["status"], "verification_error")

    def test_correcting_any_claim_fails_closed(self) -> None:
        instrument = verify.INSTRUMENTS[0]
        original = (verify.ROOT / instrument.path).read_text(encoding="utf-8")
        corrected = original.replace(
            "TLS 1.3+", "TLS 1.2 minimum (TLS 1.3 preferred)", 1
        )
        report = verify.inventory_from_overrides(
            verify.ROOT, {instrument.path: corrected}
        )
        self.assertEqual(report["status"], "drift_or_incomplete")

    def test_live_mode_without_credentials_is_fail_closed(self) -> None:
        with patch.dict(
            verify.os.environ,
            {"CLOUDFLARE_API_TOKEN": "", "CF_API_TOKEN": ""},
            clear=False,
        ):
            code, report = verify.verify(verify.ROOT, live=True, expect_open=True)
        self.assertEqual(code, 2)
        self.assertEqual(report["live_floor"]["status"], "credential_unavailable")


if __name__ == "__main__":
    unittest.main()
