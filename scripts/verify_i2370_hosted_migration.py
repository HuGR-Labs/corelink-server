#!/usr/bin/env python3
"""Prove the bounded 17-job #2370 runner migration without running target jobs."""
from __future__ import annotations

import argparse
import copy
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

HOSTED = {"ubuntu-24.04", "macos-15-intel"}
FORBIDDEN = {"corelink", "self-hosted"}
CHECKOUT_SHA = "9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0"
TOOLCHAIN_ACTION = "dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8"
INVENTORY: dict[str, dict[str, tuple[str, str]]] = {
    ".github/workflows/cargo-audit.yml": {
        "cargo-audit-pr": ("corelink", "ubuntu-24.04"),
        "cargo-audit-daily": ("corelink", "ubuntu-24.04"),
    },
    ".github/workflows/cas_foundation.yml": {
        "workspace-build-test": ("corelink", "ubuntu-24.04"),
        "cargo-deny": ("corelink", "ubuntu-24.04"),
        "sbom-cyclonedx": ("corelink", "ubuntu-24.04"),
        "cosign-sign": ("corelink", "ubuntu-24.04"),
        "reproducible-build-smoke": ("corelink", "ubuntu-24.04"),
    },
    ".github/workflows/gitleaks.yml": {"gitleaks": ("corelink", "ubuntu-24.04")},
    ".github/workflows/license-policy.yml": {"license-policy": ("corelink", "ubuntu-24.04")},
    ".github/workflows/manual-install-recipes.yml": {
        "linux-x86_64-tarball-recipe": ("corelink", "ubuntu-24.04")
    },
    ".github/workflows/pnpm-audit.yml": {"pnpm-audit": ("corelink", "ubuntu-24.04")},
    ".github/workflows/release-notes.yml": {"generate": ("corelink", "ubuntu-24.04")},
    ".github/workflows/reproducible-build.yml": {
        "two-leg-diff": (["self-hosted", "mac", "corelink-builder"], "macos-15-intel")
    },
    ".github/workflows/sbom-consolidated.yml": {
        "sbom-consolidated": ("corelink", "ubuntu-24.04")
    },
    ".github/workflows/sbom.yml": {
        "sbom-tsa-attest": (["self-hosted", "mac", "corelink-builder"], "ubuntu-24.04"),
        "sbom-dt-ingest": (["self-hosted", "mac", "corelink-builder"], "ubuntu-24.04"),
        "sbom-release-upload": (["self-hosted", "mac", "corelink-builder"], "ubuntu-24.04"),
    },
}
SETUPS = {
    (".github/workflows/cargo-audit.yml", "cargo-audit-daily"),
    (".github/workflows/license-policy.yml", "license-policy"),
    (".github/workflows/reproducible-build.yml", "two-leg-diff"),
    (".github/workflows/sbom-consolidated.yml", "sbom-consolidated"),
    (".github/workflows/sbom.yml", "sbom-tsa-attest"),
    (".github/workflows/sbom.yml", "sbom-dt-ingest"),
}
PROOF_FILES = {
    ".github/workflows/issue-2370-hosted-migration.yml",
    "scripts/verify_i2370_hosted_migration.py",
}
EXPECTED_FILES = set(INVENTORY) | PROOF_FILES
SHA = re.compile(r"^[0-9a-f]{40}$")
RUST_COMMAND = re.compile(r"(?<![A-Za-z0-9_])(cargo|rustc|rustup)(?=$|[^A-Za-z0-9_-])")
RELEASE_PUSH_AUTH = '''# Checkout credential persistence is disabled. Supply this one push
# with the repository-scoped App token through ephemeral Git config;
# do not write credentials into the checkout's .git/config.
auth="$(printf 'x-access-token:%s' "$BOT_APP_TOKEN" | base64 | tr -d '\\n')"
GIT_CONFIG_COUNT=1 \\
GIT_CONFIG_KEY_0='http.https://github.com/.extraheader' \\
GIT_CONFIG_VALUE_0="AUTHORIZATION: basic ${auth}" \\
  git push -u origin "$BRANCH"'''


class ContractError(RuntimeError):
    pass


def fail(message: str) -> None:
    raise ContractError(message)


def parse_yaml(text: str, origin: str) -> dict[str, Any]:
    # Psych is part of Ruby's standard library on GitHub-hosted Ubuntu. Safe-load
    # treats workflow YAML as inert data; candidate code and actions are never run.
    command = [
        "ruby",
        "-rjson",
        "-ryaml",
        "-e",
        'puts JSON.generate(YAML.safe_load(STDIN.read, aliases: true))',
    ]
    result = subprocess.run(command, input=text, text=True, capture_output=True)
    if result.returncode:
        fail(f"{origin}: YAML parse failed: {result.stderr.strip()}")
    value = json.loads(result.stdout)
    if not isinstance(value, dict):
        fail(f"{origin}: expected workflow mapping")
    return value


def load_tree(root: Path) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for path in INVENTORY:
        try:
            result[path] = parse_yaml((root / path).read_text(encoding="utf-8"), path)
        except OSError as error:
            fail(f"{path}: unreadable: {error}")
    return result


