#!/usr/bin/env python3
"""Fail-closed verifier for the bounded owner-action packet population.

This guard validates the packet's portable engineering contract only. It never
contacts GitHub, PagerDuty, Stripe, Drata, Cloudflare, Apple, Windows, or a
customer, and it never independently treats an owner action as completed merely
because a packet field is present. Missing, duplicate,
ambiguous, or mutated packet fields are errors rather than an empty result.
It does not perform or independently reproduce an owner action; a closed row is
accepted only when its packet metadata and canonical BACKLOG contract record the
corresponding repository/evidence closure.
"""

from __future__ import annotations

import argparse
import copy
import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PACKET = ROOT / "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json"
EXPECTED_IDS = (
    "B-008", "B-012", "B-013", "B-032", "B-035", "B-065",
    "B-086", "B-089", "B-097", "B-110", "B-111", "B-154",
    "B-029", "B-044", "B-046", "B-054", "B-063", "B-068", "B-071",
    "B-072", "B-083", "B-102", "B-106", "B-113", "B-125", "B-128",
    "B-134", "B-142", "B-165",
)
# B-013 was reconciled from an external owner action to a repository-side
# closure record after its redacted deletion evidence received a strict focal
# verifier.  It therefore no longer belongs to the legacy ``owner`` subset;
# the other ten legacy rows remain owner-controlled until their actions are
# evidenced and reclassified.
LEGACY_OWNER_IDS = frozenset(EXPECTED_IDS[:12]) - {"B-013", "B-110"}
CLOSED_PACKET_IDS = frozenset({"B-013", "B-110", "B-165"})
ITEM_FIELDS = {
    "id", "owner", "status", "action_type", "procedure",
    "inputs_and_credentials_boundary", "evidence", "expected_postcondition",
    "retry_and_rollback", "references",
}
BOUNDARY_FIELDS = {"inputs", "credentials"}
EVIDENCE_FIELDS = {"path", "format", "required_fields", "item_schema"}
FORBIDDEN_MARKERS = ("<", ">", "TBD", "TODO", "FIXME", "SECRET_VALUE")
SECRET_SHAPES = (
    re.compile(r"-----BEGIN [A-Z ]+ PRIVATE KEY-----"),
    re.compile(r"\b(?:gh[pousr]_|github_pat_|sk_live_|whsec_)\S+", re.IGNORECASE),
)

# B-111 is an owner acquisition item, not a claim that any certificate exists.
# Keep its contract synchronized with the executable workflow_call interfaces;
# comments and stale prose must not be allowed to satisfy this guard.
B111_WORKFLOW_SECRETS = {
    ".github/workflows/notarize-macos.yml": frozenset({
        "CORELINK_CLI_RELEASE_TOKEN",
        "APPLE_DEVELOPER_ID",
        "APPLE_DEVELOPER_ID_PASSWORD",
        "APPLE_TEAM_ID",
        "APPLE_NOTARIZATION_API_KEY",
        "APPLE_NOTARIZATION_KEY_ID",
        "APPLE_NOTARIZATION_ISSUER",
        "APPLE_DEVELOPER_ID_FINGERPRINT",
    }),
    ".github/workflows/sign-windows.yml": frozenset({
        "CORELINK_CLI_RELEASE_TOKEN",
        "WINDOWS_CODE_SIGNING_CERT",
        "WINDOWS_CODE_SIGNING_PASSWORD",
        "WINDOWS_CODE_SIGNING_FINGERPRINT",
        "WINDOWS_CODE_SIGNING_SUBJECT",
    }),
}
B111_ACQUISITION_SECRETS = frozenset().union(
    B111_WORKFLOW_SECRETS[".github/workflows/notarize-macos.yml"]
    - {"CORELINK_CLI_RELEASE_TOKEN"},
    B111_WORKFLOW_SECRETS[".github/workflows/sign-windows.yml"]
    - {"CORELINK_CLI_RELEASE_TOKEN"},
)
B111_RELEASE_CHAIN = {
    "release": {"build"},
    "sign-linux": {"release"},
    "sign-windows": {"sign-linux", "release"},
    "notarize-macos": {"sign-windows", "release"},
}
B110_EVIDENCE_PATH = "evidence/owner-actions/B-110/ci-capacity-decision.json"
B110_EVIDENCE_REQUIRED_FIELDS = [
    "schema_version",
    "captured_at",
    "selected_option",
    "workflows",
    "capacity_or_billing_reference",
    "coverage_impact",
    "runner_labels",
    "rollback_owner",
    "operator",
]
B110_EVIDENCE_ITEM_SCHEMA = (
    "workflows[] contains workflow, current_runner, selected_runner, and action; "
    "selected_option is hosted_billing, linux_self_hosted, or owner_authorized_park."
)
B110_EVIDENCE_WORKFLOWS = [
    {
        "workflow": ".github/workflows/cas_foundation.yml",
        "current_runner": "ubuntu-latest / ubuntu-x64-4core",
        "selected_runner": "corelink",
        "action": "migrated",
    },
    {
        "workflow": ".github/workflows/coverage.yml",
        "current_runner": "ubuntu-x64-4core",
        "selected_runner": "corelink",
        "action": "migrated",
    },
    {
        "workflow": ".github/workflows/ffi-matrix-ci.yml",
        "current_runner": "ubuntu-latest",
        "selected_runner": "corelink",
        "action": "migrated",
    },
    {
        "workflow": ".github/workflows/mutation-nightly.yml",
        "current_runner": "ubuntu-x64-4core",
        "selected_runner": "corelink",
        "action": "migrated",
    },
]
B110_EXPECTED_POSTCONDITION = (
    "The owner-approved Linux self-hosted capacity decision is recorded and the four lanes use viable "
    "CoreLink capacity without deletion; B-110 is closed only after the workflow and verifier changes are present."
)
B110_PROCEDURE = [
    "UI: Choose one authorized capacity path for cas_foundation, coverage, ffi-matrix-ci, and mutation-nightly: restore hosted billing, provision an adequate Linux self-hosted box, or authorize deletion/parking of the named lanes.",
    "RUN: For a self-hosted path, register a dedicated runner label with documented CPU/RAM/disk limits and verify the four workflows’ `runs-on` and toolchain assumptions before enabling it.",
    "UI: For hosted billing, confirm the GitHub account spending limit and payment state; for deletion/parking, obtain explicit owner approval describing the coverage loss. Record the selected option before any workflow mutation.",
]

