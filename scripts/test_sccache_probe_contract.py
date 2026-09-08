#!/usr/bin/env python3
"""Closed-world checks for the archived cargo latency workflow contract."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
NEEDLE = "cargo-cache-latency-" + "probe.yml"
WORKFLOW = ".github/workflows/" + NEEDLE
ALLOWED_REFERENCES = {
    "BACKLOG.md",
    "docs/campaigns/remediation/devenv-manifest.tsv",
    # The work package keeps the archived filename in its historical
    # allowlist/non-goals prose; it is not an instruction to dispatch it.
    "docs/campaigns/remediation/work-packages/B091-B130.md",
    "docs/internal/sccache-pilot-diagnosis-2026-08-04.md",
    "docs/internal/secrets-checklist.md",
    # The owner packet retains the archived path as a historical evidence
    # reference while the item remains open; it has no active dispatch step.
    "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json",
}
OWNER_PACKET = "docs/handoff/2026-09-05-owner-action-packets-b008-b154.json"
ACTIVE_DISPATCH_PATTERNS = (
    re.compile(r"\bgh\s+workflow\s+(?:run|dispatch)\b", re.IGNORECASE),
    re.compile(r"\bworkflow_dispatch\b", re.IGNORECASE),
)


def _contains_active_probe_dispatch(text: str) -> bool:
    """Reject executable dispatch instructions for this archived probe only."""
    for line in text.splitlines():
        if NEEDLE.lower() in line.lower() and any(
            pattern.search(line) for pattern in ACTIVE_DISPATCH_PATTERNS
        ):
            return True
    return False


def _tracked_references() -> list[tuple[str, str]]:
    paths = subprocess.check_output(
        ["git", "ls-files", "-z"], cwd=ROOT, text=False
    ).split(b"\0")
    found: list[tuple[str, str]] = []
    for raw_path in paths:
        if not raw_path:
            continue
        relative = raw_path.decode("utf-8")
        try:
            text = (ROOT / relative).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if NEEDLE in text:
            found.append((relative, text))
    return found


def _archive_errors(references: list[tuple[str, str]]) -> list[str]:
    errors: list[str] = []
    unexpected = sorted(path for path, _ in references if path not in ALLOWED_REFERENCES)
    if unexpected:
        errors.append(f"unexpected tracked references: {', '.join(unexpected)}")

    documents = dict(references)
    required_markers = {
        "BACKLOG.md": ("foi arquivada", "não existe"),
        "docs/campaigns/remediation/devenv-manifest.tsv": (
            "ARQUIVAR",
            "absent from origin/main",
        ),
        "docs/internal/sccache-pilot-diagnosis-2026-08-04.md": (
            "workflow is archived",
            "manual, non-CI",
        ),
        "docs/internal/secrets-checklist.md": (
            f"former `{NEEDLE}` workflow was archived",
        ),
        OWNER_PACKET: (
            "historical, non-executable reference only",
            "archived; do not dispatch",
        ),
    }
    for path, markers in required_markers.items():
        text = documents.get(path, "")
        for marker in markers:
            if marker not in text:
                errors.append(f"{path} lacks archival marker: {marker!r}")

    stale_dispatch = f"Dispatch `{NEEDLE}`"
    for path, text in references:
        if stale_dispatch in text:
            errors.append(f"{path} contains an active dispatch instruction")
        if _contains_active_probe_dispatch(text):
            errors.append(f"{path} contains an executable GitHub workflow dispatch instruction")

    return errors


def _assert_dispatch_mutations_fail() -> None:
    references = _tracked_references()
    mutations = (
        f"\n# mutation: gh workflow run {WORKFLOW}\n",
        f"\n# mutation: gh workflow dispatch {WORKFLOW}\n",
        f"\n# mutation: workflow_dispatch for {NEEDLE}\n",
    )
    for path, original in references:
        if path not in ALLOWED_REFERENCES:
            continue
        for mutation in mutations:
            mutated = [
                (candidate_path, original + mutation if candidate_path == path else text)
                for candidate_path, text in references
            ]
            if not _archive_errors(mutated):
                raise AssertionError(f"dispatch mutation accepted for {path}: {mutation.strip()}")


def verify() -> None:
    errors: list[str] = []
    if (ROOT / WORKFLOW).exists():
        errors.append(f"archived workflow was resurrected: {WORKFLOW}")
    errors.extend(_archive_errors(_tracked_references()))

    if errors:
        raise AssertionError("\n".join(errors))


if __name__ == "__main__":
    verify()
    _assert_dispatch_mutations_fail()
    print("sccache probe archive contract: PASS")
