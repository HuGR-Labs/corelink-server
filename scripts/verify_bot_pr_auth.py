#!/usr/bin/env python3
"""Verify the B-012 bot-PR credential contract without contacting GitHub.

The owner action provisions ``BOT_PR_TOKEN`` out of band.  This check keeps a
future workflow edit from silently reverting to ``GITHUB_TOKEN`` (whose events
do not schedule pull-request workflows) or to an optional fallback.
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

AUTO_PR_HEAD_MARKERS = (
    "bot/subprocessors-sync-",
    "auto/okf-reconcile-",
    "auto/compliance-digest-",
    "bot/api-reference-sync-",
    "releases/v",
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
        if "secrets.BOT_PR_TOKEN" not in text:
            errors.append(f"{name}: does not consume secrets.BOT_PR_TOKEN")
        if "gh pr create" not in text:
            errors.append(f"{name}: no gh pr create command found")

        step = _pr_step(text)
        if "BOT_PR_TOKEN" not in step:
            errors.append(f"{name}: PR step is not bound to BOT_PR_TOKEN")
        if re.search(r"secrets\.GITHUB_TOKEN|github\.token", step):
            errors.append(f"{name}: PR step still uses an Actions token")
        if "BOT_PR_TOKEN:?" not in step and ': "${BOT_PR_TOKEN:?' not in step:
            errors.append(f"{name}: PR step lacks a fail-closed token preflight")
        if any(re.search(r"\|\|\s*true", command) for command in _pr_create_commands(step)):
            errors.append(f"{name}: PR creation failure is being swallowed")

    # A branch push performed by checkout must use the same dedicated token.
    for name in discovered:
        path = workflows / name
        if not path.is_file():
            continue
        text = path.read_text(encoding="utf-8")
        checkout_blocks = re.findall(r"uses: actions/checkout@.*?(?=\n\s*- name:|\Z)", text, re.S)
        if not any("token: ${{ secrets.BOT_PR_TOKEN }}" in block for block in checkout_blocks):
            errors.append(f"{name}: checkout that pushes the bot branch is not bound to BOT_PR_TOKEN")

    okf = workflows / "okf-autoreconcile.yml"
    if okf.is_file():
        text = okf.read_text(encoding="utf-8")
        if "token: ${{ secrets.BOT_PR_TOKEN }}" not in text:
            errors.append("okf-autoreconcile.yml: checkout is not bound to BOT_PR_TOKEN")
        if "persist-credentials: false" not in text:
            errors.append("okf-autoreconcile.yml: checkout must not persist credentials")
        if "gh auth setup-git" not in text:
            errors.append("okf-autoreconcile.yml: PR step lacks explicit checkout push auth")
        if "OKF_BOT_PAT" in text:
            errors.append("okf-autoreconcile.yml: optional OKF_BOT_PAT fallback reintroduces an ungated PR path")

    release = workflows / "release-notes.yml"
    if release.is_file():
        text = release.read_text(encoding="utf-8")
        if "token: ${{ secrets.BOT_PR_TOKEN }}" not in text[text.find("Create draft GitHub Release") :]:
            errors.append("release-notes.yml: draft release action is not bound to BOT_PR_TOKEN")

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

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    errors = verify(args.root.resolve())
    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print("B-012 bot-PR credential and compensating-gate contract: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
