"""Mutation coverage for the frozen D03 graduation register."""

import copy
from pathlib import Path

import pytest

from scripts.verify_d03_graduation import (
    COMMAND_CONTRACTS,
    COMMAND_OPERATIONS,
    GraduationError,
    _check_packets,
    _load_packets,
    self_test,
    verify_document,
)


ROOT = Path(__file__).resolve().parents[1]


def test_closed_population_and_inverted_guards() -> None:
    result = verify_document(run_guards=True, run_gates=False)
    assert result == {"original": 42, "graduated": 40, "done": 2, "parked": 38}


def test_register_mutations_are_red() -> None:
    self_test()


def test_flagged_commands_have_load_bearing_semantic_mutations() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    _check_packets(packet, ROOT)
    for item, contract in COMMAND_CONTRACTS.items():
        mutated = copy.deepcopy(packet)
        mutated["packets"][item]["command"] = mutated["packets"][item]["command"].replace(
            contract["safety"][0], ""
        )
        with pytest.raises(GraduationError, match=item):
            _check_packets(mutated, ROOT)

        mutated = copy.deepcopy(packet)
        mutated["packets"][item]["command_contract"]["sample_count"] += 1
        with pytest.raises(GraduationError, match=item):
            _check_packets(mutated, ROOT)

        for operation in COMMAND_OPERATIONS[item]:
            mutated = copy.deepcopy(packet)
            mutated["packets"][item]["command"] = mutated["packets"][item]["command"].replace(
                operation, "", 1
            )
            with pytest.raises(GraduationError, match=item):
                _check_packets(mutated, ROOT)
