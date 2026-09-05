#!/usr/bin/env python3
"""Executable B-123 regression: mutate a source above a cited range.

This is deliberately a git harness rather than a source-text grep.  It proves
the validator sees the old and new blobs and rejects the cheap re-anchor when
the citation was left behind, while accepting the same mutation with a
correctly renumbered citation.
"""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = ROOT / "scripts" / "validate_okf.py"


def run(*args: str, cwd: Path, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=check)


def write_concept(repo: Path, blob: str, cite_line: int, checkpoint: str, note: str) -> None:
    (repo / "docs/knowledge/auth").mkdir(parents=True, exist_ok=True)
    (repo / "docs/knowledge/index.md").write_text(
        "---\ntype: Index\nokf_version: '0.1'\nprofile_version: '0.1'\n---\n"
        "# index\n- [x](/auth/x.md)\n",
        encoding="utf-8",
    )
    (repo / "docs/knowledge/auth/x.md").write_text(
        f"---\ntype: AuthMechanism\ntitle: Anchor mutation\n"
        f"description: {note}\nsource_files:\n  - src.txt\nsource_blobs:\n  - src.txt@{blob}\n"
        f"checkpoint_sha: {checkpoint}\nprovenance: AUTHORED\n---\n\n"
        f"# Anchor mutation\n{note}\n\n# How it works\n"
        f"- the anchor is at `src.txt:{cite_line}`.\n\n# Invariants\n"
        f"- the anchor remains at `src.txt:{cite_line}`.\n\n# Citations\n"
        f"1. `src.txt:{cite_line}` — the anchor.\n",
        encoding="utf-8",
    )


def scenario(citation_line: int) -> str:
    with tempfile.TemporaryDirectory(prefix="okf-b123-") as raw:
        repo = Path(raw)
        run("git", "init", "-q", cwd=repo)
        run("git", "config", "user.email", "test@example.invalid", cwd=repo)
        run("git", "config", "user.name", "B-123 test", cwd=repo)
        (repo / "src.txt").write_text("alpha\nbeta\ngamma\ndelta\nepsilon\n", encoding="utf-8")
        base_blob = run("git", "hash-object", "-w", "src.txt", cwd=repo).stdout.strip()
        run("git", "add", "src.txt", cwd=repo)
        run("git", "commit", "-qm", "source base", cwd=repo)
        source_base = run("git", "rev-parse", "HEAD", cwd=repo).stdout.strip()
        write_concept(repo, base_blob, 3, source_base, "base")
        run("git", "add", "-A", cwd=repo)
        run("git", "commit", "-qm", "concept", cwd=repo)
        base = run("git", "rev-parse", "HEAD", cwd=repo).stdout.strip()

        (repo / "src.txt").write_text(
            "PREPENDED\nalpha\nbeta\ngamma\ndelta\nepsilon\n", encoding="utf-8"
        )
        moved_blob = run("git", "hash-object", "-w", "src.txt", cwd=repo).stdout.strip()
        write_concept(repo, moved_blob, citation_line, base, "source moved; citation mutation")
        run("git", "add", "-A", cwd=repo)
        run("git", "commit", "-qm", "move source", cwd=repo)
        result = run(
            "python3", str(VALIDATOR), "--bundle", "docs/knowledge",
            "--manifest", "/nonexistent-b123-manifest", "--base-ref", base,
            cwd=repo, check=False,
        )
        return result.stdout + result.stderr


