#!/usr/bin/env python3
"""
GA-1 feature-freeze gate — pre-commit / pre-merge enforcement.

Canonical source: `specs/_audits/2026-05-16-ga-1-feature-freeze.md`
(read its §2 frozen surfaces + §3 allowed exceptions + §4 decision protocol
before touching this script).

The gate runs in one of three modes:

  1. **--staged** (default for pre-commit) — inspect `git diff --cached --name-only`
     and refuse if any frozen-surface file is staged without a matching
     `FREEZE-EXCEPTION:` token in the current commit's draft message (when
     available via `--commit-msg-file`).

  2. **--range <base>..<head>** (default for pre-merge / CI) — inspect every
     commit in the range and require each one whose diff touches a frozen
     surface to carry a `FREEZE-EXCEPTION: <class>` trailer in its body.

  3. **--check-empty** (audit invocation) — verify there are no *currently
     unstaged* violations of the freeze in the working tree. Used by the
     deliverable's quality gates to assert "no current violations" at commit
     authoring time. Exit 0 iff the working tree contains no untracked /
     unstaged modifications to frozen surfaces.

Exit codes:

    0 — gate passes (no violations, or every violation is justified).
    1 — gate fails (a frozen surface was modified without a matching
        `FREEZE-EXCEPTION:` token, or a token references an unknown class).
    2 — invocation error (bad flags, git not available, etc.).

Allowed `FREEZE-EXCEPTION:` classes (mirror §3 of the canonical doc):

    P0-security      — §3.a; CVSS ≥ 7.0 on a reachable surface, 2-key required.
    P1-ga-blocker    — §3.b; blocks RB-GA-CUTOVER §0 greenlight, 2-key required.
    cosmetic-doc     — §3.c; typo/link/format only, single CODEOWNER OK.
    implicit-allow   — §3.d; under _audits/_compliance/reports/etc.

Frozen surfaces (mirror §2): see FROZEN_PATH_PATTERNS / SKIP_PATH_PATTERNS below.

Usage:

    python3 scripts/check-ga-freeze-allowed.py --check-empty
    python3 scripts/check-ga-freeze-allowed.py --staged
    python3 scripts/check-ga-freeze-allowed.py --staged --commit-msg-file .git/COMMIT_EDITMSG
    python3 scripts/check-ga-freeze-allowed.py --range origin/main..HEAD
    python3 scripts/check-ga-freeze-allowed.py --range origin/main..HEAD --json

The gate is intentionally conservative: it rejects on doubt and the operator
must add a `FREEZE-EXCEPTION:` trailer with the correct class.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable

REPO_ROOT = Path(__file__).resolve().parent.parent

# --- Allowed exception classes (mirror specs/_audits/2026-05-16-ga-1-feature-freeze.md §3) ---
ALLOWED_CLASSES = {
    "P0-security",
    "P1-ga-blocker",
    "cosmetic-doc",
    "implicit-allow",
}

# Body trailer regex; matches lines like:
#   FREEZE-EXCEPTION: P0-security
#   FREEZE-EXCEPTION:cosmetic-doc
FREEZE_TRAILER_RE = re.compile(
    r"(?im)^\s*FREEZE-EXCEPTION\s*:\s*(?P<cls>[A-Za-z0-9-]+)\s*$"
)

# --- Frozen surfaces (§2 of canonical doc). Paths are matched against the
#     repo-root-relative POSIX form of each changed file. ---
FROZEN_PATH_PATTERNS: list[re.Pattern[str]] = [
    # §2.1 spec corpus — everything under specs/ except §3.d implicit-allow subtrees
    re.compile(r"^specs/(?!_audits/|_archive/|_compliance/|_schemas/|_templates/).+\.md$"),
    # §2.2 invariant registry (already covered by 2.1 but explicit for grepability)
    re.compile(r"^specs/03_architecture/invariant_registry\.md$"),
    # §2.3 ADR set
    re.compile(r"^specs/03_architecture/adrs/ADR-.+\.md$"),
    # §2.4 public API surface
    re.compile(r"^crates/corelink-api/.+$"),
    re.compile(r"^apps/server/src/routes/.+$"),
    re.compile(r"^crates/.+/src/lib\.rs$"),
    # §2.5 OpenAPI envelope
    re.compile(r"^openapi/.+\.ya?ml$"),
    # §2.6 runbooks
    re.compile(r"^specs/_runbooks/RB-.+\.md$"),
    # §2.7 dashboards
    re.compile(r"^dashboards/.+\.(json|ya?ml)$"),
    # §2.8 migrations
    re.compile(r"^migrations/.+\.sql$"),
    # §2.9 schemas
    re.compile(r"^schemas/.+\.(json|ya?ml)$"),
]

# --- §2 + §3.d implicit-allow subtrees (NOT frozen) ---
SKIP_PATH_PATTERNS: list[re.Pattern[str]] = [
    re.compile(r"^specs/_audits/.+$"),
    re.compile(r"^specs/_compliance/.+$"),
    re.compile(r"^specs/_archive/.+$"),
    re.compile(r"^specs/_schemas/.+$"),
    re.compile(r"^specs/_templates/.+$"),
    re.compile(r"^reports/.+$"),
    re.compile(r"^mutants\.out(?:\.old)?/.+$"),
    re.compile(r"^target/.+$"),
    re.compile(r"^node_modules/.+$"),
    re.compile(r"^_archive/.+$"),
    re.compile(r"^CHANGELOG\.md$"),
    re.compile(r"^TODO\.md$"),
    re.compile(r"^ROADMAP-TO-GA\.md$"),
]


def is_frozen(path: str) -> bool:
    """Return True iff `path` (repo-root-relative POSIX) is on a frozen surface."""
    for skip in SKIP_PATH_PATTERNS:
        if skip.search(path):
            return False
    for frozen in FROZEN_PATH_PATTERNS:
        if frozen.search(path):
            return True
    return False


def run_git(args: list[str]) -> str:
    """Run `git <args>` from REPO_ROOT, returning stdout. Empty string on non-zero
    when the command is a benign "no output" case (e.g. clean diff)."""
    try:
        result = subprocess.run(
            ["git", *args],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
    except FileNotFoundError:
        print("error: git not found on PATH", file=sys.stderr)
        sys.exit(2)
    if result.returncode != 0 and result.stderr.strip():
        # Diff/log returning non-zero with stderr is a real failure.
        print(f"git {' '.join(args)} failed: {result.stderr.strip()}", file=sys.stderr)
        sys.exit(2)
    return result.stdout


def staged_files() -> list[str]:
    out = run_git(["diff", "--cached", "--name-only"])
    return [line.strip() for line in out.splitlines() if line.strip()]


def unstaged_or_untracked_files() -> list[str]:
    """Files modified but not staged + untracked files."""
    out = run_git(["status", "--porcelain"])
    paths: list[str] = []
    for line in out.splitlines():
        if not line.strip():
            continue
        # Porcelain format: XY <path>, where X=index, Y=worktree
        if len(line) < 4:
            continue
        worktree_flag = line[1]
        path = line[3:].strip()
        # Strip rename arrow if present: "old -> new"
        if " -> " in path:
            path = path.split(" -> ", 1)[1]
        # Strip surrounding quotes git adds for paths with special chars.
        if path.startswith('"') and path.endswith('"'):
            path = path[1:-1]
        # We care about anything not fully committed: worktree dirty OR untracked.
        if worktree_flag != " " or line.startswith("??"):
            paths.append(path)
    return paths


def commits_in_range(rev_range: str) -> list[str]:
    # Include merge commits so a `FREEZE-EXCEPTION` trailer attached to the
    # merge itself is honoured (regression net for W26-P2-01: merge-commit-only
    # trailer would otherwise be silently dropped under `--no-merges`).
    # `files_in_commit` handles merge vs non-merge diffing below.
    out = run_git(["rev-list", rev_range])
    return [line.strip() for line in out.splitlines() if line.strip()]


def is_merge_commit(sha: str) -> bool:
    """Return True iff `sha` has more than one parent."""
    out = run_git(["rev-list", "--parents", "-n", "1", sha]).strip()
    if not out:
        return False
    # Format: "<sha> <parent1> [<parent2> ...]"
    parts = out.split()
    return len(parts) > 2


def files_in_commit(sha: str) -> list[str]:
    # For non-merge commits, plain `diff-tree -r <sha>` shows the diff vs the
    # single parent. For merge commits, plain `diff-tree` is silent by default,
    # which would let frozen-surface changes introduced by the merge slip
    # through. We use `-m --first-parent` for merges so the diff is computed
    # against the first parent (the branch being merged into), capturing the
    # net-incoming files brought in by the merge.
    if is_merge_commit(sha):
        out = run_git(
            ["diff-tree", "-m", "--first-parent", "--no-commit-id",
             "--name-only", "-r", sha]
        )
    else:
        out = run_git(["diff-tree", "--no-commit-id", "--name-only", "-r", sha])
    # De-duplicate while preserving order (merge diffs may repeat under `-m`).
    seen: set[str] = set()
    files: list[str] = []
    for line in out.splitlines():
        stripped = line.strip()
        if stripped and stripped not in seen:
            seen.add(stripped)
            files.append(stripped)
    return files


def commit_message(sha: str) -> str:
    return run_git(["log", "-1", "--format=%B", sha])


def extract_exception_classes(message: str) -> list[str]:
    return [m.group("cls") for m in FREEZE_TRAILER_RE.finditer(message)]


def classify_message(message: str) -> tuple[bool, list[str], list[str]]:
    """Return (ok, recognised_classes, unknown_classes)."""
    classes = extract_exception_classes(message)
    if not classes:
        return False, [], []
    recognised = [c for c in classes if c in ALLOWED_CLASSES]
    unknown = [c for c in classes if c not in ALLOWED_CLASSES]
    # Gate passes iff at least one recognised class AND no unknown classes.
    return (bool(recognised) and not unknown), recognised, unknown


# --- Mode implementations -----------------------------------------------------


def mode_check_empty(emit_json: bool) -> int:
    """Verify no working-tree violation of the freeze right now."""
    dirty = unstaged_or_untracked_files()
    violators = [p for p in dirty if is_frozen(p)]
    payload = {
        "mode": "check-empty",
        "dirty_files": dirty,
        "frozen_violations": violators,
        "ok": not violators,
    }
    if emit_json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        if violators:
            print("freeze-check FAIL — frozen surfaces have uncommitted changes:")
            for path in violators:
                print(f"  • {path}")
            print(
                "\nHint: stage + commit these with a "
                "'FREEZE-EXCEPTION: <class>' trailer, or stash them."
            )
        else:
            print("freeze-check OK — no working-tree violations.")
    return 0 if not violators else 1


def mode_staged(emit_json: bool, commit_msg_file: str | None) -> int:
    """Inspect staged files; require a draft commit message with the trailer
    when frozen surfaces are touched."""
    staged = staged_files()
    violators = [p for p in staged if is_frozen(p)]
    msg_text = ""
    if commit_msg_file:
        try:
            msg_text = Path(commit_msg_file).read_text(encoding="utf-8")
        except OSError as exc:
            print(f"error reading commit-msg file: {exc}", file=sys.stderr)
            return 2
    ok_msg, recognised, unknown = classify_message(msg_text)
    if violators and not msg_text:
        # No commit message available yet (e.g. pre-commit hook with no -m).
        # Refuse defensively — the operator must reinvoke with --commit-msg-file
        # or use the --range mode at PR time.
        payload = {
            "mode": "staged",
            "staged": staged,
            "frozen_violations": violators,
            "commit_msg_available": False,
            "ok": False,
        }
        if emit_json:
            print(json.dumps(payload, indent=2, sort_keys=True))
        else:
            print("freeze-check FAIL — frozen surfaces are staged but no")
            print("commit message is available to inspect for a")
            print("FREEZE-EXCEPTION: trailer. Re-run with --commit-msg-file")
            print("or rely on the --range mode at PR time.")
        return 1
    ok = (not violators) or ok_msg
    payload = {
        "mode": "staged",
        "staged": staged,
        "frozen_violations": violators,
        "commit_msg_available": bool(msg_text),
        "recognised_classes": recognised,
        "unknown_classes": unknown,
        "ok": ok,
    }
    if emit_json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        if ok:
            if violators:
                print(
                    "freeze-check OK — frozen surfaces touched but justified "
                    f"({', '.join(recognised)})."
                )
            else:
                print("freeze-check OK — no frozen surfaces staged.")
        else:
            print("freeze-check FAIL — frozen surfaces touched without a")
            print("valid FREEZE-EXCEPTION: trailer.")
            if unknown:
                print(f"  unknown classes: {', '.join(unknown)}")
            print(f"  allowed classes: {', '.join(sorted(ALLOWED_CLASSES))}")
            print("  violating paths:")
            for path in violators:
                print(f"    • {path}")
    return 0 if ok else 1


def mode_range(rev_range: str, emit_json: bool) -> int:
    shas = commits_in_range(rev_range)
    rows: list[dict] = []
    any_fail = False
    for sha in shas:
        files = files_in_commit(sha)
        violators = [p for p in files if is_frozen(p)]
        if not violators:
            rows.append(
                {
                    "sha": sha,
                    "frozen_violations": [],
                    "ok": True,
                }
            )
            continue
        ok, recognised, unknown = classify_message(commit_message(sha))
        rows.append(
            {
                "sha": sha,
                "frozen_violations": violators,
                "recognised_classes": recognised,
                "unknown_classes": unknown,
                "ok": ok,
            }
        )
        if not ok:
            any_fail = True
    payload = {
        "mode": "range",
        "range": rev_range,
        "commits": rows,
        "ok": not any_fail,
    }
    if emit_json:
        print(json.dumps(payload, indent=2, sort_keys=True))
    else:
        if any_fail:
            print(f"freeze-check FAIL across range {rev_range}:")
            for row in rows:
                if not row.get("ok", True):
                    print(f"  • {row['sha']}:")
                    for path in row["frozen_violations"]:
                        print(f"      - {path}")
                    if row.get("unknown_classes"):
                        print(
                            "      unknown classes: "
                            + ", ".join(row["unknown_classes"])
                        )
        else:
            print(f"freeze-check OK across range {rev_range}.")
    return 0 if not any_fail else 1


# --- CLI ---------------------------------------------------------------------


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description=(
            "GA-1 feature-freeze gate (see "
            "specs/_audits/2026-05-16-ga-1-feature-freeze.md)."
        )
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--staged",
        action="store_true",
        help="Inspect `git diff --cached` (pre-commit mode).",
    )
    mode.add_argument(
        "--range",
        dest="rev_range",
        metavar="A..B",
        help="Inspect every commit in `A..B` (pre-merge / CI mode).",
    )
    mode.add_argument(
        "--check-empty",
        action="store_true",
        help="Audit-only: assert the working tree has no freeze violations.",
    )
    parser.add_argument(
        "--commit-msg-file",
        help="Optional path to a draft commit message; used with --staged.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON instead of human-readable output.",
    )
    return parser


def main(argv: Iterable[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(list(argv) if argv is not None else None)
    if args.check_empty:
        return mode_check_empty(args.json)
    if args.rev_range:
        return mode_range(args.rev_range, args.json)
    # Default to --staged
    return mode_staged(args.json, args.commit_msg_file)


if __name__ == "__main__":
    sys.exit(main())
