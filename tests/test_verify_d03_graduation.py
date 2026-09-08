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
        "B251_OBSERVED_SEED": "B251_OBSERVED_SEED_MISSING",
        "B251_OBSERVED_FAILURE": "B251_OBSERVED_FAILURE_MISSING",
        "B251_OBSERVED_BLOB": "B251_OBSERVED_BLOB_MISSING",
        "reports/owner-actions/b251-d02-identity.json": "reports/owner-actions/unretained.json",
        "measurement_mode:\"fixture_only\"": "measurement_mode:\"production\"",
        "production_latency_measured:false": "production_latency_measured:true",
        ".measurement_mode == \"fixture_only\" and .production_latency_measured == false": ".measurement_mode == \"fixture_only\" and .production_latency_measured == true",
    }
    for needle, replacement in mutations.items():
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-251"]["command"] = mutated["packets"]["B-251"]["command"].replace(
            needle, replacement
        )
        with pytest.raises(GraduationError, match="B-251"):
            _check_packets(mutated, ROOT)


def test_b216_b251_reject_shell_escape_and_inert_token_mutations() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    mutations = {
        "B-216": (
            ("__APPEND__", "; rm -f artifacts/d03/B216-dlq-incident.json"),
            ("__APPEND__", "; :"),
            ("; export D03_SAMPLE_COUNT=1", " && export D03_SAMPLE_COUNT=1"),
            ("; test -n", "; $(date); test -n"),
            (".event == \"dsr.erasure.dead_letter\"", ".event == \"inert-token\""),
            ("exit \"$tail_rc\"", "rm \"$tail_rc\""),
        ),
        "B-251": (
            ("__APPEND__", "; curl https://example.invalid"),
            ("__APPEND__", "; :"),
            ("; export D03_SAMPLE_COUNT=1000", " && export D03_SAMPLE_COUNT=1000"),
            ("; test -n", "; $(date); test -n"),
            ("jq -n '{measurement_mode:\"fixture_only\", production_latency_measured:false}'", "echo fixture_only"),
            ("| tee artifacts/d03/B251-latency-probe.log", "> /tmp/inert.log"),
        ),
    }
    for item, item_mutations in mutations.items():
        for needle, replacement in item_mutations:
            mutated = copy.deepcopy(packet)
            command = mutated["packets"][item]["command"]
            if needle == "__APPEND__":
                command = command + replacement
            else:
                command = command.replace(needle, replacement, 1)
            mutated["packets"][item]["command"] = command
            with pytest.raises(GraduationError, match=item):
                _check_packets(mutated, ROOT)


def test_b216_b251_reject_exact_inert_jq_comment_and_string_mutations() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    mutations = (
        (
            "B-216",
            "(.worker == $worker and .revision == $revision and .event == \"dsr.erasure.dead_letter\" and .event_id == $event_id and .exhausted == true and .action == \"requeue_once\" and .requeue_count == 1)",
            "(.worker == $worker and .revision == $revision and .event == \"inert-event\" and .event_id == $event_id and .exhausted == true and .action == \"requeue_once\" and .requeue_count == 1) # .event == \"dsr.erasure.dead_letter\"",
        ),
        (
            "B-216",
            "(.event_id == $event_id and .revision == $revision and .channel == \"paging\" and .delivery_status == \"delivered\" and (.receipt_id|type==\"string\" and length > 0))",
            "(true and .revision == $revision and .channel == \"paging\" and .delivery_status == \"delivered\" and (.receipt_id|type==\"string\" and length > 0)) | if false then \"event_id == $event_id\" else . end",
        ),
        (
            "B-251",
            "(.source == \"D02\" and (.seed|type==\"string\") and (.failure|type==\"string\") and (.blob|type==\"string\") and .seed == $seed and .failure == $failure and .blob == $blob)",
            "(true and (.seed|type==\"string\") and (.failure|type==\"string\") and (.blob|type==\"string\") and .seed == $seed and .failure == $failure and .blob == $blob) # .source == \"D02\"",
        ),
        (
            "B-251",
            ".measurement_mode == \"fixture_only\" and .production_latency_measured == false",
            ".measurement_mode == \"fixture_only\" and true | if false then \".production_latency_measured == false\" else . end",
        ),
    )
    for item, needle, replacement in mutations:
        mutated = copy.deepcopy(packet)
        command = mutated["packets"][item]["command"]
        assert needle in command
        mutated["packets"][item]["command"] = command.replace(needle, replacement, 1)
        with pytest.raises(GraduationError, match=item):
            _check_packets(mutated, ROOT)
