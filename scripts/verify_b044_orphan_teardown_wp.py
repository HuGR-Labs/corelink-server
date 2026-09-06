#!/usr/bin/env python3
"""Fail-closed static guard for the B-044 external teardown work package."""

from __future__ import annotations

import argparse
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
WP = ROOT / "docs/campaigns/remediation/WP-B044-orphan-teardown.md"
BACKLOG = ROOT / "BACKLOG.md"
CHANGELOG = ROOT / "changelog.d/PENDING-b044-orphan-teardown-wp.md"
WORKFLOW = ROOT / ".github/workflows/b044-orphan-teardown.yml"
OKF_WORKFLOW = ROOT / ".github/workflows/okf_wiki.yml"

REQUIRED_WP_PHRASES = (
    "Execution repository:",
    "production teardown remains open",
    "reconcileOrphanBoxes",
    "listSpawnedBoxHandles",
    "complete paginated `sbox:`",
    "stable runner application",
    "`instance.name` matches an existing `sbox:` record",
    "record's `h` is the exact handle",
    "re-read immediately before the action",
    "type-1",
    "type-2",
    "HALT",
    "inert-by-default",
    "RECONCILE_ORPHAN_TEARDOWN",
    "getContainer(RUNNER_NAMESPACE, h)",
    "`instance.name` is not a durable-object handle",
    "cross-tenant",
    "cross-application",
    "Type-2, unknown, cross-tenant, stale, and otherwise indeterminate candidates are never destroyed",
    "pagination error, truncation",
    "per-tick destroy cap",
    "age floor above the longest job",
    "idempotent",
    "conditionally removed only after a confirmed destroy",
    "retained for a safe retry",
    "success marker or equivalent dedupe",
    "orphan_box_reaped",
    "Closed-world completeness clause",
    "No candidate,",
    "programmatic-teardown limitation",
)


def read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as exc:
        raise AssertionError(f"cannot read required artifact {path}: {exc}") from exc


def require(text: str, needles: tuple[str, ...], label: str) -> None:
    compact = " ".join(text.split())
    missing = [needle for needle in needles if needle not in text and " ".join(needle.split()) not in compact]
    if missing:
        raise AssertionError(f"{label} missing required clauses: {', '.join(missing)}")


def b044_block(backlog: str) -> str:
    marker = "### B-044"
    start = backlog.find(marker)
    if start < 0:
        raise AssertionError("BACKLOG.md has no B-044 heading")
    end = backlog.find("### B-", start + len(marker))
    return backlog[start:] if end < 0 else backlog[start:end]


def verify_texts(wp: str, backlog: str, changelog: str, workflow: str, okf: str) -> None:
    if wp.count("# WP-B044") != 1:
        raise AssertionError("WP must contain exactly one primary WP-B044 heading")
    require(wp, REQUIRED_WP_PHRASES, "B-044 WP")
    if "must not claim that teardown is live" not in wp:
        raise AssertionError("WP must prohibit an unmeasured live-teardown claim")

    block = b044_block(backlog)
    require(block, ("id: B-044", "repo: corelink-runners", "status: parked"), "B-044 backlog block")
    if "verify: python3 scripts/verify_b044_orphan_teardown_wp.py" not in block:
        raise AssertionError("B-044 backlog must invoke the focal verifier")
    if "unarmed" not in block or "external" not in block:
        raise AssertionError("B-044 backlog must preserve the external/unarmed disposition")

    require(
        changelog,
        ("B-044", "static guard", "does not arm", "remain pending"),
        "B-044 changelog fragment",
    )
    require(
        workflow,
        (
            "verify_b044_orphan_teardown_wp.py",
            "tests/test_verify_b044_orphan_teardown_wp.py",
            "No Cargo/Node/full CI",
            "RECONCILE_ORPHAN_TEARDOWN",
        ),
        "B-044 workflow",
    )
    forbidden_armers = ("RECONCILE_ORPHAN_TEARDOWN=1", "secrets.RECONCILE_ORPHAN_TEARDOWN")
    if any(marker in workflow for marker in forbidden_armers):
        raise AssertionError("B-044 workflow must not arm the teardown secret")
    if "docs/campaigns/remediation/WP-B044-orphan-teardown.md" not in okf:
        raise AssertionError("OKF workflow must rerun when the B-044 grounding contract changes")


def verify_repo() -> None:
    verify_texts(read(WP), read(BACKLOG), read(CHANGELOG), read(WORKFLOW), read(OKF_WORKFLOW))


MUTATIONS = (
    ("join", "`instance.name` matches an existing `sbox:` record", "exact instance↔handle join"),
    ("handle", "`instance.name` is not a durable-object handle", "handle namespace guard"),
    ("type", "Type-2, unknown, cross-tenant, stale, and otherwise indeterminate candidates\n  are never destroyed", "type-2 fail-closed guard"),
    ("tenant", "cross-tenant or cross-application teardown", "tenant isolation"),
    ("pagination", "pagination error, truncation, missing tick", "pagination fail-closed"),
    ("ordering", "conditionally removed only after a confirmed destroy", "delete ordering"),
    ("armed", "inert-by-default", "default-off teardown"),
    ("retry", "Destroy is idempotent for retries", "retry idempotence"),
)


def verify_mutations() -> int:
    baseline = read(WP)
    backlog = read(BACKLOG)
    changelog = read(CHANGELOG)
    workflow = read(WORKFLOW)
    okf = read(OKF_WORKFLOW)
    passed = 0
    for name, needle, label in MUTATIONS:
        mutated = baseline.replace(needle, "", 1)
        try:
            verify_texts(mutated, backlog, changelog, workflow, okf)
        except AssertionError:
            passed += 1
        else:
            raise AssertionError(f"mutation {name} did not invalidate {label}")
    return passed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true", help="run mutation guards")
    args = parser.parse_args(argv)
    try:
        verify_repo()
        mutations = verify_mutations() if args.self_test else 0
    except AssertionError as exc:
        print(f"B044 FAIL: {exc}", file=sys.stderr)
        return 1
    suffix = f"; mutations={mutations}/{len(MUTATIONS)}" if args.self_test else ""
    print(f"B044 PASS: external teardown remains gated and fail-closed{suffix}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
