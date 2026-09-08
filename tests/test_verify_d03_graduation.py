"""Mutation coverage for the frozen D03 graduation register."""

import copy
from pathlib import Path

import pytest

from scripts.backlog_verify import parse
from scripts.verify_d03_graduation import (
    COMMAND_CONTRACTS,
    COMMAND_OPERATIONS,
    GraduationError,
    POST_GRADUATION_RETIRED,
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


def test_b210_is_retired_and_not_parked_debt() -> None:
    packet_text = (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    packet = _load_packets(packet_text)
    assert tuple(packet["post_graduation_retired"]) == POST_GRADUATION_RETIRED
    assert "post_graduation_parked" not in packet
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    result = verify_document(backlog_text=backlog, packet_text=packet_text, run_guards=False, run_gates=False)
    assert result["done"] == 3
    b210 = next(record.raw for record in parse(backlog) if record.id == "B-210")
    assert b210["status"] == "done"
    assert b210["verify-means"].lstrip().startswith("done —")


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


def test_b112_uses_push_tag_identity_and_terminal_revalidation() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    command = packet["packets"]["B-112"]["command"]
    assert '.event == "push"' in command
    assert '.headBranch | test("^cli-v[0-9]+\\.[0-9]+\\.[0-9]+$")' in command
    assert '.status == "completed"' in command
    assert '.conclusion == "failure"' in command
    assert '.conclusion != null' in command
    assert '.databaseId == ($run_id | tonumber)' in command
    assert 'all(.jobs[]; ((.name | ascii_downcase | startswith("create release")) | not) or .conclusion == "skipped")' in command
    assert 'all(.jobs[]; ((.name | ascii_downcase | startswith("publish verified signed release")) | not) or .conclusion == "skipped")' in command
    assert '.status == "completed" and .conclusion == "skipped"' in command
    assert 'B112_EXPECTED_ATTEMPT' in command
    assert 'gh run rerun "$run_id" --failed' in command
    assert '--attempt "$attempt_after" --log' in command
    assert 'caller_attempt="$(gh api' in command
    assert 'referenced_workflows' in command
    assert '.head_sha == $sha' in command
    assert '(.run_attempt | type) == "number"' in command
    assert '.slsa_workflow.sha == $sha' in command
    assert '.slsa_ref == $ref' in command
    assert 'slsa_event="workflow_call"' in command
    assert 'slsa_run_id="$(jq -r' in command
    assert 'slsa_meta=' in command
    assert 'lock_dir="${artifact}.lock"' in command
    assert 'workflow_dispatch' not in command
    head_sha = '.headSha == $sha'
    initial_context = command.split('; lock_dir="${artifact}.lock";', 1)[0]
    terminal_context = command.split('; gh run rerun "$run_id" --failed;', 1)[1].split(
        '; caller_attempt="$(gh api', 1
    )[0]
    assert initial_context.count(head_sha) == 1
    assert terminal_context.count(head_sha) == 2

    mutated = copy.deepcopy(packet)
    mutated["packets"]["B-112"]["command"] = command.replace(
        '.event == "push"', '.event == "workflow_dispatch"', 1
    )
    with pytest.raises(GraduationError, match="B-112"):
        _check_packets(mutated, ROOT)

    mutated = copy.deepcopy(packet)
    mutated["packets"]["B-112"]["command"] = command.replace(
        '.event == "push"', '# .event == "push"', 1
    )
    with pytest.raises(GraduationError, match="comment predicate bait"):
        _check_packets(mutated, ROOT)

    mutations = (
        (
            'all(.jobs[]; ((.name | ascii_downcase | startswith("create release")) | not) or .conclusion == "skipped")',
            'all(.jobs[]; ((.name | ascii_downcase | startswith("create release")) | not) or .conclusion != "skipped")',
        ),
        (
            'all(.jobs[]; ((.name | ascii_downcase | startswith("publish verified signed release")) | not) or .conclusion == "skipped")',
            'all(.jobs[]; ((.name | ascii_downcase | startswith("publish verified signed release")) | not) or .conclusion != "skipped")',
        ),
        ('.head_sha == $sha', '.head_sha != $sha'),
        ('(.run_attempt | type) == "number"', '(.run_attempt | type) != "number"'),
        ('.run_attempt == $expected_attempt', '.run_attempt != $expected_attempt'),
        ('.slsa_workflow.ref == $ref', '.slsa_workflow.ref != $ref'),
        ('lock_dir="${artifact}.lock"', 'lock_dir="${run_id}.lock"'),
    )
    for original, inverted in mutations:
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-112"]["command"] = command.replace(original, inverted)
        with pytest.raises(GraduationError, match="B-112"):
            _check_packets(mutated, ROOT)

    mutated = copy.deepcopy(packet)
    initial_index = command.index(head_sha)
    mutated["packets"]["B-112"]["command"] = (
        command[:initial_index] + '.headSha != $sha' + command[initial_index + len(head_sha) :]
    )
    with pytest.raises(GraduationError, match="initial release headSha"):
        _check_packets(mutated, ROOT)

    mutated = copy.deepcopy(packet)
    terminal_start = command.index('; gh run rerun "$run_id" --failed;')
    terminal_index = command.index(head_sha, terminal_start)
    mutated["packets"]["B-112"]["command"] = (
        command[:terminal_index] + '.headSha != $sha' + command[terminal_index + len(head_sha) :]
    )
    with pytest.raises(GraduationError, match="terminal release headSha"):
        _check_packets(mutated, ROOT)


def test_b105_packet_binds_to_production_evidence_workflow() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    _check_packets(packet, ROOT)
    mutated = copy.deepcopy(packet)
    mutated["packets"]["B-105"]["command"] = mutated["packets"]["B-105"]["command"].replace(
        "perf-production-evidence.yml", "release-slsa3.yml", 1
    )
    with pytest.raises(GraduationError, match="B-105"):
        _check_packets(mutated, ROOT)

    missing_download = copy.deepcopy(packet)
    missing_download["packets"]["B-105"]["command"] = missing_download["packets"]["B-105"]["command"].replace(
        'gh run download "$run_id"', "gh run list", 1
    )
    with pytest.raises(GraduationError, match="B-105"):
        _check_packets(missing_download, ROOT)

    bad_even_median = copy.deepcopy(packet)
    bad_even_median["packets"]["B-104"]["command"] = bad_even_median["packets"]["B-104"]["command"].replace(
        "((v[int((n+1)/2)] + v[int((n+2)/2)]) / 2)", "v[int((n+1)/2)]", 1
    )
    with pytest.raises(GraduationError, match="B-104"):
        _check_packets(bad_even_median, ROOT)
