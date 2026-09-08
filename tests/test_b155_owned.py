"""Lightweight regression tests for the ten B-155 semantic proof adapters."""
from __future__ import annotations

import hashlib
import subprocess
import sys
import urllib.error
import unittest
from unittest.mock import patch
from pathlib import Path

from scripts import verify_b155_owned as verifier


ROOT = Path(__file__).resolve().parents[1]
EXPECTED = {
    "B-014": "done", "B-015": "done", "B-035": "open", "B-039": "open",
    "B-045": "done", "B-047": "done", "B-050": "done", "B-051": "done",
    "B-052": "done", "B-060": "done",
}


class _MirrorResponse:
    status = 200

    def __init__(self, payload: bytes) -> None:
        self._payload = payload

    def __enter__(self) -> "_MirrorResponse":
        return self

    def __exit__(self, exc_type, exc_value, traceback) -> None:
        return None

    def read(self, size: int = -1) -> bytes:
        if not self._payload:
            return b""
        if size < 0:
            chunk, self._payload = self._payload, b""
        else:
            chunk, self._payload = self._payload[:size], self._payload[size:]
        return chunk


class B155OwnedSemanticTests(unittest.TestCase):
    def test_cas_include_path_drift_fails_closed(self) -> None:
        entry = verifier._read(ROOT, verifier.CAS_ROUTE_ENTRY)
        foundation_drift = entry.replace(
            "cas/foundation_core.rs", "cas/foundation_state.rs", 1
        )
        single_drift = entry.replace(
            "cas/single_handlers.rs", "cas/single_setup.rs", 1
        )
        with self.assertRaises(verifier.VerificationError):
            verifier._b051(ROOT, {verifier.CAS_ROUTE_ENTRY: foundation_drift})
        with self.assertRaises(verifier.VerificationError):
            verifier._b052(ROOT, {verifier.CAS_ROUTE_ENTRY: single_drift})

    def test_each_focal_mutation_adapter_is_live(self) -> None:
        helper = ROOT / "scripts/verify_b155_owned.py"
        for ident, polarity in EXPECTED.items():
            with self.subTest(ident=ident):
                result = subprocess.run(
                    [sys.executable, str(helper), "--id", ident, "--expect", polarity,
                     "--offline", "--self-test"],
                    cwd=ROOT, text=True, capture_output=True, timeout=15,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("mutations=24 rejected", result.stdout)

    def test_backlog_wires_only_the_owned_ids_to_helper(self) -> None:
        backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
        for ident, polarity in EXPECTED.items():
            needle = f"python3 scripts/verify_b155_owned.py --id {ident} --expect {polarity}"
            self.assertEqual(backlog.count(needle), 1, ident)
            if ident == "B-039":
                self.assertEqual(backlog.count(needle + " --offline"), 1, ident)

    def test_b039_mirror_forbidden_fails_closed(self) -> None:
        forbidden = urllib.error.HTTPError(
            verifier.MIRROR_URL, 403, "forbidden", {}, None
        )
        with patch.object(verifier.urllib.request, "urlopen", side_effect=forbidden):
            with self.assertRaisesRegex(verifier.VerificationError, "mirror query failed"):
                verifier._b039(ROOT, live=True)

    def test_b039_mirror_uses_explicit_user_agent_and_verifies_sha256(self) -> None:
        payload = b"hermetic tla2tools mirror fixture"
        expected = hashlib.sha256(payload).hexdigest()
        seen = {}

        def open_fixture(request, timeout):
            seen["request"] = request
            seen["timeout"] = timeout
            return _MirrorResponse(payload)

        with (
            patch.object(verifier, "MIRROR_URL", f"https://mirror.invalid/{expected}/tla2tools.jar"),
            patch.object(verifier, "MIRROR_SHA256_PINNED", expected),
            patch.object(verifier.urllib.request, "urlopen", side_effect=open_fixture),
        ):
            result = verifier._b039(ROOT, live=True)

        self.assertIn("SHA-256 verified", result)
        self.assertEqual(seen["timeout"], 30)
        self.assertEqual(
            seen["request"].get_header("User-agent"), verifier.MIRROR_USER_AGENT
        )

    def test_b039_mirror_hash_mutation_fails_closed(self) -> None:
        response = _MirrorResponse(b"mutated mirror object")
        with patch.object(verifier.urllib.request, "urlopen", return_value=response):
            with self.assertRaisesRegex(verifier.VerificationError, "SHA-256 mismatch"):
                verifier._b039(ROOT, live=True)


if __name__ == "__main__":
    unittest.main()
