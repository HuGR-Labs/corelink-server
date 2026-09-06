from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b314_gdpr_sigstore as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def test_parked_baseline_and_mutations_pass() -> None:
    verify.verify()
    assert verify.mutation_checks() == 16


@pytest.mark.parametrize("path", verify.LOCALES)
def test_each_locale_requires_the_exact_baseline_row(path: str) -> None:
    source = _text(path)
    row = next(line for line in source.splitlines() if verify.SIGSTORE_ROW.fullmatch(line))
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: source.replace(row, "", 1)})


def test_packet_duplicate_key_is_rejected() -> None:
    source = _text(verify.PACKET)
    mutated = source.replace('"status": "parked-owner-action-pending",', '"status": "parked-owner-action-pending",\n  "status": "parked-owner-action-pending",', 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.PACKET: mutated})


def test_missing_target_and_unknown_override_fail_closed(tmp_path: Path) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(root=tmp_path)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={"unknown": ""})


def test_runtime_wiring_mutation_is_red() -> None:
    source = _text(verify.WORKFLOW)
    mutated = source.replace('      - "' + verify.PACKET + '"', "", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.WORKFLOW: mutated})
