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
    assert result == {"original": 42, "graduated": 40, "done": 3, "parked": 37}


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


def test_b216_worker_event_requeue_and_paging_mutations_are_red() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    mutations = {
        "corelink-signup-worker": "corelink",
        "dsr.erasure.dead_letter": "DSR_ERASURE_DLQ",
        "priorRequeues < 1": "priorRequeues <= 1",
        "paging": "page_only",
        "b216-alert-delivery.json": "missing-alert-delivery.json",
    }
    for needle, replacement in mutations.items():
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-216"]["command"] = mutated["packets"]["B-216"]["command"].replace(
            needle, replacement
        )
        with pytest.raises(GraduationError, match="B-216"):
            _check_packets(mutated, ROOT)


def test_b251_identity_and_fixture_truth_mutations_are_red() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    mutations = {
        "B251_D02_SEED": "B251_D02_SEED_MISSING",
        "B251_D02_FAILURE": "B251_D02_FAILURE_MISSING",
        "B251_D02_BLOB": "B251_D02_BLOB_MISSING",
        "B251_OBSERVED_SEED": "B251_OBSERVED_SEED_MISSING",
        "B251_OBSERVED_FAILURE": "B251_OBSERVED_FAILURE_MISSING",
        "B251_OBSERVED_BLOB": "B251_OBSERVED_BLOB_MISSING",
        "fixture_only": "production_only",
        "production_latency_measured=false": "production_latency_measured=true",
    }
    for needle, replacement in mutations.items():
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-251"]["command"] = mutated["packets"]["B-251"]["command"].replace(
            needle, replacement
        )
        with pytest.raises(GraduationError, match="B-251"):
            _check_packets(mutated, ROOT)
