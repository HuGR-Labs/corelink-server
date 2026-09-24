#!/usr/bin/env python3
"""Validate the explicit CI pack catalog and its fail-closed invariants."""

from __future__ import annotations

import argparse
import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "docs/internal/ci-issue-packs.json"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")
REQUIRED = {
    "pack_id",
    "issue",
    "changed_surfaces",
    "required_focused_jobs",
    "admission_checks",
    "negative_controls",
    "exact_sha",
    "budget",
    "escalation_policy",
    "global_suite_escalation_triggers",
    "merge_evidence",
}
JOB_IDS = {"actionlint", "ci-pack-contract"}


def valid_sha_pair(expected: str, actual: str) -> bool:
    return bool(SHA_RE.fullmatch(expected) and SHA_RE.fullmatch(actual) and expected == actual)


def select_pack(catalog: dict, pack_id: str) -> list[dict]:
    return [pack for pack in catalog.get("packs", []) if isinstance(pack, dict) and pack.get("pack_id") == pack_id]


def paths_outside_pack(changed: list[str], surfaces: list[str]) -> list[str]:
    return [path for path in changed if not any(fnmatch.fnmatchcase(path, pattern) for pattern in surfaces)]


def validate(catalog: object) -> list[str]:
    errors: list[str] = []
    if not isinstance(catalog, dict) or catalog.get("schema_version") != 1:
        return ["unsupported catalog schema"]
    packs = catalog.get("packs")
    if not isinstance(packs, list) or not packs:
        return ["packs must be a non-empty list"]
    seen: set[str] = set()
    for index, pack in enumerate(packs):
        prefix = f"packs[{index}]"
        if not isinstance(pack, dict):
            errors.append(f"{prefix} must be an object")
            continue
        missing = REQUIRED - pack.keys()
        if missing:
            errors.append(f"{prefix} missing fields: {', '.join(sorted(missing))}")
            continue
        pack_id = pack["pack_id"]
        if not isinstance(pack_id, str) or not re.fullmatch(r"issue-[0-9]+-[a-z0-9-]+", pack_id):
            errors.append(f"{prefix}.pack_id is invalid")
        elif pack_id in seen:
            errors.append(f"duplicate pack_id: {pack_id}")
        else:
            seen.add(pack_id)
        if not isinstance(pack["issue"], int) or pack["issue"] <= 0:
            errors.append(f"{prefix}.issue must be a positive integer")
        for field in ("changed_surfaces", "required_focused_jobs", "admission_checks", "negative_controls", "global_suite_escalation_triggers", "merge_evidence"):
            value = pack[field]
            if not isinstance(value, list) or not value or not all(isinstance(item, str) and item.strip() for item in value):
                errors.append(f"{prefix}.{field} must be a non-empty list of strings")
        if not set(pack["required_focused_jobs"] if isinstance(pack["required_focused_jobs"], list) else []) <= JOB_IDS:
            errors.append(f"{prefix}.required_focused_jobs contains an unknown job")
        exact = pack["exact_sha"]
        if not isinstance(exact, dict) or exact.get("source") != "required workflow_dispatch input expected_sha" or exact.get("binding") != "must equal github.sha for the checked-out dispatch ref" or exact.get("target_base_source") != "required workflow_dispatch input target_base_sha" or not exact.get("target_base_sha"):
            errors.append(f"{prefix}.exact_sha must bind required dispatch inputs to the checked-out SHA")
        budget = pack["budget"]
        if not isinstance(budget, dict):
            errors.append(f"{prefix}.budget must be an object")
        else:
            if budget.get("runner") != "ubuntu-24.04" or budget.get("job_timeout_minutes") != 15 or budget.get("max_billable_minutes_per_dispatch") != 15 or budget.get("matrix_limit") != 1 or budget.get("max_dispatches_per_pack_and_sha") != 1:
                errors.append(f"{prefix}.budget exceeds the declared hosted-runner cap")
        escalation = pack["escalation_policy"]
        if not isinstance(escalation, dict) or not set(escalation.get("critical_surfaces", [])) >= {"workflow", "security", "release", "core"} or escalation.get("required_jobs_for_this_pack") != ["actionlint", "ci-pack-contract"] or escalation.get("protected_main_full_suite_required_after_merge") is not True:
            errors.append(f"{prefix}.escalation_policy weakens a critical-surface gate")
        if pack.get("issue") and isinstance(pack_id, str) and str(pack["issue"]) not in pack_id:
            errors.append(f"{prefix}.pack_id does not name its issue")
    return errors


