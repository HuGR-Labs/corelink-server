#!/usr/bin/env python3
"""Closed-world checks for the archived cargo latency workflow contract."""

from __future__ import annotations

import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
NEEDLE = "cargo-cache-latency-" + "probe.yml"
WORKFLOW = ".github/workflows/" + NEEDLE
ALLOWED_REFERENCES = {
    "BACKLOG.md",
    "docs/campaigns/remediation/devenv-manifest.tsv",
    "docs/internal/sccache-pilot-diagnosis-2026-08-04.md",
    "docs/internal/secrets-checklist.md",
}


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


def verify() -> None:
    errors: list[str] = []
    if (ROOT / WORKFLOW).exists():
        errors.append(f"archived workflow was resurrected: {WORKFLOW}")

    references = _tracked_references()
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

    if errors:
        raise AssertionError("\n".join(errors))


if __name__ == "__main__":
    verify()
    print("sccache probe archive contract: PASS")
