"""Focused tests for the B-154 active-Markdown claim guard."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b154_instrument_claims.py"
SPEC = importlib.util.spec_from_file_location("verify_b154_instrument_claims", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def _sources() -> tuple[str, str]:
    return (
        MODULE.DPA.read_text(encoding="utf-8"),
        MODULE.SLA.read_text(encoding="utf-8"),
    )


def test_current_instruments_find_bold_claims() -> None:
    dpa, sla = _sources()
    found = MODULE.verify_texts(dpa, sla)
    assert {label for label, _line, _text in found} == {
        "dpa_object_lock",
        "sla_byok_kill_switch",
    }


@pytest.mark.parametrize(
    ("dpa_mutation", "sla_mutation"),
    [
        ("immutable R2 with retention metadata", "BYOK kill-switch p99 ≤ 5 min"),
        ("immutable R2 with Object Lock", "BYOK activation is unavailable"),
        ("## Object Lock\nR2 retention is not configured.", "BYOK kill-switch p99 ≤ 5 min"),
        ("R2 does not implement Object Lock.", "BYOK kill-switch is not available."),
        ("No immutable R2 with Object Lock is guaranteed.", "BYOK kill-switch p99 ≤ 5 min"),
        ("immutable R2 with Object Lock", "BYOK kill-switch p99 ≤ 5 min is not guaranteed."),
    ],
)
def test_claim_removal_or_negative_status_fails_closed(
    dpa_mutation: str, sla_mutation: str
) -> None:
    dpa, sla = _sources()
    if dpa_mutation != "immutable R2 with Object Lock":
        dpa = dpa_mutation
    if sla_mutation != "BYOK kill-switch p99 ≤ 5 min":
        sla = sla_mutation
    with pytest.raises(MODULE.VerificationError):
        MODULE.verify_texts(dpa, sla)


def test_markdown_emphasis_is_not_a_claim_evasion() -> None:
    dpa, sla = _sources()
    plain_dpa = dpa.replace("**immutable R2 with Object Lock**", "immutable R2 with Object Lock")
    plain_sla = sla.replace("**BYOK kill-switch p99 ≤ 5 min**", "BYOK kill-switch p99 ≤ 5 min")
    assert len(MODULE.verify_texts(plain_dpa, plain_sla)) == 2


def test_long_filler_cannot_hide_sentence_negation() -> None:
    _, sla = _sources()
    filler = "x" * 120
    dpa = f"No {filler} immutable R2 with Object Lock is guaranteed.\n"
    with pytest.raises(MODULE.VerificationError):
        MODULE.verify_texts(dpa, sla)


def test_wrapped_sentence_negation_cannot_hide_behind_a_line_break() -> None:
    _, sla = _sources()
    dpa = f"No {'x' * 120}\nimmutable R2 with Object Lock is guaranteed.\n"
    with pytest.raises(MODULE.VerificationError):
        MODULE.verify_texts(dpa, sla)


def test_sentence_scope_does_not_borrow_an_unrelated_prior_sentence() -> None:
    _, sla = _sources()
    dpa = "No unrelated retention feature is guaranteed. Immutable R2 with Object Lock is active.\n"
    found = MODULE.verify_texts(dpa, sla)
    assert {label for label, _line, _text in found} == {
        "dpa_object_lock",
        "sla_byok_kill_switch",
    }
