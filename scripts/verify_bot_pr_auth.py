#!/usr/bin/env python3
"""Verify the B-012 GitHub App bot-PR contract without contacting GitHub.

The owner creates and installs the App out of band. Each creator job mints a
repository-scoped, short-lived installation token; no PAT or installation
token is stored as a durable Actions secret.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]

# These are the automatic PR creators present in the immutable B-012 base.
# pre-cutover-weekly-cron was retired before this base and must not be revived
# merely to satisfy the historical six-workflow description.
CREATOR_WORKFLOWS = (
    "subprocessors-sync.yml",
    "okf-autoreconcile.yml",
    "compliance-weekly.yml",
    "api-reference-sync.yml",
    "release-notes.yml",
)

# These creators also run a read-only drift check on pull requests. That job
# may execute fork-controlled code, so its default GITHUB_TOKEN must not have
# write permission and checkout must not persist credentials into the worktree.
FORK_SAFE_PR_WORKFLOWS = (
    "api-reference-sync.yml",
    "subprocessors-sync.yml",
)

AUTO_PR_HEAD_MARKERS = (
    "bot/subprocessors-sync-",
    "auto/okf-reconcile-",
    "auto/compliance-digest-",
    "bot/api-reference-sync-",
    "releases/v",
)

APP_ACTION = "actions/create-github-app-token@def152b8a737443d7af6c5722c6389146fe90c90"
APP_ID_SECRET = "CORELINK_BOT_APP_ID"
APP_KEY_SECRET = "CORELINK_BOT_APP_PRIVATE_KEY"
APP_TOKEN_OUTPUT = "steps.app-token.outputs.token"

B012_BACKLOG_REQUIRED = (
    "approval-required",
    "Dependabot PRs can trigger workflows",
    "status: open",
    "expected jobs actually complete",
    "offline/missing runner labelled `corelink`",
    "Check/status presence or a zero-job workflow run does not close it",
)


def _active_pr_creator(text: str) -> bool:
    """Find executable PR-creation commands/actions, ignoring comments."""

    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("#"):
            continue
        if re.search(r"\bgh\s+pr\s+create\b", stripped):
            return True
        if re.search(r"\bcreate-pull-request@", stripped):
            return True
    return False


def discover_creator_workflows(root: Path = ROOT) -> tuple[str, ...]:
    workflows = root / ".github" / "workflows"
    return tuple(
        sorted(
            path.name
            for path in workflows.glob("*.y*ml")
            if _active_pr_creator(path.read_text(encoding="utf-8"))
        )
    )


def _pr_step(text: str) -> str:
    """Return the workflow text from its PR-creation step(s)."""

    starts = [m.start() for m in re.finditer(r"gh pr create", text)]
    if not starts:
        return ""
    blocks = []
    for start in starts:
        step_start = text.rfind("\n      - name:", 0, start)
        # Include the env/run declaration and the complete shell command.  A
        # step is bounded by the next peer step or end of file.
        step_end = text.find("\n      - name:", start)
        if step_end < 0:
            step_end = len(text)
        blocks.append(text[max(0, step_start) : step_end])
    return "\n".join(blocks)


def _pr_create_commands(text: str) -> tuple[str, ...]:
    """Extract each multiline ``gh pr create`` shell command."""

    lines = text.splitlines()
    commands: list[str] = []
    for index, line in enumerate(lines):
        if not re.search(r"\bgh\s+pr\s+create\b", line):
            continue
        command = [line]
        cursor = index
        while command[-1].rstrip().endswith("\\") and cursor + 1 < len(lines):
            cursor += 1
            command.append(lines[cursor])
        commands.append("\n".join(command))
    return tuple(commands)


def verify(root: Path = ROOT) -> list[str]:
    errors: list[str] = []
    workflows = root / ".github" / "workflows"

    discovered = discover_creator_workflows(root)
    expected = set(CREATOR_WORKFLOWS)
    actual = set(discovered)
    for name in sorted(expected - actual):
        errors.append(f"expected active bot-PR workflow is not discoverable: {name}")
    for name in sorted(actual - expected):
        errors.append(f"unexpected active bot-PR creator discovered: {name}")

    for name in discovered:
        path = workflows / name
        if not path.is_file():
            errors.append(f"missing active bot-PR workflow: {path}")
            continue
        text = path.read_text(encoding="utf-8")
        if "approval-required" not in text or (
            "completed" not in text and "executed jobs" not in text
        ):
            errors.append(f"{name}: PR comments lost approval-required/completed-job distinction")
        if "suppresses the pull_request event" in text or "pull_request event is suppressed and no checks appear" in text:
            errors.append(f"{name}: PR comments still claim unconditional GITHUB_TOKEN suppression")
        if APP_ACTION not in text:
            errors.append(f"{name}: does not mint a GitHub App installation token")
        for secret in (APP_ID_SECRET, APP_KEY_SECRET):
            if f"secrets.{secret}" not in text:
                errors.append(f"{name}: missing App secret {secret}")
        for setting in (
            "owner: HuGR-dev",
            "repositories: corelink-server",
            "permission-metadata: read",
            "permission-contents: write",
            "permission-pull-requests: write",
        ):
            if setting not in text:
                errors.append(f"{name}: App minting is missing {setting!r}")
        if "BOT_PR_TOKEN" in text:
            errors.append(f"{name}: legacy BOT_PR_TOKEN path remains")
        if "gh pr create" not in text:
            errors.append(f"{name}: no gh pr create command found")

        if name in FORK_SAFE_PR_WORKFLOWS:
            if re.search(r"(?m)^  pull-requests:\s*write\s*$", text):
                errors.append(
                    f"{name}: pull_request jobs must not request pull-requests: write"
                )
            pr_job = text.split("\n  regenerate-and-pr:", 1)[0]
            pr_checkouts = re.findall(
                r"uses: actions/checkout@.*?(?=\n\s*- name:|\Z)",
                pr_job,
                re.S,
            )
            if not any("persist-credentials: false" in block for block in pr_checkouts):
                errors.append(
                    f"{name}: pull_request checkout must not persist GITHUB_TOKEN credentials"
                )

        step = _pr_step(text)
        if "BOT_APP_TOKEN" not in step or APP_TOKEN_OUTPUT not in step:
            errors.append(f"{name}: PR step is not bound to the minted App token")
        if re.search(r"secrets\.GITHUB_TOKEN|github\.token|secrets\.BOT_PR_TOKEN", step):
            errors.append(f"{name}: PR step still uses an Actions token")
        if "BOT_APP_TOKEN:?" not in step and ': "${BOT_APP_TOKEN:?' not in step:
            errors.append(f"{name}: PR step lacks a fail-closed App-token preflight")
        if any(re.search(r"\|\|\s*true", command) for command in _pr_create_commands(step)):
            errors.append(f"{name}: PR creation failure is being swallowed")

    # A branch push performed by checkout must use the same dedicated token.
    for name in discovered:
        path = workflows / name
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        checkout_blocks = re.findall(r"uses: actions/checkout@.*?(?=\n\s*- name:|\Z)", text, re.S)
        if not any(f"token: ${{{{ {APP_TOKEN_OUTPUT} }}}}" in block for block in checkout_blocks):
            errors.append(f"{name}: checkout that pushes the bot branch is not bound to the minted App token")

    okf = workflows / "okf-autoreconcile.yml"
    if okf.is_file():
        text = okf.read_text(encoding="utf-8")
        if APP_ACTION not in text or f"secrets.{APP_ID_SECRET}" not in text or f"secrets.{APP_KEY_SECRET}" not in text:
            errors.append("okf-autoreconcile.yml: App token minting is incomplete")
        if f"token: ${{{{ {APP_TOKEN_OUTPUT} }}}}" not in text:
            errors.append("okf-autoreconcile.yml: checkout is not bound to the minted App token")
        if "persist-credentials: false" not in text:
            errors.append("okf-autoreconcile.yml: checkout must not persist credentials")
        if "gh auth setup-git" not in text:
            errors.append("okf-autoreconcile.yml: PR step lacks explicit checkout push auth")
        if "OKF_BOT_PAT" in text or "BOT_PR_TOKEN" in text:
            errors.append("okf-autoreconcile.yml: legacy bot credential fallback remains")

    release = workflows / "release-notes.yml"
    if release.is_file():
        text = release.read_text(encoding="utf-8")
        if f"token: ${{{{ {APP_TOKEN_OUTPUT} }}}}" not in text[text.find("Create draft GitHub Release") :]:
            errors.append("release-notes.yml: draft release action is not bound to the minted App token")

    gate = workflows / "bot-pr-has-checks.yml"
    if not gate.is_file():
        errors.append("missing compensating bot-pr-has-checks gate")
    else:
        text = gate.read_text(encoding="utf-8")
        for needle in ("schedule:", "check-runs", "/status?", "--paginate", "GRACE_MINUTES"):
            if needle not in text:
                errors.append(f"bot-pr-has-checks.yml: gate missing {needle!r}")
        for marker in AUTO_PR_HEAD_MARKERS:
            if marker not in text:
                errors.append(f"bot-pr-has-checks.yml: gate missing marker {marker!r}")
        if "BOT_LOGIN_ALLOWLIST" in text or "def is_bot(" in text:
            errors.append("bot-pr-has-checks.yml: candidate selection depends on actor identity")
        if 'verdict = "present (not proof of success)"' not in text or 'verdict = "gated"' in text:
            errors.append("bot-pr-has-checks.yml: count-only verdict must not claim successful gating")
        if "Inspect the PR approval banner" not in text or "zero-job run is not CI proof" not in text:
            errors.append("bot-pr-has-checks.yml: missing approval/startup diagnostic guidance")

    return errors


def verify_b012_backlog(root: Path = ROOT) -> list[str]:
    path = root / "BACKLOG.md"
    if not path.is_file():
        return ["B-012: BACKLOG.md is missing"]
    text = path.read_text(encoding="utf-8")
    start = text.find("### B-012 —")
    end = text.find("### B-013 —", start + 1)
    if start < 0 or end < 0:
        return ["B-012: BACKLOG section boundaries are missing"]
    section = text[start:end]
    errors = [
        f"B-012: BACKLOG lost {marker!r}"
        for marker in B012_BACKLOG_REQUIRED
        if marker not in section
    ]
    if "bot-opened PRs arrive with zero checks" in section or "until a bot-opened PR shows checks" in section:
        errors.append("B-012: BACKLOG treats token author or check presence as CI proof")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    root = args.root.resolve()
    errors = verify(root) + verify_b012_backlog(root)
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("B-012 bot-PR credential and compensating-gate contract: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
