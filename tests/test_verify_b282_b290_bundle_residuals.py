from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b282_b290_bundle_residuals as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def test_current_contract_passes() -> None:
    verify.verify()


@pytest.mark.parametrize(("path", "marker"), [(p, m) for p, req, _ in verify.CONTRACTS for m in req])
def test_required_mutations_fail(path: str, marker: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: _text(path).replace(marker, "")})


@pytest.mark.parametrize(("path", "marker"), [(p, m) for p, _, bad in verify.CONTRACTS for m in bad])
def test_forbidden_mutations_fail(path: str, marker: str) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: f"{marker}\n{_text(path)}"})