def iter_checkout_steps(job: dict[str, Any]):
    for step in job.get("steps", []):
        if isinstance(step, dict) and str(step.get("uses", "")).startswith("actions/checkout@"):
            yield step


def setup_step() -> dict[str, Any]:
    return {
        "name": "Install workspace-pinned Rust toolchain on hosted VM",
        "uses": TOOLCHAIN_ACTION,
        "with": {"toolchain": "1.91.1"},
    }


def runner_has_forbidden_label(runner: Any) -> bool:
    labels = runner if isinstance(runner, list) else [runner]
    return any(str(label) in FORBIDDEN for label in labels)


def is_pinned_toolchain_setup(step: dict[str, Any]) -> bool:
    uses = str(step.get("uses", ""))
    run = str(step.get("run", ""))
    return (uses.startswith("dtolnay/rust-toolchain@") and SHA.fullmatch(uses.rsplit("@", 1)[1]) is not None) or (
        "scripts/ci-assert-pinned-toolchain.sh" in run
        or "scripts/ci-use-host-toolchain.sh" in run
    )


def validate_rust_setup_order(path: str, job_id: str, job: dict[str, Any]) -> None:
    steps = job.get("steps", [])
    ready = False
    for index, step in enumerate(steps):
        if not isinstance(step, dict):
            continue
        run = step.get("run")
        setup = is_pinned_toolchain_setup(step)
        if isinstance(run, str) and not setup and RUST_COMMAND.search(run) and not ready:
            fail(f"{path}:{job_id}: Rust-dependent step {index} precedes pinned toolchain setup")
        if setup:
            ready = True


def validate_release_push_auth(baseline: dict[str, dict[str, Any]], candidate: dict[str, dict[str, Any]]) -> None:
    path = ".github/workflows/release-notes.yml"
    old_job = baseline[path]["jobs"]["generate"]
    new_job = candidate[path]["jobs"]["generate"]
    old_step = next(step for step in old_job["steps"] if step.get("name") == "Commit notes to releases branch")
    new_step = next(step for step in new_job["steps"] if step.get("name") == "Commit notes to releases branch")
    old_run = old_step.get("run", "")
    new_run = new_step.get("run", "")
    if new_run.count(RELEASE_PUSH_AUTH) != 1:
        fail("release-notes: branch push must use one ephemeral repository-scoped App-token auth block")
    normalized_run = new_run.replace(RELEASE_PUSH_AUTH, 'git push -u origin "$BRANCH"', 1)
    if normalized_run != old_run:
        fail("release-notes: only the branch push authentication may change")
    if old_step.get("env", {}).get("BOT_APP_TOKEN") != "${{ steps.app-token.outputs.token }}":
        fail("release-notes: push token must be the existing repository-scoped App token")
    candidate[path]["jobs"]["generate"]["steps"] = [
        ({**step, "run": old_run} if step.get("name") == "Commit notes to releases branch" else step)
        for step in new_job["steps"]
    ]


def validate_pair(
    baseline: dict[str, dict[str, Any]],
    candidate: dict[str, dict[str, Any]],
    changed_files: set[str] | None = None,
) -> None:
    if set(baseline) != set(INVENTORY) or set(candidate) != set(INVENTORY):
        fail("closed workflow inventory drifted")
    if changed_files is not None and changed_files != EXPECTED_FILES:
        fail(
            "diff scope mismatch: "
            f"expected {sorted(EXPECTED_FILES)}, got {sorted(changed_files)}"
        )

    normalized = copy.deepcopy(candidate)
    validate_release_push_auth(baseline, normalized)
    seen_jobs = 0
    for path, expected in INVENTORY.items():
        old_doc = baseline[path]
        new_doc = normalized[path]
        old_jobs = old_doc.get("jobs")
        new_jobs = new_doc.get("jobs")
        expected_jobs = set(expected)
        if not isinstance(old_jobs, dict) or not isinstance(new_jobs, dict):
            fail(f"{path}: missing jobs mapping")
        if not expected_jobs.issubset(old_jobs) or set(old_jobs) != set(new_jobs):
            fail(f"{path}: job inventory changed")
        for job_id, (old_runner, hosted_runner) in expected.items():
            seen_jobs += 1
            old_job = old_jobs[job_id]
            new_job = new_jobs[job_id]
            validate_rust_setup_order(path, job_id, new_job)
            if old_job.get("runs-on") != old_runner:
                fail(f"{path}:{job_id}: base runner changed from frozen inventory")
            if new_job.get("runs-on") != hosted_runner or runner_has_forbidden_label(new_job.get("runs-on")):
                fail(f"{path}:{job_id}: wrong hosted runner {new_job.get('runs-on')!r}")

            old_checkouts = list(iter_checkout_steps(old_job))
            checkouts = list(iter_checkout_steps(new_job))
            if len(old_checkouts) != len(checkouts):
                fail(f"{path}:{job_id}: checkout inventory changed")
            for old_step, step in zip(old_checkouts, checkouts):
                if not isinstance(step.get("with"), dict) or step["with"].get("persist-credentials") is not False:
                    fail(f"{path}:{job_id}: checkout must disable credential persistence")
                # Keep a baseline's existing false setting as a semantic input;
                # erase only the additive setting where the base omitted it.
                if old_step.get("with", {}).get("persist-credentials") is not False:
                    step["with"].pop("persist-credentials")
                    if not step["with"]:
                        step.pop("with")

            new_job["runs-on"] = old_job["runs-on"]
            setup_key = (path, job_id)
            steps = new_job.get("steps", [])
            if setup_key in SETUPS:
                exact = setup_step()
                matches = [i for i, item in enumerate(steps) if item == exact]
                if len(matches) != 1:
                    fail(f"{path}:{job_id}: expected one exact pinned hosted toolchain setup")
                del steps[matches[0]]
            elif any(
                isinstance(item, dict)
                and item.get("name") == "Install workspace-pinned Rust toolchain on hosted VM"
                for item in steps
            ):
                fail(f"{path}:{job_id}: unexpected toolchain setup")

    if seen_jobs != 17:
        fail(f"expected 17 migrated jobs, saw {seen_jobs}")

    # The protected production build job stays byte-for-byte unchanged while
    # the separate #2176 owner decision is outstanding.
    protected = ".github/workflows/container-build-push-prod.yml"
    if protected in candidate:
        fail("protected workflow must not be part of the candidate inventory")
    if changed_files is not None and protected in changed_files:
        fail("protected build-push workflow changed")

    if normalized != baseline:
        fail("workflow semantics drifted outside runner, credentialless checkout, or pinned setup")


