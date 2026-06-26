#!/usr/bin/env python3
"""
okf_status.py — reconcile the OKF concept-manifest `status:` field with reality.

The manifest (`docs/internal/okf-wiki/concept-manifest.yaml`) is the C10/C10b
oracle: every candidate carries a `status: planned | active`. The truth of that
field is mechanical — a candidate is `active` iff its concept doc
`docs/knowledge/<id>.md` actually exists in the bundle, and `planned` otherwise.
This tool keeps the field in sync with that reality so the foundation can stay
green before fill waves run and flip back to drift-free after they land.

It is a deterministic, **stdlib-only**, **line-based** editor: it rewrites ONLY
the value token of a candidate's `status:` line, preserving every byte of
surrounding formatting, indentation, quote style, and trailing comments. The
file's comments and the `excludes:` block are never touched (they carry no
`status:` line inside a candidate). The edit is idempotent — running twice is a
no-op.

Relationship to the contract (01-okf-corelink-profile.contract.md §4):
  - C10 requires every `status: active` candidate to have a concept doc OR a
    `deferred:` marker. This tool makes the `active`/`planned` split mechanically
    true, so C10 can never be tripped by a stale status label.

Usage:
    python3 scripts/okf_status.py            # rewrite the manifest in place
    python3 scripts/okf_status.py --check     # report drift, write nothing
    python3 scripts/okf_status.py --manifest <yaml> --bundle <dir>

Exit contract:
    write mode  -> 0 always (reports how many lines it changed)
    --check     -> 0 if no drift, 1 if any candidate's status is wrong
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

# A candidate's id line:  `  - id: "planes/worker-edge"`  (quotes optional).
ID_RE = re.compile(r'^(\s*-\s*id:\s*)(["\']?)([^"\'#\n]+?)\2\s*(#.*)?$')
# A candidate's status line: `    status: "active"`  (quotes optional, comment ok).
STATUS_RE = re.compile(
    r'^(?P<prefix>\s*status:\s*)(?P<q>["\']?)(?P<val>[A-Za-z_]+)(?P=q)(?P<rest>\s*(#.*)?)$'
)

ACTIVE = "active"
PLANNED = "planned"


def repo_root() -> Path:
    cp = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
    )
    if cp.returncode != 0:
        sys.exit("ERROR: not inside a git repository")
    return Path(cp.stdout.strip())


def desired_status(concept_id: str, bundle_root: Path) -> str:
    """active iff the concept doc exists in the bundle, else planned."""
    return ACTIVE if (bundle_root / f"{concept_id}.md").exists() else PLANNED


def reconcile(lines: list[str], bundle_root: Path):
    """Return (new_lines, drifts) where drifts is a list of
    (line_no, concept_id, current_status, desired_status) for every status line
    whose value disagrees with reality. new_lines has the corrected values."""
    out: list[str] = []
    drifts: list[tuple[int, str, str, str]] = []
    cur_id: str | None = None
    for i, raw in enumerate(lines, start=1):
        line = raw.rstrip("\n")
        nl = "\n" if raw.endswith("\n") else ""

        m_id = ID_RE.match(line)
        if m_id:
            cur_id = m_id.group(3).strip()
            out.append(raw)
            continue

        m_st = STATUS_RE.match(line)
        if m_st and cur_id is not None:
            want = desired_status(cur_id, bundle_root)
            have = m_st.group("val")
            if have != want:
                drifts.append((i, cur_id, have, want))
                new_line = (
                    m_st.group("prefix")
                    + m_st.group("q")
                    + want
                    + m_st.group("q")
                    + m_st.group("rest")
                )
                out.append(new_line + nl)
                continue
            out.append(raw)
            continue

        out.append(raw)
    return out, drifts


def main() -> int:
    root = repo_root()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--manifest",
        default=str(root / "docs/internal/okf-wiki/concept-manifest.yaml"),
        help="concept manifest to reconcile",
    )
    parser.add_argument(
        "--bundle",
        default=str(root / "docs/knowledge"),
        help="knowledge bundle root (concept docs live here)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="report drift and exit 1 if any; write nothing",
    )
    args = parser.parse_args()

    manifest = Path(args.manifest)
    if not manifest.is_absolute():
        manifest = (Path.cwd() / manifest).resolve()
    bundle = Path(args.bundle)
    if not bundle.is_absolute():
        bundle = (Path.cwd() / bundle).resolve()

    if not manifest.exists():
        sys.exit(f"ERROR: manifest not found: {manifest}")

    with manifest.open(encoding="utf-8") as fh:
        lines = fh.readlines()

    new_lines, drifts = reconcile(lines, bundle)

    if args.check:
        if not drifts:
            print("✅ okf_status: manifest status in sync (no drift)")
            return 0
        print("OKF manifest status DRIFT (status label disagrees with bundle):\n")
        for line_no, cid, have, want in drifts:
            print(
                f"  • line {line_no}: `{cid}` is `{want}` on disk but manifest says `{have}`"
            )
        print(
            f"\n⛔ okf_status: {len(drifts)} status drift(s) — "
            "run `python3 scripts/okf_status.py` to fix"
        )
        return 1

    if not drifts:
        print("✅ okf_status: manifest status already in sync (no change)")
        return 0

    manifest.write_text("".join(new_lines), encoding="utf-8")
    print(f"✅ okf_status: flipped {len(drifts)} status label(s) to match the bundle:")
    for _, cid, have, want in drifts:
        print(f"  • `{cid}`: {have} -> {want}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
