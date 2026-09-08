#!/usr/bin/env python3
"""
Validate the OKF-CoreLink knowledge bundle against the FROZEN profile contract.

This is the completeness oracle for the knowledge wiki (W-VALIDATOR). It is a
TRANSCRIPTION of the frozen contract at
`docs/internal/okf-wiki/01-okf-corelink-profile.contract.md` — §2 (frontmatter
schema), §3 (body conventions), §4 (checks C1-C10b + secondary C-AGE/C-REV),
§4.1 (ADR/doc sub-profile), §5 (invariants). It does NOT design beyond it.

All STRUCTURAL checks validate the working tree at HEAD (the thing being gated);
`checkpoint_sha` is used ONLY as the content baseline for the C5 freshness check.

BLOB ADDRESSING (C4b/C4c/C5, §2.2 `source_blobs`): C5 wants exactly one thing —
"the file's CONTENT at authoring time". A commit id is an indirect and fragile way
to name that: rebase, squash and cherry-pick all destroy commit ids, and the dead
object survives only in the author's clone, never in CI's fresh one. A git BLOB id
IS that content, and is immutable under all three. So a concept may anchor each
cited FILE on its blob id (`path@<40-hex>`), additively — `checkpoint_sha` stays
required for provenance and for C-AGE / C-REV / C5b, which want a commit_date.
When a file has a blob anchor, C5 compares against that blob and NOTHING else. C4b
demands TWO things of an anchor, not one: the object must resolve as a blob AND it
must be the content that path had at some commit REACHABLE FROM HEAD. Presence
alone was a timing artifact — an anchor taken from an intermediate PR commit that a
later commit on the same branch superseded is reachable only from the PR head ref,
which GitHub auto-deletes at merge, so the `push:main` run that clones seconds
after the merge still resolves it (green) while every clone after it cannot (red).
Reachability is a property of the history being gated, so it answers the same at
merge time and forever after. That is
the point — it deletes the recurring re-anchor tax (a rebase that leaves the cited
file byte-identical can no longer make its citations read STALE) and it closes the
laundering residual the tolerance accepted (a forged unreachable blob is an absent
object, so it REDs C4b instead of re-anchoring to a base ref that already contains
the landed drift). C4c keeps that closure from merely MOVING: an anchored path may
not lose its anchor while it is still a declared source.

CHECKPOINT REACHABILITY (C4/C5) — every `checkpoint_sha` must name a commit
reachable from the gated `HEAD`. A squash/rebase can orphan the pre-merge branch
tip, but that is precisely a broken provenance anchor: the local clone and the CI
clone must resolve the same baseline. The validator therefore fails closed on a
missing commit and on a commit object that is present locally but is not an
ancestor of `HEAD`; there is no base-ref fallback and no orphan tolerance. The
first reconcile of a concept creates immutable per-file `source_blobs` anchors,
so future freshness checks name the authored content directly and survive history
rewriting.

C5 freshness mechanism (CONTENT-ANCHOR): for each cited `path:Lx-Ly`, compare the
CONTENT of lines Lx..Ly of `path` between the concept's `checkpoint_sha` and the
on-disk WORKING TREE (each line trailing-whitespace-stripped, internal whitespace
preserved). If the content the author cited no longer occupies those exact lines,
the concept is STALE on that cite. This catches an in-range edit AND — critically
— a pure POSITION-SHIFT (code inserted ABOVE the cited range), which slides the
cite off its authored content WITHOUT the cited line numbers ever appearing in a
diff hunk (the blind spot of the old two-tree `git diff` ∩ cited-range mechanism
this replaces). The HEAD side is read from the WORKING TREE — the SAME tree C3
(file-exists) and C6 (line-bounds) read — so a dirty/pre-commit run is self-
consistent (worktree≠HEAD can't produce a false verdict); in CI worktree==HEAD so
behavior is unchanged. We NEVER use `git log -L` / blame. Per-file fast skip: when
the file is byte-identical at checkpoint and in the working tree, no content can
have drifted. EVERY drifted range is reported (C5 does not stop at the first hit
per (concept, file)), so the offender list is the COMPLETE worklist, not a lower
bound that has to be re-derived by hand after each fix.

Usage:
    python3 scripts/validate_okf.py                       # default bundle docs/knowledge/
    python3 scripts/validate_okf.py --bundle <dir>
    python3 scripts/validate_okf.py --manifest <yaml>     # enable C10/C10b
    python3 scripts/validate_okf.py --nightly             # also run C-AGE/C-REV (WARN)

Exit contract (mirrors validate_specs.py):
    0        -> "✅ OKF-CoreLink profile valid: <N> concepts, <D> deferred, 0 stale, 0 drift"
    non-zero -> per-offender list grouped by check ID, then
                "⛔ OKF-CoreLink profile INVALID: <k> failures"

Dependencies: Python stdlib + PyYAML if available. When PyYAML is absent (or
OKF_NO_YAML=1 is set) a minimal hand-rolled YAML-subset reader is used — the
frontmatter schema only uses top-level scalars and block/inline lists, exactly
the house approach in scripts/validate_specs.py.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

_SCRIPT_DIR = Path(__file__).resolve().parent
if str(_SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(_SCRIPT_DIR))

from okf_git_batch import file_blob_sha as _file_blob_sha, line_count as _line_count, preload_sha_exists as _preload_sha_exists, preload_show_files as _preload_show_files, worktree_blob_sha as _worktree_blob_sha

# ---------------------------------------------------------------------------
# Optional YAML — hand-rolled fallback keeps the gate self-contained on
# ubuntu-latest (no pip step required).
# ---------------------------------------------------------------------------
# The validator is split into dependency-ordered modules. Import-star is
# intentional: this file has historically been imported by fixture tests, and
# the re-export keeps its public and private helper names stable.
from validate_okf_runtime import *
from validate_okf_core1 import *
from validate_okf_core2 import *

# ---------------------------------------------------------------------------
def _under(p: Path, root: Path) -> bool:
    try:
        p.relative_to(root)
        return True
    except ValueError:
        return False


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", default="docs/knowledge",
                        help="bundle root to validate (default: docs/knowledge)")
    parser.add_argument("--manifest", default="docs/internal/okf-wiki/concept-manifest.yaml",
                        help="concept manifest for C10/C10b (skipped-with-warning if absent)")
    parser.add_argument("--surface-root", default=None,
                        help="root for C10b repo-surface enumeration (default: repo root)")
    parser.add_argument("--base-ref", default="main",
                        help="git ref the PR forks from, for C5b (default: main)")
    parser.add_argument("--base-bundle", default=None,
                        help="a 'before' bundle dir overriding git for C5b (fixture testing)")
    parser.add_argument("--nightly", action="store_true",
                        help="also run secondary WARN checks C-AGE/C-REV (nightly)")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    cp = subprocess.run(["git", "rev-parse", "--show-toplevel"],
                        capture_output=True, text=True)
    if cp.returncode != 0:
        sys.exit("ERROR: not inside a git repository")
    repo_root = Path(cp.stdout.strip())
    if args.surface_root is None:
        args.surface_root = str(repo_root)
    args._repo_root = repo_root  # type: ignore[attr-defined]
    args._planned_ids = set()  # type: ignore[attr-defined]

    git = Git(repo_root)
    fails = Failures()

    # Manifest is loaded inside run_checks (it also populates planned ids used
    # by C7), but C7/C8 run after C10 in the function body, so order is fine.
    n_concepts, n_deferred = run_checks(args, git, fails)

    if fails.warnings and args.verbose:
        print("⚠️  WARNINGS (non-blocking):")
        for check, loc, msg in fails.warnings:
            print(f"  [{check}] {loc}: {msg}")

    if len(fails) == 0:
        print(
            f"✅ OKF-CoreLink profile valid: {n_concepts} concepts, "
            f"{n_deferred} deferred, 0 stale, 0 drift"
        )
        return 0

    # group by check ID, in canonical order
    print("OKF-CoreLink profile FAILURES (grouped by check):\n")
    by_check: dict[str, list[tuple[str, str]]] = {}
    for check, loc, msg in fails.items:
        by_check.setdefault(check, []).append((loc, msg))
    ordered = [c for c in Failures.ORDER if c in by_check] + [
        c for c in by_check if c not in Failures.ORDER
    ]
    for check in ordered:
        print(f"[{check}]")
        for loc, msg in by_check[check]:
            print(f"  • {loc}: {msg}")
        print()
    print(f"⛔ OKF-CoreLink profile INVALID: {len(fails)} failures")
    return 1


if __name__ == "__main__":
    sys.exit(main())
# marker-test
