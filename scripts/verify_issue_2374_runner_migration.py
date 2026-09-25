#!/usr/bin/env python3
"""Verify the bounded #2374 runner migration against the immutable PR base."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import tempfile
from pathlib import Path

import yaml


ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    "bot-pr-has-checks.yml": {"audit"},
    "coverage.yml": {"coverage"},
    "dependabot-auto-merge.yml": {"auto-merge"},
    "dependabot-policy-trust-boundary.yml": {"trust-boundary-teeth"},
    "dependabot-policy.yml": {"sentinel", "policy-gate"},
    "lockfile-diff.yml": {"lockfile-diff"},
    "permission-matrix.yml": {"permission-matrix"},
    "pr-labels.yml": {"label", "size"},
    "stale.yml": {"stale"},
    "welcome-first-pr.yml": {"welcome"},
}
TOOLCHAIN_ACTION = (
    "dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8"
)
APPROVED_TOOLCHAIN_SETUP = {
    ("dependabot-policy.yml", "policy-gate"): {
        "name": "Install the workspace-pinned toolchain on the hosted runner",
        "if": "hashFiles('_pr-data/Cargo.toml') != ''",
        "uses": TOOLCHAIN_ACTION,
        "with": {"toolchain": "1.91.1"},
    },
    ("lockfile-diff.yml", "lockfile-diff"): {
        "name": "Install the workspace-pinned toolchain on the hosted runner",
        "uses": TOOLCHAIN_ACTION,
        "with": {"toolchain": "1.91.1"},
    },
}


class ContractError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ContractError(message)


def workflow_at(revision: str, filename: str) -> dict:
    if revision == "HEAD":
        raw = (ROOT / ".github/workflows" / filename).read_text()
    else:
        raw = subprocess.check_output(
            ["git", "show", f"{revision}:.github/workflows/{filename}"],
            cwd=ROOT,
            text=True,
        )
    parsed = yaml.load(raw, Loader=yaml.BaseLoader)
    if not isinstance(parsed, dict):
        fail(f"{filename}: workflow is not a mapping")
    return parsed


def checkout_steps(job: dict) -> list[dict]:
    return [step for step in job.get("steps", []) if str(step.get("uses", "")).startswith("actions/checkout@")]


def normalize_steps(steps: list[dict]) -> list[dict]:
    result = []
    for step in steps:
        item = json.loads(json.dumps(step))
        if str(item.get("uses", "")).startswith("actions/checkout@"):
            with_values = item.get("with", {})
            with_values.pop("persist-credentials", None)
            if not with_values:
                item.pop("with", None)
        result.append(item)
    return result


def validate(base_sha: str, mutations: bool = False) -> dict:
    total_jobs = total_checkouts = 0
    new_toolchain_steps = []
    for filename, expected_jobs in TARGETS.items():
        base = workflow_at(base_sha, filename)
        head = workflow_at("HEAD", filename)
        for key in set(base) | set(head):
            if key == "jobs":
                continue
            if base.get(key) != head.get(key):
                fail(f"{filename}: protected workflow-level field changed: {key}")
        base_jobs, head_jobs = base.get("jobs", {}), head.get("jobs", {})
        if set(base_jobs) != set(head_jobs) or set(head_jobs) != expected_jobs:
            fail(f"{filename}: job census changed; expected {sorted(expected_jobs)}")
        for job_id in sorted(expected_jobs):
            before, after = base_jobs[job_id], head_jobs[job_id]
            total_jobs += 1
            if after.get("runs-on") != "ubuntu-24.04":
                fail(f"{filename}:{job_id}: runner is not ubuntu-24.04")
            if before.get("runs-on") == after.get("runs-on"):
                fail(f"{filename}:{job_id}: runner selector was not migrated")
            if {k: v for k, v in before.items() if k not in {"runs-on", "steps"}} != {
                k: v for k, v in after.items() if k not in {"runs-on", "steps"}
            }:
                fail(f"{filename}:{job_id}: job protection/permissions changed")
            old_steps, new_steps = before.get("steps", []), after.get("steps", [])
            total_checkouts += len(checkout_steps(after))
            for step in checkout_steps(after):
                if step.get("with", {}).get("persist-credentials") != "false":
                    fail(f"{filename}:{job_id}: checkout credentials persist")
            added_toolchain_steps = []
            for step in new_steps:
                use = str(step.get("uses", ""))
                if use == TOOLCHAIN_ACTION and step.get("with", {}).get("toolchain") == "1.91.1":
                    added_toolchain_steps.append(step)
                    new_toolchain_steps.append((filename, job_id))
                elif use.startswith("dtolnay/rust-toolchain@") and not any(
                    old.get("uses") == use for old in old_steps
                ):
                    fail(f"{filename}:{job_id}: unapproved toolchain setup")
            stripped = [s for s in new_steps if s not in added_toolchain_steps]
            if normalize_steps(old_steps) != normalize_steps(stripped):
                fail(f"{filename}:{job_id}: original action/run baseline changed")
    if (total_jobs, total_checkouts) != (12, 9):
        fail(f"wrong census: {total_jobs} jobs, {total_checkouts} checkouts")
    if sorted(new_toolchain_steps) != sorted([
        ("dependabot-policy.yml", "policy-gate"),
        ("lockfile-diff.yml", "lockfile-diff"),
    ]):
        fail(f"unexpected hosted toolchain setup: {new_toolchain_steps}")
    for filename, job_id in new_toolchain_steps:
        setup = next(
            step
            for step in workflow_at("HEAD", filename)["jobs"][job_id]["steps"]
            if step.get("uses") == TOOLCHAIN_ACTION
            and step.get("with", {}).get("toolchain") == "1.91.1"
        )
        if setup != APPROVED_TOOLCHAIN_SETUP[(filename, job_id)]:
            fail(f"{filename}:{job_id}: hosted setup differs from the approved step")
    if mutations:
        mutation_checks(base_sha)
    return {
        "base_sha": base_sha,
        "jobs": total_jobs,
        "checkouts": total_checkouts,
        "runners": "ubuntu-24.04",
        "toolchain_setups": len(new_toolchain_steps),
        "write_bearing_target_jobs_executed": False,
    }


def rejected(label: str, base_sha: str, mutate) -> None:
    original = workflow_at
    try:
        def altered(revision: str, filename: str) -> dict:
            value = original(revision, filename)
            return mutate(revision, filename, value)
        globals()["workflow_at"] = altered
        try:
            validate(base_sha)
        except (ContractError, subprocess.CalledProcessError):
            return
        fail(f"negative control was accepted: {label}")
    finally:
        globals()["workflow_at"] = original


def mutation_checks(base_sha: str) -> None:
    original = workflow_at
    # Test that each protected property fails closed when mutated in-memory.
    def runner(rev, name, value):
        if rev == "HEAD" and name == "stale.yml": value["jobs"]["stale"]["runs-on"] = "corelink"
        return value
    rejected("self-hosted selector", base_sha, runner)

    def checkout_credentials(rev, name, value):
        if rev == "HEAD" and name == "coverage.yml":
            value["jobs"]["coverage"]["steps"][0]["with"]["persist-credentials"] = "true"
        return value
    rejected("checkout credential persistence", base_sha, checkout_credentials)

    def permissions(rev, name, value):
        if rev == "HEAD" and name == "stale.yml": value["permissions"]["issues"] = "read"
        return value
    rejected("workflow permissions", base_sha, permissions)

    def command(rev, name, value):
        if rev == "HEAD" and name == "coverage.yml":
            value["jobs"]["coverage"]["steps"][-1]["run"] = "echo altered"
        return value
    rejected("immutable command baseline", base_sha, command)

    # A deliberately local gh double proves mutation commands are denied. The
    # real token is never consulted and no target job is executed by this test.
    with tempfile.TemporaryDirectory(prefix="i2374-gh-deny-") as directory:
        fake = Path(directory) / "gh"
        log = Path(directory) / "denied-write.log"
        fake.write_text(
            "#!/bin/sh\n"
            "case \"$1 $2\" in\n"
            "  'pr merge'|'pr edit'|'issue create'|'api -X'|'api --method')\n"
            f"    echo \"$*\" >> '{log}'\n"
            "    echo 'mock denied write' >&2\n"
            "    exit 73;;\n"
            "esac\n"
            "exit 0\n"
        )
        fake.chmod(0o755)
        environment = dict(os.environ, PATH=f"{directory}:{os.environ['PATH']}", GH_TOKEN="")
        result = subprocess.run(["gh", "pr", "merge", "2374"], env=environment, check=False)
        if result.returncode != 73 or not log.exists():
            fail("mocked GitHub write denial did not reject a merge")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    result = validate(args.base_sha, mutations=args.self_test)
    print(json.dumps(result, sort_keys=True, indent=2))
    print("Migration proof does not claim write-bearing jobs executed.")


if __name__ == "__main__":
    main()
