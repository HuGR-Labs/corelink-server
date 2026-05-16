#!/usr/bin/env python3
"""
test_check_ga_freeze_allowed.py — pytest suite for `scripts/check-ga-freeze-allowed.py`.

Scope:

The GA-1 feature-freeze gate enumerates commits in a `--range A..B` and
requires every commit that touches a frozen surface (per
`specs/_audits/2026-05-16-ga-1-feature-freeze.md §2`) to carry a recognised
`FREEZE-EXCEPTION:` trailer in its body. The regression target of this test
suite is **W26-P2-01** — pre-fix the gate passed `--no-merges` to `git log`,
which silently dropped any `FREEZE-EXCEPTION` trailer that lived only on a
merge commit. The fix scans merge commits as well; this suite is the net.

The script is loaded via `importlib` so the hyphenated filename works, and
each test spins a throw-away git repo via `tmp_path` so commits exercise the
real `subprocess.run(['git', ...])` paths in the script. The script's
`REPO_ROOT` constant is monkey-patched at the module level to point at each
per-test fixture repo.

Run:
    python3 -m pytest tests/test_check_ga_freeze_allowed.py -v
"""

from __future__ import annotations

import importlib.util
import io
import os
import subprocess
import sys
from contextlib import redirect_stdout
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT_PATH = REPO_ROOT / "scripts" / "check-ga-freeze-allowed.py"


def _load_script_module():
    spec = importlib.util.spec_from_file_location(
        "check_ga_freeze_allowed", SCRIPT_PATH
    )
    assert spec is not None, f"could not load {SCRIPT_PATH}"
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


SCRIPT = _load_script_module()


# --- git fixture helpers ------------------------------------------------------


def _git(repo: Path, *args: str, env: dict[str, str] | None = None) -> str:
    """Run a git command in `repo` and return stdout (raises on non-zero)."""
    full_env = os.environ.copy()
    full_env.update(
        {
            "GIT_AUTHOR_NAME": "Test Author",
            "GIT_AUTHOR_EMAIL": "test@example.invalid",
            "GIT_COMMITTER_NAME": "Test Author",
            "GIT_COMMITTER_EMAIL": "test@example.invalid",
        }
    )
    if env:
        full_env.update(env)
    result = subprocess.run(
        ["git", *args],
        cwd=repo,
        capture_output=True,
        text=True,
        check=True,
        env=full_env,
    )
    return result.stdout


def _init_repo(repo: Path) -> None:
    repo.mkdir(parents=True, exist_ok=True)
    _git(repo, "init", "--initial-branch=main", "--quiet")
    _git(repo, "config", "commit.gpgsign", "false")
    # Seed an empty initial commit so we always have a baseline ref.
    _git(repo, "commit", "--allow-empty", "-m", "seed: initial commit")


def _write(repo: Path, rel: str, content: str = "x\n") -> None:
    path = repo / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _commit(repo: Path, message: str, *paths: str) -> str:
    for p in paths:
        _git(repo, "add", "--", p)
    _git(repo, "commit", "-m", message)
    return _git(repo, "rev-parse", "HEAD").strip()


