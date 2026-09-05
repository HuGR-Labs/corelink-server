"""Python-level regression entrypoint for the B246 static/mutation guard."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts/verify_b246_webauthn_otp.py"


def test_b246_verifier_and_mutations_pass() -> None:
    result = subprocess.run(
        [sys.executable, str(VERIFIER)],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert "100-cycle single-use seam" in result.stdout
