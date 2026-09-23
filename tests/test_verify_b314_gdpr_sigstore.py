from __future__ import annotations

import re
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b314_gdpr_sigstore as verify


def _text(path: str) -> str:
    return (verify.ROOT / path).read_text(encoding="utf-8")


def test_open_baseline_and_mutations_pass() -> None:
    verify.verify()
    assert verify.mutation_checks() == 24


def test_wrapped_scoped_posture_is_not_a_false_negative() -> None:
    trust = _text(verify.TRUST)
    wrapped = trust.replace(
        "no customer-data path is wired",
        "no customer-data path is\n  wired",
        1,
    )
    assert "no customer-data path is wired" not in wrapped
    verify.verify(overrides={verify.TRUST: wrapped})


def test_contractual_and_vendor_posture_are_load_bearing() -> None:
    for path in (verify.LEGAL_REGISTER, verify.VENDOR_REGISTER):
        source = _text(path)
        if path == verify.LEGAL_REGISTER:
            mutated = source.replace(
                "Sigstore is not a customer-data sub-processor",
                "Sigstore is a customer-data sub-processor",
                1,
            )
        else:
            mutated = source.replace(
                "no customer-data path is wired",
                "customer-data path is wired",
                1,
            )
        with pytest.raises(verify.VerificationError):
            verify.verify(overrides={path: mutated})


@pytest.mark.parametrize("path", verify.LOCALES)
def test_each_locale_requires_the_exact_baseline_row(path: str) -> None:
    source = _text(path)
    row = next(line for line in source.splitlines() if verify.SIGSTORE_ROW.fullmatch(line))
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: source.replace(row, "", 1)})


@pytest.mark.parametrize("path", verify.LOCALES)
def test_mixed_case_sigstore_duplicate_is_rejected(path: str) -> None:
    source = _text(path)
    row = next(line for line in source.splitlines() if verify.SIGSTORE_ROW.fullmatch(line))
    duplicate = row.replace("Sigstore", "SIGSTORE", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={path: source.replace(row, row + "\n" + duplicate, 1)})


def test_packet_duplicate_key_is_rejected() -> None:
    source = _text(verify.PACKET)
    mutated = source.replace('"status": "owner-action-pending",', '"status": "owner-action-pending",\n  "status": "owner-action-pending",', 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.PACKET: mutated})


def test_packet_completion_missing_field_is_rejected() -> None:
    source = _text(verify.PACKET)
    mutated = source.replace('"notice_version", ', "", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.PACKET: mutated})


def test_packet_completion_invalid_field_is_rejected() -> None:
    source = _text(verify.PACKET)
    mutated = source.replace('"effective_timestamp"', '"effective_at"', 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.PACKET: mutated})


def test_packet_completion_ambiguous_decision_is_rejected() -> None:
    source = _text(verify.PACKET)
    mutated = source.replace('"state": "pending"', '"state": "ambiguous"', 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.PACKET: mutated})


@pytest.mark.parametrize("event", ("pull_request_target", "push"))
def test_workflow_event_population_covers_the_complete_tree(event: str) -> None:
    source = _text(verify.WORKFLOW)
    assert source.count(f"  {event}:") == 1
    boundary = {"pull_request_target": "push", "push": "schedule"}[event]
    match = re.search(
        rf"^  {event}:\n(?P<body>.*?)(?=^  {boundary}:)",
        source,
        re.MULTILINE | re.DOTALL,
    )
    assert match is not None
    body = match.group("body")
    assert body.count('paths: ["**"]') == 1

    mutated = source.replace('    paths: ["**"]', "", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.WORKFLOW: mutated})


def test_missing_target_and_unknown_override_fail_closed(tmp_path: Path) -> None:
    with pytest.raises(verify.VerificationError):
        verify.verify(root=tmp_path)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={"unknown": ""})


def test_runtime_wiring_mutation_is_red() -> None:
    source = _text(verify.WORKFLOW)
    mutated = source.replace("python3 -m pytest -q " + verify.TEST, "", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.WORKFLOW: mutated})


def test_pull_request_trigger_is_rejected() -> None:
    source = _text(verify.WORKFLOW)
    mutated = source.replace("pull_request_target:", "pull_request:", 1)
    with pytest.raises(verify.VerificationError):
        verify.verify(overrides={verify.WORKFLOW: mutated})