@pytest.fixture
def repo(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """A throw-away git repo with the script's REPO_ROOT redirected to it."""
    r = tmp_path / "freeze-fixture"
    _init_repo(r)
    monkeypatch.setattr(SCRIPT, "REPO_ROOT", r)
    return r


def _run_range(rev_range: str) -> tuple[int, str]:
    """Invoke `mode_range` and capture (exit_code, stdout)."""
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = SCRIPT.mode_range(rev_range, emit_json=False)
    return rc, buf.getvalue()


# --- tests --------------------------------------------------------------------


def test_linear_range_non_frozen_only_passes(repo: Path) -> None:
    """Test 1 — Linear range with one commit on a NON-frozen surface PASSES.

    The script's frozen-surface set is strictly defined; a change under
    `specs/_audits/` (an `implicit-allow` subtree) is the cleanest non-frozen
    analogue of a `fix`-style commit.
    """
    _write(repo, "specs/_audits/example.md", "# audit note\n")
    _commit(repo, "audit: add note (non-frozen surface)",
            "specs/_audits/example.md")
    rc, out = _run_range("main~1..main")
    assert rc == 0, out
    assert "OK" in out


def test_linear_range_frozen_without_trailer_fails(repo: Path) -> None:
    """Test 2 — Linear range with one commit on a FROZEN surface and no
    `FREEZE-EXCEPTION:` trailer FAILS.

    Frozen surface: `specs/03_architecture/invariant_registry.md` (§2.2 of
    the freeze declaration).
    """
    _write(repo, "specs/03_architecture/invariant_registry.md",
           "# INV registry\n")
    _commit(
        repo,
        "feat(invariants): add new INV row (no trailer)",
        "specs/03_architecture/invariant_registry.md",
    )
    rc, out = _run_range("main~1..main")
    assert rc == 1, out
    assert "FAIL" in out


def test_linear_range_frozen_with_valid_trailer_passes(repo: Path) -> None:
    """Test 3 — Linear range with one frozen-surface commit carrying a
    recognised `FREEZE-EXCEPTION:` trailer PASSES.
    """
    _write(repo, "specs/03_architecture/invariant_registry.md",
           "# INV registry v2\n")
    _commit(
        repo,
        "fix(invariants): tighten INV-CAS-001 wording\n\n"
        "FREEZE-EXCEPTION: cosmetic-doc\n",
        "specs/03_architecture/invariant_registry.md",
    )
    rc, out = _run_range("main~1..main")
    assert rc == 0, out
    assert "OK" in out


def test_merge_commit_with_trailer_passes(repo: Path) -> None:
    """Test 4 — REGRESSION NET for W26-P2-01.

    A merge commit that itself introduces frozen-surface content (an "evil
    merge" — conflict resolution that touches a frozen path) and carries a
    valid `FREEZE-EXCEPTION:` trailer MUST PASS. Pre-fix the merge was
    skipped by `--no-merges` and its frozen-surface changes were invisible
    to the gate; post-fix the merge is scanned and its trailer is honoured.
    """
    # Create a divergent main-line edit so the merge will be a real merge
    # commit, then add NEW frozen-surface content at merge time only.
    _git(repo, "checkout", "-b", "feat/branch-side", "--quiet")
    _write(repo, "docs/branch-marker.md", "branch side\n")
    _commit(repo, "docs: branch-side marker", "docs/branch-marker.md")
    _git(repo, "checkout", "main", "--quiet")
    _write(repo, "docs/main-marker.md", "main side\n")
    _commit(repo, "docs: main-side marker", "docs/main-marker.md")
    # Do the merge without committing so we can inject the evil-merge content.
    _git(repo, "merge", "--no-ff", "--no-commit", "feat/branch-side")
    _write(repo, "openapi/corelink.yaml", "openapi: 3.1.0\n")
    _git(repo, "add", "--", "openapi/corelink.yaml")
    _git(
        repo,
        "commit",
        "-m",
        "merge feat/branch-side into main (evil merge: openapi bump)\n\n"
        "FREEZE-EXCEPTION: P1-ga-blocker\n",
    )
    # `main^..main` ranges from the first parent of the merge (the main-side
    # marker commit) to the merge tip. That set is {merge, branch-side
    # marker}. The branch-side marker only touches `docs/` (non-frozen); the
    # merge itself introduces the frozen-surface change (openapi) and carries
    # the trailer that justifies it.
    rc, out = _run_range("main^..main")
    assert rc == 0, out
    assert "OK" in out


def test_merge_without_trailer_plus_frozen_source_commit_fails(
    repo: Path,
) -> None:
    """Test 5 — REGRESSION NET (other side) for W26-P2-01.

    A merge commit with no `FREEZE-EXCEPTION` trailer combined with a source
    commit that touches a frozen surface (and lacks its own trailer) MUST
    FAIL. Proves the fix doesn't accidentally wave through feature merges.
    """
    _git(repo, "checkout", "-b", "feat/freeze-unprotected", "--quiet")
    _write(repo, "openapi/corelink.yaml", "openapi: 3.1.0\n")
    _commit(repo, "feat(openapi): bump version (no trailer)",
            "openapi/corelink.yaml")
    _git(repo, "checkout", "main", "--quiet")
    # Capture the seed (current main tip, pre-merge) so we can range from
    # there once the merge lands.
    seed = _git(repo, "rev-parse", "HEAD").strip()
    _git(
        repo,
        "merge",
        "--no-ff",
        "feat/freeze-unprotected",
        "-m",
        "merge feat/freeze-unprotected into main (no trailer)",
    )
    # `seed..HEAD` walks ALL reachable commits in the merge (the source feat
    # commit + the merge commit itself). The source commit touches a frozen
    # surface and carries no trailer → gate must FAIL even though the merge
    # commit also lacks one.
    rc, out = _run_range(f"{seed}..HEAD")
    assert rc == 1, out
    assert "FAIL" in out


def test_empty_range_passes(repo: Path) -> None:
    """Test 6 — Empty range (no commits) is vacuously OK."""
    rc, out = _run_range("main..main")
    assert rc == 0, out
    assert "OK" in out


def test_skip_subtrees_pass(repo: Path) -> None:
    """Test 7 — Commits that only touch implicit-allow subtrees (audits,
    compliance, reports, archive) PASS without a trailer."""
    _write(repo, "specs/_audits/foo.md", "# foo\n")
    _write(repo, "specs/_compliance/bar.md", "# bar\n")
    _write(repo, "reports/baz.json", "{}\n")
    _commit(
        repo,
        "docs: add audit/compliance/report notes (implicit-allow)",
        "specs/_audits/foo.md",
        "specs/_compliance/bar.md",
        "reports/baz.json",
    )
    rc, out = _run_range("main~1..main")
    assert rc == 0, out
    assert "OK" in out


def test_unknown_exception_class_fails(repo: Path) -> None:
    """Test 8 — A frozen-surface commit whose only `FREEZE-EXCEPTION:`
    trailer references an UNKNOWN class FAILS (gate is conservative: an
    unknown class taints the commit even if otherwise plausible).
    """
    _write(repo, "specs/03_architecture/invariant_registry.md",
           "# INV registry v3\n")
    _commit(
        repo,
        "chore(invariants)!: breaking change to INV-CAS-001\n\n"
        "FREEZE-EXCEPTION: unrecognised-class\n",
        "specs/03_architecture/invariant_registry.md",
    )
    rc, out = _run_range("main~1..main")
    assert rc == 1, out
    assert "FAIL" in out


def test_help_smoke() -> None:
    """Smoke — `--help` exits 0 (CLI parser sanity, complements DoD §6)."""
    result = subprocess.run(
        [sys.executable, str(SCRIPT_PATH), "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert "feature-freeze" in result.stdout.lower()


if __name__ == "__main__":
    sys.exit(pytest.main([__file__, "-v"]))
