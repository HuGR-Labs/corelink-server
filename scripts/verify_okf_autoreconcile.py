#!/usr/bin/env python3
"""Fail-closed contract for the OKF auto-reconcile workflow.

This is a static/fork-safety gate.  It does not contact GitHub, invoke Codex, or
need credentials.  The self-test mutates each trust-boundary marker so a green
workflow cannot silently become dispatch-only, target the ephemeral runner, or
lose the post-agent containment checks.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/okf-autoreconcile.yml"
REQUIRED_PATHS = (
    "docs/knowledge/**",
    "docs/internal/okf-wiki/**",
    "crates/**",
    "migrations/**",
    "specs/**",
    "worker/**",
    "apps/**",
    "scripts/validate_okf.py",
    "scripts/validate_permission_matrix.py",
    "scripts/okf_reconcile.py",
    "scripts/okf_git_batch.py",
    "scripts/okf_resolve_abbrev_cites.py",
    "scripts/okf_shift_citations.py",
    "scripts/okf_anchor_reverify.py",
    "scripts/okf_index.py",
    "scripts/okf_status.py",
    "scripts/okf-reconcile-local.sh",
    "scripts/okf-agent-*.sh",
    "scripts/okf-state-sha.sh",
    "tests/okf/**",
    "tests/test_okf_resolve_abbrev_cites.py",
    "tests/test_okf_shift_citations.py",
    "tests/test_okf_anchor_reverify.py",
    ".claude/skills/okf-reconcile/**",
    ".github/workflows/okf*.yml",
    ".github/workflows/okf-autoreconcile.yml",
)


class ContractError(ValueError):
    pass


def read_workflow() -> str:
    try:
        text = WORKFLOW.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        raise ContractError(f"workflow is unreadable: {WORKFLOW}") from exc
    if not text.strip() or len(text.encode("utf-8")) > 200_000:
        raise ContractError("workflow is empty or exceeds the bounded contract size")
    return text


def validate(text: str) -> None:
    required = (
        "name: OKF auto-reconcile",
        "push:\n    branches: [main]",
        "workflow_dispatch:",
        "if: github.ref == 'refs/heads/main'",
        "runs-on: [self-hosted, mac, corelink-builder]",
        "timeout-minutes: 20",
        "secrets.OPENAI_API_KEY",
        "codex-version: '0.152.1'",
        "codex-args: '[\"--ephemeral\", \"--ignore-user-config\", \"--sandbox\", \"workspace-write\"]'",
        "--ignore-user-config",
        "actions/checkout@",
        "openai/codex-action@",
        "persist-credentials: false",
        "scripts/okf-agent-git-guard.sh",
        "scripts/okf-agent-gh-guard.sh",
        "Snapshot git state before Codex",
        "Verify Codex did not mutate git state",
        "scripts/okf-state-sha.sh \"$RUNNER_TEMP/okf_state_before\"",
        "scripts/okf-state-sha.sh \"$RUNNER_TEMP/okf_state_after\"",
        "git diff --name-only -z HEAD",
        "git ls-files --others --exclude-standard -z",
        "python3 scripts/validate_okf.py",
        "bash tests/okf/run_fixtures.sh",
        "gh pr create",
    )
    for marker in required:
        if marker not in text:
            raise ContractError(f"workflow contract marker missing: {marker!r}")
    if re.search(r"(?m)^\s+pull_request(?:_target)?\s*:", text):
        raise ContractError("credentialed OKF workflow must not run on pull requests")
    for action in ("actions/checkout", "openai/codex-action"):
        if not re.search(rf"{re.escape(action)}@[0-9a-f]{{40}}\b", text):
            raise ContractError(f"{action} must be pinned to a full commit SHA")
    for path in REQUIRED_PATHS:
        if f"      - '{path}'" not in text:
            raise ContractError(f"OKF source path is missing from push filter: {path}")
    if "ANTHROPIC_API_KEY" in text or re.search(r"(?i)(?<!\.)\bclaude(?:\s+code)?\b", text):
        raise ContractError("obsolete Claude/Anthropic execution path remains")
    if "npm install" in text or "pull_request_target" in text:
        raise ContractError("workflow contains an unhermetic install or target trigger")
    if re.search(r"(?m)^\s*state_sha\s*\(\)", text):
        raise ContractError("workflow must not define a step-local state_sha function")


def self_test(text: str) -> None:
    mutations = {
        "runner": ("runs-on: [self-hosted, mac, corelink-builder]", "runs-on: corelink"),
        "automatic trigger": ("push:\n    branches: [main]", "workflow_dispatch:"),
        "fork boundary": ("workflow_dispatch:", "pull_request:"),
        "git guard": ("scripts/okf-agent-git-guard.sh", "scripts/missing-guard.sh"),
        "path filter": ("      - 'crates/**'", "      - 'crates-missing/**'"),
        "state snapshot": ("Snapshot git state before Codex", "Snapshot removed"),
    }
    failures = []
    for name, (needle, replacement) in mutations.items():
        mutant = text.replace(needle, replacement, 1)
        if mutant == text:
            failures.append(f"{name}: fixture did not mutate")
            continue
        try:
            validate(mutant)
        except ContractError:
            continue
        failures.append(f"{name}: mutation was accepted")
    if failures:
        raise ContractError("; ".join(failures))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    try:
        text = read_workflow()
        validate(text)
        if args.self_test:
            self_test(text)
    except ContractError as exc:
        print(f"B-058 DRIFTED: {exc}", file=sys.stderr)
        return 1
    suffix = "; mutations 6/6 rejected" if args.self_test else ""
    print(f"B-058 OKF workflow contract valid{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
