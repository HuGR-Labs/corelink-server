#!/usr/bin/env python3
"""Fail closed when Dependabot's pull_request_target trust boundary drifts."""
from __future__ import annotations
import argparse
import os
import re
import stat
import sys
from pathlib import Path
from typing import Any
import yaml

ROOT = Path(__file__).resolve().parents[1]
REFERENCE_ROOT = ROOT
POLICY = ROOT / ".github/workflows/dependabot-policy.yml"
TEETH = ROOT / ".github/workflows/dependabot-policy-trust-boundary.yml"
CONTROL_FILES = (
    ".github/workflows/dependabot-policy.yml",
    ".github/workflows/dependabot-policy-trust-boundary.yml",
    "scripts/check_dependabot_policy_trusted_tree.py",
    "scripts/test_dependabot_policy_trust_boundary.sh",
    "scripts/test_ci_use_host_toolchain.sh",
    "scripts/prepare_b133_python.sh",
    "scripts/ci-use-host-toolchain.sh",
    "rust-toolchain.toml",
)
CHECKOUT_SHA = "9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0"
METADATA_SHA = "25dd0e34f4fe68f24cc83900b1fe3fe149efef98"
INSTALL_SHA = "07b4745e0c39a41822af610387492e3e53aa222b"
TEETH_TYPES = {"opened", "synchronize", "reopened"}
MAX_LINES = 900

class StrictLoader(yaml.SafeLoader):
    pass

def _mapping(loader: StrictLoader, node: yaml.nodes.MappingNode) -> dict:
    result: dict[Any, Any] = {}
    for key_node, value_node in node.value:
        key = loader.construct_object(key_node, deep=True)
        if key in result:
            raise ValueError(f"duplicate YAML key: {key!r}")
        result[key] = loader.construct_object(value_node, deep=True)
    return result

StrictLoader.add_constructor(yaml.resolver.BaseResolver.DEFAULT_MAPPING_TAG, _mapping)

def load(path: Path) -> dict:
    text = path.read_text(encoding="utf-8")
    if len(text.splitlines()) > MAX_LINES:
        raise ValueError(f"{path}: bounded guard exceeded ({MAX_LINES} lines)")
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("&") or stripped.startswith("*"):
            raise ValueError(f"{path}: YAML anchors/aliases are not allowed")
    data = yaml.load(text, Loader=StrictLoader)
    if not isinstance(data, dict):
        raise ValueError(f"{path}: workflow must be a mapping")
    return data

def fail(message: str) -> None:
    raise ValueError("B133: " + message)

def eq(actual: Any, expected: Any, label: str) -> None:
    if actual != expected:
        fail(f"{label}: expected {expected!r}, got {actual!r}")

def trigger(workflow: dict) -> dict:
    value = workflow.get("on", workflow.get(True))
    if not isinstance(value, dict):
        fail("on must be a mapping")
    return value

def check_policy() -> None:
    workflow = load(POLICY)
    if "defaults" in workflow:
        fail("workflow defaults are forbidden: shell/workdir must stay explicit")
    event = trigger(workflow).get("pull_request_target")
    if not isinstance(event, dict):
        fail("policy must use a mapping pull_request_target event")
    eq(set(event.get("types", [])), {"opened", "synchronize", "reopened", "labeled", "ready_for_review"}, "policy event types")
    eq(workflow.get("permissions"), {"contents": "read", "pull-requests": "read", "checks": "read"}, "policy permissions")
    jobs = workflow.get("jobs")
    eq(set(jobs or {}), {"sentinel", "policy-gate"}, "policy jobs")
    eq(jobs["sentinel"].get("runs-on"), ["self-hosted", "mac", "corelink-builder"], "sentinel runner")
    eq(jobs["sentinel"].get("if"), "github.actor != 'dependabot[bot]'", "sentinel guard")
    gate = jobs["policy-gate"]
    eq(gate.get("runs-on"), "corelink", "policy runner")
    eq(gate.get("timeout-minutes"), 10, "policy timeout")
    eq(gate.get("if"), "github.actor == 'dependabot[bot]' && github.event.pull_request.user.login == 'dependabot[bot]'", "Dependabot identity guard")
    eq(gate.get("env", {}).get("B133_PYTHON"), "${{ github.workspace }}/.b133-python", "policy interpreter path")
    steps = gate.get("steps")
    if not isinstance(steps, list):
        fail("policy steps missing")
    required = ["Checkout PR merge ref (SHA-pinned)", "Checkout BASE ref into _base (trusted tree; SHA-pinned)", "Prepare hermetic checker interpreter (BASE tree)", "Assert trust-boundary wiring (BASE checker)", "Verify fetched PR history (fail-closed)", "Fetch Dependabot metadata", "Prepare isolated Cargo policy tree (PR data only)", "Use the workspace-pinned host toolchain (provisions nothing)", "Install cargo-deny (SHA-pinned)", "Run cargo-deny licenses (fail-closed)", "Banned-license signature scan (Cargo.lock)", "npm banned-license scan", "Forbid skip-hook / skip-ci flags", "Forbid governance-file modifications", "Verify required checks are present", "Policy-gate audit log"]
    eq([s.get("name") for s in steps], required, "policy step order")
    for step in steps:
        if "uses" in step:
            uses = str(step["uses"])
            if uses.startswith("./") or "@" not in uses:
                fail(f"untrusted/local action: {uses}")
            if uses.split("@", 1)[1].strip().split()[0] not in {CHECKOUT_SHA, METADATA_SHA, INSTALL_SHA}:
                fail(f"action is not an approved immutable pin: {uses}")
        if "run" in step:
            eq(step.get("working-directory"), "_base", f"run step {step.get('name')} working-directory")
            if "shell" in step:
                fail(f"run step {step.get('name')} overrides shell")
            text = str(step["run"])
            code = "\n".join(line for line in text.splitlines() if not line.lstrip().startswith("#"))
            if "|| true" in code:
                fail(f"run step {step.get('name')} masks a failure")
            executable = re.search(r"(?:^|[;&|]\s*)(?:bash|python(?:3)?|source|chmod|cargo|npm|node)\s", code)
            if "_pr-data" in code and executable:
                fail(f"run step {step.get('name')} executes from PR data")
            if "UNTRUSTED_TREE" in code and executable and step.get("name") not in {"Prepare isolated Cargo policy tree (PR data only)", "Assert trust-boundary wiring (BASE checker)"}:
                fail(f"run step {step.get('name')} executes a PR path")
    checkout = next(s for s in steps if s.get("name") == required[0])
    eq(checkout.get("with", {}).get("path"), "_pr-data", "PR checkout path")
    eq(checkout.get("with", {}).get("fetch-depth"), 2, "PR checkout history depth")
    eq(checkout.get("with", {}).get("ref"), "refs/pull/${{ github.event.pull_request.number }}/merge", "PR checkout ref expression")
    base = next(s for s in steps if s.get("name") == required[1])
    eq(base.get("with", {}).get("path"), "_base", "BASE checkout path")
    eq(base.get("with", {}).get("ref"), "${{ github.event.pull_request.base.sha }}", "BASE checkout ref")
    history = next(s for s in steps if s.get("name") == "Verify fetched PR history (fail-closed)")
    history_code = str(history.get("run", ""))
    for ref in ("HEAD", "HEAD^1", "HEAD^2"):
        if f"git -C \"$UNTRUSTED_TREE\" rev-parse --verify {ref}" not in history_code:
            fail(f"history guard does not verify {ref}")