def self_test(catalog: dict) -> list[str]:
    failures: list[str] = []
    if validate(catalog):
        return ["canonical catalog is invalid"]
    unknown = json.loads(json.dumps(catalog))
    unknown["packs"][0]["pack_id"] = "unknown"
    if not validate(unknown):
        failures.append("unknown pack ID was accepted")
    if select_pack(catalog, "unknown"):
        failures.append("unlisted pack ID resolved to a pack")
    missing = json.loads(json.dumps(catalog))
    del missing["packs"][0]["negative_controls"]
    if not validate(missing):
        failures.append("pack missing negative controls was accepted")
    oversized = json.loads(json.dumps(catalog))
    oversized["packs"][0]["budget"]["job_timeout_minutes"] = 60
    if not validate(oversized):
        failures.append("over-cap timeout was accepted")
    unbounded = json.loads(json.dumps(catalog))
    unbounded["packs"][0]["required_focused_jobs"].append("full-suite-skipped")
    if not validate(unbounded):
        failures.append("unknown focused job was accepted")
    downgraded = json.loads(json.dumps(catalog))
    downgraded["packs"][0]["escalation_policy"]["protected_main_full_suite_required_after_merge"] = False
    if not validate(downgraded):
        failures.append("critical-surface downgrade was accepted")
    wrong_sha = json.loads(json.dumps(catalog))
    wrong_sha["packs"][0]["exact_sha"]["binding"] = "best effort branch name"
    if not validate(wrong_sha):
        failures.append("weak SHA binding was accepted")
    if valid_sha_pair("a" * 40, "b" * 40) or valid_sha_pair("not-a-sha", "not-a-sha"):
        failures.append("malformed or mismatched candidate SHA was accepted")
    if not paths_outside_pack(["crates/corelink-server/src/lib.rs"], catalog["packs"][0]["changed_surfaces"]):
        failures.append("out-of-pack changed file was accepted")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate-sha", required=True)
    parser.add_argument("--actual-sha", required=True)
    parser.add_argument("--target-base-sha", required=True)
    parser.add_argument("--pack-id", required=True)
    parser.add_argument("--run-self-test", action="store_true")
    args = parser.parse_args()
    if not valid_sha_pair(args.candidate_sha, args.actual_sha) or not SHA_RE.fullmatch(args.target_base_sha):
        print("expected candidate and base SHA must be full 40-character hex IDs", file=sys.stderr)
        return 1
    if args.candidate_sha != args.actual_sha:
        print("candidate SHA does not match the exact dispatched revision", file=sys.stderr)
        return 1
    try:
        catalog = json.loads(CATALOG.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"cannot read canonical pack catalog: {exc}", file=sys.stderr)
        return 1
    problems = validate(catalog)
    if problems:
        print("invalid issue CI pack catalog:", *problems, sep="\n- ", file=sys.stderr)
        return 1
    selected = select_pack(catalog, args.pack_id)
    if len(selected) != 1:
        print(f"unknown or ambiguous pack ID: {args.pack_id}", file=sys.stderr)
        return 1
    changed = subprocess.run(
        ["git", "diff", "--name-only", args.target_base_sha, args.candidate_sha],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    allowed = selected[0]["changed_surfaces"]
    unexpected = paths_outside_pack(changed, allowed)
    if unexpected:
        print("changed-file boundary exceeded:", *unexpected, sep="\n- ", file=sys.stderr)
        return 1
    merge_base = subprocess.run(
        ["git", "merge-base", "--is-ancestor", args.target_base_sha, args.candidate_sha],
        cwd=ROOT,
        check=False,
    )
    if merge_base.returncode != 0:
        print("target base SHA is not an ancestor of the candidate SHA", file=sys.stderr)
        return 1
    if args.run_self_test:
        failures = self_test(catalog)
        if failures:
            print("adversarial pack checks failed:", *failures, sep="\n- ", file=sys.stderr)
            return 1
    print(f"valid pack {args.pack_id}: candidate={args.candidate_sha} base={args.target_base_sha} files={len(changed)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
