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
    "focused_checks",
    "admission_checks",
    "negative_controls",
    "exact_sha",
    "budget",
    "escalation_policy",
    "global_suite_escalation_triggers",
    "merge_evidence",
}
JOB_IDS = {
    "issue-pack",
}
CHECK_IDS = {
    "actionlint",
    "ci-pack-contract",
    "rust-worker-three-arm",
    "python-mutants-receipt",
}
REQUIRED_PACK_CHECKS = {
    "issue-2440-ci-scoping": {"actionlint", "ci-pack-contract"},
    "issue-2437-worker-three-arm": {"ci-pack-contract", "rust-worker-three-arm"},
    "issue-1948-mutants-receipt": {"ci-pack-contract", "python-mutants-receipt"},
}
REQUIRED_PACK_JOBS = {pack_id: {"issue-pack"} for pack_id in REQUIRED_PACK_CHECKS}


def validate_workflow_contract(workflow_text: str) -> list[str]:
    required = (
        "type: workflow_dispatch",
        "github.ref == 'refs/heads/main' && github.ref_protected",
        "path: candidate",
        'ACTUAL_SHA="$(git -C candidate rev-parse HEAD)"',
        "issue-2440-ci-scoping|issue-2437-worker-three-arm|issue-1948-mutants-receipt",
        "id: actionlint",
        "id: ci-pack-contract",
        "id: rust-worker-three-arm",
        "id: python-mutants-receipt",
        "if: inputs.pack_id == 'issue-2440-ci-scoping' || inputs.pack_id == 'issue-2437-worker-three-arm'",
        "if: inputs.pack_id == 'issue-2437-worker-three-arm'",
        "if: inputs.pack_id == 'issue-1948-mutants-receipt'",
        "timeout-minutes: 15",
        "timeout-minutes: 10",
        "cargo test --package corelink-worker --features tower-middleware --lib three_arm_ -- --nocapture",
        "python3 -m unittest -q tests/test_i1863_mutants_hosted_receipt.py",
        "python3 scripts/verify_i1863_mutants_hosted.py",
        "--candidate-root candidate",
    )
    return [f"central workflow is missing required contract: {item}" for item in required if item not in workflow_text]


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
        for field in ("changed_surfaces", "required_focused_jobs", "focused_checks", "admission_checks", "negative_controls", "global_suite_escalation_triggers", "merge_evidence"):
            value = pack[field]
            if not isinstance(value, list) or not value or not all(isinstance(item, str) and item.strip() for item in value):
                errors.append(f"{prefix}.{field} must be a non-empty list of strings")
        workflow_files = pack.get("workflow_files_to_lint")
        if not isinstance(workflow_files, list) or not all(isinstance(path, str) and path.startswith(".github/workflows/") and path.endswith((".yml", ".yaml")) for path in workflow_files):
            errors.append(f"{prefix}.workflow_files_to_lint must be a list of workflow paths")
        if not set(pack["required_focused_jobs"] if isinstance(pack["required_focused_jobs"], list) else []) <= JOB_IDS:
            errors.append(f"{prefix}.required_focused_jobs contains an unknown job")
        if not set(pack["focused_checks"] if isinstance(pack["focused_checks"], list) else []) <= CHECK_IDS:
            errors.append(f"{prefix}.focused_checks contains an unknown check")
        if pack_id in REQUIRED_PACK_JOBS and set(pack["required_focused_jobs"]) != REQUIRED_PACK_JOBS[pack_id]:
            errors.append(f"{prefix}.required_focused_jobs weakens the immutable job baseline")
        if pack_id in REQUIRED_PACK_CHECKS and set(pack["focused_checks"]) != REQUIRED_PACK_CHECKS[pack_id]:
            errors.append(f"{prefix}.focused_checks weakens the immutable focused-check baseline")
        exact = pack["exact_sha"]
        if not isinstance(exact, dict) or exact.get("source") != "required workflow_dispatch input candidate_sha" or exact.get("binding") != "must equal HEAD of the isolated candidate checkout, controlled by the protected-main workflow" or exact.get("target_base_source") != "required workflow_dispatch input target_base_sha" or not exact.get("target_base_sha"):
            errors.append(f"{prefix}.exact_sha must bind required dispatch inputs to the checked-out SHA")
        budget = pack["budget"]
        if not isinstance(budget, dict):
            errors.append(f"{prefix}.budget must be an object")
        else:
            job_timeout = budget.get("job_timeout_minutes")
            step_timeout = budget.get("step_timeout_minutes")
            cost_cap = budget.get("max_billable_minutes_per_dispatch")
            if budget.get("runner") != "ubuntu-24.04" or not isinstance(job_timeout, int) or not 1 <= job_timeout <= 15 or not isinstance(step_timeout, int) or not 1 <= step_timeout <= job_timeout or not isinstance(cost_cap, int) or not 1 <= cost_cap <= job_timeout or budget.get("matrix_limit") != 1 or budget.get("max_dispatches_per_pack_and_sha") != 1:
                errors.append(f"{prefix}.budget exceeds the declared hosted-runner cap")
        escalation = pack["escalation_policy"]
        if not isinstance(escalation, dict) or not set(escalation.get("critical_surfaces", [])) >= {"workflow", "security", "release", "core"} or escalation.get("required_jobs_for_this_pack") != pack["required_focused_jobs"] or escalation.get("protected_main_full_suite_required_after_merge") is not True:
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
    unbounded["packs"][0]["focused_checks"].append("full-suite-skipped")
    if not validate(unbounded):
        failures.append("unknown focused job was accepted")
    downgraded = json.loads(json.dumps(catalog))
    downgraded["packs"][0]["escalation_policy"]["protected_main_full_suite_required_after_merge"] = False
    if not validate(downgraded):
        failures.append("critical-surface downgrade was accepted")
    removed_check = json.loads(json.dumps(catalog))
    removed_check["packs"][0]["focused_checks"].remove("actionlint")
    if not validate(removed_check):
        failures.append("required focused check removal was accepted")
    removed_job = json.loads(json.dumps(catalog))
    removed_job["packs"][0]["required_focused_jobs"].clear()
    if not validate(removed_job):
        failures.append("required focused job removal was accepted")
    wrong_sha = json.loads(json.dumps(catalog))
    wrong_sha["packs"][0]["exact_sha"]["binding"] = "best effort branch name"
    if not validate(wrong_sha):
        failures.append("weak SHA binding was accepted")
    if valid_sha_pair("a" * 40, "b" * 40) or valid_sha_pair("not-a-sha", "not-a-sha"):
        failures.append("malformed or mismatched candidate SHA was accepted")
    if not validate_workflow_contract((ROOT / ".github/workflows/issue-ci-pack.yml").read_text(encoding="utf-8")):
        failures.append("workflow missing the per-step candidate SHA binding was accepted")
    unbound_workflow = (ROOT / ".github/workflows/issue-ci-pack.yml").read_text(encoding="utf-8").replace(
        'ACTUAL_SHA="$(git -C candidate rev-parse HEAD)"', "ACTUAL_SHA=$EXPECTED_SHA"
    )
    if not validate_workflow_contract(unbound_workflow):
        failures.append("workflow losing the candidate SHA binding was accepted")
    if not paths_outside_pack(["crates/corelink-server/src/lib.rs"], catalog["packs"][0]["changed_surfaces"]):
        failures.append("out-of-pack changed file was accepted")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--candidate-sha", required=True)
    parser.add_argument("--actual-sha", required=True)
    parser.add_argument("--target-base-sha", required=True)
    parser.add_argument("--pack-id", required=True)
    parser.add_argument("--candidate-root", default=".")
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
    try:
        workflow_text = (ROOT / ".github/workflows/issue-ci-pack.yml").read_text(encoding="utf-8")
    except OSError as exc:
        print(f"cannot read trusted issue pack workflow: {exc}", file=sys.stderr)
        return 1
    problems.extend(validate_workflow_contract(workflow_text))
    if problems:
        print("invalid issue CI pack catalog:", *problems, sep="\n- ", file=sys.stderr)
        return 1
    selected = select_pack(catalog, args.pack_id)
    if len(selected) != 1:
        print(f"unknown or ambiguous pack ID: {args.pack_id}", file=sys.stderr)
        return 1
    changed = subprocess.run(
        ["git", "diff", "--name-only", args.target_base_sha, args.candidate_sha],
        cwd=Path(args.candidate_root).resolve(),
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    allowed = selected[0]["changed_surfaces"]
    unexpected = paths_outside_pack(changed, allowed)
    if unexpected:
        print("changed-file boundary exceeded:", *unexpected, sep="\n- ", file=sys.stderr)
        return 1
    workflow_changes = {path for path in changed if path.startswith(".github/workflows/")}
    linted_workflows = set(selected[0]["workflow_files_to_lint"])
    if workflow_changes - linted_workflows:
        print("changed workflow files are not covered by actionlint:", *(sorted(workflow_changes - linted_workflows)), sep="\n- ", file=sys.stderr)
        return 1
    merge_base = subprocess.run(
        ["git", "merge-base", "--is-ancestor", args.target_base_sha, args.candidate_sha],
        cwd=Path(args.candidate_root).resolve(),
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
