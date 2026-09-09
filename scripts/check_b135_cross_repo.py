#!/usr/bin/env python3
"""Fail-closed verifier for the redacted B-135 sibling-repo receipt.

The server checkout cannot safely grep the sibling repository during CI. This
checker therefore requires a committed, exact-SHA receipt that records both the
tested sibling tree and the merge that delivered that same tree to sibling
``main``. Missing, shortened, or behavior-free evidence is a hard failure.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


EVIDENCE = "docs/campaigns/remediation/B-135-corelink-runners-closure.md"
BACKLOG = "BACKLOG.md"
EXPECTED = {
    "repository": "HuGR-Labs/corelink-runners",
    "tested_ref": "main",
    "tested_head": "d8124b1ab89cf6afb08682442e94c4f4d18c6ba8",
    "tested_tree": "d9cadd3741c77bb58d7162922f1510ded41c844f",
    "merged_pr": "561",
    "merge_commit": "ec9b6d69dbb1f3bd64ef4f9a4ce7e9d9100e69c1",
    "merge_tree": "d9cadd3741c77bb58d7162922f1510ded41c844f",
    "merge_parent": "1119143dc0edd6dea9274c00c2a96024474553d7",
}

# No real credential can appear in a committed receipt. Keep this list narrow
# so ordinary prose such as ``token interpolation`` remains allowed.
FORBIDDEN_SECRET = re.compile(
    r"(?:gh[oprsu]_[A-Za-z0-9]{20,}|cfut_[A-Za-z0-9_-]{16,}|"
    r"CLOUDFLARE_API_TOKEN\s*=\s*[^<\s`]+)",
    re.IGNORECASE,
)

REQUIRED_BEHAVIOR = (
    "`.github/workflows/image-build-impact.yml` has a `pull_request` trigger",
    "scripts/ci/runner-image-build-validation.sh",
    "non-persistent checkout",
    "structural verifier rejects registry publication, deploy commands, and token interpolation",
    "`corelink.rust.*`, `corelink.gh.version`, or `corelink.node.version`",
    "nightly and cargo-fuzz values remain single-sourced",
)


class ContractError(RuntimeError):
    """The committed receipt is absent, stale, or too weak to close B-135."""


def read(root: Path, relative: str) -> str:
    try:
        return (root / relative).read_text(encoding="utf-8")
    except (OSError, UnicodeError) as exc:
        raise ContractError(f"cannot read {relative}: {exc}") from exc


def receipt_fields(text: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for raw in text.splitlines():
        match = re.fullmatch(r"([a-z_]+):\s*(\S.*)", raw)
        if match:
            fields[match.group(1)] = match.group(2).strip()
    return fields


def b135_block(backlog: str) -> str:
    match = re.search(r"(?ms)^### B-135\b.*?(?=^### B-136\b|\Z)", backlog)
    if not match:
        raise ContractError("BACKLOG.md: B-135 block is missing")
    return match.group(0)


def check(root: Path) -> None:
    evidence = read(root, EVIDENCE)
    if FORBIDDEN_SECRET.search(evidence):
        raise ContractError(f"{EVIDENCE}: credential-shaped value is not redacted")
    fields = receipt_fields(evidence)
    for key, expected in EXPECTED.items():
        actual = fields.get(key)
        if actual != expected:
            raise ContractError(f"{EVIDENCE}: {key} must be exact {expected!r}, got {actual!r}")
    if not fields.get("sensitive_values", "").startswith("redacted;"):
        raise ContractError(f"{EVIDENCE}: sensitive_values must declare redaction")
    normalized_evidence = re.sub(r"\s+", " ", evidence)
    for behavior in REQUIRED_BEHAVIOR:
        if behavior not in normalized_evidence:
            raise ContractError(f"{EVIDENCE}: missing delivered behavior evidence {behavior!r}")

    block = b135_block(read(root, BACKLOG))
    if not re.search(r"(?m)^id: B-135\s*$", block):
        raise ContractError("BACKLOG.md: B-135 id marker is missing")
    if not re.search(r"(?m)^status: done\s*$", block):
        raise ContractError("BACKLOG.md: B-135 is not marked done")
    if not re.search(r"(?m)^verify: \|\s*$", block):
        raise ContractError("BACKLOG.md: B-135 verify block is missing")
    if "python3 scripts/check_b135_cross_repo.py" not in block:
        raise ContractError("BACKLOG.md: B-135 verify does not invoke this fail-closed checker")
    if "tested head" not in block or "merge commit" not in block:
        raise ContractError("BACKLOG.md: B-135 verify-means omits delivered SHA evidence")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    try:
        check(args.root.resolve())
    except ContractError as exc:
        print(f"B-135 contract FAILED: {exc}", file=sys.stderr)
        return 1
    print("B-135 contract OK: sibling PR #561 delivered the tested tree to main")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