# These three receipts are repository-side observations of owner-gated work.
# Their presence is deliberately required even when the external action was
# NOT_EXECUTED/BLOCKED: an absent artifact is ambiguous, while a typed blocker
# cannot be mistaken for a production success.
B054_EVIDENCE_PATH = "evidence/owner-actions/B-054/keyed-audit-epoch-rollout.json"
B054_EVIDENCE_REQUIRED_FIELDS = [
    "schema_version", "captured_at", "legacy_epoch", "keyed_epoch",
    "migration_receipts", "witness_receipt", "archive_verification",
    "rotation_receipt", "rollback_plan", "operator", "repository_checks",
]
B083_EVIDENCE_PATH = "evidence/owner-actions/B-083/byok-real-kms-lifecycle.json"
B083_EVIDENCE_REQUIRED_FIELDS = [
    "schema_version", "captured_at", "tenant_redacted", "image_digest",
    "kms_provider", "check_access", "activation", "cas_ac_round_trip",
    "revocation", "run_loop", "operator", "repository_checks",
]
B097_EVIDENCE_PATH = "evidence/owner-actions/B-097/cloudflare-vcpu-quota-case.json"
B097_EVIDENCE_REQUIRED_FIELDS = [
    "schema_version", "captured_at", "account_id_redacted", "case_id",
    "requested_total_vcpu", "current_total_vcpu", "declared_reservation_vcpu",
    "active_tenants_concurrent", "provider_decision", "effective_at", "operator",
    "read_only_capture",
]
RECEIPT_STATUSES = frozenset({"PASS", "FAIL", "BLOCKED", "NOT_EXECUTED"})
PACKET_BASE_SHA = "908d3bdc86f17a8b41a280baaea9352d7ed450ab"
BASE_SHA_PROVENANCE_DISCLAIMER = (
    "The base_sha is an immutable D03 packet reference for provenance, not a claim that the reference "
    "is an ancestor of every checkout carrying this packet."
)
B054_REPOSITORY_CHECKS = [
    {
        "command": "python3 scripts/verify_b054_audit_chain_contract.py",
        "status": "PASS",
        "detail": "Unknown, partial, and downgrade epoch metadata fail closed; mutation fixtures are rejected.",
    },
    {
        "command": "python3 scripts/test_audit_chain_epoch_schema.py",
        "status": "PASS",
        "detail": "Additive schema constraints and no-replace guards pass with recursive_triggers=OFF.",
    },
]
B083_REPOSITORY_CHECKS = [
    {
        "command": "python3 scripts/verify_b083_revocation_wiring.py",
        "status": "PASS",
        "detail": "Durable source, pre-bind run_loop wiring, focal behavior, and mutation checks pass.",
    },
    {
        "command": "python3 tests/test_verify_b083_byok.py",
        "status": "PASS",
        "detail": "B083 verifier mutation: green baseline and named red mutant.",
    },
]


class PacketError(ValueError):
    pass


