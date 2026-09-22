"""Focused tests for the B-154 active-Markdown claim guard."""

from __future__ import annotations

import importlib.util
import copy
import json
import subprocess
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


def test_backlog_verify_uses_claims_and_current_evidence_not_any_501_grep() -> None:
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    section = backlog.split("### B-154 —", 1)[1].split("### B-155 —", 1)[0]
    assert "status: open" in section
    assert "python3 -S scripts/verify_owner_action_packets.py --id B-154 &&\n  python3 -S scripts/verify_b154_instrument_claims.py --self-test" in section
    assert "python3 -S scripts/verify_b154_instrument_claims.py --self-test" in section
    assert "grep -qE" not in section
    assert "B-083 sem ciclo CMK/p99 executado" in section
    assert "probe B-046 mais recente `INDETERMINATE`" in section


def test_backlog_verify_cannot_mask_packet_failure_with_later_success() -> None:
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    section = backlog.split("### B-154 —", 1)[1].split("### B-155 —", 1)[0]
    command = section.split("verify: |", 1)[1].split("verify-means: |", 1)[0].strip()
    first = "python3 -S scripts/verify_owner_action_packets.py --id B-154"
    second = "python3 -S scripts/verify_b154_instrument_claims.py --self-test"
    assert command.count(first) == command.count(second) == 1
    red = command.replace(first, "false").replace(second, "true")
    green = command.replace(first, "true").replace(second, "true")
    assert subprocess.run(["bash", "-c", red], check=False).returncode != 0
    assert subprocess.run(["bash", "-c", green], check=False).returncode == 0


def test_current_instruments_find_bold_claims() -> None:
    dpa, sla = _sources()
    found = MODULE.verify_texts(dpa, sla)
    assert {label for label, _line, _text in found} == {
        "dpa_object_lock",
        "sla_byok_kill_switch",
    }
    MODULE.verify_repository_state()


def _capability_sources() -> tuple[dict, dict, str]:
    return (
        json.loads((ROOT / MODULE.B083_RECEIPT).read_text(encoding="utf-8")),
        json.loads((ROOT / MODULE.B046_PROBE).read_text(encoding="utf-8")),
        (ROOT / MODULE.DOCKERFILE).read_text(encoding="utf-8"),
    )


def test_current_capability_boundary_is_not_an_unconditional_501_claim() -> None:
    byok, probe, dockerfile = _capability_sources()
    MODULE.verify_capability_state(byok, probe, dockerfile)
    assert "--features byok-aws-real" in dockerfile


@pytest.mark.parametrize("mutant", ["byok", "object_lock", "dockerfile"])
def test_capability_evidence_drift_fails_closed(mutant: str) -> None:
    byok, probe, dockerfile = _capability_sources()
    if mutant == "byok":
        byok = copy.deepcopy(byok)
        byok["lifecycle"]["customer_create_or_import"]["status"] = "PASS"
    elif mutant == "object_lock":
        probe = copy.deepcopy(probe)
        probe["classification"] = "SUPPORTED"
    else:
        dockerfile = dockerfile.replace("--features byok-aws-real;", "--features byok-no-real-provider;", 1)
        dockerfile += "\n# cargo build -p corelink-server --bin corelink-server --features byok-aws-real;\n"
    with pytest.raises(MODULE.VerificationError):
        MODULE.verify_capability_state(byok, probe, dockerfile)


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
