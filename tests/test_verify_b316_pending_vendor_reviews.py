from __future__ import annotations

import copy
import json
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import verify_b316_pending_vendor_reviews as verifier  # noqa: E402


@pytest.fixture()
def texts() -> dict[str, str]:
    return verifier.load()


def test_canonical_population_is_truthfully_open(texts: dict[str, str]):
    assert verifier.assess(texts) == "open"


def test_mutation_suite_has_teeth(texts: dict[str, str]):
    verifier.mutation_self_test(texts)


def test_template_cannot_be_present_with_a_claimed_signature(texts: dict[str, str]):
    changed = copy.deepcopy(texts)
    changed[verifier.LEGAL] = changed[verifier.LEGAL].replace(
        "contract_signed_at: null", 'contract_signed_at: "2026-09-06"', 1
    )
    with pytest.raises(verifier.ReviewError, match="mixed pending/completed"):
        verifier.assess(changed)


def test_closed_risk_action_cannot_coexist_with_pending_packet(texts: dict[str, str]):
    changed = copy.deepcopy(texts)
    changed[verifier.REGISTER] = changed[verifier.REGISTER].replace(
        "| Legal | 2026-09-23 | Open |", "| Legal | 2026-09-23 | Closed |", 1
    )
    with pytest.raises(verifier.ReviewError, match="mixed pending/completed"):
        verifier.assess(changed)


def test_action_packet_population_is_closed(texts: dict[str, str]):
    changed = copy.deepcopy(texts)
    packet = json.loads(changed[verifier.ACTION_PACKET])
    packet["vendors"]["extra"] = packet["vendors"]["resend"]
    changed[verifier.ACTION_PACKET] = json.dumps(packet)
    with pytest.raises(verifier.ReviewError, match="population is not exact"):
        verifier.assess(changed)


def test_evidence_path_must_be_exact(texts: dict[str, str]):
    changed = copy.deepcopy(texts)
    changed[verifier.LEGAL] = changed[verifier.LEGAL].replace(
        verifier.VENDORS[0].artifact,
        "docs/compliance/vendor-reviews/resend-dpa-review-latest.md",
        1,
    )
    with pytest.raises(verifier.ReviewError, match="evidence path drifted"):
        verifier.assess(changed)
