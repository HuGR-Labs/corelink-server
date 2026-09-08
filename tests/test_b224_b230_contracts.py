"""Focused closed-world and mutation checks for D03 B-224..B-230."""

from pathlib import Path
import sys

import pytest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import verify_b224_b230_contracts as contracts  # noqa: E402


def test_all_d03_contracts_pass() -> None:
    assert contracts.verify(ROOT) == {lane: "pass" for lane in contracts.CHECKS}


def test_each_lane_has_an_adversarial_inverted_guard() -> None:
    contracts.self_test(ROOT)


@pytest.mark.parametrize("lane", tuple(contracts.CHECKS))
def test_each_contract_is_independently_callable(lane: str) -> None:
    assert contracts.verify(ROOT, lane) == {lane: "pass"}
