#!/usr/bin/env python3
"""Verify the exact B-057 ledger successor on a PR candidate or integrated head."""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import verify_backlog_wp_ledger  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expected-head", required=True)
    parser.add_argument("--expected-base", default="cfc88425df1330458b29fb28ac34a60b0f7f5729")
    parser.add_argument("--trusted-root", type=Path)
    args = parser.parse_args()

    head = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT,
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    if head != args.expected_head:
        parser.error(f"checked-out HEAD {head} differs from expected {args.expected_head}")

    if args.trusted_root is not None:
        policy = verify_backlog_wp_ledger._successor_policy()
        receipt = policy.validate_candidate_successor(
            args.trusted_root,
            REPO_ROOT,
            base_sha=args.expected_base,
        )
    else:
        receipt = verify_backlog_wp_ledger.load_successor_chain(REPO_ROOT)

    if (
        receipt.get("sequence") != 9
        or receipt.get("base_commit") != args.expected_base
        or receipt.get("changed_ids") != ["B-057"]
    ):
        parser.error("ledger chain did not end in the exact B-057 v0009 transition")
    print(f"B-057 ledger successor v0009 verified at {head}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