def _read_packet(path: Path = PACKET) -> dict[str, object]:
    if not path.is_file() or path.is_symlink():
        raise PacketError(f"missing/non-regular packet: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PacketError(f"packet is not valid UTF-8 JSON: {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise PacketError("packet root must be an object")
    return value


def _text(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise PacketError(f"{label} must be a non-empty string")
    if any(marker in value for marker in FORBIDDEN_MARKERS):
        raise PacketError(f"{label} contains an unresolved/ambiguous marker")
    if any(pattern.search(value) for pattern in SECRET_SHAPES):
        raise PacketError(f"{label} contains credential material")
    return value


def _string_list(value: object, label: str, minimum: int = 1) -> list[str]:
    if not isinstance(value, list) or len(value) < minimum:
        raise PacketError(f"{label} must be a non-empty list")
    result = []
    for index, entry in enumerate(value):
        result.append(_text(entry, f"{label}[{index}]"))
    return result


def _read_backlog_contracts() -> dict[str, tuple[str, str]]:
    """Read the canonical owner/status contract for the closed packet IDs."""
    path = ROOT / "BACKLOG.md"
    if not path.is_file() or path.is_symlink():
        raise PacketError(f"missing/non-regular backlog: {path}")
    text = path.read_text(encoding="utf-8")
    blocks = re.findall(r"```backlog\n(.*?)```", text, flags=re.DOTALL)
    contracts: dict[str, tuple[str, str]] = {}
    for block in blocks:
        id_match = re.search(r"^id:\s*(B-\d+)\s*$", block, flags=re.MULTILINE)
        if not id_match:
            continue
        item_id = id_match.group(1)
        owner_match = re.search(r"^owner:\s*([^\s]+)\s*$", block, flags=re.MULTILINE)
        status_match = re.search(r"^status:\s*([^\s]+)\s*$", block, flags=re.MULTILINE)
        if not owner_match or not status_match:
            raise PacketError(f"BACKLOG contract is missing owner/status for {item_id}")
        if item_id in contracts:
            raise PacketError(f"BACKLOG contract is duplicated for {item_id}")
        contracts[item_id] = (owner_match.group(1), status_match.group(1))
    missing = sorted(set(EXPECTED_IDS) - set(contracts))
    if missing:
        raise PacketError(f"BACKLOG contract missing packet IDs: {missing}")
    return {item_id: contracts[item_id] for item_id in EXPECTED_IDS}


_SECRET_EXPRESSION = re.compile(r"\$\{\{\s*secrets\.([A-Z0-9_]+)\s*\}\}")


def _strip_yaml_comment(line: str) -> str:
    """Strip YAML comments without treating quoted/string content as syntax."""
    quote: str | None = None
    escaped = False
    index = 0
    while index < len(line):
        character = line[index]
        if quote == '"':
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                quote = None
        elif quote == "'":
            if character == "'":
                if index + 1 < len(line) and line[index + 1] == "'":
                    index += 1
                else:
                    quote = None
        elif character in {'"', "'"}:
            quote = character
        elif character == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index].rstrip()
        index += 1
    return line.rstrip()


def _workflow_lines(path: Path) -> list[tuple[int, str, int]]:
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise PacketError(f"workflow is not valid UTF-8: {path}: {exc}") from exc
    result: list[tuple[int, str, int]] = []
    for line_number, raw_line in enumerate(text.splitlines(), 1):
        line = _strip_yaml_comment(raw_line)
        if not line.strip() or line.strip() == "---":
            continue
        indentation = len(line) - len(line.lstrip(" "))
        if "\t" in line[:indentation]:
            raise PacketError(f"workflow uses tab indentation: {path}:{line_number}")
        result.append((indentation, line[indentation:], line_number))
    return result


def _mapping_entry(content: str) -> tuple[str, str] | None:
    if ":" not in content:
        return None
    key, value = content.split(":", 1)
    key = key.strip()
    if not key:
        return None
    if len(key) >= 2 and key[0] == key[-1] and key[0] in {'"', "'"}:
        key = key[1:-1]
    return key, value.strip()


def _inline_needs(value: str) -> set[str]:
    value = value.strip()
    if not value:
        return set()
    if value.startswith("[") and value.endswith("]"):
        value = value[1:-1]
    entries = value.split(",")
    result: set[str] = set()
    for entry in entries:
        entry = entry.strip()
        if len(entry) >= 2 and entry[0] == entry[-1] and entry[0] in {'"', "'"}:
            entry = entry[1:-1]
        if entry:
            result.add(entry)
    return result


def _read_yaml_workflow(path: Path, require_workflow_call: bool = True) -> dict[str, object]:
    """Read only the workflow semantics required by the B-111 guard.

    This deliberately avoids a YAML library: the canonical verifier is run
    with ``python3 -S``.  Comments are removed with quote awareness, and secret
    expressions are accepted only from actual ``env:`` mappings, so prose,
    comments, and arbitrary run-string bait cannot satisfy the contract.
    """
    if not path.is_file() or path.is_symlink():
        raise PacketError(f"missing/non-regular workflow: {path}")
    lines = _workflow_lines(path)

    top_keys: dict[str, int] = {}
    for index, (indent, content, _line_number) in enumerate(lines):
        if indent != 0:
            continue
        entry = _mapping_entry(content)
        if entry:
            top_keys[entry[0]] = index
    on_index = top_keys.get("on")
    jobs_index = top_keys.get("jobs")
    if on_index is None and require_workflow_call:
        raise PacketError(f"workflow on contract missing: {path}")
    if jobs_index is None:
        raise PacketError(f"workflow jobs contract missing: {path}")

    declared: set[str] = set()
    if require_workflow_call:
        assert on_index is not None
        workflow_call_index = None
        for index in range(on_index + 1, len(lines)):
            indent, content, _line_number = lines[index]
            if indent <= 0:
                break
            entry = _mapping_entry(content)
            if indent == 2 and entry and entry[0] == "workflow_call" and not entry[1]:
                workflow_call_index = index
                break
        if workflow_call_index is None:
            raise PacketError(f"workflow_call contract missing: {path}")
        secrets_index = None
        for index in range(workflow_call_index + 1, len(lines)):
            indent, content, _line_number = lines[index]
            if indent <= 2:
                break
            entry = _mapping_entry(content)
            if indent == 4 and entry and entry[0] == "secrets" and not entry[1]:
                secrets_index = index
                break
        if secrets_index is None:
            raise PacketError(f"workflow_call.secrets contract missing: {path}")
        for indent, content, _line_number in lines[secrets_index + 1:]:
            if indent <= 4:
                break
            entry = _mapping_entry(content)
            if indent == 6 and entry:
                declared.add(entry[0])

    referenced: set[str] = set()
    known_secrets = B111_ACQUISITION_SECRETS | {"CORELINK_CLI_RELEASE_TOKEN"}
    env_indent: int | None = None
    for indent, content, _line_number in lines:
        if env_indent is not None and indent <= env_indent:
            env_indent = None
        entry = _mapping_entry(content)
        if entry and entry[0] == "env" and not entry[1]:
            env_indent = indent
            continue
        if env_indent is not None and indent == env_indent + 2:
            referenced.update(
                name for name in _SECRET_EXPRESSION.findall(content) if name in known_secrets
            )

    jobs: dict[str, dict[str, object]] = {}
    for index in range(jobs_index + 1, len(lines)):
        indent, content, _line_number = lines[index]
        if indent == 0:
            break
        entry = _mapping_entry(content)
        if indent == 2 and entry and not entry[1]:
            if entry[0] in jobs:
                raise PacketError(f"duplicate workflow job: {path}: {entry[0]}")
            jobs[entry[0]] = {}
        elif indent == 4 and entry and entry[0] == "needs" and jobs:
            current_job = next(reversed(jobs))
            if "needs" in jobs[current_job]:
                raise PacketError(f"duplicate workflow needs: {path}: {current_job}")
            jobs[current_job]["needs"] = _inline_needs(entry[1])
    return {
        "declared": frozenset(declared),
        "referenced": frozenset(referenced),
        "jobs": jobs,
    }


def _read_b111_workflow_contracts(root: Path = ROOT) -> dict[str, object]:
    contracts = {
        path: _read_yaml_workflow(root / path)
        for path in B111_WORKFLOW_SECRETS
    }
    release = _read_yaml_workflow(root / ".github/workflows/release-cli.yml", require_workflow_call=False)
    contracts["release-chain"] = release
    return contracts


def _needs_set(value: object) -> set[str]:
    if isinstance(value, str):
        return {value}
    if isinstance(value, set) and all(isinstance(entry, str) for entry in value):
        return set(value)
    if isinstance(value, list) and all(isinstance(entry, str) for entry in value):
        return set(value)
    return set()


def _check_b111_workflow_contract(workflow_contracts: dict[str, object]) -> None:
    for path, expected in B111_WORKFLOW_SECRETS.items():
        contract = workflow_contracts.get(path)
        if not isinstance(contract, dict):
            raise PacketError(f"B-111 workflow contract missing: {path}")
        declared = contract.get("declared")
        if declared != expected:
            raise PacketError(
                f"B-111 {path} workflow_call secrets drifted: "
                f"expected={sorted(expected)}, got={sorted(declared or ())}"
            )
        referenced = contract.get("referenced")
        if referenced != expected:
            raise PacketError(
                f"B-111 {path} executable secret references drifted: "
                f"expected={sorted(expected)}, got={sorted(referenced or ())}"
            )

    release = workflow_contracts.get("release-chain")
    if not isinstance(release, dict) or not isinstance(release.get("jobs"), dict):
        raise PacketError("B-111 B-112 release chain contract missing")
    jobs = release["jobs"]
    for job, expected_needs in B111_RELEASE_CHAIN.items():
        node = jobs.get(job)
        if not isinstance(node, dict):
            raise PacketError(f"B-111 B-112 release chain job missing: {job}")
        actual_needs = _needs_set(node.get("needs"))
        if not expected_needs.issubset(actual_needs):
            raise PacketError(
                f"B-111 B-112 release chain ordering drifted for {job}: "
                f"requires={sorted(expected_needs)}, got={sorted(actual_needs)}"
            )


def _read_b110_evidence() -> dict[str, object]:
    path = ROOT / B110_EVIDENCE_PATH
    if not path.is_file() or path.is_symlink():
        raise PacketError(f"B-110 evidence is missing/non-regular: {B110_EVIDENCE_PATH}")
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PacketError(f"B-110 evidence is not valid UTF-8 JSON: {exc}") from exc
    if not isinstance(record, dict):
        raise PacketError("B-110 evidence root must be an object")
    return record


def _check_b110_evidence(item: dict[str, object]) -> None:
    evidence = item["evidence"]
    assert isinstance(evidence, dict)
    if evidence["path"] != B110_EVIDENCE_PATH:
        raise PacketError("B-110 evidence path is not the canonical capacity decision")
    if evidence["format"] != "json":
        raise PacketError("B-110 evidence format is not JSON")
    if evidence["required_fields"] != B110_EVIDENCE_REQUIRED_FIELDS:
        raise PacketError("B-110 evidence required fields drifted")
    if evidence["item_schema"] != B110_EVIDENCE_ITEM_SCHEMA:
        raise PacketError("B-110 evidence item schema drifted")
    if item["action_type"] != "ci_capacity_decision":
        raise PacketError("B-110 action type drifted")
    if item["procedure"] != B110_PROCEDURE:
        raise PacketError("B-110 procedure drifted")
    if item["expected_postcondition"] != B110_EXPECTED_POSTCONDITION:
        raise PacketError("B-110 expected postcondition drifted")

    record = _read_b110_evidence()
    if set(record) != set(B110_EVIDENCE_REQUIRED_FIELDS):
        raise PacketError("B-110 evidence fields drifted")
    if record["schema_version"] != 1 or not isinstance(record["captured_at"], str) or not record["captured_at"].strip():
        raise PacketError("B-110 evidence capture metadata drifted")
    if record["selected_option"] != "linux_self_hosted":
        raise PacketError("B-110 capacity decision is not linux_self_hosted")
    if record["workflows"] != B110_EVIDENCE_WORKFLOWS:
        raise PacketError("B-110 workflow evidence drifted")
    if record["capacity_or_billing_reference"] != (
        "CoreLink runner image documentation: Linux x86_64, 4 vCPU, 12.5 GB; migration commit 41c47f236"
    ):
        raise PacketError("B-110 capacity reference drifted")
    if record["coverage_impact"] != (
        "No lane was deleted or parked. The FFI Python/Go/Node matrices remain intact; cache guards remain "
        "self-hosted-safe; workflow assertions are unchanged."
    ):
        raise PacketError("B-110 coverage impact drifted")
    if record["runner_labels"] != ["corelink"]:
        raise PacketError("B-110 runner labels drifted")
    if record["rollback_owner"] != "owner" or record["operator"] != "owner-authorized automation":
        raise PacketError("B-110 evidence ownership metadata drifted")


def _read_json_evidence(path_text: str, expected_fields: list[str], label: str) -> dict[str, object]:
    """Load one canonical receipt and reject missing/extra root fields and secrets."""
    path = ROOT / path_text
    if not path.is_file() or path.is_symlink():
        raise PacketError(f"{label} evidence is missing/non-regular: {path_text}")
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PacketError(f"{label} evidence is not valid UTF-8 JSON: {exc}") from exc
    if not isinstance(record, dict):
        raise PacketError(f"{label} evidence root must be an object")
    if set(record) != set(expected_fields):
        raise PacketError(
            f"{label} evidence fields drifted; expected={expected_fields}, got={sorted(record)}"
        )
    serialized = json.dumps(record, ensure_ascii=False)
    if any(pattern.search(serialized) for pattern in SECRET_SHAPES):
        raise PacketError(f"{label} evidence contains credential material")
    return record


def _exact_keys(value: object, expected: set[str], label: str) -> dict[str, object]:
    if not isinstance(value, dict) or set(value) != expected:
        got = sorted(value) if isinstance(value, dict) else type(value).__name__
        raise PacketError(f"{label} fields drifted; expected={sorted(expected)}, got={got}")
    return value


def _receipt_status(value: object, label: str) -> str:
    if value not in RECEIPT_STATUSES:
        raise PacketError(f"{label} has invalid status: {value!r}")
    return str(value)


def _receipt_blocker(value: object, status: str, label: str) -> None:
    if status != "PASS":
        if not isinstance(value, str) or not value.strip():
            raise PacketError(f"{label} must name an exact blocker for {status}")


def _check_b054_evidence(item: dict[str, object]) -> None:
    evidence = item["evidence"]
    assert isinstance(evidence, dict)
    if evidence["path"] != B054_EVIDENCE_PATH or evidence["format"] != "json":
        raise PacketError("B-054 evidence binding drifted")
    if evidence["required_fields"] != B054_EVIDENCE_REQUIRED_FIELDS:
        raise PacketError("B-054 evidence required fields drifted")
    record = _read_json_evidence(B054_EVIDENCE_PATH, B054_EVIDENCE_REQUIRED_FIELDS, "B-054")
    if record["schema_version"] != 1 or not isinstance(record["captured_at"], str) or not record["captured_at"].strip():
        raise PacketError("B-054 evidence capture metadata drifted")
    expected_epochs = {
        "legacy_epoch": ("E0/unkeyed", "NOT_EXECUTED"),
        "keyed_epoch": ("E1/keyed", "BLOCKED"),
    }
    for name in ("legacy_epoch", "keyed_epoch"):
        epoch = _exact_keys(record[name], {"algorithm_version", "row_count", "verification", "status", "blocker"}, f"B-054 {name}")
        status = _receipt_status(epoch["status"], f"B-054 {name}")
        expected_algorithm, expected_status = expected_epochs[name]
        if epoch["algorithm_version"] != expected_algorithm:
            raise PacketError(f"B-054 {name} algorithm version drifted")
        if epoch["row_count"] is not None:
            raise PacketError(f"B-054 {name} row count would overstate an unexecuted probe")
        if epoch["verification"] != expected_status or status != expected_status or status != epoch["verification"]:
            raise PacketError(f"B-054 {name} status/verification coherence drifted")
        _receipt_blocker(epoch["blocker"], status, f"B-054 {name}")
    migration = _exact_keys(record["migration_receipts"], {"status", "migrations", "references", "blocker"}, "B-054 migration_receipts")
    migration_status = _receipt_status(migration["status"], "B-054 migration_receipts")
    if migration_status != "NOT_EXECUTED":
        raise PacketError("B-054 migration status would overstate an unexecuted rollout")
    if migration["migrations"] != ["0109_audit_chain_epoch_contract.sql", "0110_audit_chain_epoch_row_metadata.sql"]:
        raise PacketError("B-054 migration list drifted")
    if migration["references"] != []:
        raise PacketError("B-054 migration references would overstate an applied migration")
    _receipt_blocker(migration["blocker"], migration_status, "B-054 migration_receipts")
    expected_receipt_statuses = {
        "witness_receipt": "BLOCKED",
        "archive_verification": "NOT_EXECUTED",
        "rotation_receipt": "BLOCKED",
    }
    for name, fields in {
        "witness_receipt": {"status", "reference", "blocker"},
        "archive_verification": {"status", "proof_reference", "blocker"},
        "rotation_receipt": {"status", "reference", "blocker"},
    }.items():
        nested = _exact_keys(record[name], fields, f"B-054 {name}")
        status = _receipt_status(nested["status"], f"B-054 {name}")
        if status != expected_receipt_statuses[name]:
            raise PacketError(f"B-054 {name} status would overstate the current rollout state")
        reference_key = "proof_reference" if name == "archive_verification" else "reference"
        if status != "PASS" and nested[reference_key] is not None:
            raise PacketError(f"B-054 {name} has a reference despite status {status}")
        _receipt_blocker(nested["blocker"], status, f"B-054 {name}")
    rollback = _exact_keys(record["rollback_plan"], {"status", "decision", "blocker"}, "B-054 rollback_plan")
    rollback_status = _receipt_status(rollback["status"], "B-054 rollback_plan")
    if rollback_status != "NOT_EXECUTED" or rollback["decision"] != "preserve E0 and do not cut over":
        raise PacketError("B-054 rollback decision/status drifted")
    _receipt_blocker(rollback["blocker"], rollback_status, "B-054 rollback_plan")
    if not isinstance(record["operator"], str) or not record["operator"].strip():
        raise PacketError("B-054 operator missing")
    _check_repository_checks(record["repository_checks"], "B-054", B054_REPOSITORY_CHECKS)


def _check_repository_checks(value: object, label: str, expected: list[dict[str, str]] | None = None) -> None:
    if not isinstance(value, list) or not value:
        raise PacketError(f"{label} repository_checks must be a non-empty list")
    for index, entry in enumerate(value):
        check = _exact_keys(entry, {"command", "status", "detail"}, f"{label} repository_checks[{index}]")
        if not isinstance(check["command"], str) or not check["command"].strip():
            raise PacketError(f"{label} repository_checks[{index}] command missing")
        if check["status"] not in {"PASS", "FAIL", "NOT_EXECUTED", "BLOCKED"}:
            raise PacketError(f"{label} repository_checks[{index}] status invalid")
        if not isinstance(check["detail"], str) or not check["detail"].strip():
            raise PacketError(f"{label} repository_checks[{index}] detail missing")
    if expected is not None and value != expected:
        raise PacketError(f"{label} repository_checks commands/details drifted")


def _check_b083_evidence(item: dict[str, object]) -> None:
    evidence = item["evidence"]
    assert isinstance(evidence, dict)
    if evidence["path"] != B083_EVIDENCE_PATH or evidence["format"] != "json":
        raise PacketError("B-083 evidence binding drifted")
    if evidence["required_fields"] != B083_EVIDENCE_REQUIRED_FIELDS:
        raise PacketError("B-083 evidence required fields drifted")
    record = _read_json_evidence(B083_EVIDENCE_PATH, B083_EVIDENCE_REQUIRED_FIELDS, "B-083")
    if record["schema_version"] != 1 or not isinstance(record["captured_at"], str) or not record["captured_at"].strip():
        raise PacketError("B-083 evidence capture metadata drifted")
    if record["tenant_redacted"] != "NOT_PROVISIONED" or record["image_digest"] is not None or record["kms_provider"] != "aws":
        raise PacketError("B-083 evidence must identify the unprovisioned AWS runtime without a digest")
    if record["check_access"] != "BLOCKED":
        raise PacketError("B-083 check_access must be BLOCKED while the runtime is not provisioned")
    for name in ("activation", "cas_ac_round_trip", "revocation", "run_loop"):
        nested = _exact_keys(record[name], {"status", "audit_event_reference", "completed_at", "blocker"}, f"B-083 {name}")
        status = _receipt_status(nested["status"], f"B-083 {name}")
        if status != "PASS" and (nested["audit_event_reference"] is not None or nested["completed_at"] is not None):
            raise PacketError(f"B-083 {name} has completion evidence despite status {status}")
        if status != "NOT_EXECUTED":
            raise PacketError(f"B-083 {name} cannot run without a provisioned runtime")
        _receipt_blocker(nested["blocker"], status, f"B-083 {name}")
    if not isinstance(record["operator"], str) or not record["operator"].strip():
        raise PacketError("B-083 operator missing")
    _check_repository_checks(record["repository_checks"], "B-083", B083_REPOSITORY_CHECKS)


def _check_b097_evidence(item: dict[str, object]) -> None:
    evidence = item["evidence"]
    assert isinstance(evidence, dict)
    if evidence["path"] != B097_EVIDENCE_PATH or evidence["format"] != "json":
        raise PacketError("B-097 evidence binding drifted")
    if evidence["required_fields"] != B097_EVIDENCE_REQUIRED_FIELDS:
        raise PacketError("B-097 evidence required fields drifted")
    record = _read_json_evidence(B097_EVIDENCE_PATH, B097_EVIDENCE_REQUIRED_FIELDS, "B-097")
    if record["schema_version"] != 1 or not isinstance(record["captured_at"], str) or not record["captured_at"].strip():
        raise PacketError("B-097 evidence capture metadata drifted")
    if not isinstance(record["account_id_redacted"], str) or not re.fullmatch(r"[0-9a-f]{4}\.\.\.[0-9a-f]{4}", record["account_id_redacted"]):
        raise PacketError("B-097 account identifier must remain redacted")
    if record["case_id"] is not None or record["requested_total_vcpu"] is not None or record["effective_at"] is not None:
        raise PacketError("B-097 receipt must not invent an owner support case or requested/effective quota")
    if record["current_total_vcpu"] != 1500 or record["declared_reservation_vcpu"] != 1250 or record["active_tenants_concurrent"] is not None:
        raise PacketError("B-097 quota/active-tenant capture is not the measured truthful state")
    if record["provider_decision"] != "pending":
        raise PacketError("B-097 provider decision must remain pending without an owner case")
    capture = _exact_keys(record["read_only_capture"], {"cloudchamber_account_endpoint", "provider_limit_confirmed", "provider_response", "active_tenant_metric", "activity_readback", "support_case"}, "B-097 read_only_capture")
    if capture["cloudchamber_account_endpoint"] != "GET /accounts/{account}/containers/me":
        raise PacketError("B-097 read_only_capture endpoint drifted")
    if capture["provider_limit_confirmed"] is not True or capture["support_case"] != "not submitted; owner Cloudflare support/account session is required":
        raise PacketError("B-097 read_only_capture support/limit boundary drifted")
    response = _exact_keys(capture["provider_response"], {"http_success", "total_vcpu", "vcpu_per_deployment", "total_memory_mib", "usage"}, "B-097 provider_response")
    if response != {"http_success": True, "total_vcpu": 1500, "vcpu_per_deployment": 4, "total_memory_mib": 6291456, "usage": None}:
        raise PacketError("B-097 provider response drifted")
    if capture["active_tenant_metric"] != "unavailable: CONFIG_DB activity is stale relative to capture and is not a concurrent-active metric":
        raise PacketError("B-097 active-tenant metric disclaimer drifted")
    activity = _exact_keys(capture["activity_readback"], {"database", "access", "customer_audit_rows", "distinct_tenants_all_time", "last_customer_audit_ts_ms", "distinct_tenants_last_15m", "distinct_tenants_last_1h", "distinct_tenants_last_24h", "interpretation"}, "B-097 activity_readback")
    if activity["database"] != "CONFIG_DB/prod" or activity["access"] != "remote-read-only" or any(activity[name] != expected for name, expected in {"customer_audit_rows": 71, "distinct_tenants_all_time": 16, "last_customer_audit_ts_ms": 1784669020765, "distinct_tenants_last_15m": 0, "distinct_tenants_last_1h": 0, "distinct_tenants_last_24h": 0}.items()):
        raise PacketError("B-097 activity readback drifted")
    if activity["interpretation"] != "retained audit activity is stale; these aggregates are not a concurrent-active-tenant measurement":
        raise PacketError("B-097 activity readback disclaimer drifted")


def _check_item(
    item: object,
    expected_id: str,
    backlog_contracts: dict[str, tuple[str, str]],
    workflow_contracts: dict[str, object],
) -> None:
    if not isinstance(item, dict):
        raise PacketError(f"{expected_id}: item must be an object")
    if set(item) != ITEM_FIELDS:
        missing = sorted(ITEM_FIELDS - set(item))
        extra = sorted(set(item) - ITEM_FIELDS)
        raise PacketError(f"{expected_id}: item fields mismatch; missing={missing}, extra={extra}")
    if item.get("id") != expected_id:
        raise PacketError(f"item id mismatch: expected {expected_id!r}, got {item.get('id')!r}")
    canonical_owner, canonical_status = backlog_contracts[expected_id]
    expected_owner = "owner" if expected_id in LEGACY_OWNER_IDS else "tl"
    allowed_statuses = {"open", "parked"}
    # B-013 closes on its owner-authorized redacted deletion record, while
    # B-110 closes after the owner selects the already-provisioned CoreLink
    # Linux substrate and the four workflow migrations are evidenced. Other
    # legacy owner items remain external-action pending by contract.
    if expected_id in CLOSED_PACKET_IDS:
        allowed_statuses.add("done")
    if canonical_owner != expected_owner or canonical_status not in allowed_statuses:
        raise PacketError(
            f"{expected_id}: BACKLOG canonical contract drifted from "
            f"{expected_owner}/allowed-status ({canonical_owner}/{canonical_status})"
        )
    # The packet remains status-aligned with BACKLOG; parked legacy items may
    # retain an open packet while external action is pending.
    packet_status_ok = item.get("status") == canonical_status or (
        canonical_status == "parked" and item.get("status") == "open"
    )
    if item.get("owner") != canonical_owner or not packet_status_ok:
        raise PacketError(
            f"{expected_id}: packet owner/status disagrees with BACKLOG "
            f"({canonical_owner}/{canonical_status})"
        )
    _text(item["action_type"], f"{expected_id}.action_type")

    procedure = _string_list(item["procedure"], f"{expected_id}.procedure", minimum=2)
    if not all(step.startswith(("RUN:", "UI:")) for step in procedure):
        raise PacketError(f"{expected_id}.procedure: every step must be an explicit RUN: or UI: procedure")

    boundary = item["inputs_and_credentials_boundary"]
    if not isinstance(boundary, dict) or set(boundary) != BOUNDARY_FIELDS:
        raise PacketError(f"{expected_id}.inputs_and_credentials_boundary fields are ambiguous")
    _string_list(boundary["inputs"], f"{expected_id}.inputs_and_credentials_boundary.inputs")
    _text(boundary["credentials"], f"{expected_id}.inputs_and_credentials_boundary.credentials")

    evidence = item["evidence"]
    if not isinstance(evidence, dict) or set(evidence) != EVIDENCE_FIELDS:
        raise PacketError(f"{expected_id}.evidence fields are ambiguous")
    path = _text(evidence["path"], f"{expected_id}.evidence.path")
    if not path.startswith("evidence/owner-actions/") and not path.startswith("docs/compliance/vendor-reviews/"):
        raise PacketError(f"{expected_id}.evidence.path is outside the canonical owner-evidence roots")
    if Path(path).is_absolute() or ".." in Path(path).parts:
        raise PacketError(f"{expected_id}.evidence.path must be repository-relative")
    if evidence["format"] not in ("json", "markdown"):
        raise PacketError(f"{expected_id}.evidence.format is unsupported")
    _string_list(evidence["required_fields"], f"{expected_id}.evidence.required_fields")
    _text(evidence["item_schema"], f"{expected_id}.evidence.item_schema")

    _text(item["expected_postcondition"], f"{expected_id}.expected_postcondition")
    _text(item["retry_and_rollback"], f"{expected_id}.retry_and_rollback")
    references = _string_list(item["references"], f"{expected_id}.references", minimum=1)
    if not any(reference.startswith("BACKLOG.md#") for reference in references):
        raise PacketError(f"{expected_id}.references must include its BACKLOG anchor")
    if expected_id == "B-110":
        _check_b110_evidence(item)
    if expected_id == "B-054":
        _check_b054_evidence(item)
    if expected_id == "B-083":
        _check_b083_evidence(item)
    if expected_id == "B-097":
        _check_b097_evidence(item)
    if expected_id == "B-111":
        procedure_text = " ".join(item["procedure"])
        schema_text = item["evidence"]["item_schema"]
        missing = sorted(
            name for name in B111_ACQUISITION_SECRETS
            if name not in procedure_text or name not in schema_text
        )
        if missing:
            raise PacketError(f"B-111 procedure/evidence omits executable secret names: {missing}")
        if "B-112" not in procedure_text:
            raise PacketError("B-111 procedure must name B-112 as the upstream release prerequisite")
        evidence_fields = set(item["evidence"]["required_fields"])
        if "release_prerequisite" not in evidence_fields:
            raise PacketError("B-111 evidence must retain the B-112 release prerequisite")
        _check_b111_workflow_contract(workflow_contracts)


def check_data(
    data: object,
    identifier: str | None = None,
    backlog_contracts: dict[str, tuple[str, str]] | None = None,
    workflow_contracts: dict[str, object] | None = None,
) -> dict[str, int]:
    if not isinstance(data, dict):
        raise PacketError("packet root must be an object")
    required_root = {"schema_version", "packet_id", "repository", "base_sha", "status", "non_claim", "population", "items"}
    if set(data) != required_root:
        raise PacketError(f"packet root fields mismatch; expected {sorted(required_root)}")
    if data["schema_version"] != 1 or data["repository"] != "HuGR-Labs/corelink-server":
        raise PacketError("packet schema or repository identity changed")
    # This packet was authored against the D03 reconciliation snapshot.  The
    # field is a provenance reference, not a merge-base assertion: recovery
    # restacks may legitimately carry the packet on a branch that does not
    # descend from that snapshot.  Keep the exact immutable reference while
    # avoiding the false claim that it is an ancestor of every checkout.
    if data["base_sha"] != PACKET_BASE_SHA:
        raise PacketError("packet base SHA is not the requested exact D03 provenance reference")
    if backlog_contracts is None:
        backlog_contracts = _read_backlog_contracts()
    if workflow_contracts is None:
        workflow_contracts = _read_b111_workflow_contracts()
    if set(backlog_contracts) != set(EXPECTED_IDS):
        raise PacketError("BACKLOG owner/status reconciliation population is not closed")
    _text(data["packet_id"], "packet_id")
    _text(data["status"], "status")
    non_claim = _text(data["non_claim"], "non_claim")
    if BASE_SHA_PROVENANCE_DISCLAIMER not in non_claim:
        raise PacketError("non_claim must include the base_sha provenance disclaimer")

    population = data["population"]
    if population != list(EXPECTED_IDS):
        raise PacketError(f"closed population mismatch: expected {list(EXPECTED_IDS)}, got {population!r}")
    items = data["items"]
    if not isinstance(items, list) or len(items) != len(EXPECTED_IDS):
        raise PacketError("items must contain exactly the closed population")
    seen: set[str] = set()
    evidence_paths: set[str] = set()
    for item in items:
        if not isinstance(item, dict):
            raise PacketError("items contains a non-object entry")
        item_id = item.get("id")
        if item_id in seen:
            raise PacketError(f"duplicate item: {item_id}")
        seen.add(str(item_id))
        _check_item(item, str(item_id), backlog_contracts, workflow_contracts)
        evidence_path = str(item["evidence"]["path"])
        if evidence_path in evidence_paths:
            raise PacketError(f"duplicate canonical evidence path: {evidence_path}")
        evidence_paths.add(evidence_path)
    if seen != set(EXPECTED_IDS):
        raise PacketError(f"item population mismatch: expected {list(EXPECTED_IDS)}, got {sorted(seen)}")
    if identifier is not None and identifier not in EXPECTED_IDS:
        raise PacketError(f"unknown packet id: {identifier}")
    return {"items": len(items), "population": len(population)}


def mutation_self_test(data: dict[str, object]) -> int:
    """Remove/mangle every load-bearing packet boundary in memory."""
    mutations = 0
    backlog_contracts = _read_backlog_contracts()
    workflow_contracts = _read_b111_workflow_contracts()
    for index, item in enumerate(data["items"]):
        assert isinstance(item, dict)
        for field in ITEM_FIELDS:
            mutated = copy.deepcopy(data)
            del mutated["items"][index][field]
            try:
                check_data(
                    mutated,
                    backlog_contracts=backlog_contracts,
                    workflow_contracts=workflow_contracts,
                )
            except PacketError:
                mutations += 1
            else:
                raise PacketError(f"mutation was accepted: {item['id']}.{field}")
        for field, mutated_value in (("owner", "owner" if item["owner"] == "tl" else "tl"), ("status", "closed")):
            mutated = copy.deepcopy(data)
            mutated["items"][index][field] = mutated_value
            try:
                check_data(
                    mutated,
                    backlog_contracts=backlog_contracts,
                    workflow_contracts=workflow_contracts,
                )
            except PacketError:
                mutations += 1
            else:
                raise PacketError(f"cross-file {field} mutation was accepted: {item['id']}")
    mutated_population = copy.deepcopy(data)
    mutated_population["population"] = list(EXPECTED_IDS[:-1])
    try:
        check_data(
            mutated_population,
            backlog_contracts=backlog_contracts,
            workflow_contracts=workflow_contracts,
        )
    except PacketError:
        mutations += 1
    else:
        raise PacketError("population mutation was accepted")
    mutated_path = copy.deepcopy(data)
    mutated_path["items"][1]["evidence"]["path"] = mutated_path["items"][0]["evidence"]["path"]
    try:
        check_data(
            mutated_path,
            backlog_contracts=backlog_contracts,
            workflow_contracts=workflow_contracts,
        )
    except PacketError:
        mutations += 1
    else:
        raise PacketError("duplicate evidence-path mutation was accepted")
    return mutations


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--id", choices=EXPECTED_IDS)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)
    try:
        data = _read_packet()
        result = check_data(data, args.id)
        mutations = mutation_self_test(data) if args.self_test else 0
    except (OSError, PacketError) as exc:
        print(f"owner-action packet: FAIL: {exc}", file=sys.stderr)
        return 1
    suffix = f", {mutations} mutation(s) rejected" if args.self_test else ""
    print(f"owner-action packet: PASS: {result['items']} item(s), closed population verified{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
