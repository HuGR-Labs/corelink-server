#!/usr/bin/env python3
"""Verify #1863/#1948 bounded hosted mutation aggregate receipts."""

import argparse
import json
import re
from pathlib import Path
from typing import Any, Mapping

from scripts.verify_i1666_mutants_evidence import verify as verify_workflow
from scripts.verify_i2457_mutants_shards import AGGREGATE_SCHEMA, SHARD_COUNT, TOOL_VERSION, config_digest, digest


WORKFLOW = Path(".github/workflows/issue-1863-mutants-hosted.yml")
LEGACY_WORKFLOW = Path(".github/workflows/nightly.yml")
BACKLOG = Path("BACKLOG.md")
OWNER_PACKET = Path("docs/internal/b113-six-lanes-owner-actions.md")
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
LEGACY_DISABLED_IF = "if: github.event_name == 'schedule' && github.event_name == 'workflow_dispatch'"


def validate_successful_receipt(
    receipt: Mapping[str, Any], run: Mapping[str, Any], expected_sha: str
) -> None:
    """Reject aggregates missing a complete, successful exact-SHA campaign."""
    run_id = run.get("databaseId", run.get("id"))
    attempt = run.get("attempt", run.get("run_attempt"))
    head_sha = run.get("headSha", run.get("head_sha"))
    head_branch = run.get("headBranch", run.get("head_branch"))
    if not SHA_RE.fullmatch(expected_sha or ""):
        raise ValueError("expected SHA is invalid")
    if run.get("status") != "completed" or run.get("conclusion") != "success":
        raise ValueError("hosted mutants run did not complete successfully")
    if receipt.get("schema") != AGGREGATE_SCHEMA or receipt.get("status") != "success":
        raise ValueError("hosted mutants aggregate receipt is unsupported or not successful")
    if run_id is None or str(receipt.get("run_id")) != str(run_id):
        raise ValueError("aggregate receipt run_id does not match the GitHub run")
    if attempt is None or receipt.get("run_attempt") != attempt:
        raise ValueError("aggregate receipt run_attempt does not match the GitHub run")
    if receipt.get("sha") != expected_sha or head_sha != expected_sha:
        raise ValueError("aggregate receipt SHA does not match the expected exact SHA")
    if head_branch != "main":
        raise ValueError("hosted mutants aggregate is not from protected main")
    if receipt.get("shard_count") != SHARD_COUNT or receipt.get("covered_mutants", 0) <= 0:
        raise ValueError("aggregate receipt does not prove all 27 nonempty shards")
    if receipt.get("tool_version") != TOOL_VERSION:
        raise ValueError("aggregate receipt tool version is not pinned cargo-mutants 27.0.0")
    if receipt.get("config_digest") != config_digest():
        raise ValueError("aggregate receipt configuration digest is not canonical")
    for field in ("config_digest", "inventory_digest", "baseline_digest", "coverage_digest"):
        value = receipt.get(field)
        if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
            raise ValueError(f"aggregate receipt lacks a valid {field}")
    lineage = receipt.get("attempt_lineage")
    if not isinstance(lineage, list) or not lineage or any(not isinstance(item, int) or item < 1 or item > attempt for item in lineage):
        raise ValueError("aggregate receipt attempt lineage is invalid")
    artifacts = receipt.get("shard_artifact_digests")
    if not isinstance(artifacts, list) or len(artifacts) != SHARD_COUNT:
        raise ValueError("aggregate receipt lacks all per-shard artifact digests")
    seen: set[int] = set()
    for item in artifacts:
        if not isinstance(item, dict) or not isinstance(item.get("index"), int) or item["index"] in seen:
            raise ValueError("aggregate receipt has invalid per-shard artifact identity")
        if item["index"] < 0 or item["index"] >= SHARD_COUNT or not isinstance(item.get("run_attempt"), int) or not 1 <= item["run_attempt"] <= attempt:
            raise ValueError("aggregate receipt has invalid per-shard artifact lineage")
        for field in ("evidence_digest", "artifact_digest"):
            value = item.get(field)
            if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
                raise ValueError(f"aggregate receipt has invalid per-shard {field}")
        seen.add(item["index"])
    if seen != set(range(SHARD_COUNT)):
        raise ValueError("aggregate receipt per-shard artifact digest coverage is incomplete")
    inventory_counts = receipt.get("inventory_mutant_counts")
    covered_counts = receipt.get("covered_mutant_counts")
    per_shard_counts = receipt.get("per_shard_occurrence_counts")
    if not isinstance(inventory_counts, list) or not inventory_counts or inventory_counts != covered_counts:
        raise ValueError("aggregate receipt does not preserve exact inventory multiplicity")
    previous_identity = ""
    inventory_total = 0
    for item in inventory_counts:
        if not isinstance(item, dict) or set(item) != {"identity", "count"}:
            raise ValueError("aggregate receipt has invalid inventory multiplicity")
        identity = item["identity"]
        count = item["count"]
        if not isinstance(identity, str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", identity) or identity <= previous_identity:
            raise ValueError("aggregate receipt has noncanonical inventory multiplicity")
        if isinstance(count, bool) or not isinstance(count, int) or count < 1:
            raise ValueError("aggregate receipt has invalid inventory multiplicity count")
        previous_identity = identity
        inventory_total += count
    if inventory_total != receipt["covered_mutants"]:
        raise ValueError("aggregate receipt inventory multiplicity does not equal coverage")
    if not isinstance(per_shard_counts, list) or len(per_shard_counts) != SHARD_COUNT:
        raise ValueError("aggregate receipt lacks all per-shard occurrence counts")
    total = 0
    count_indexes: set[int] = set()
    for item in per_shard_counts:
        if not isinstance(item, dict) or isinstance(item.get("index"), bool) or not isinstance(item.get("index"), int) or item["index"] in count_indexes:
            raise ValueError("aggregate receipt has invalid per-shard occurrence identity")
        occurrences = item.get("occurrences")
        multiset_digest = item.get("multiset_digest")
        if item["index"] < 0 or item["index"] >= SHARD_COUNT or isinstance(occurrences, bool) or not isinstance(occurrences, int) or occurrences < 0:
            raise ValueError("aggregate receipt has invalid per-shard occurrence count")
        if not isinstance(multiset_digest, str) or not re.fullmatch(r"[0-9a-f]{64}", multiset_digest):
            raise ValueError("aggregate receipt has invalid per-shard multiplicity digest")
        total += occurrences
        count_indexes.add(item["index"])
    if count_indexes != set(range(SHARD_COUNT)) or total != receipt["covered_mutants"]:
        raise ValueError("aggregate receipt occurrence counts do not equal coverage")
    if receipt["coverage_digest"] != digest(
        {"mutant_counts": covered_counts, "per_shard_occurrence_counts": per_shard_counts}
    ):
        raise ValueError("aggregate receipt coverage digest does not bind multiplicity")


def validate_receipt_files(receipt_path: Path, run_path: Path, expected_sha: str) -> None:
    validate_successful_receipt(
        json.loads(receipt_path.read_text(encoding="utf-8")),
        json.loads(run_path.read_text(encoding="utf-8")),
        expected_sha,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path, help="downloaded mutants-aggregate-receipt.json")
    parser.add_argument("--run", type=Path, help="terminal GitHub run JSON")
    parser.add_argument("--expected-sha", help="exact protected-main SHA")
    args = parser.parse_args()
    supplied = (args.receipt is not None, args.run is not None, args.expected_sha is not None)
    if any(supplied) and not all(supplied):
        parser.error("--receipt, --run, and --expected-sha must be supplied together")
    if all(supplied):
        try:
            validate_receipt_files(args.receipt, args.run, args.expected_sha)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            raise SystemExit(f"hosted mutants aggregate rejected: {error}") from error
        print("hosted mutants aggregate: PASS")
        return 0

    verify_workflow(WORKFLOW.read_text(encoding="utf-8"))
    legacy = LEGACY_WORKFLOW.read_text(encoding="utf-8")
    if LEGACY_DISABLED_IF not in legacy:
        raise SystemExit("legacy nightly mutants-workspace must remain disabled")
    for document in (BACKLOG, OWNER_PACKET):
        text = document.read_text(encoding="utf-8")
        if "issue-1863-mutants-hosted.yml" not in text or "27" not in text:
            raise SystemExit(f"{document} does not retain the bounded #2457 ownership mapping")
    print("issue #1863 / #1948 bounded hosted mutants contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