def scenario_anchor_then_source_edit() -> str:
    """The new anchor has the moved block, but HEAD edits it afterward."""
    with tempfile.TemporaryDirectory(prefix="okf-b123-second-edit-") as raw:
        repo = Path(raw)
        run("git", "init", "-q", cwd=repo)
        run("git", "config", "user.email", "test@example.invalid", cwd=repo)
        run("git", "config", "user.name", "B-123 second-edit test", cwd=repo)
        (repo / "src.txt").write_text("alpha\nbeta\ngamma\n", encoding="utf-8")
        base_blob = run("git", "hash-object", "-w", "src.txt", cwd=repo).stdout.strip()
        run("git", "add", "src.txt", cwd=repo)
        run("git", "commit", "-qm", "source base", cwd=repo)
        source_base = run("git", "rev-parse", "HEAD", cwd=repo).stdout.strip()
        write_concept(repo, base_blob, 3, source_base, "base")
        run("git", "add", "-A", cwd=repo)
        run("git", "commit", "-qm", "concept", cwd=repo)
        base = run("git", "rev-parse", "HEAD", cwd=repo).stdout.strip()

        # Anchor B contains the old cited `gamma` block at line 4.
        (repo / "src.txt").write_text(
            "PREPENDED\nalpha\nbeta\ngamma\n", encoding="utf-8"
        )
        moved_blob = run("git", "hash-object", "-w", "src.txt", cwd=repo).stdout.strip()
        run("git", "add", "src.txt", cwd=repo)
        run("git", "commit", "-qm", "source anchor blob", cwd=repo)

        # HEAD subsequently rewrites the moved block.  C5 compares anchor B's
        # line 3 (`beta`) with HEAD's line 3 and therefore stays green; C5c
        # must still inspect B and reject the unchanged old citation (line 3).
        (repo / "src.txt").write_text(
            "PREPENDED\nalpha\nbeta\nGAMMA\n", encoding="utf-8"
        )
        write_concept(repo, moved_blob, 3, base, "anchor then source edit")
        run("git", "add", "-A", cwd=repo)
        run("git", "commit", "-qm", "source edit after anchor", cwd=repo)
        result = run(
            "python3", str(VALIDATOR), "--bundle", "docs/knowledge",
            "--manifest", "/nonexistent-b123-manifest", "--base-ref", base,
            cwd=repo, check=False,
        )
        return result.stdout + result.stderr


def scenario_orphaned_previous_checkpoint() -> str:
    """An orphaned legacy checkpoint must fall back to the reachable base."""
    with tempfile.TemporaryDirectory(prefix="okf-b123-orphan-") as raw:
        repo = Path(raw)
        run("git", "init", "-q", cwd=repo)
        run("git", "config", "user.email", "test@example.invalid", cwd=repo)
        run("git", "config", "user.name", "B-123 orphan test", cwd=repo)
        (repo / "src.txt").write_text("alpha\nbeta\ngamma\n", encoding="utf-8")
        run("git", "add", "src.txt", cwd=repo)
        run("git", "commit", "-qm", "source base", cwd=repo)
        # The previous concept is intentionally legacy and carries a
        # well-formed, unreachable checkpoint, as allowed by C4.
        write_concept(
            repo,
            "",
            2,
            "0123456789012345678901234567890123456789",
            "legacy orphan",
        )
        run("git", "add", "-A", cwd=repo)
        run("git", "commit", "-qm", "legacy concept", cwd=repo)
        base = run("git", "rev-parse", "HEAD", cwd=repo).stdout.strip()

        (repo / "src.txt").write_text(
            "PREPENDED\nalpha\nbeta\ngamma\n", encoding="utf-8"
        )
        moved_blob = run("git", "hash-object", "-w", "src.txt", cwd=repo).stdout.strip()
        run("git", "add", "src.txt", cwd=repo)
        run("git", "commit", "-qm", "source anchor blob", cwd=repo)
        current_checkpoint = run("git", "rev-parse", "HEAD", cwd=repo).stdout.strip()
        write_concept(repo, moved_blob, 2, current_checkpoint, "orphan anchor")
        run("git", "add", "-A", cwd=repo)
        run("git", "commit", "-qm", "add anchor", cwd=repo)
        result = run(
            "python3", str(VALIDATOR), "--bundle", "docs/knowledge",
            "--manifest", "/nonexistent-b123-manifest", "--base-ref", base,
            cwd=repo, check=False,
        )
        return result.stdout + result.stderr


def main() -> int:
    stale = scenario(3)
    assert "[C5c]" in stale, stale
    fixed = scenario(4)
    assert "[C5c]" not in fixed, fixed
    second_edit = scenario_anchor_then_source_edit()
    assert "[C5c]" in second_edit, second_edit
    orphan = scenario_orphaned_previous_checkpoint()
    assert "[C5c]" in orphan, orphan
    print(
        "B-123 anchor mutation guard: stale, post-anchor-edit, and orphaned-checkpoint "
        "citations rejected; renumbered citation accepted"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
