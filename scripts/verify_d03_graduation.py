#!/usr/bin/env python3
"""Closed-population graduation gate for the D03 TL backlog lane.

This is intentionally a bounded register check.  It does not dispatch CI,
contact GitHub, or pretend that production evidence is present.  It proves
that the one DCO candidate accounts for the exact original population, that
every parked item has an executable owner packet, and that the three DONE items
still pass their local inverted guards.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
from collections import Counter
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.backlog_verify import parse


ROOT = Path(__file__).resolve().parents[1]
PACKET_PATH = ROOT / "docs/handoff/2026-09-06-d03-graduation-packets.json"
OWNER_POPULATION = ("B-039",)
POST_GRADUATION_PARKED = ("B-210",)
OWNER_PACKET_FIELDS = {"owner", "status", "dependency", "action", "artifact", "command"}
OWNER_MIRROR_URL = "https://corelink-artifacts.humangr.com/tlaplus/v1.8.0/eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a/tla2tools.jar"
OWNER_MIRROR_SHA256 = "eabd140a70f49eb9305a3bd3f3df944eddf87e5a90d329789085f8953a80533a"

# The compact D03 register is only a handoff index.  Its command must still
# preserve the load-bearing parts of the detailed owner packet.  Keep this
# map closed: accepting a self-declared profile/sample count would let a
# command redirect arbitrary output while claiming to run the right proof.
COMMAND_CONTRACTS: dict[str, dict[str, Any]] = {
    "B-044": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-044",
        "profiles": [], "sample_count": 1,
        "required": ["orphan_reconcile_scan", "sbox", "observe", "type-1", "type-2", "tail_rc", "124"],
        "safety": ["RECONCILE_ORPHAN_TEARDOWN=0"],
        "forbidden": ["RECONCILE_ORPHAN_TEARDOWN=1", "wrangler delete", "wrangler destroy"],
    },
    "B-046": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-046",
        "profiles": [], "sample_count": 2,
        "required": ["--probe", "object-lock", "COMPLIANCE"],
        "safety": ["B046_PROBE_ALLOW_MUTATION=1", "corelink-b046-probe-"],
        "forbidden": ["corelink-prod", "0102_cas_retention", "compliance database"],
    },
    "B-063": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-063",
        "profiles": ["93da3f7a", "enam"], "sample_count": 3,
        "required": ["audit_outbox", "GROUP BY", "part_last_archived", "audit-archive-lag", "PagerDuty", "workflow_dispatch"],
        "safety": ["--remote", "--ref main", "SELECT", "B063_REPAIR_APPROVED=1"],
        "forbidden": ["DELETE FROM", "UPDATE ", "INSERT INTO"],
    },
    "B-068": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-068",
        "profiles": ["d1", "r2", "stripe", "neon"], "sample_count": 4,
        "required": ["real-ignored-harnesses.yml", "workflow_dispatch", "seed", "active_yaml", "awk"],
        "safety": ["--ref main", "profile=", "OWNER_APPROVED_REAL_INTEGRATION=1"],
        "forbidden": ["emit_e2e_seed", "PAT signing seed"],
    },
    "B-071": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-071",
        "profiles": [], "sample_count": 1,
        "required": ["gc-sweep-dry-run.yml", "GC_LIVE_DELETE=false", "deleted_count", "0"],
        "safety": ["credentialless", "dry-run"],
        "forbidden": ["GC_LIVE_DELETE=true", "container-build-push-prod.yml", "DELETE "],
    },
    "B-072": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-072",
        "profiles": ["0 * * * *", "*/15 * * * *"], "sample_count": 1,
        "required": ["SCHEDULED_DRILL_DELIVERY", "correlation", "PagerDuty", "synthetic_page_drills"],
        "safety": ["receiver", "202", "SCHEDULED_DRILL_RECEIVER_DEPLOYED"],
        "forbidden": ["PAGERDUTY_ROUTING_KEY=", "Authorization: Bearer"],
    },
    "B-083": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-083",
        "profiles": ["byok-aws-real"], "sample_count": 1,
        "required": ["/v1/admin/byok/activate", "/v1/admin/byok/deactivate", "check_access", "cas", "run_loop"],
        "safety": ["test-tenant", "fail-closed", "OWNER_APPROVED_BYOK_TEST=1"],
        "forbidden": ["production tenant", "plaintext Tcs", "customer CMK"],
    },
    "B-098": {
        "owner_packet": "docs/handoff/2026-09-05-b098-owner-action-packet.md",
        "profiles": [], "sample_count": 1,
        "required": ["verify_b098_repo_hygiene.py", "--dry-run", "--expected-main-sha"],
        "safety": ["no semver release tag", "--dry-run"],
        "forbidden": ["git tag -a", "git push", "--force"],
    },
    "B-102": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-102",
        "profiles": ["cas:rw"], "sample_count": 3,
        "required": ["/cargo", "1 KiB", "Server-Timing", "third_request_hot"],
        "safety": ["-le 60", "cas:rw", "OWNER_APPROVED_PROBE=1"],
        "forbidden": ["/v1/customer/cas/probe"],
    },
    "B-107": {
        "owner_packet": "docs/handoff/2026-09-05-b103-b129-attribution-owner-packet.md#B-107",
        "profiles": ["ostore", "oaccounting"], "sample_count": 3,
        "required": ["/cargo/", "1 KiB", "Server-Timing", "three sequential", "phase sum", "deployed commit"],
        "safety": ["-le 60", "timeout 25s", "--max-time 20", "Authorization: Bearer", "OWNER_APPROVED_PROBE=1", "CORELINK_DEPLOYED_COMMIT"],
        "forbidden": ["/v1/customer/cas/probe", "X-Server-Timing-Wdb-Detail", "sqlite_master", "DROP ", "DELETE FROM"],
    },
    "B-104": {
        "owner_packet": "docs/handoff/2026-09-05-b103-b129-attribution-owner-packet.md#B-104",
        "profiles": ["authenticated"], "sample_count": 10,
        "required": ["missing", "median", "p90", "version"],
        "safety": ["Authorization: Bearer", "HTTP status", "OWNER_APPROVED_PROBE=1"],
        "forbidden": ["--fail", "maximum"],
    },
    "B-106": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-106",
        "profiles": ["d1", "kv", "l1"], "sample_count": 2,
        "required": ["cold", "sleep 61", "Server-Timing"],
        "safety": ["CORELINK_COLD_PAT", "revocation", "OWNER_APPROVED_PROBE=1"],
        "forbidden": ["TTL", "KV_TTL", "--fail"],
    },
    "B-112": {
        "owner_packet": "docs/campaigns/remediation/work-packages/B091-B130.md#WP-B112",
        "profiles": ["linux", "windows"], "sample_count": 1,
        "required": ["release-cli.yml", "cargo-zigbuild", "checksums", "SLSA", "gh run rerun", "B112_RUN_ID", "B112_EXPECTED_REF", "B112_EXPECTED_SHA", "workflowName", "headBranch", "headSha", "event"],
        "safety": ["--root", "OWNER_APPROVED_RELEASE_RERUN=1"],
        "forbidden": ["git tag", "cosign-sign.yml", "--force"],
    },
    "B-113": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-113",
        "profiles": ["nightly.yml", "sbom.yml", "buck2-starter-ci.yml", "fuzz-nightly.yml", "endurance-2h-nightly.yml", "load-test-nightly.yml", "billing-health-daily.yml"],
        "sample_count": 7,
        "required": ["runner", "conclusion", "terraform-drift.yml"],
        "safety": ["--ref main", "B-111", "OWNER_APPROVED_LANE_DISPATCH=1"],
        "forbidden": ["terraform-drift.yml --dispatch"],
    },
    "B-122": {
        "owner_packet": "docs/handoff/2026-09-05-b103-b129-attribution-owner-packet.md#B-122",
        "profiles": ["B-102", "B-107"], "sample_count": 3,
        "required": ["deployed commit", "ostore", "oaccounting", "phase sum"],
        "safety": ["1 KiB", "-le 60", "timeout 25s", "--max-time 20", "Authorization: Bearer", "OWNER_APPROVED_PROBE=1"],
        "forbidden": ["sqlite_master", "DROP ", "DELETE FROM", "X-Server-Timing-Wdb-Detail", "/v1/customer/cas/probe"],
    },
    "B-125": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-125",
        "profiles": ["audit_outbox"], "sample_count": 1,
        "required": ["D-1-audit-trail", "arrivals", "sealed_rows", "oldest_unsealed", "seal_latency", "512"],
        "safety": ["--remote", "SELECT", "OWNER_APPROVED_READONLY=1"],
        "forbidden": ["AUDIT_DRAIN", "DELETE ", "UPDATE ", "drain --run"],
    },
    "B-127": {
        "owner_packet": "docs/campaigns/remediation/work-packages/B091-B130.md#WP-B127",
        "profiles": ["satisfied", "violated", "unevaluable", "weur"], "sample_count": 1,
        "required": ["LEFT JOIN", "audit_outbox", "tenants", "total_rows", "denominator"],
        "safety": ["--remote", "SELECT", "OWNER_APPROVED_READONLY=1"],
        "forbidden": ["sqlite_master", "DELETE ", "UPDATE ", "DROP "],
    },
    "B-129": {
        "owner_packet": "docs/handoff/2026-09-05-b103-b129-attribution-owner-packet.md#B-129",
        "profiles": ["qtier", "qdo", "qbatch", "qresid", "qcontrol"], "sample_count": 10,
        "required": ["SERVER_TIMING_WDB_DETAIL=on", "deployed commit", "timestamp", "region", "sample", "residual", "<10%", "phase sum", "ohop", "wdb", "origin", "/cargo/"],
        "safety": ["diagnostic", "Authorization: Bearer", "OWNER_APPROVED_PROBE=1", "DEPLOYED_SERVER_TIMING_WDB_DETAIL", "timeout 25s", "--max-time 20", "deadline"],
        "forbidden": ["--fail", "production config", "X-Server-Timing-Wdb-Detail", "/v1/customer/cas/probe"],
    },
    "B-134": {
        "owner_packet": "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json#B-134",
        "profiles": ["smoke-install.yml", "cosign-sign.yml"], "sample_count": 2,
        "required": ["docker info", "image_digest", "Rekor", "webhook", "UNMEASURED", "headBranch", "headSha", "event", "expected_sha"],
        "safety": ["workflow_dispatch", "corelink", "OWNER_APPROVED_B134=1"],
        "forbidden": ["codeql.yml", "secrets-drift.yml", "--force"],
    },
    "B-216": {
        "owner_packet": "docs/internal/b215-b230-runtime-owner-actions.md#B-216",
        "profiles": ["corelink-signup-worker", "corelink-dsr-erasure-dlq"], "sample_count": 1,
        "required": ["dsr.erasure.dead_letter", "requeue_once", "paging", "revision", "priorRequeues < 1", "B216_EXPECTED_REVISION", "B216_EVENT_ID", "event_id", "exhausted", "requeue_count", "delivery_status", "receipt_id"],
        "safety": ["OWNER_APPROVED_B216=1", "timeout 30s"],
        "forbidden": ["wrangler queue send", "wrangler queues delete", "DELETE FROM", "--force"],
    },
    "B-251": {
        "owner_packet": "docs/campaigns/remediation/work-packages/B131-B167.md#WP-B251",
        "profiles": ["deterministic", "D02", "fixture"], "sample_count": 1000,
        "required": ["verify_b251_quota_cas_budget.py", "--ignored", "--exact", "p99", "seed", "failure", "blob", "OBSERVED_SEED", "OBSERVED_FAILURE", "OBSERVED_BLOB", "b251-d02-identity.json", "fixture_only", "production_latency_measured", "InMemoryAtomicQuotaChecker"],
        "safety": ["opt-in", "--nocapture", "B251_ALLOW_IGNORED_PROBE=1"],
        "forbidden": ["--force", "production quota redesign"],
    },
}
SEMANTIC_PACKET_FIELDS = {"owner_packet", "profiles", "sample_count", "required", "safety", "forbidden"}

# Each operation is deliberately unique within its command.  A token-only
# contract could survive removal of the actual probe while retaining a quoted
# profile name or artifact path, so the verifier also requires these concrete
# owner-packet operations.
COMMAND_OPERATIONS: dict[str, tuple[str, ...]] = {
    "B-044": ("timeout 30s", "tail_rc=0"),
    "B-046": ("verify_b046_object_lock_probe.py --probe",),
    "B-063": ("audit-archive-lag.yml --ref main",),
    "B-068": ("gh workflow run real-ignored-harnesses.yml", "active_yaml=\"$(awk"),
    "B-071": ("gh workflow run gc-sweep-dry-run.yml",),
    "B-072": ("verify_b072_scheduled_drills.py",),
    "B-083": ("verify_b083_revocation_wiring.py",),
    "B-098": ("verify_b098_repo_hygiene.py",),
    "B-102": ("$CORELINK_PROD_BASE/cargo",),
    "B-107": ("$CORELINK_PROD_BASE/cargo/${CORELINK_DOGFOOD_TENANT:?}/b107-${ordinal}",),
    "B-104": ("does-not-exist-$ordinal",),
    "B-106": ("$CORELINK_PROD_BASE/v1/customer/keys",),
    "B-112": ("gh run rerun", "run_meta=\"$(gh run view", "expected_sha=\"${B112_EXPECTED_SHA", ".workflowName == \"release-cli\""),
    "B-113": ("gh workflow run \"$workflow\"",),
    "B-125": ("wrangler d1 execute corelink-prod",),
    "B-127": ("wrangler d1 execute corelink-prod",),
    "B-122": ("$CORELINK_PROD_BASE/cargo/${CORELINK_DOGFOOD_TENANT:?}/b122-${ordinal}",),
    "B-129": ("$CORELINK_PROD_BASE/cargo/${CORELINK_DOGFOOD_TENANT:?}/${CORELINK_DOGFOOD_CARGO_KEY:?}",),
    "B-134": ("check_b134_observability.py", "expected_sha=\"$(gh api", ".headSha == $sha"),
    "B-216": ("wrangler tail corelink-signup-worker", "jq -e --arg worker \"corelink-signup-worker\" --arg queue \"corelink-dsr-erasure-dlq\"", "jq -e --arg worker \"corelink-signup-worker\" --arg revision \"$B216_EXPECTED_REVISION\" --arg event_id \"$B216_EVENT_ID\"", "test -s reports/owner-actions/b216-alert-delivery.json", "test -s reports/owner-actions/b216-exhausted-observation.md"),
    "B-251": ("cargo test -p corelink-billing", "jq -e --arg seed", "jq -n '{measurement_mode:\"fixture_only\", production_latency_measured:false}'", "jq -e '.measurement_mode == \"fixture_only\"", "test -s reports/owner-actions/b251-d02-identity.json"),
}

# Frozen from the 42 TL/open records at the D03 starting head.  Do not derive
# this set from the candidate: doing so would make deletion look like closure.
ORIGINAL_TL_OPEN = (
    "B-044", "B-046", "B-216", "B-229", "B-028", "B-029", "B-054", "B-112",
    "B-113", "B-061", "B-063", "B-068", "B-071", "B-072", "B-074", "B-083",
    "B-098", "B-102", "B-103", "B-104", "B-105", "B-106", "B-107", "B-114",
    "B-118", "B-122", "B-125", "B-126", "B-127", "B-128", "B-129", "B-134",
    "B-135", "B-138", "B-139", "B-142", "B-152", "B-250", "B-251", "B-155",
    "B-165", "B-253",
)
EXCLUDED = frozenset(("B-061", "B-126", "B-155"))
GRADUATED = tuple(item for item in ORIGINAL_TL_OPEN if item not in EXCLUDED) + ("B-006",)
GRADUATED_SET = frozenset(GRADUATED)
ORIGINAL_SET = frozenset(ORIGINAL_TL_OPEN)
DONE_SET = frozenset(("B-028", "B-074", "B-253"))
EXCLUDED_FINGERPRINTS = {
    "B-061": "d759a0591e6f867b4245f09512963f2ae10924c7b25cfad02754dc1b323657dc",
    "B-126": "88e2fe5082ad1ad9c393c633c862f947043b378c1fd36393a949b64eab34b089",
    "B-155": "b675f68b3f5c81c0b0cf4bdd48403dbb05f0cb13c865efb295ace6eab829c58c",
}


class GraduationError(ValueError):
    pass


def _json_no_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise GraduationError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _load_packets(text: str) -> dict[str, Any]:
    try:
        value = json.loads(text, object_pairs_hook=_json_no_duplicates)
    except (json.JSONDecodeError, GraduationError) as exc:
        raise GraduationError(f"invalid graduation packet JSON: {exc}") from exc
    if not isinstance(value, dict):
        raise GraduationError("graduation packet root must be an object")
    return value


def _read(path: Path) -> str:
    if not path.is_file() or path.is_symlink():
        raise GraduationError(f"missing/non-regular graduation input: {path}")
    return path.read_text(encoding="utf-8")


def _require_string(mapping: dict[str, Any], key: str, item: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value.strip():
        raise GraduationError(f"{item}: packet field {key!r} is missing or empty")
    return value


def _check_bounded_shell(item: str, command: str, artifact: str) -> None:
    """Reject shell features outside the two bounded owner-command grammars.

    This is deliberately a small allowlist rather than a general shell parser:
    the packet commands are reviewable evidence collectors, not an escape hatch
    for arbitrary owner-provided scripts.  Bash still performs syntax checking,
    while this layer rejects command substitution, unsafe separators/redirects,
    and commands which only smuggle a required token through an inert branch.
    """
    if "\n" in command or "$ (" in command:
        raise GraduationError(f"{item}: command contains an unbounded shell construct")
    if "$(__" in command or "$(" in command or "`" in command:
        raise GraduationError(f"{item}: command substitution is not allowed")
    syntax = subprocess.run(
        ["bash", "-n"], input=command, text=True, capture_output=True, check=False
    )
    if syntax.returncode != 0:
        raise GraduationError(f"{item}: command is not valid bash syntax")
    if command.rstrip().endswith((";", "|", "||", "&&")):
        raise GraduationError(f"{item}: command has a trailing separator")

    try:
        tokens = list(shlex.shlex(command, posix=True, punctuation_chars=";&|><()"))
    except ValueError as exc:
        raise GraduationError(f"{item}: command cannot be tokenized safely") from exc
    forbidden_operators = {"&&", "&", "<<", "<<<", ">>", "<", "&>"}
    if any(token in forbidden_operators for token in tokens):
        raise GraduationError(f"{item}: command contains an unapproved shell operator")
    expected_operators = {
        "B-216": {";": 22, ">": 6, ";;": 2, "||": 1, "|": 1},
        "B-251": {";": 22, ">": 4, "|": 2, ">&": 1},
    }[item]
    operators = Counter(
        token for token in tokens if token in {";", ";;", "||", "|", ">", "<", ">>", ">&", "&&", "&"}
    )
    if dict(operators) != expected_operators:
        raise GraduationError(f"{item}: command separator/redirect shape is not the reviewed bounded form")
    if item == "B-251" and ">" in tokens:
        # B-251 emits its structured envelope exactly once.  Probe output is
        # tee'd into a separate log, so a second redirect cannot hide evidence.
        redirect_targets = [tokens[index + 1] for index, token in enumerate(tokens[:-1]) if token == ">"]
        if redirect_targets != ["/dev/null", artifact, "/dev/null", "/dev/null"]:
            raise GraduationError(f"{item}: redirect is not the declared evidence artifact")
    if item == "B-216" and ">" in tokens:
        redirect_targets = [tokens[index + 1] for index, token in enumerate(tokens[:-1]) if token == ">"]
        if redirect_targets != [artifact, "/dev/null", "/dev/null", artifact, "/dev/null", "/dev/null"]:
            raise GraduationError(f"{item}: redirects must only capture the declared event artifact")
    if item == "B-251" and tokens.count(">&") != 1:
        raise GraduationError(f"{item}: probe stderr must have exactly one bounded 2>&1 capture")
    if item == "B-216" and tokens.count(";;") != 2:
        raise GraduationError(f"{item}: timeout status case must remain structurally bounded")

    allowed = {
        "B-216": {"set", "export", "test", ":", "grep", "tail_rc=0", "timeout", "wrangler", "jq", "case", "exit"},
        "B-251": {"set", "export", "test", ":", "grep", "jq", "tee", "python3", "cargo"},
    }[item]
    control = {";", "||", "|", ")", "(", ";;", "in", "esac", "*"}
    at_command_start = True
    in_case = False
    in_case_pattern = False
    for token in tokens:
        if token == "case":
            in_case = True
            in_case_pattern = True
            at_command_start = False
            continue
        if in_case:
            if token == "esac":
                in_case = False
                in_case_pattern = False
                at_command_start = True
            elif token == ";;":
                in_case_pattern = True
                at_command_start = True
            elif token == ")":
                in_case_pattern = False
                at_command_start = True
            elif in_case_pattern or token in {"in", "|", "*"}:
                continue
            elif at_command_start:
                if token not in allowed:
                    raise GraduationError(f"{item}: unapproved shell command {token!r}")
                at_command_start = False
            continue
        if token in control:
            at_command_start = token not in {")", "(", "in", "*"}
            continue
        if token in {">", ">&"}:
            at_command_start = False
            continue
        if at_command_start:
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", token):
                at_command_start = False
                continue
            if token not in allowed:
                raise GraduationError(f"{item}: unapproved shell command {token!r}")
            at_command_start = False


def _extract_jq_filters(command: str) -> list[tuple[str, bool, dict[str, str]]]:
    """Extract bounded jq filters and their ``-n`` mode from shell tokens."""

    try:
        tokens = shlex.split(command, posix=True)
    except ValueError as exc:
        raise GraduationError("jq filter command cannot be tokenized") from exc
    entries: list[tuple[str, bool, dict[str, str]]] = []
    index = 0
    while index < len(tokens):
        if tokens[index] != "jq":
            index += 1
            continue
        index += 1
        null_input = False
        args: dict[str, str] = {}
        while index < len(tokens):
            token = tokens[index]
            if token in {"-e", "--exit-status"}:
                index += 1
            elif token in {"-n", "--null-input"}:
                null_input = True
                index += 1
            elif token in {"--arg", "--argjson"} and index + 2 < len(tokens):
                args[tokens[index + 1]] = tokens[index + 2]
                index += 3
            elif token.startswith("-"):
                index += 1
            else:
                entries.append((token, null_input, args))
                index += 1
                break
        else:
            raise GraduationError("jq invocation has no executable filter")
    return entries


def _run_jq_filter(
    filter_text: str,
    payload: object | None,
    args: dict[str, str],
    null_input: bool = False,
) -> tuple[int, str]:
    argv = ["jq", "-e"]
    if null_input:
        argv.append("-n")
    for name, value in args.items():
        argv.extend(("--arg", name, value))
    argv.append(filter_text)
    completed = subprocess.run(
        argv,
        input=None if null_input else json.dumps(payload),
        text=True,
        capture_output=True,
        check=False,
        timeout=5,
    )
    return completed.returncode, completed.stdout


def _assert_jq_filter_active(
    label: str,
    filter_text: str,
    positive: dict[str, object],
    all_wrong: dict[str, object],
    args: dict[str, str],
) -> None:
    status, _ = _run_jq_filter(filter_text, positive, args)
    if status != 0:
        raise GraduationError(f"{label}: positive jq fixture did not match")
    for key, value in all_wrong.items():
        negative = dict(positive)
        negative[key] = value
        status, _ = _run_jq_filter(filter_text, negative, args)
        if status == 0:
            raise GraduationError(f"{label}: jq predicate is inert for field {key!r}")


def _assert_jq_envelope(filter_text: str) -> None:
    status, output = _run_jq_filter(filter_text, None, {}, null_input=True)
    if status != 0:
        raise GraduationError("B-251 envelope jq filter did not execute")
    try:
        value = json.loads(output)
    except json.JSONDecodeError as exc:
        raise GraduationError("B-251 envelope jq filter did not emit JSON") from exc
    if value != {"measurement_mode": "fixture_only", "production_latency_measured": False}:
        raise GraduationError("B-251 envelope jq filter is not the declared fixture envelope")


def _check_b216_semantics(command: str) -> None:
    required_fragments = (
        "test -n \"${B216_EXPECTED_REVISION:?provide deployed revision}\"",
        "test -n \"${B216_EVENT_ID:?provide exhausted event ID}\"",
        "test -s reports/owner-actions/b216-deployed-signup-worker.json",
        "test -s reports/owner-actions/b216-alert-delivery.json",
        "test -s reports/owner-actions/b216-exhausted-observation.md",
        "timeout 30s wrangler tail corelink-signup-worker --format json --search dsr.erasure.dead_letter > artifacts/d03/B216-dlq-incident.json",
        "event_id == $event_id",
        "revision == $revision",
        ".channel == \"paging\"",
        ".event == \"dsr.erasure.dead_letter\"",
        "exhausted == true",
        "action == \"requeue_once\"",
        "requeue_count == 1",
        "delivery_status == \"delivered\"",
        "receipt_id|type==\"string\"",
        "event_id=$B216_EVENT_ID revision=$B216_EXPECTED_REVISION exhausted=true action=requeue_once requeue_count=1 operator_disposition=manual_followup",
    )
    for fragment in required_fragments:
        if fragment not in command:
            raise GraduationError(f"B-216: command lacks correlated evidence assertion {fragment!r}")
    if "grep -Eiq" in command or "|" not in command:
        raise GraduationError("B-216: evidence must use structured all-fields correlation, not OR greps")

    filters = _extract_jq_filters(command)
    if len(filters) != 4 or any(entry[1] for entry in filters):
        raise GraduationError("B-216: expected four input-backed jq filters")
    _assert_jq_filter_active(
        "B-216 deployed worker correlation",
        filters[0][0],
        {"worker": "corelink-signup-worker", "queue": "corelink-dsr-erasure-dlq", "revision": "rev-216"},
        {"worker": "wrong-worker", "queue": "wrong-queue", "revision": "wrong-revision"},
        {"worker": "corelink-signup-worker", "queue": "corelink-dsr-erasure-dlq", "revision": "rev-216"},
    )
    _assert_jq_filter_active(
        "B-216 alert correlation",
        filters[1][0],
        {"event_id": "evt-216", "revision": "rev-216", "channel": "paging", "delivery_status": "delivered", "receipt_id": "receipt-216"},
        {"event_id": "wrong-event", "revision": "wrong-revision", "channel": "other", "delivery_status": "failed", "receipt_id": ""},
        {"event_id": "evt-216", "revision": "rev-216"},
    )
    _assert_jq_filter_active(
        "B-216 tail event correlation",
        filters[2][0],
        {"worker": "corelink-signup-worker", "revision": "rev-216", "event": "dsr.erasure.dead_letter", "event_id": "evt-216", "exhausted": True, "action": "requeue_once", "requeue_count": 1},
        {"worker": "wrong-worker", "revision": "wrong-revision", "event": "wrong-event", "event_id": "wrong-id", "exhausted": False, "action": "drop", "requeue_count": 2},
        {"worker": "corelink-signup-worker", "revision": "rev-216", "event_id": "evt-216"},
    )
    _assert_jq_filter_active(
        "B-216 repeated alert correlation",
        filters[3][0],
        {"event_id": "evt-216", "revision": "rev-216", "channel": "paging", "delivery_status": "delivered", "receipt_id": "receipt-216"},
        {"event_id": "wrong-event", "revision": "wrong-revision", "channel": "other", "delivery_status": "failed", "receipt_id": ""},
        {"event_id": "evt-216", "revision": "rev-216", "channel": "paging", "delivery_status": "delivered", "receipt_id": "receipt-216"},
    )


def _check_b251_semantics(command: str, artifact: str) -> None:
    required_fragments = (
        "test -s reports/owner-actions/b251-d02-identity.json",
        "jq -e --arg seed \"$B251_OBSERVED_SEED\" --arg failure \"$B251_OBSERVED_FAILURE\" --arg blob \"$B251_OBSERVED_BLOB\"",
        ".source == \"D02\"",
        ".seed == $seed",
        ".failure == $failure",
        ".blob == $blob",
        "jq -n '{measurement_mode:\"fixture_only\", production_latency_measured:false}' > " + artifact,
        "jq -e '.measurement_mode == \"fixture_only\" and .production_latency_measured == false' " + artifact,
        "artifacts/d03/B251-latency-probe.log",
    )
    for fragment in required_fragments:
        if fragment not in command:
            raise GraduationError(f"B-251: command lacks structural identity/measurement assertion {fragment!r}")
    if "B251_D02_" in command:
        raise GraduationError("B-251: D02 identity must come from the retained artifact, not environment fields")
    if "echo" in command or "grep -Eiq" in command:
        raise GraduationError("B-251: fixture/production truth must be a structured jq assertion")

    filters = _extract_jq_filters(command)
    if len(filters) != 4:
        raise GraduationError("B-251: expected identity, envelope, and repeated truth jq filters")
    identity = next((entry for entry in filters if ".source == \"D02\"" in entry[0]), None)
    envelope = next((entry for entry in filters if entry[1]), None)
    truth = [entry for entry in filters if ".measurement_mode" in entry[0]]
    if identity is None or envelope is None or len(truth) != 2:
        raise GraduationError("B-251: jq filter roles are not structurally present")
    _assert_jq_filter_active(
        "B-251 D02 identity correlation",
        identity[0],
        {"source": "D02", "seed": "seed-251", "failure": "failure-251", "blob": "blob-251"},
        {"source": "wrong-source", "seed": "wrong-seed", "failure": "wrong-failure", "blob": "wrong-blob"},
        {"seed": "seed-251", "failure": "failure-251", "blob": "blob-251"},
    )
    _assert_jq_envelope(envelope[0])
    for index, (filter_text, null_input, _args) in enumerate(truth):
        if null_input:
            raise GraduationError(f"B-251 truth filter {index} unexpectedly uses -n")
        _assert_jq_filter_active(
            f"B-251 fixture/production truth {index}",
            filter_text,
            {"measurement_mode": "fixture_only", "production_latency_measured": False},
            {"measurement_mode": "production", "production_latency_measured": True},
            {},
        )


def _check_command_contract(item: str, packet: dict[str, Any], root: Path) -> None:
    expected = COMMAND_CONTRACTS.get(item)
    if expected is None:
        return
    contract = packet.get("command_contract")
    if not isinstance(contract, dict) or set(contract) != SEMANTIC_PACKET_FIELDS:
        raise GraduationError(f"{item}: command_contract fields are missing or ambiguous")
    if contract != expected:
        raise GraduationError(f"{item}: command_contract disagrees with the authoritative owner packet")
    command = packet["command"]
    if item in {"B-216", "B-251"}:
        _check_bounded_shell(item, command, packet["artifact"])
        if item == "B-216":
            _check_b216_semantics(command)
        else:
            _check_b251_semantics(command, packet["artifact"])
    if "set -euo pipefail" not in command:
        raise GraduationError(f"{item}: command must enable fail-closed shell options")
    if "printf" in command:
        raise GraduationError(f"{item}: assertions must come from tool output, not printf token bait")
    if not re.search(r"\b(?:grep|jq\s+-e|awk|test|case)\b", command):
        raise GraduationError(f"{item}: command has no executable assertion over captured tool output")
    for curl_fragment in re.findall(r"\bcurl\b([^;]*)", command):
        if "--output /dev/null" not in curl_fragment:
            raise GraduationError(f"{item}: curl response body must be discarded from evidence")
    if "wrangler tail" in command and not re.search(r"\btimeout\s+[0-9]+s\s+wrangler tail\b", command):
        raise GraduationError(f"{item}: wrangler tail must have a finite timeout")
    if "gh workflow run" in command:
        if not all(token in command for token in ("run_id", "gh run view", "status", "completed")):
            raise GraduationError(f"{item}: dispatched workflow must correlate its run ID and wait terminal")
    if "gh run rerun" in command and not all(token in command for token in ("run_id", "gh run view", "completed")):
        raise GraduationError(f"{item}: rerun must correlate its run ID and wait terminal")
    marker = f"D03_SAMPLE_COUNT={expected['sample_count']}"
    if marker not in command:
        raise GraduationError(f"{item}: command must carry the explicit {marker} population marker")
    token_aliases = {
        # YAML env syntax is the authoritative workflow operation; accepting
        # the equivalent shell spelling here avoids requiring a second, local
        # flag that could be mistaken for proof about the remote run.
        "GC_LIVE_DELETE=false": ('GC_LIVE_DELETE: "false"',),
        "sleep 61": ("sleep_interval_seconds=61",),
    }
    for token in (*expected["required"], *expected["safety"]):
        if token not in command and not any(alias in command for alias in token_aliases.get(token, ())):
            raise GraduationError(f"{item}: command is missing semantic token {token!r}")
    for operation in COMMAND_OPERATIONS.get(item, ()):
        if operation not in command:
            raise GraduationError(f"{item}: command omits load-bearing owner operation {operation!r}")
    for profile in expected["profiles"]:
        if profile not in command:
            raise GraduationError(f"{item}: command omits required profile/population {profile!r}")
    for token in expected["forbidden"]:
        if token in command:
            raise GraduationError(f"{item}: command contains forbidden unsafe token {token!r}")
    reference = expected["owner_packet"].split("#", 1)[0]
    source = root / reference
    if not source.is_file() or source.is_symlink():
        raise GraduationError(f"{item}: authoritative owner packet is missing/non-regular: {reference}")
    source_text = source.read_text(encoding="utf-8")
    if item not in source_text:
        raise GraduationError(f"{item}: authoritative owner packet does not mention the item")


def _check_packets(packets: dict[str, Any], root: Path = ROOT) -> dict[str, dict[str, Any]]:
    if packets.get("schema_version") != 2:
        raise GraduationError("packet schema_version must be 2")
    if tuple(packets.get("original_tl_open", ())) != ORIGINAL_TL_OPEN:
        raise GraduationError("packet original population is not the frozen 42-item order")
    if tuple(packets.get("graduated_scope", ())) != GRADUATED:
        raise GraduationError("packet graduated scope is not exactly the 39-item lane plus B-006")
    entries = packets.get("packets")
    if not isinstance(entries, dict):
        raise GraduationError("packets must be an object")
    if set(entries) != GRADUATED_SET:
        missing = sorted(GRADUATED_SET - set(entries))
        extra = sorted(set(entries) - GRADUATED_SET)
        raise GraduationError(f"packet population mismatch: missing={missing}, extra={extra}")
    for item in GRADUATED:
        packet = entries[item]
        if not isinstance(packet, dict):
            raise GraduationError(f"{item}: packet must be an object")
        disposition = _require_string(packet, "disposition", item)
        if disposition not in ("DONE", "PARKED"):
            raise GraduationError(f"{item}: unclassified disposition {disposition!r}")
        _require_string(packet, "artifact", item)
        _require_string(packet, "command", item)
        _require_string(packet, "owner", item)
        _require_string(packet, "dependency", item)
        _require_string(packet, "action", item)
        _check_command_contract(item, packet, root)
        if disposition == "PARKED":
            artifact = _require_string(packet, "artifact", item)
            command = _require_string(packet, "command", item)
            if artifact not in command:
                raise GraduationError(f"{item}: gate command does not create declared artifact {artifact}")
            if not re.search(r"(?:>|tee)\s*[^\n;]*" + re.escape(artifact), command):
                raise GraduationError(f"{item}: gate command has no stdout/tee capture for {artifact}")
        if disposition == "DONE":
            _require_string(packet, "evidence", item)
        else:
            if packet.get("verify_means") != "parked":
                raise GraduationError(f"{item}: parked packet must declare verify_means=parked")
    return entries


def _check_owner_packets(
    packets: dict[str, Any], records: dict[str, Any]
) -> dict[str, dict[str, Any]]:
    """Validate owner-owned records without changing the frozen TL lane."""
    population = packets.get("owner_population")
    if tuple(population or ()) != OWNER_POPULATION:
        raise GraduationError("packet owner population is not the closed B-039 scope")
    entries = packets.get("owner_packets")
    if not isinstance(entries, dict) or set(entries) != set(OWNER_POPULATION):
        raise GraduationError("owner packet population is missing or not closed")
    for item in OWNER_POPULATION:
        packet = entries[item]
        if not isinstance(packet, dict) or set(packet) != OWNER_PACKET_FIELDS:
            raise GraduationError(f"{item}: owner packet fields are missing or ambiguous")
        record = records.get(item)
        if record is None:
            raise GraduationError(f"{item}: owner packet has no BACKLOG record")
        raw = record.raw
        if raw.get("owner") != packet["owner"] or raw.get("status") != packet["status"]:
            raise GraduationError(f"{item}: owner packet status disagrees with BACKLOG")
        for field in OWNER_PACKET_FIELDS:
            _require_string(packet, field, item)
        artifact = packet["artifact"]
        command = packet["command"]
        if artifact not in command or not re.search(r">\s*" + re.escape(artifact), command):
            raise GraduationError(f"{item}: owner command does not capture declared artifact")
        if "--offline" in command or OWNER_MIRROR_URL not in command:
            raise GraduationError(f"{item}: owner command must probe the immutable mirror URL")
        if OWNER_MIRROR_SHA256 not in command or "sha256sum" not in command:
            raise GraduationError(f"{item}: owner command must verify the pinned SHA-256")
        if raw.get("action-packet") != str(PACKET_PATH.relative_to(ROOT)):
            raise GraduationError(f"{item}: BACKLOG action-packet wiring is missing")
    return entries


def verify_document(
    root: Path = ROOT,
    *,
    backlog_text: str | None = None,
    packet_text: str | None = None,
    run_guards: bool = True,
    run_gates: bool = True,
) -> dict[str, int]:
    backlog_text = _read(root / "BACKLOG.md") if backlog_text is None else backlog_text
    packet_text = _read(root / PACKET_PATH.relative_to(ROOT)) if packet_text is None else packet_text
    records = parse(backlog_text)
    ids = [record.id for record in records]
    if len(ids) != len(set(ids)):
        raise GraduationError("BACKLOG contains duplicate ids")
    by_id = {record.id: record for record in records}
    if not ORIGINAL_SET.issubset(by_id) or "B-006" not in by_id:
        raise GraduationError("BACKLOG is missing an original TL/open item or B-006")
    # This is a historical D03 graduation gate.  New post-graduation backlog
    # findings are checked by their own records and must not invalidate the
    # closed 42-item D03 population merely because they are engineering-owned.
    remaining_open = sorted(
        record.id for record in records
        if record.id in ORIGINAL_SET
        and record.raw.get("owner") == "tl"
        and record.raw.get("status") == "open"
    )
    if remaining_open:
        raise GraduationError(f"repository still has owner tl/status open: {remaining_open}")
    packet_data = _load_packets(packet_text)
    if tuple(packet_data.get("post_graduation_parked", ())) != POST_GRADUATION_PARKED:
        raise GraduationError("post-graduation parked population is missing or not closed")
    for item in POST_GRADUATION_PARKED:
        record = by_id.get(item)
        if record is None or record.raw.get("owner") != "tl" or record.raw.get("status") != "parked":
            raise GraduationError(f"{item}: post-graduation disposition must remain tl/parked")
    _check_owner_packets(packet_data, by_id)
    packets = _check_packets(packet_data, root)
    packet_done = frozenset(item for item, packet in packets.items() if packet["disposition"] == "DONE")
    if packet_done != DONE_SET:
        raise GraduationError(f"DONE population is not exactly {sorted(DONE_SET)}: {sorted(packet_done)}")

    for item in ORIGINAL_TL_OPEN:
        record = by_id[item]
        data = record.raw
        if data.get("owner") != "tl":
            raise GraduationError(f"{item}: owner changed from tl")
        if item in EXCLUDED:
            if data.get("status") != "done":
                raise GraduationError(f"{item}: excluded lane must be done on the integrated head")
            load_bearing = "\0".join(str(data.get(key, "")) for key in ("status", "verify", "verify-means"))
            fingerprint = hashlib.sha256(load_bearing.encode()).hexdigest()
            if fingerprint != EXCLUDED_FINGERPRINTS[item]:
                raise GraduationError(f"{item}: parent load-bearing fingerprint changed")
            continue
        packet = packets[item]
        status = data.get("status")
        expected = packet["disposition"].lower()
        if status != expected:
            raise GraduationError(f"{item}: BACKLOG status {status!r} disagrees with packet {expected!r}")
        means = str(data.get("verify-means", ""))
        if expected == "done":
            if not means.lstrip().lower().startswith("done —"):
                raise GraduationError(f"{item}: DONE verify-means is not inverted to done")
            if re.search(r"(?im)^\s*(?:open|manual|parked)\b", means):
                raise GraduationError(f"{item}: DONE verify-means contains stale status language")
        else:
            if not means.lstrip().lower().startswith("parked —"):
                raise GraduationError(f"{item}: PARKED verify-means must start with parked —")
            if re.search(r"(?im)^\s*(?:open|manual)\b", means):
                raise GraduationError(f"{item}: PARKED verify-means contains stale open/manual language")

    b006 = by_id["B-006"]
    if b006.raw.get("owner") != "tl" or b006.raw.get("status") != "parked":
        raise GraduationError("B-006 must remain owner tl / parked")
    if not str(b006.raw.get("verify-means", "")).lstrip().lower().startswith("parked —"):
        raise GraduationError("B-006 verify-means must be truthful parked language")
    if packets["B-129"]["disposition"] != "PARKED" or "<10%" not in packets["B-129"]["evidence"]:
        raise GraduationError("B-129 must remain parked until production residual is <10%")

    if run_guards:
        commands = (
            ("B-028", (sys.executable, "scripts/verify_b028_dependabot.py")),
            ("B-074", (sys.executable, "scripts/verify_b074_money_path_auth.py", "--self-test")),
            ("B-253", (sys.executable, "-m", "pytest", "-q", "tests/test_b253_openapi_version.py")),
        )
        for item, command in commands:
            result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=120)
            if result.returncode != 0:
                raise GraduationError(f"{item}: inverted guard failed: {result.stdout}{result.stderr}")
    if run_gates:
        _run_parked_gates(root, by_id)
    return {"original": len(ORIGINAL_TL_OPEN), "graduated": len(GRADUATED), "done": 3, "parked": len(GRADUATED) - 3}


_OFFLINE_ONLY_MARKERS = ("manual", "gh ", "gh\\n", "wrangler", "cargo ")
_OFFLINE_ONLY_IDS = frozenset((
    "B-028", "B-083", "B-098", "B-112", "B-129", "B-216", "B-229", "B-250",
))


def _run_parked_gates(root: Path, records: dict[str, Any]) -> None:
    """Run each parked gate once, replacing external actions with its offline guard."""
    for item in GRADUATED:
        if records[item].raw.get("status") != "parked":
            continue
        declared = str(records[item].raw.get("verify", ""))
        if item in _OFFLINE_ONLY_IDS or any(marker in declared for marker in _OFFLINE_ONLY_MARKERS):
            command = (sys.executable, "scripts/verify_d03_parked_gate.py", "--id", item)
        else:
            command = (sys.executable, "scripts/backlog_verify.py", "--id", item, "--format", "json")
        environment = dict(os.environ)
        environment["D03_GRADUATION_NESTED"] = "1"
        environment["D03_GRADUATION_OFFLINE"] = "1"
        try:
            result = subprocess.run(
                command, cwd=root, env=environment, capture_output=True, text=True, timeout=120
            )
        except subprocess.TimeoutExpired as exc:
            raise GraduationError(f"{item}: parked gate exceeded 120s") from exc
        if result.returncode != 0:
            output = (result.stdout + result.stderr).strip()[-1000:]
            raise GraduationError(f"{item}: parked gate failed rc={result.returncode}: {output}")


def self_test(root: Path = ROOT) -> None:
    backlog = _read(root / "BACKLOG.md")
    packets = _read(root / PACKET_PATH.relative_to(ROOT))
    cases = (
        ("status-open", backlog.replace("id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: parked", "id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: open", 1), packets),
        ("stale-means", backlog.replace("verify-means: |\n  parked —", "verify-means: |\n  open —", 1), packets),
        ("packet-command-removed", backlog, packets.replace('"command":"', '"command_removed":"', 1)),
        ("packet-unclassified", backlog, packets.replace('"disposition":"PARKED"', '"disposition":"UNKNOWN"', 1)),
        ("post-graduation-omitted", backlog, packets.replace('  "post_graduation_parked": ["B-210"],\n', "", 1)),
        ("fake-done", backlog.replace("id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: parked", "id: B-044\nrepo: corelink-runners\nowner: tl\nstatus: done", 1), packets.replace('"B-044": {"disposition":"PARKED"', '"B-044": {"disposition":"DONE"', 1)),
        (
            "artifact-capture-removed",
            backlog,
            packets.replace(" : > artifacts/d03/B102-server-timing.json", "", 1).replace(
                " | tee -a artifacts/d03/B102-server-timing.json", "", 3
            ).replace(" >> artifacts/d03/B102-server-timing.json", "", 1),
        ),
    )
    for name, mutated_backlog, mutated_packets in cases:
        try:
            verify_document(root, backlog_text=mutated_backlog, packet_text=mutated_packets, run_guards=False, run_gates=False)
        except GraduationError:
            continue
        raise GraduationError(f"mutation unexpectedly passed: {name}")
    for item in EXCLUDED:
        marker = f"id: {item}"
        start = backlog.index(marker)
        end = backlog.index("```", start)
        original = backlog[start:end]
        mutated = backlog[:start] + original.replace("verify-means:", "verify-means: MUTATED ", 1) + backlog[end:]
        try:
            verify_document(root, backlog_text=mutated, packet_text=packets, run_guards=False, run_gates=False)
        except GraduationError:
            continue
        raise GraduationError(f"mutation unexpectedly passed: {item} verify-means")
    owner_data = _load_packets(packets)
    for name, mutation in (
        ("owner-packet-missing", lambda data: data["owner_packets"].pop("B-039")),
        ("owner-status-missing", lambda data: data["owner_packets"]["B-039"].pop("status")),
        (
            "owner-offline-command",
            lambda data: data["owner_packets"]["B-039"].update(
                command="python3 scripts/verify_b155_owned.py --id B-039 --expect open --offline > artifacts/d03/B039-mirror-availability.json"
            ),
        ),
    ):
        mutated = copy.deepcopy(owner_data)
        mutation(mutated)
        try:
            verify_document(
                root,
                packet_text=json.dumps(mutated),
                run_guards=False,
                run_gates=False,
            )
        except GraduationError:
            continue
        raise GraduationError(f"mutation unexpectedly passed: {name}")

    # Every flagged parked command has a population/safety contract.  Kill one
    # load-bearing safety token, declared count, and concrete owner operation
    # per item; a verifier that checks only artifact redirection must not accept
    # any of these mutants.
    for item, contract in COMMAND_CONTRACTS.items():
        mutations = [
            ("semantic-token-removed", lambda data, token=contract["safety"][0]: data["packets"][item].update(command=data["packets"][item]["command"].replace(token, ""))),
            ("semantic-count-mutated", lambda data: data["packets"][item]["command_contract"].update(sample_count=contract["sample_count"] + 1)),
        ]
        mutations.extend(
            (f"load-bearing-operation-removed:{operation}", lambda data, operation=operation: data["packets"][item].update(command=data["packets"][item]["command"].replace(operation, "", 1)))
            for operation in COMMAND_OPERATIONS[item]
        )
        for name, mutation in mutations:
            mutated = copy.deepcopy(owner_data)
            # Start from the full graduation document, not only owner packets.
            full = _load_packets(packets)
            mutation(full)
            try:
                verify_document(
                    root,
                    packet_text=json.dumps(full),
                    run_guards=False,
                    run_gates=False,
                )
            except GraduationError:
                continue
            raise GraduationError(f"mutation unexpectedly passed: {item} {name}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--no-guards", action="store_true")
    parser.add_argument("--schema-only", action="store_true")
    args = parser.parse_args(argv)
    try:
        nested = bool(os.environ.get("D03_GRADUATION_NESTED"))
        result = verify_document(
            run_guards=not args.no_guards and not args.schema_only and not nested,
            run_gates=not args.schema_only and not nested,
        )
        if args.self_test:
            self_test()
        print(f"D03 graduation: PASS; original={result['original']} graduated={result['graduated']} done={result['done']} parked={result['parked']}")
        return 0
    except (GraduationError, OSError, subprocess.SubprocessError) as exc:
        print(f"D03 graduation FAIL: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
