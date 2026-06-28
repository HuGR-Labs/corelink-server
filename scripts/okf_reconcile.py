#!/usr/bin/env python3
"""
okf_reconcile.py — the deterministic "what needs re-authoring" brief.

When the OKF freshness gate (C5) goes red, an author needs to know EXACTLY which
concepts drifted, which declared source files moved under them, which cited line
ranges the change hit, and what the change was. This reporter computes that
worklist mechanically — no LLM, no judgement. It is the deterministic half of
the self-healing loop; the LLM half (re-authoring) is in
`.claude/skills/okf-reconcile/SKILL.md`.

Staleness is detected with the EXACT C5 mechanic shared with
`scripts/validate_okf.py` (and the frozen contract §4): the CONTENT-ANCHOR
predicate `validate_okf.cited_range_drifted` — for each cited `path:Lx-Ly` it
compares the CONTENT of lines Lx..Ly between `checkpoint_sha` and HEAD (each line
trailing-whitespace-stripped). This fires on an in-range edit AND on a pure
position-shift (an insertion above the cited range). It NEVER uses `git log -L`/
blame. The Concept model AND the drift predicate are imported from `validate_okf`
so the two tools can never disagree on what "stale" means.

For the ADR sub-profile (§4.1) an accepted ADR does not go stale on the code it
governs, so — mirroring C5 — non-`.md` source files of an ADR concept are skipped.

Usage:
    python3 scripts/okf_reconcile.py            # human-readable worklist
    python3 scripts/okf_reconcile.py --json      # machine worklist
    python3 scripts/okf_reconcile.py --bundle <dir>

Exit contract:
    0 ALWAYS — this is a reporter, not a gate. Prints "0 stale" cleanly.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Import the frozen C5 machinery so reconcile and validate can never diverge.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import validate_okf as okf  # noqa: E402


def repo_root() -> Path:
    import subprocess

    cp = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
    )
    if cp.returncode != 0:
        sys.exit("ERROR: not inside a git repository")
    return Path(cp.stdout.strip())


def _full_diff(git: "okf.Git", base_sha: str, path: str) -> str:
    """The human-readable patch (with context) for a changed source file."""
    cp = git.run(["diff", "--unified=3", base_sha, "HEAD", "--", path])
    return cp.stdout if cp.returncode == 0 else ""


def collect_stale(git: "okf.Git", bundle_root: Path):
    """Return a list of stale-concept worklist entries (dicts)."""
    worklist = []
    if not bundle_root.is_dir():
        return worklist

    for md in sorted(bundle_root.rglob("*.md")):
        rel = md.relative_to(bundle_root).as_posix()
        if "/" not in rel and md.name in okf.RESERVED_NAMES:
            continue
        c = okf.Concept(md, bundle_root)
        if c.is_deferred or not c.has_frontmatter:
            continue
        if not c.checkpoint_sha or not isinstance(c.checkpoint_sha, str):
            continue
        if not okf.HEX40_RE.match(c.checkpoint_sha) or not git.sha_exists(c.checkpoint_sha):
            continue

        # cited line ranges per file (HEAD coordinates) — same as validate_okf C5.
        ranges_by_file: dict[str, list[tuple[int, int]]] = {}
        for (f, l1, l2) in c.cites:
            ranges_by_file.setdefault(f, []).append((l1, l2))

        stale_sources = []
        for sf in c.source_files:
            if not (git.repo_root / sf).exists():
                continue  # C3 will flag a vanished source; nothing to diff here.
            # ADR sub-profile: accepted ADR doesn't go stale on governed code.
            if c.is_adr and not sf.endswith(".md"):
                continue
            cranges = ranges_by_file.get(sf, [])
            if not cranges:
                continue

            hit: list[tuple[int, int]] = []
            for (l1, l2) in cranges:
                if okf.cited_range_drifted(git, c.checkpoint_sha, sf, l1, l2):
                    hit.append((l1, l2))
            # dedup while preserving order (a concept may cite a range N times)
            hit_ranges = [[a, b] for (a, b) in dict.fromkeys(hit)]
            if not hit_ranges:
                continue

            stale_sources.append(
                {
                    "source_file": sf,
                    "changed_cited_ranges": hit_ranges,
                    "diff": _full_diff(git, c.checkpoint_sha, sf),
                }
            )

        if stale_sources:
            loc = (
                c.path.relative_to(git.repo_root).as_posix()
                if okf._under(c.path, git.repo_root)
                else f"{bundle_root.name}/{c.rel}"
            )
            worklist.append(
                {
                    "concept_id": c.concept_id,
                    "path": loc,
                    "checkpoint_sha": c.checkpoint_sha,
                    "is_adr": c.is_adr,
                    "stale_sources": stale_sources,
                }
            )
    return worklist


def print_human(worklist: list[dict]) -> None:
    if not worklist:
        print("✅ okf_reconcile: 0 stale concepts — nothing to reconcile")
        return
    n_src = sum(len(w["stale_sources"]) for w in worklist)
    print(
        f"OKF reconciliation worklist: {len(worklist)} stale concept(s), "
        f"{n_src} drifted source-file(s)\n"
    )
    for w in worklist:
        print(f"━━ {w['concept_id']}  ({w['path']})")
        print(f"   checkpoint_sha: {w['checkpoint_sha'][:12]}")
        for s in w["stale_sources"]:
            ranges = ", ".join(f"{a}-{b}" for a, b in s["changed_cited_ranges"])
            print(f"   • {s['source_file']}  cited lines changed: {ranges}")
        print()
        for s in w["stale_sources"]:
            if s["diff"]:
                print(f"   --- diff {s['source_file']} (checkpoint..HEAD) ---")
                for dl in s["diff"].splitlines():
                    print(f"     {dl}")
                print()
    print(
        "Next: re-author each concept against HEAD (cite the implementing line, "
        "not the doc-comment), advance checkpoint_sha to HEAD, re-run "
        "validate_okf until green. See .claude/skills/okf-reconcile/SKILL.md."
    )


def main() -> int:
    root = repo_root()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--bundle",
        default=str(root / "docs/knowledge"),
        help="knowledge bundle root (default: docs/knowledge)",
    )
    parser.add_argument("--json", action="store_true", help="emit JSON worklist")
    args = parser.parse_args()

    bundle = Path(args.bundle)
    if not bundle.is_absolute():
        bundle = (Path.cwd() / bundle).resolve()

    git = okf.Git(root)
    worklist = collect_stale(git, bundle)

    if args.json:
        print(json.dumps({"stale_count": len(worklist), "worklist": worklist}, indent=2))
    else:
        print_human(worklist)

    return 0  # reporter — never blocks


if __name__ == "__main__":
    sys.exit(main())
