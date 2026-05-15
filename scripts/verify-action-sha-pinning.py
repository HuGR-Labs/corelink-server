#!/usr/bin/env python3
"""verify-action-sha-pinning.py — DEBT-018+019 CI gate.

Walks ``.github/workflows/*.yml`` and fails if any ``uses:`` line references a
GitHub Action by anything other than a 40-character commit SHA.

Supply-chain policy (CC7.1 / SOC2 rollup):
  * Tag pins (``@v4``, ``@v3.27.0``, ``@stable``) are mutable — a maintainer
    (or attacker who compromised one) can re-point them at malicious code.
  * SHA pins are immutable — the only safe form for third-party actions.

Each pin SHOULD also carry a trailing comment ``# <action>@<friendly-tag>`` so
humans can read the workflow without resolving SHAs. The verifier WARNs (does
not fail) when the comment is missing, so CI gates the security property and
leaves comment hygiene to review.

Exit codes:
  0 — every ``uses:`` line is SHA-pinned (40 hex chars after ``@``)
  1 — at least one violation found
  2 — argument / IO error

Usage::

  python3 scripts/verify-action-sha-pinning.py
  python3 scripts/verify-action-sha-pinning.py --workflows-dir .github/workflows
  python3 scripts/verify-action-sha-pinning.py --strict-comments
"""
from __future__ import annotations

import argparse
import re
import sys
from dataclasses import dataclass
from pathlib import Path

SHA_RE = re.compile(r"^[0-9a-f]{40}$")
USES_RE = re.compile(r"^(?P<lead>\s*-?\s*uses:\s*)(?P<ref>\S+)(?P<tail>.*)$")
COMMENT_RE = re.compile(r"#\s*(?P<action>[^@\s]+/[^@\s]+)@(?P<tag>\S+)")


@dataclass
class Violation:
    file: Path
    line_no: int
    ref: str
    reason: str

    def format(self) -> str:
        return f"{self.file}:{self.line_no}: {self.reason}: {self.ref}"


def is_local_or_docker(ref: str) -> bool:
    return ref.startswith("./") or ref.startswith("docker://")


def scan_file(path: Path, strict_comments: bool) -> tuple[list[Violation], int]:
    """Return (violations, total_uses_count) for one workflow file."""
    violations: list[Violation] = []
    total = 0
    try:
        text = path.read_text()
    except OSError as exc:  # pragma: no cover — IO error path
        violations.append(Violation(path, 0, "<unreadable>", f"IO error: {exc}"))
        return violations, 0

    for line_no, line in enumerate(text.splitlines(), start=1):
        match = USES_RE.match(line)
        if not match:
            continue
        ref = match.group("ref")
        if is_local_or_docker(ref):
            continue
        total += 1
        if "@" not in ref:
            violations.append(Violation(path, line_no, ref, "missing-ref"))
            continue
        _, ver = ref.rsplit("@", 1)
        if not SHA_RE.match(ver):
            violations.append(Violation(path, line_no, ref, "not-sha-pinned"))
            continue
        if strict_comments:
            tail = match.group("tail")
            if not COMMENT_RE.search(tail):
                violations.append(
                    Violation(path, line_no, ref, "missing-friendly-tag-comment")
                )
    return violations, total


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Verify all GitHub Actions are pinned by 40-char commit SHA.",
    )
    parser.add_argument(
        "--workflows-dir",
        default=".github/workflows",
        help="Directory containing workflow YAML files (default: .github/workflows).",
    )
    parser.add_argument(
        "--strict-comments",
        action="store_true",
        help="Also fail if a SHA pin lacks a trailing '# <action>@<tag>' comment.",
    )
    parser.add_argument(
        "--quiet",
        action="store_true",
        help="Suppress per-file summary on success.",
    )
    args = parser.parse_args(argv)

    workflows_dir = Path(args.workflows_dir)
    if not workflows_dir.is_dir():
        print(
            f"error: workflows directory not found: {workflows_dir}",
            file=sys.stderr,
        )
        return 2

    files = sorted(workflows_dir.glob("*.yml")) + sorted(workflows_dir.glob("*.yaml"))
    if not files:
        print(f"error: no workflow files in {workflows_dir}", file=sys.stderr)
        return 2

    all_violations: list[Violation] = []
    total_uses = 0
    for path in files:
        violations, count = scan_file(path, args.strict_comments)
        all_violations.extend(violations)
        total_uses += count

    if all_violations:
        print(
            f"FAIL: {len(all_violations)} violation(s) across {len(files)} workflow file(s):",
            file=sys.stderr,
        )
        for v in all_violations:
            print(f"  {v.format()}", file=sys.stderr)
        print(
            f"\nPolicy: every `uses:` line MUST reference a 40-char commit SHA "
            f"(DEBT-018+019, SOC2 CC7.1). Resolve tags with:\n"
            f"  gh api repos/<owner>/<repo>/git/refs/tags/<tag> --jq '.object.sha'\n"
            f"and (if annotated) deref via repos/<owner>/<repo>/git/tags/<sha>.",
            file=sys.stderr,
        )
        return 1

    if not args.quiet:
        print(
            f"OK: {total_uses} `uses:` line(s) across {len(files)} workflow file(s) "
            f"are SHA-pinned.",
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
