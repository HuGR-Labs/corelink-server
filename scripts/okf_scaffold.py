#!/usr/bin/env python3
"""
Scaffold a new OKF-CoreLink concept doc from the FROZEN template.

Emits `docs/knowledge/<id>.md` from the §A stub of
`docs/internal/okf-wiki/02-concept-template.md` with the required frontmatter
pre-filled: `type`, `title`, an empty `source_files` list (for the author to
fill), `checkpoint_sha` set to the current `git rev-parse HEAD`, and
`provenance: AUTHORED`.

This is a pure, stdlib-only transcriber of the frozen template — it does NOT
design content; the author fills the marked `<…>` slots and the body claims.

Usage:
    python3 scripts/okf_scaffold.py --type AuthMechanism \\
        --id auth/pat-gauntlet --title "The PAT gauntlet"

Rules (transcribed from the contract):
    - `--id` is the bundle-relative concept id (no `.md`); it MUST live under a
      taxonomy dir (profile §1.1) and MUST NOT be a reserved name (index/log, C9).
    - Refuses to overwrite an existing file.
    - After scaffolding, re-run `scripts/okf_index.py` to list the concept.

Exit:
    0  concept file written.
    1  refused (reserved id, bad path, file exists, or git error).
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
BUNDLE = REPO_ROOT / "docs" / "knowledge"

RESERVED_STEMS = {"index", "log"}

# Frozen taxonomy dirs (profile §1.1). A concept MUST live under one.
TAXONOMY_DIRS = {
    "planes",
    "surfaces",
    "auth",
    "storage",
    "tenancy",
    "flows",
    "crates",
    "adr",
    "ops",
    "security",
    "compliance",
    "testing",
    "launch",
}


def head_sha() -> str:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=REPO_ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError) as exc:
        sys.exit(f"ERROR: could not read git HEAD: {exc}")
    sha = out.stdout.strip()
    if len(sha) != 40:
        sys.exit(f"ERROR: unexpected HEAD sha {sha!r}")
    return sha


STUB = """\
---
type: "{type}"
title: "{title}"
description: "<one sentence>"
source_files: []  # FILL: >=1 repo-relative path this concept is grounded in
checkpoint_sha: "{sha}"
provenance: "AUTHORED"
tags: []
---

# {title}

<Lead paragraph: the WHY — the role this thing plays in the system, the cross-crate
context, the thing rustdoc cannot tell you. 2-5 sentences. No fluff.>

# Role
<What problem it solves / where it sits in the plane topology.>

# How it works
<Mechanics, in structural markdown. Every bullet here carries a `path:line` token (C6c).>

# Invariants
<The hard rules that must not break. Tie each to a code anchor `path:line` (C6c).>

# Gotchas
<The non-obvious traps — the tribal knowledge. Optional but high-value.>

# Citations
1. `<path:line>` — <what this anchor proves>
   (every `source_files` path MUST appear in at least one citation)
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--type", required=True, help="Taxonomy type (profile §1.1).")
    parser.add_argument(
        "--id",
        required=True,
        help="Bundle-relative concept id (no .md), e.g. auth/pat-gauntlet.",
    )
    parser.add_argument("--title", required=True, help="Human title (non-empty).")
    args = parser.parse_args()

    concept_id = args.id.strip().lstrip("/")
    if not concept_id or not args.type.strip() or not args.title.strip():
        sys.exit("ERROR: --type, --id and --title must all be non-empty.")
    if concept_id.endswith(".md"):
        concept_id = concept_id[:-3]

    parts = Path(concept_id).parts
    if any(p in ("..", ".") for p in parts) or Path(concept_id).is_absolute():
        sys.exit(f"ERROR: bad concept id {concept_id!r}.")

    # C9: reserved names never used as concepts.
    if len(parts) == 1 and parts[0] in RESERVED_STEMS:
        sys.exit(f"ERROR: {concept_id!r} is a reserved name (index/log) — refused (C9).")
    if Path(concept_id).name in RESERVED_STEMS and len(parts) == 1:
        sys.exit(f"ERROR: reserved stem refused (C9).")

    # Must live under a taxonomy dir (profile §1.1).
    if len(parts) < 2 or parts[0] not in TAXONOMY_DIRS:
        sys.exit(
            f"ERROR: concept must live under a taxonomy dir {sorted(TAXONOMY_DIRS)} "
            f"— got {concept_id!r}."
        )

    target = BUNDLE / f"{concept_id}.md"
    if target.exists():
        sys.exit(f"ERROR: refusing to overwrite existing {target.relative_to(REPO_ROOT)}.")

    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(
        STUB.format(type=args.type.strip(), title=args.title.strip(), sha=head_sha()),
        encoding="utf-8",
    )
    print(f"✅ Scaffolded {target.relative_to(REPO_ROOT)} (type={args.type}).")
    print("   Next: fill source_files + body, then `python3 scripts/okf_index.py`.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
