"""Mutation coverage for the frozen D03 graduation register."""

import copy
import json
import subprocess
import tempfile
from pathlib import Path

import pytest

from scripts.backlog_verify import parse
from scripts.verify_d03_graduation import (
    B165_ARTIFACT,
    B165_COMPLETE_ARTIFACT,
    B165_COMMAND,
    B165_SERVER_TIMING_ARTIFACT,
    COMMAND_CONTRACTS,
    COMMAND_OPERATIONS,
    GraduationError,
    POST_GRADUATION_RETIRED,
    _check_b165_done_evidence,
    _check_packets,
    _load_packets,
    self_test,
    verify_document,
)


ROOT = Path(__file__).resolve().parents[1]


def test_closed_population_and_inverted_guards() -> None:
    result = verify_document(run_guards=True, run_gates=False)
    assert result == {"original": 42, "graduated": 40, "done": 8, "parked": 32, "reopened": 0}


def test_register_mutations_are_red() -> None:
    self_test()


def test_b210_is_retired_and_not_parked_debt() -> None:
    packet_text = (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    packet = _load_packets(packet_text)
    assert tuple(packet["post_graduation_retired"]) == POST_GRADUATION_RETIRED
    assert "post_graduation_parked" not in packet
    backlog = (ROOT / "BACKLOG.md").read_text(encoding="utf-8")
    result = verify_document(backlog_text=backlog, packet_text=packet_text, run_guards=False, run_gates=False)
    assert result["done"] == 8
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


def test_b063_b127_reject_worker_name_instead_of_declared_d1_target() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    for item in ("B-063", "B-127"):
        mutated = copy.deepcopy(packet)
        command = mutated["packets"][item]["command"]
        assert command.count("corelink-config-prod") == 1
        mutated["packets"][item]["command"] = command.replace("corelink-config-prod", "corelink-prod", 1)
        with pytest.raises(GraduationError, match=item):
            _check_packets(mutated, ROOT)


def test_b063_b127_reject_d1_shell_injection_mutations() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    mutations = {
        "semicolon in query": lambda command: command.replace('--command "SELECT', '--command "SELECT; ', 1),
        "and operator in invocation": lambda command: command.replace(
            "corelink-config-prod --remote", "corelink-config-prod && echo injected --remote", 1
        ),
        "or operator in query": lambda command: command.replace('--command "SELECT', '--command "SELECT || ', 1),
        "pipe in query": lambda command: command.replace('--command "SELECT', '--command "SELECT | cat ', 1),
        "substitution in query": lambda command: command.replace(
            '--command "SELECT', '--command "$(touch /tmp/injected) SELECT', 1
        ),
        "redirect after query": lambda command: command.replace(
            '" | tee', '" > /tmp/injected | tee', 1
        ),
        "duplicate invocation": lambda command: command + (
            '; wrangler d1 execute corelink-config-prod --remote --command "SELECT 1" '
            "| tee artifacts/d03/injected.json"
        ),
        "appended command": lambda command: command + "; rm -f /tmp/injected",
        "appended newline": lambda command: command + "\ntrue",
        "appended comment": lambda command: command + " # injected",
        "appended subshell": lambda command: command + "; (echo injected)",
        "appended background": lambda command: command + " &",
        "appended redirect": lambda command: command + " > /tmp/injected",
    }
    for item in ("B-063", "B-127"):
        for label, mutate in mutations.items():
            mutated = copy.deepcopy(packet)
            mutated["packets"][item]["command"] = mutate(mutated["packets"][item]["command"])
            try:
                _check_packets(mutated, ROOT)
            except GraduationError as exc:
                assert item in str(exc), f"{item} {label}: wrong rejection: {exc}"
            else:
                pytest.fail(f"{item} {label}: mutation was accepted")

def test_b216_worker_event_requeue_and_paging_mutations_are_red() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    mutations = {
        "corelink-signup-worker": "corelink",
        "dsr.erasure.dead_letter": "DSR_ERASURE_DLQ",
        "priorRequeues < MAX_DLQ_REQUEUES": "priorRequeues <= MAX_DLQ_REQUEUES",
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
        "scripts/run_b251_latency_probe.py": "scripts/missing_b251_runner.py",
        "reports/owner-actions/b251-d02-identity.json": "reports/owner-actions/unretained.json",
        "reports/owner-actions/b251-d03-observed-identity.json": "reports/owner-actions/unobserved.json",
        ".identity.match == true": ".identity.match == false",
        '[\"seed\",\"failure\",\"blob\"]': '[\"seed\",\"blob\"]',
        ".measurement.sample_count == 1000": ".measurement.sample_count == 999",
        ".measurement.p99_us <= .measurement.limit_us": ".measurement.p99_us < .measurement.limit_us",
        '.measurement.fixture == \"InMemoryAtomicQuotaChecker\"': '.measurement.fixture == \"production\"',
        ".measurement.production_latency_measured == false": ".measurement.production_latency_measured == true",
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
            ("; python3 scripts/run_b251_latency_probe.py", "; $(date); python3 scripts/run_b251_latency_probe.py"),
            ("python3 scripts/run_b251_latency_probe.py", "echo scripts/run_b251_latency_probe.py"),
            (">/dev/null", "> /tmp/inert.log"),
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
            ".identity.match == true",
            "true # .identity.match == true",
        ),
        (
            "B-251",
            ".measurement.p99_us <= .measurement.limit_us",
            "true | if false then \".measurement.p99_us <= .measurement.limit_us\" else . end",
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
    assert '.conclusion == "success"' in command
    assert '.conclusion != null' not in command
    assert '.databaseId == ($run_id | tonumber)' in command
    assert 'all(.jobs[]; ((.name | ascii_downcase | startswith("create release")) | not) or .conclusion == "skipped")' in command
    assert 'all(.jobs[]; ((.name | ascii_downcase | startswith("publish verified signed release")) | not) or .conclusion == "skipped")' in command
    assert '.status == "completed" and .conclusion == "success"' in command
    assert '.status == "completed" and .conclusion == "skipped"' not in command
    assert "saw_active" not in command
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
        (
            '.status == "completed" and .conclusion == "success"',
            '.status == "completed" and .conclusion == "failure"',
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

    echoed_download = copy.deepcopy(packet)
    echoed_download["packets"]["B-105"]["command"] = echoed_download["packets"]["B-105"]["command"].replace(
        'gh run download "$run_id"', "echo 'gh run download \"$run_id\"'", 1
    )
    with pytest.raises(GraduationError, match="B-105"):
        _check_packets(echoed_download, ROOT)

    commented_download = copy.deepcopy(packet)
    commented_download["packets"]["B-105"]["command"] = commented_download["packets"]["B-105"]["command"].replace(
        'gh run download "$run_id"', '# gh run download "$run_id"', 1
    )
    with pytest.raises(GraduationError, match="B-105"):
        _check_packets(commented_download, ROOT)

    echoed_jq = copy.deepcopy(packet)
    echoed_jq["packets"]["B-105"]["command"] = echoed_jq["packets"]["B-105"]["command"].replace(
        "jq -e", "echo 'jq -e'", 2
    )
    with pytest.raises(GraduationError, match="B-105"):
        _check_packets(echoed_jq, ROOT)

    commented_jq = copy.deepcopy(packet)
    commented_jq["packets"]["B-105"]["command"] = commented_jq["packets"]["B-105"]["command"].replace(
        "jq -e", "# jq -e", 2
    )
    with pytest.raises(GraduationError, match="B-105"):
        _check_packets(commented_jq, ROOT)


def test_b105_run_selection_requires_exact_head_sha() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    command = packet["packets"]["B-105"]["command"]
    for needle, replacement in (
        ("expected_sha=\"$(gh api repos/HuGR-Labs/corelink-server/commits/main --jq .sha)\"", "expected_sha=\"stale\""),
        (".headSha == $expected_sha", ".headSha != $expected_sha"),
    ):
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-105"]["command"] = command.replace(needle, replacement, 1)
        with pytest.raises(GraduationError, match="B-105"):
            _check_packets(mutated, ROOT)


def test_b229_packet_matches_redacted_production_receipt() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    assert packet["packets"]["B-229"]["disposition"] == "DONE"
    assert "B-229 production" in packet["packets"]["B-229"]["evidence"]
    subprocess.run(
        ["python3", "scripts/verify_b229_clerk_webhook.py", "--evidence", packet["packets"]["B-229"]["artifact"], "--self-test"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    mutated = copy.deepcopy(packet)
    mutated["packets"]["B-229"]["disposition"] = "PARKED"
    with pytest.raises(GraduationError, match="B-229"):
        _check_packets(mutated, ROOT)


def test_b165_packet_matches_complete_committed_receipt_and_verifier() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    b165 = packet["packets"]["B-165"]
    assert b165["disposition"] == "DONE"
    assert b165["artifact"] == B165_ARTIFACT
    assert b165["command"] == B165_COMMAND
    assert b165["verify_means"] == "done"
    _check_packets(packet, ROOT)

    mutations = (
        ("disposition", "PARKED"),
        ("artifact", "artifacts/d03/B165-production-latency.json"),
        ("command", B165_COMMAND.replace("--require-served", "", 1)),
        ("evidence", "complete B-165 evidence is elsewhere"),
    )
    for field, replacement in mutations:
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-165"][field] = replacement
        with pytest.raises(GraduationError, match="B-165"):
            _check_packets(mutated, ROOT)

    receipt_mutations = (
        ("complete_sha256", "0" * 64),
        ("server_timing_sha256", "0" * 64),
    )
    for field, replacement in receipt_mutations:
        mutated_receipt = _load_packets(
            (ROOT / B165_ARTIFACT).read_text(encoding="utf-8")
        )
        with pytest.raises(GraduationError, match="B-165"):
            mutated_receipt["served_samples"][field] = replacement
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                for relative in (
                    B165_ARTIFACT,
                    "evidence/owner-actions/B-165/rejection-raw-2026-09-09.tsv",
                    "evidence/owner-actions/B-165/oci-recovery-raw-2026-09-09.tsv",
                    "evidence/owner-actions/B-165/served-raw-2026-09-09.tsv",
                    B165_COMPLETE_ARTIFACT,
                    B165_SERVER_TIMING_ARTIFACT,
                ):
                    target = root / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    source = ROOT / relative
                    target.write_bytes(source.read_bytes())
                (root / B165_ARTIFACT).write_text(
                    json.dumps(mutated_receipt), encoding="utf-8"
                )
                _check_b165_done_evidence(packet["packets"]["B-165"], root)


def test_b118_packet_is_retired_and_rejects_dispatch_or_parked_mutations() -> None:
    packet = _load_packets(
        (ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json").read_text(encoding="utf-8")
    )
    b118 = packet["packets"]["B-118"]
    assert b118["disposition"] == "DONE"
    assert b118["command"] == "python3 scripts/verify_b118_retirement.py"
    assert "RETIRED (B-118)" in b118["evidence"]
    assert "cosign-sign.yml absent" in b118["evidence"]
    for mutation in (
        {"disposition": "PARKED"},
        {"command": "gh run list --workflow cosign-sign.yml"},
        {"evidence": b118["evidence"].replace("RETIRED (B-118)", "UNMEASURED")},
    ):
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-118"].update(mutation)
        with pytest.raises(GraduationError, match="B-118"):
            _check_packets(mutated, ROOT)
    for field, replacement in (
        ("artifact", "evidence/production/other.md"),
        ("command", packet["packets"]["B-229"]["command"].replace("--self-test", "--expect done")),
        ("evidence", "B-229 production receipt is elsewhere."),
    ):
        mutated = copy.deepcopy(packet)
        mutated["packets"]["B-229"][field] = replacement
        with pytest.raises(GraduationError, match="B-229"):
            _check_packets(mutated, ROOT)

    bad_even_median = copy.deepcopy(packet)
    bad_even_median["packets"]["B-104"]["command"] = bad_even_median["packets"]["B-104"]["command"].replace(
        "((v[int((n+1)/2)] + v[int((n+2)/2)]) / 2)", "v[int((n+1)/2)]", 1
    )
    with pytest.raises(GraduationError, match="B-104"):
        _check_packets(bad_even_median, ROOT)

    bad_p90 = copy.deepcopy(packet)
    bad_p90["packets"]["B-104"]["command"] = bad_p90["packets"]["B-104"]["command"].replace(
        "rank=0.9*(n-1)+1; lo=int(rank); frac=rank-lo", "rank=0.9*n; lo=int(rank); frac=0", 1
    )
    with pytest.raises(GraduationError, match="B-104"):
        _check_packets(bad_p90, ROOT)
