#!/usr/bin/env python3
"""Static contract for the #1863 hosted receipt and disabled legacy lane."""

import argparse
import json
import re
from pathlib import Path
from typing import Any, Mapping


WORKFLOW = Path(".github/workflows/issue-1863-mutants-hosted.yml")
LEGACY_WORKFLOW = Path(".github/workflows/nightly.yml")
BACKLOG = Path("BACKLOG.md")
OWNER_PACKET = Path("docs/internal/b113-six-lanes-owner-actions.md")
LEGACY_DISABLED_IF = "if: github.event_name == 'schedule' && github.event_name == 'workflow_dispatch'"
REQUIRED = (
    "workflow_dispatch:",
    "permissions:\n  contents: read",
    "runs-on: ubuntu-latest",
    "timeout-minutes: 240",
    "timeout-minutes: 225",
    "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
    "Swatinem/rust-cache@400e7407cfd7a091e5fbb6afec01ec146c432b7c",
    "arduino/setup-protoc@f4d5893b897028ff5739576ea0409746887fa536",
    "taiki-e/install-action@07b4745e0c39a41822af610387492e3e53aa222b",
    "tool: cargo-mutants@27.0.0",
    "fallback: cargo-binstall",
    "cargo mutants --workspace --no-shuffle --minimum-test-timeout=600",
)


def validate_successful_receipt(
    receipt: Mapping[str, Any], run: Mapping[str, Any], expected_sha: str
) -> None:
    """Reject artifacts that are not evidence of a successful exact-SHA run.

    `receipt` is the uploaded mutants-receipt.json. `run` is the terminal run
    record returned by the GitHub API (or `gh run view --json`). Keeping both
    records explicit prevents a failed or still-running job artifact from
    being mistaken for a completed hosted receipt.
    """
    run_id = run.get("databaseId", run.get("id"))
    attempt = run.get("attempt", run.get("run_attempt"))
    head_sha = run.get("headSha", run.get("head_sha"))
    head_branch = run.get("headBranch", run.get("head_branch"))

    if run.get("status") != "completed":
        raise ValueError("hosted mutants run is not completed")
    if run.get("conclusion") != "success":
        raise ValueError("hosted mutants run conclusion is not success")
    if receipt.get("status") != "success":
        raise ValueError("hosted mutants receipt status is not success")
    if receipt.get("schema") != "corelink.hosted-mutants-receipt.v1":
        raise ValueError("hosted mutants receipt schema is unsupported")
    if run_id is None or str(receipt.get("run_id")) != str(run_id):
        raise ValueError("receipt run_id does not match the GitHub run")
    if attempt is None or str(receipt.get("run_attempt")) != str(attempt):
        raise ValueError("receipt run_attempt does not match the GitHub run")
    if not expected_sha or receipt.get("sha") != expected_sha:
        raise ValueError("receipt SHA does not match the expected exact SHA")
    if head_sha != expected_sha:
        raise ValueError("GitHub run SHA does not match the expected exact SHA")
    if head_branch != "main" or receipt.get("ref") != "refs/heads/main":
        raise ValueError("hosted mutants receipt is not from protected main")


def validate_receipt_files(receipt_path: Path, run_path: Path, expected_sha: str) -> None:
    receipt = json.loads(receipt_path.read_text(encoding="utf-8"))
    run = json.loads(run_path.read_text(encoding="utf-8"))
    validate_successful_receipt(receipt, run, expected_sha)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--receipt", type=Path, help="downloaded mutants-receipt.json")
    parser.add_argument("--run", type=Path, help="terminal GitHub run JSON")
    parser.add_argument("--expected-sha", help="exact protected-main SHA to verify")
    args = parser.parse_args()
    supplied = (args.receipt is not None, args.run is not None, args.expected_sha is not None)
    if any(supplied) and not all(supplied):
        parser.error("--receipt, --run, and --expected-sha must be supplied together")
    if all(supplied):
        try:
            validate_receipt_files(args.receipt, args.run, args.expected_sha)
        except (OSError, json.JSONDecodeError, ValueError) as error:
            raise SystemExit(f"hosted mutants receipt rejected: {error}") from error
        print("hosted mutants receipt: PASS")
        return 0

    text = WORKFLOW.read_text(encoding="utf-8")
    legacy = LEGACY_WORKFLOW.read_text(encoding="utf-8")
    backlog = BACKLOG.read_text(encoding="utf-8")
    owner_packet = OWNER_PACKET.read_text(encoding="utf-8")
    if "schedule:" in text or "pull_request:" in text or "push:" in text:
        raise SystemExit("#1863 lane must remain workflow_dispatch-only")
    missing = [marker for marker in REQUIRED if marker not in text]
    if missing:
        raise SystemExit("missing contract markers: " + ", ".join(missing))
    if "self-hosted" in text or "corelink-builder" in text or "runs-on: corelink" in text:
        raise SystemExit("#1863 lane must use GitHub-hosted Linux only")
    if "publish" in text.lower() or "deploy" in text.lower():
        raise SystemExit("#1863 lane must not publish or deploy")
    required_guards = (
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "cancel-in-progress: false",
    )
    missing_guards = [marker for marker in required_guards if marker not in text]
    if missing_guards:
        raise SystemExit("missing dispatch guards: " + ", ".join(missing_guards))
    if "run: cargo mutants --workspace --no-shuffle --minimum-test-timeout=600" not in text:
        raise SystemExit("mutants command drifted from the nightly lane")
    legacy_job = re.search(
        r"(?ms)^  mutants-workspace:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        legacy,
    )
    if legacy_job is None or LEGACY_DISABLED_IF not in legacy_job.group("body"):
        raise SystemExit("legacy nightly mutants-workspace must remain disabled")
    mapping_markers = (
        "issue-1863-mutants-hosted.yml",
        "dispatch-only GitHub-hosted scheduled-equivalent evidence path",
        "any dispatched run succeeded",
    )
    for marker in mapping_markers:
        if marker not in backlog or marker not in owner_packet:
            raise SystemExit(f"missing B-113/nightly hosted-evidence mapping marker: {marker}")
    print("issue #1863 / B-113 nightly hosted receipt contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