def check_teeth() -> None:
    workflow = load(TEETH)
    eq(workflow.get("name"), "dependabot-policy trust-boundary teeth", "teeth name")
    event = trigger(workflow).get("pull_request_target")
    if not isinstance(event, dict):
        fail("teeth must use mapping pull_request_target")
    eq(set(event.get("types", [])), TEETH_TYPES, "teeth event types")
    eq(set(event.get("paths", [])), set(CONTROL_FILES), "teeth path filter")
    eq(workflow.get("permissions"), {"contents": "read"}, "teeth permissions")
    jobs = workflow.get("jobs")
    eq(set(jobs or {}), {"trust-boundary-teeth"}, "teeth jobs")
    job = jobs["trust-boundary-teeth"]
    eq(job.get("runs-on"), "corelink", "teeth runner")
    eq(job.get("timeout-minutes"), 10, "teeth timeout")
    eq(job.get("if"), "github.actor == 'dependabot[bot]' && github.event.pull_request.user.login == 'dependabot[bot]'", "teeth identity guard")
    steps = job.get("steps")
    if not isinstance(steps, list) or len(steps) != 6:
        fail("teeth step count must remain bounded at six")
    for step in steps:
        if "uses" in step and str(step["uses"]).split("@", 1)[-1].split()[0] != CHECKOUT_SHA:
            fail("teeth action is not pinned")
        if "run" in step and step.get("working-directory") != "_base":
            fail(f"teeth run step {step.get('name')} is not base-owned")
    eq(steps[0].get("with", {}).get("path"), "_base", "teeth BASE path")
    eq(steps[1].get("with", {}).get("path"), "_pr-data", "teeth PR data path")
    eq(steps[2].get("name"), "Prepare hermetic checker interpreter (BASE tree)", "teeth interpreter step")
    eq(steps[2].get("env", {}).get("B133_PYTHON"), "${{ github.workspace }}/.b133-python", "teeth interpreter path")
    eq(steps[3].get("env", {}).get("B133_UNTRUSTED_TREE"), "${{ github.workspace }}/_pr-data", "teeth data expression")
    eq(steps[3].get("env", {}).get("B133_PYTHON"), "${{ github.workspace }}/.b133-python", "teeth checker interpreter expression")
    eq(steps[0].get("with", {}).get("ref"), "${{ github.event.pull_request.base.sha }}", "teeth BASE ref expression")
    eq(steps[1].get("with", {}).get("ref"), "refs/pull/${{ github.event.pull_request.number }}/merge", "teeth PR ref expression")

def check_tree(untrusted: Path) -> None:
    if not untrusted.is_dir():
        fail(f"untrusted tree missing: {untrusted}")
    for relative in CONTROL_FILES:
        trusted = REFERENCE_ROOT / relative
        candidate = untrusted / relative
        parent = untrusted
        for component in Path(relative).parts[:-1]:
            parent = parent / component
            if parent.is_symlink():
                fail(f"untrusted control parent is a symlink: {relative}")
        try:
            mode = candidate.lstat().st_mode
        except FileNotFoundError:
            fail(f"untrusted control missing: {relative}")
        if not stat.S_ISREG(mode) or stat.S_ISLNK(mode):
            fail(f"untrusted control is not a regular non-symlink file: {relative}")
        if trusted.read_bytes() != candidate.read_bytes():
            fail(f"untrusted control mutation: {relative}")

def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--untrusted-tree", type=Path)
    args = parser.parse_args()
    try:
        global REFERENCE_ROOT
        if "B133_TRUSTED_ROOT" in os.environ:
            REFERENCE_ROOT = Path(os.environ["B133_TRUSTED_ROOT"])
        check_policy()
        check_teeth()
        if args.untrusted_tree:
            check_tree(args.untrusted_tree)
    except (OSError, ValueError, yaml.YAMLError) as error:
        print(error, file=sys.stderr)
        return 1
    print("B133: trusted base workflow, data-only PR checkout, and exact teeth verified")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