def run_git(root: Path, *args: str) -> str:
    result = subprocess.run(["git", "-C", str(root), *args], text=True, capture_output=True)
    if result.returncode:
        fail(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def verify(root: Path, base_root: Path, base_sha: str, head_sha: str) -> None:
    if SHA.fullmatch(base_sha) is None or SHA.fullmatch(head_sha) is None:
        fail("base and head must be full lowercase commit SHAs")
    actual_head = run_git(root, "rev-parse", "HEAD")
    actual_base = run_git(base_root, "rev-parse", "HEAD")
    if actual_head != head_sha:
        fail(f"candidate checkout is {actual_head}, expected {head_sha}")
    if actual_base != base_sha:
        fail(f"baseline checkout is {actual_base}, expected {base_sha}")
    changed = set(run_git(root, "diff", "--name-only", base_sha, head_sha).splitlines())
    validate_pair(load_tree(base_root), load_tree(root), changed)


def self_test(baseline: dict[str, dict[str, Any]], candidate: dict[str, dict[str, Any]]) -> None:
    validate_pair(baseline, candidate)
    cases = []
    bad_runner = copy.deepcopy(candidate)
    bad_runner[".github/workflows/cargo-audit.yml"]["jobs"]["cargo-audit-pr"]["runs-on"] = "corelink"
    cases.append(("self-hosted runner", bad_runner))
    bad_command = copy.deepcopy(candidate)
    steps = bad_command[".github/workflows/cargo-audit.yml"]["jobs"]["cargo-audit-pr"]["steps"]
    run_step = next(step for step in steps if isinstance(step, dict) and isinstance(step.get("run"), str))
    run_step["run"] += "\n# mutation"
    cases.append(("command drift", bad_command))
    bad_checkout = copy.deepcopy(candidate)
    step = next(iter_checkout_steps(bad_checkout[".github/workflows/cargo-audit.yml"]["jobs"]["cargo-audit-pr"]))
    step["with"]["persist-credentials"] = True
    cases.append(("credential persistence", bad_checkout))
    bad_permission = copy.deepcopy(candidate)
    # Workflow-level permission mutations affect authority and must be rejected.
    bad_permission[".github/workflows/cargo-audit.yml"]["permissions"]["contents"] = "write"
    cases.append(("permission drift", bad_permission))
    bad_order = copy.deepcopy(candidate)
    job = bad_order[".github/workflows/cas_foundation.yml"]["jobs"]["workspace-build-test"]
    setup_index = next(i for i, step in enumerate(job["steps"]) if is_pinned_toolchain_setup(step))
    setup = job["steps"].pop(setup_index)
    dependent_index = next(i for i, step in enumerate(job["steps"]) if isinstance(step, dict) and RUST_COMMAND.search(str(step.get("run", ""))))
    job["steps"].insert(dependent_index + 1, setup)
    cases.append(("Rust setup ordering", bad_order))
    for label, mutation in cases:
        try:
            validate_pair(baseline, mutation)
        except ContractError:
            continue
        fail(f"negative control escaped: {label}")
    print("PASS: 17-job census, semantic immutability, and 5 negative controls")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--base-root", type=Path, required=True)
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--head-sha", required=True)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        if args.self_test:
            verify(args.root, args.base_root, args.base_sha, args.head_sha)
            self_test(load_tree(args.base_root), load_tree(args.root))
        else:
            verify(args.root, args.base_root, args.base_sha, args.head_sha)
            print("PASS: exact SHA, 17 hosted jobs, credentialless checkout, immutable workflow semantics")
    except ContractError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
