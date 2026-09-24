#!/usr/bin/env python3
"""Fail-closed inventory, shard receipt, and aggregate checks for #2457.

The workflow is deliberately a thin transport.  This module owns the data
contract so a later Actions retry cannot turn an incomplete mutation campaign
into a green aggregate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Iterable, Mapping


SHARD_COUNT = 27
SHARDING = "round-robin"
TOOL_VERSION = "27.0.0"
INVENTORY_SCHEMA = "corelink.hosted-mutants-inventory.v2"
BASELINE_SCHEMA = "corelink.hosted-mutants-baseline-receipt.v2"
SHARD_SCHEMA = "corelink.hosted-mutants-shard-receipt.v2"
AGGREGATE_SCHEMA = "corelink.hosted-mutants-aggregate-receipt.v2"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
DIGEST_RE = re.compile(r"^[0-9a-f]{64}$")
CANONICAL_ARGUMENTS = (
    "--workspace",
    "--no-shuffle",
    "--minimum-test-timeout=600",
    "--sharding=round-robin",
)


class VerificationError(ValueError):
    """A condition that must make the campaign red."""


def canonical_bytes(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def digest(value: Any) -> str:
    return hashlib.sha256(canonical_bytes(value)).hexdigest()


def config_digest() -> str:
    return digest(
        {
            "cargo_mutants_version": TOOL_VERSION,
            "arguments": CANONICAL_ARGUMENTS,
            "shard_count": SHARD_COUNT,
            "sharding": SHARDING,
        }
    )


def require_string(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value:
        raise VerificationError(f"{field} must be a non-empty string")
    return value


def require_sha(value: Any, field: str) -> str:
    value = require_string(value, field)
    if not SHA_RE.fullmatch(value):
        raise VerificationError(f"{field} must be a 40-character lowercase SHA")
    return value


def require_digest(value: Any, field: str) -> str:
    value = require_string(value, field)
    if not DIGEST_RE.fullmatch(value):
        raise VerificationError(f"{field} must be a sha256 digest")
    return value


def require_attempt(value: Any, field: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 1:
        raise VerificationError(f"{field} must be a positive integer")
    return value


def mutant_ids(raw: Any) -> list[str]:
    if not isinstance(raw, list):
        raise VerificationError("cargo-mutants list must be a JSON array")
    ids: list[str] = []
    for index, item in enumerate(raw):
        if not isinstance(item, dict):
            raise VerificationError(f"mutant[{index}] must be an object")
        name = item.get("name")
        if not isinstance(name, str) or not name:
            raise VerificationError(f"mutant[{index}].name must be a non-empty string")
        ids.append(name)
    if not ids:
        raise VerificationError("full workspace mutant inventory must not be empty")
    if len(ids) != len(set(ids)):
        raise VerificationError("full workspace mutant inventory has duplicate identities")
    return ids


def inventory_mutant_ids(value: Any) -> list[str]:
    """Validate the redacted identity list stored in an inventory receipt."""
    if not isinstance(value, list):
        raise VerificationError("inventory mutant_ids must be a JSON array")
    if not value or any(not isinstance(name, str) or not name for name in value):
        raise VerificationError("inventory mutant_ids must contain non-empty string identities")
    if len(value) != len(set(value)):
        raise VerificationError("inventory mutant_ids has duplicate identities")
    return value


def membership(ids: list[str], shard: int) -> list[str]:
    if shard < 0 or shard >= SHARD_COUNT:
        raise VerificationError("shard index is outside the frozen denominator")
    return [name for index, name in enumerate(ids) if index % SHARD_COUNT == shard]


def make_inventory(raw: Any, sha: str, run_id: str, attempt: int) -> dict[str, Any]:
    ids = mutant_ids(raw)
    return {
        "schema": INVENTORY_SCHEMA,
        "run_id": require_string(run_id, "run_id"),
        "run_attempt": require_attempt(attempt, "run_attempt"),
        "sha": require_sha(sha, "sha"),
        "tool_version": TOOL_VERSION,
        "config_digest": config_digest(),
        "shard_count": SHARD_COUNT,
        "sharding": SHARDING,
        "mutant_ids": ids,
        "inventory_digest": digest(ids),
    }


def validate_inventory(document: Mapping[str, Any]) -> list[str]:
    if document.get("schema") != INVENTORY_SCHEMA:
        raise VerificationError("unsupported inventory schema")
    require_string(document.get("run_id"), "inventory.run_id")
    require_attempt(document.get("run_attempt"), "inventory.run_attempt")
    require_sha(document.get("sha"), "inventory.sha")
    if document.get("tool_version") != TOOL_VERSION:
        raise VerificationError("inventory tool version is not pinned cargo-mutants 27.0.0")
    if document.get("config_digest") != config_digest():
        raise VerificationError("inventory configuration digest is not canonical")
    if document.get("shard_count") != SHARD_COUNT or document.get("sharding") != SHARDING:
        raise VerificationError("inventory shard definition drifted")
    ids = inventory_mutant_ids(document.get("mutant_ids"))
    if document.get("inventory_digest") != digest(ids):
        raise VerificationError("inventory digest does not bind its identities")
    return ids


def validate_baseline(receipt: Mapping[str, Any], inventory: Mapping[str, Any]) -> None:
    if receipt.get("schema") != BASELINE_SCHEMA:
        raise VerificationError("unsupported baseline receipt schema")
    for field in ("run_id", "sha", "tool_version", "config_digest", "inventory_digest"):
        if receipt.get(field) != inventory.get(field):
            raise VerificationError(f"baseline {field} does not bind the inventory")
    require_attempt(receipt.get("run_attempt"), "baseline.run_attempt")
    require_digest(receipt.get("baseline_digest"), "baseline.baseline_digest")
    if receipt.get("status") != "success" or receipt.get("exit_code") != 0:
        raise VerificationError("unmutated baseline did not succeed")


def validate_shard_receipt(receipt: Mapping[str, Any], inventory: Mapping[str, Any]) -> int:
    if receipt.get("schema") != SHARD_SCHEMA:
        raise VerificationError("unsupported shard receipt schema")
    for field in ("run_id", "sha", "tool_version", "config_digest", "inventory_digest"):
        if receipt.get(field) != inventory.get(field):
            raise VerificationError(f"shard {field} does not bind the inventory")
    attempt = require_attempt(receipt.get("run_attempt"), "shard.run_attempt")
    shard = receipt.get("shard")
    if not isinstance(shard, dict):
        raise VerificationError("shard identity is missing")
    index = shard.get("index")
    total = shard.get("total")
    if isinstance(index, bool) or not isinstance(index, int) or index < 0 or index >= SHARD_COUNT:
        raise VerificationError("shard index is invalid")
    if total != SHARD_COUNT or shard.get("sharding") != SHARDING:
        raise VerificationError("shard denominator or algorithm drifted")
    ids = validate_inventory(inventory)
    expected = membership(ids, index)
    observed = receipt.get("mutant_ids")
    if observed != expected:
        raise VerificationError(f"shard {index} identities are not the expected deterministic membership")
    if receipt.get("membership_digest") != digest(expected):
        raise VerificationError(f"shard {index} membership digest is invalid")
    require_digest(receipt.get("baseline_digest"), "shard.baseline_digest")
    require_digest(receipt.get("evidence_digest"), "shard.evidence_digest")
    require_digest(receipt.get("artifact_digest"), "shard.artifact_digest")
    if receipt.get("evidence_present") is not True:
        raise VerificationError(f"shard {index} evidence artifact is missing")
    if receipt.get("status") != "success" or receipt.get("cargo_exit_code") != 0:
        raise VerificationError(f"shard {index} did not complete successfully")
    if receipt.get("outcomes_complete") is not True:
        raise VerificationError(f"shard {index} does not prove complete outcomes")
    return index


def _latest_by_attempt(receipts: Iterable[Mapping[str, Any]], label: str) -> Mapping[str, Any]:
    candidates = list(receipts)
    if not candidates:
        raise VerificationError(f"missing {label} receipt")
    attempts: dict[int, Mapping[str, Any]] = {}
    for receipt in candidates:
        attempt = require_attempt(receipt.get("run_attempt"), f"{label}.run_attempt")
        if attempt in attempts:
            raise VerificationError(f"duplicate {label} receipt for attempt {attempt}")
        attempts[attempt] = receipt
    return attempts[max(attempts)]


def aggregate_receipts(
    inventory_receipts: Iterable[Mapping[str, Any]],
    baseline_receipts: Iterable[Mapping[str, Any]],
    shard_receipts: Iterable[Mapping[str, Any]],
    expected_sha: str,
    current_run_id: str,
    current_attempt: int,
    downloaded_shard_digests: Mapping[tuple[int, int], Mapping[str, str]] | None = None,
) -> dict[str, Any]:
    """Select latest same-run attempt evidence and prove a complete 27-way union."""
    expected_sha = require_sha(expected_sha, "expected_sha")
    current_run_id = require_string(current_run_id, "current_run_id")
    current_attempt = require_attempt(current_attempt, "current_attempt")
    inventory = _latest_by_attempt(inventory_receipts, "inventory")
    ids = validate_inventory(inventory)
    if inventory.get("sha") != expected_sha or inventory.get("run_id") != current_run_id:
        raise VerificationError("inventory is not from the exact current campaign")
    if inventory["run_attempt"] > current_attempt:
        raise VerificationError("inventory is from a future attempt")
    baseline = _latest_by_attempt(baseline_receipts, "baseline")
    validate_baseline(baseline, inventory)
    if baseline["run_attempt"] > current_attempt:
        raise VerificationError("baseline is from a future attempt")

    by_shard: dict[int, list[Mapping[str, Any]]] = defaultdict(list)
    for receipt in shard_receipts:
        shard = receipt.get("shard")
        if not isinstance(shard, dict) or not isinstance(shard.get("index"), int):
            raise VerificationError("shard receipt lacks a usable shard index")
        by_shard[shard["index"]].append(receipt)
    if set(by_shard) != set(range(SHARD_COUNT)):
        missing = sorted(set(range(SHARD_COUNT)) - set(by_shard))
        unexpected = sorted(set(by_shard) - set(range(SHARD_COUNT)))
        raise VerificationError(f"expected all 27 shard receipts; missing={missing}, unexpected={unexpected}")

    selected: list[Mapping[str, Any]] = []
    aggregate_artifacts: list[dict[str, Any]] = []
    for index in range(SHARD_COUNT):
        receipt = _latest_by_attempt(by_shard[index], f"shard {index}")
        if receipt.get("run_attempt", 0) > current_attempt:
            raise VerificationError(f"shard {index} is from a future attempt")
        if receipt.get("baseline_digest") != baseline.get("baseline_digest"):
            raise VerificationError(f"shard {index} is bound to another baseline")
        if validate_shard_receipt(receipt, inventory) != index:
            raise VerificationError(f"shard {index} identity drifted")
        if downloaded_shard_digests is not None:
            key = (index, receipt["run_attempt"])
            downloaded = downloaded_shard_digests.get(key)
            if downloaded is None:
                raise VerificationError(f"downloaded evidence is missing for shard {index} attempt {receipt['run_attempt']}")
            if downloaded.get("evidence_digest") != receipt["evidence_digest"]:
                raise VerificationError(f"downloaded evidence bytes do not match shard {index} receipt digest")
            if downloaded.get("artifact_digest") != receipt["artifact_digest"]:
                raise VerificationError(f"downloaded artifact bytes do not match shard {index} receipt digest")
            aggregate_artifacts.append(
                {
                    "index": index,
                    "run_attempt": receipt["run_attempt"],
                    "evidence_digest": downloaded["evidence_digest"],
                    "artifact_digest": downloaded["artifact_digest"],
                }
            )
        selected.append(receipt)

    union = [mutant for receipt in selected for mutant in receipt["mutant_ids"]]
    if len(union) != len(set(union)):
        raise VerificationError("shard union contains duplicate mutant identities")
    if set(union) != set(ids) or len(union) != len(ids):
        raise VerificationError("shard union is not the complete unsharded inventory")
    lineage = sorted({inventory["run_attempt"], baseline["run_attempt"], *(item["run_attempt"] for item in selected)})
    return {
        "schema": AGGREGATE_SCHEMA,
        "run_id": current_run_id,
        "run_attempt": current_attempt,
        "sha": expected_sha,
        "tool_version": TOOL_VERSION,
        "config_digest": config_digest(),
        "inventory_digest": inventory["inventory_digest"],
        "baseline_digest": baseline["baseline_digest"],
        "shard_count": SHARD_COUNT,
        "sharding": SHARDING,
        "covered_mutants": len(union),
        "coverage_digest": digest(sorted(union)),
        "shard_artifact_digests": aggregate_artifacts,
        "attempt_lineage": lineage,
        "status": "success",
    }


def write_json(path: Path, value: Mapping[str, Any]) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def directory_digest(root: Path) -> str:
    if not root.is_dir():
        raise VerificationError(f"evidence directory is missing: {root}")
    records: list[dict[str, str]] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        records.append({"path": path.relative_to(root).as_posix(), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    if not records:
        raise VerificationError("evidence directory is empty")
    return digest(records)


def shard_artifact_digest(expected: Path, evidence: Path) -> str:
    """Digest each uploaded shard payload byte, excluding the self-referential receipt."""
    records: list[dict[str, str]] = []
    if expected.is_file():
        records.append({"path": "expected-shard.json", "sha256": hashlib.sha256(expected.read_bytes()).hexdigest()})
    if evidence.is_dir():
        for path in sorted(item for item in evidence.rglob("*") if item.is_file()):
            records.append(
                {
                    "path": f"mutants.out/{path.relative_to(evidence).as_posix()}",
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                }
            )
    return digest(records)


def command_build_inventory(args: argparse.Namespace) -> None:
    raw = read_json(args.raw)
    receipt = make_inventory(raw, args.sha, args.run_id, args.run_attempt)
    write_json(args.out, receipt)


def command_write_baseline(args: argparse.Namespace) -> None:
    inventory = read_json(args.inventory)
    validate_inventory(inventory)
    receipt = {
        "schema": BASELINE_SCHEMA,
        "run_id": args.run_id,
        "run_attempt": args.run_attempt,
        "sha": args.sha,
        "tool_version": TOOL_VERSION,
        "config_digest": config_digest(),
        "inventory_digest": inventory["inventory_digest"],
        "baseline_digest": digest(
            {"sha": args.sha, "run_id": args.run_id, "run_attempt": args.run_attempt, "command": "cargo test --workspace --locked"}
        ),
        "status": "success" if args.exit_code == 0 else "failure",
        "exit_code": args.exit_code,
    }
    write_json(args.out, receipt)


def command_write_shard(args: argparse.Namespace) -> None:
    inventory = read_json(args.inventory)
    baseline = read_json(args.baseline)
    ids = validate_inventory(inventory)
    validate_baseline(baseline, inventory)
    expected_ids = membership(ids, args.shard)
    if args.exit_code == 0:
        if not args.expected.is_file() or not args.observed.is_file():
            raise VerificationError("successful shard lacks list/output inventory")
        expected = mutant_ids(read_json(args.expected))
        observed = mutant_ids(read_json(args.observed))
        if expected != expected_ids or observed != expected_ids:
            raise VerificationError("cargo-mutants shard list/output disagrees with frozen inventory")
    evidence = Path(args.evidence)
    outcomes = evidence / "outcomes.json"
    evidence_present = evidence.is_dir()
    evidence_digest = directory_digest(evidence) if evidence_present else digest([])
    receipt = {
        "schema": SHARD_SCHEMA,
        "run_id": args.run_id,
        "run_attempt": args.run_attempt,
        "sha": args.sha,
        "tool_version": TOOL_VERSION,
        "config_digest": config_digest(),
        "inventory_digest": inventory["inventory_digest"],
        "baseline_digest": baseline["baseline_digest"],
        "shard": {"index": args.shard, "total": SHARD_COUNT, "sharding": SHARDING},
        "mutant_ids": expected_ids,
        "membership_digest": digest(expected_ids),
        "evidence_digest": evidence_digest,
        "artifact_digest": shard_artifact_digest(args.expected, evidence),
        "evidence_present": evidence_present,
        "status": "success" if args.exit_code == 0 else "failure",
        "cargo_exit_code": args.exit_code,
        "outcomes_complete": args.exit_code == 0 and outcomes.is_file(),
    }
    write_json(args.out, receipt)


def command_aggregate(args: argparse.Namespace) -> None:
    root = args.artifacts
    inventories = [read_json(path) for path in root.rglob("inventory-manifest.json")]
    baselines = [read_json(path) for path in root.rglob("baseline-receipt.json")]
    shard_paths = list(root.rglob("shard-receipt.json"))
    shards = [read_json(path) for path in shard_paths]
    downloaded: dict[tuple[int, int], dict[str, str]] = {}
    for receipt_path, shard in zip(shard_paths, shards, strict=True):
        shard_identity = shard.get("shard")
        if not isinstance(shard_identity, dict):
            raise VerificationError("downloaded shard receipt lacks shard identity")
        index = shard_identity.get("index")
        if isinstance(index, bool) or not isinstance(index, int):
            raise VerificationError("downloaded shard receipt has invalid shard index")
        attempt = require_attempt(shard.get("run_attempt"), "downloaded shard.run_attempt")
        key = (index, attempt)
        if key in downloaded:
            raise VerificationError(f"duplicate downloaded shard artifact for shard {index} attempt {attempt}")
        artifact_root = receipt_path.parent
        # upload-artifact preserves the least-common-ancestor relative path.
        # The two receipt files are direct children, while the raw payload was
        # uploaded from `${RUNNER_TEMP}/mutants-shard-N/mutants.out`.
        evidence = artifact_root / f"mutants-shard-{index}" / "mutants.out"
        expected = artifact_root / "expected-shard.json"
        downloaded[key] = {
            "evidence_digest": directory_digest(evidence) if evidence.is_dir() else digest([]),
            "artifact_digest": shard_artifact_digest(expected, evidence),
        }
    receipt = aggregate_receipts(
        inventories,
        baselines,
        shards,
        args.sha,
        args.run_id,
        args.run_attempt,
        downloaded,
    )
    write_json(args.out, receipt)


def command_select_latest(args: argparse.Namespace) -> None:
    receipts = [read_json(path) for path in args.artifacts.rglob(args.filename)]
    receipt = _latest_by_attempt(receipts, args.filename)
    write_json(args.out, receipt)


def command_verify_workflow(args: argparse.Namespace) -> None:
    text = args.workflow.read_text(encoding="utf-8")
    required = (
        "workflow_dispatch:",
        "contents: read",
        "actions: read",
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "SHARD_COUNT: 27",
        "max-parallel: 9",
        "timeout-minutes: 45",
        "cargo mutants --workspace --no-shuffle --minimum-test-timeout=600 --sharding=round-robin --list --json",
        "--shard ${{ matrix.shard }}/27",
        "--baseline=skip",
        "cargo test --workspace --locked",
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c",
        "verify_i2457_mutants_shards.py aggregate",
        "retention-days: 30",
        "cancel-in-progress: false",
    )
    missing = [marker for marker in required if marker not in text]
    if missing:
        raise VerificationError("workflow is missing required #2457 controls: " + ", ".join(missing))
    if text.count("retention-days: 30") != 4 or text.count("retention-days:") != 4:
        raise VerificationError("workflow must retain each of the four bounded evidence artifacts for exactly 30 days")
    forbidden = (
        "self-hosted",
        "runs-on: corelink",
        "git push",
        "gh issue",
        "deploy",
        "publish",
        "schedule:",
        "pull_request:",
        "push:",
        "--baseline=run",
        "--sharding=slice",
    )
    found = [token for token in forbidden if token in text.lower()]
    if found:
        raise VerificationError("workflow contains forbidden execution or mutation controls: " + ", ".join(found))


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    commands = result.add_subparsers(dest="command", required=True)
    inventory = commands.add_parser("build-inventory")
    inventory.add_argument("--raw", type=Path, required=True)
    inventory.add_argument("--sha", required=True)
    inventory.add_argument("--run-id", required=True)
    inventory.add_argument("--run-attempt", type=int, required=True)
    inventory.add_argument("--out", type=Path, required=True)
    inventory.set_defaults(func=command_build_inventory)
    baseline = commands.add_parser("write-baseline")
    baseline.add_argument("--inventory", type=Path, required=True)
    baseline.add_argument("--sha", required=True)
    baseline.add_argument("--run-id", required=True)
    baseline.add_argument("--run-attempt", type=int, required=True)
    baseline.add_argument("--exit-code", type=int, required=True)
    baseline.add_argument("--out", type=Path, required=True)
    baseline.set_defaults(func=command_write_baseline)
    shard = commands.add_parser("write-shard")
    shard.add_argument("--inventory", type=Path, required=True)
    shard.add_argument("--baseline", type=Path, required=True)
    shard.add_argument("--expected", type=Path, required=True)
    shard.add_argument("--observed", type=Path, required=True)
    shard.add_argument("--evidence", type=Path, required=True)
    shard.add_argument("--sha", required=True)
    shard.add_argument("--run-id", required=True)
    shard.add_argument("--run-attempt", type=int, required=True)
    shard.add_argument("--shard", type=int, required=True)
    shard.add_argument("--exit-code", type=int, required=True)
    shard.add_argument("--out", type=Path, required=True)
    shard.set_defaults(func=command_write_shard)
    aggregate = commands.add_parser("aggregate")
    aggregate.add_argument("--artifacts", type=Path, required=True)
    aggregate.add_argument("--sha", required=True)
    aggregate.add_argument("--run-id", required=True)
    aggregate.add_argument("--run-attempt", type=int, required=True)
    aggregate.add_argument("--out", type=Path, required=True)
    aggregate.set_defaults(func=command_aggregate)
    select = commands.add_parser("select-latest")
    select.add_argument("--artifacts", type=Path, required=True)
    select.add_argument("--filename", required=True)
    select.add_argument("--out", type=Path, required=True)
    select.set_defaults(func=command_select_latest)
    workflow = commands.add_parser("verify-workflow")
    workflow.add_argument("--workflow", type=Path, required=True)
    workflow.set_defaults(func=command_verify_workflow)
    return result


def main() -> int:
    args = parser().parse_args()
    try:
        args.func(args)
    except (OSError, json.JSONDecodeError, VerificationError) as error:
        print(f"#2457 mutants shards rejected: {error}", file=sys.stderr)
        return 1
    print("#2457 mutants shards: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
