#!/usr/bin/env python3
"""Hermetic B-049 proofs: strict checkpoint reachability and first blob anchors."""

from __future__ import annotations

import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VALIDATE = ROOT / "scripts" / "validate_okf.py"
REANCHOR = ROOT / "scripts" / "okf_reanchor.py"
RECONCILE = ROOT / "scripts" / "okf_reconcile.py"


def git(repo: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=repo, check=True, capture_output=True, text=True
    ).stdout.strip()


def setup(repo: Path) -> None:
    git(repo, "init", "-q", "-b", "main")
    git(repo, "config", "user.email", "b049@test.invalid")
    git(repo, "config", "user.name", "B-049")
    git(repo, "config", "commit.gpgsign", "false")


def write_concept(repo: Path, checkpoint: str, *, source_blobs: str = "") -> None:
    (repo / "docs/knowledge").mkdir(parents=True, exist_ok=True)
    (repo / "docs/knowledge/index.md").write_text(
        "---\ntype: Index\nokf_version: '0.1'\nprofile_version: '0.1'\n---\n"
        "# index\n- [x](/ops/x.md)\n",
        encoding="utf-8",
    )
    blob_block = f"source_blobs:\n  - \"src.txt@{source_blobs}\"\n" if source_blobs else ""
    (repo / "docs/knowledge/ops").mkdir(parents=True, exist_ok=True)
    (repo / "docs/knowledge/ops/x.md").write_text(
        f"---\n"
        f"type: Runbook\n"
        f"title: B-049 hermetic concept\n"
        f"description: strict anchor test\n"
        f"source_files:\n  - src.txt\n"
        f"{blob_block}"
        f"checkpoint_sha: \"{checkpoint}\"\n"
        f"provenance: AUTHORED\n---\n\n"
        f"# B-049 hermetic concept\n\n"
        f"Reconciled body.\n\n"
        f"# How it works\n- the source (`src.txt:1`).\n\n"
        f"# Invariants\n- the source remains authoritative (`src.txt:1`).\n\n"
        f"# Citations\n1. `src.txt:1` — source.\n",
        encoding="utf-8",
    )


def validate(repo: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(VALIDATE), "--bundle", "docs/knowledge", "--manifest", "/missing"],
        cwd=repo,
        capture_output=True,
        text=True,
    )


def test_unreachable_commit_fails_even_when_object_is_present() -> None:
    with tempfile.TemporaryDirectory(prefix="okf-b049-reach-") as raw:
        repo = Path(raw)
        setup(repo)
        (repo / "src.txt").write_text("stable\n", encoding="utf-8")
        git(repo, "add", "src.txt")
        git(repo, "commit", "-qm", "base")
        git(repo, "checkout", "-qb", "abandoned")
        (repo / "dead.txt").write_text("present object\n", encoding="utf-8")
        git(repo, "add", "dead.txt")
        git(repo, "commit", "-qm", "unreachable checkpoint")
        orphan = git(repo, "rev-parse", "HEAD")
        git(repo, "checkout", "-q", "main")
        git(repo, "branch", "-Dq", "abandoned")
        # The object remains in this clone, so a presence-only check would pass.
        assert git(repo, "cat-file", "-t", orphan) == "commit"
        write_concept(repo, orphan)
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "concept")
        result = validate(repo)
        output = result.stdout + result.stderr
        assert result.returncode != 0
        assert "[C4]" in output and "not reachable from HEAD" in output
        worklist = subprocess.run(
            ["python3", str(RECONCILE), "--json", "--bundle", "docs/knowledge"],
            cwd=repo,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
        assert "not reachable from HEAD" in worklist


def test_malformed_frontmatter_fails_closed() -> None:
    with tempfile.TemporaryDirectory(prefix="okf-b049-frontmatter-") as raw:
        repo = Path(raw)
        setup(repo)
        (repo / "src.txt").write_text("stable\n", encoding="utf-8")
        git(repo, "add", "src.txt")
        git(repo, "commit", "-qm", "base")
        base = git(repo, "rev-parse", "HEAD")
        write_concept(repo, base)
        concept = repo / "docs/knowledge/ops/x.md"
        concept.write_text(concept.read_text(encoding="utf-8").replace("type: Runbook", "type: [Runbook"), encoding="utf-8")
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "malformed frontmatter")
        result = validate(repo)
        output = result.stdout + result.stderr
        assert result.returncode != 0
        assert "[C1]" in output and "frontmatter" in output


def test_first_blob_anchor_is_created_and_validated() -> None:
    with tempfile.TemporaryDirectory(prefix="okf-b049-first-") as raw:
        repo = Path(raw)
        setup(repo)
        (repo / "src.txt").write_text("stable\n", encoding="utf-8")
        git(repo, "add", "src.txt")
        git(repo, "commit", "-qm", "base")
        base = git(repo, "rev-parse", "HEAD")
        write_concept(repo, base)
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "concept")
        concept = repo / "docs/knowledge/ops/x.md"
        text = concept.read_text(encoding="utf-8").replace("Reconciled body.", "First blob reconcile.")
        concept.write_text(text, encoding="utf-8")
        result = subprocess.run(
            ["python3", str(REANCHOR), "docs/knowledge/ops/x.md", "--blob", "src.txt"],
            cwd=repo,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, result.stdout + result.stderr
        blob = git(repo, "hash-object", "src.txt")
        updated = concept.read_text(encoding="utf-8")
        assert f'source_blobs:\n  - "src.txt@{blob}"' in updated
        assert validate(repo).returncode == 0


def test_generated_index_and_render_have_no_phantom_or_stale_concepts() -> None:
    docs = {
        p.relative_to(ROOT / "docs/knowledge").with_suffix("").as_posix()
        for p in (ROOT / "docs/knowledge").rglob("*.md")
        if p.name not in {"index.md", "log.md"}
    }
    index = (ROOT / "docs/knowledge/index.md").read_text(encoding="utf-8")
    links = {
        match.group(1).removesuffix(".md").lstrip("/")
        for match in __import__("re").finditer(r"\]\(/([^)]*\.md)\)", index)
    }
    assert len(docs) == 170
    assert links == docs
    rendered = (ROOT / "docs/okf-wiki-site/index.html").read_text(encoding="utf-8")
    assert "170 concepts" in rendered
    assert "phantom.md" not in rendered.lower()


def test_c5_still_reports_reachable_cited_drift() -> None:
    with tempfile.TemporaryDirectory(prefix="okf-b049-c5-") as raw:
        repo = Path(raw)
        setup(repo)
        (repo / "src.txt").write_text("original\n", encoding="utf-8")
        git(repo, "add", "src.txt")
        git(repo, "commit", "-qm", "base")
        base = git(repo, "rev-parse", "HEAD")
        write_concept(repo, base)
        git(repo, "add", "-A")
        git(repo, "commit", "-qm", "concept")
        (repo / "src.txt").write_text("changed\n", encoding="utf-8")
        git(repo, "add", "src.txt")
        git(repo, "commit", "-qm", "drift")
        result = validate(repo)
        output = result.stdout + result.stderr
        assert result.returncode != 0
        assert "[C5]" in output and "src.txt:1-1" in output


if __name__ == "__main__":
    test_unreachable_commit_fails_even_when_object_is_present()
    test_first_blob_anchor_is_created_and_validated()
    test_c5_still_reports_reachable_cited_drift()
    print("B-049: unreachable checkpoints fail closed; first blob anchors created; C5 preserved")
