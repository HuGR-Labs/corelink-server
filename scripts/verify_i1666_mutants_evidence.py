#!/usr/bin/env python3
"""Verify the hosted mutants lane retains a bounded, redacted receipt."""

from pathlib import Path


WORKFLOW = Path(".github/workflows/issue-1863-mutants-hosted.yml")
UPLOAD_SHA = "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"


def verify(text: str) -> None:
    required = (
        "workflow_dispatch:",
        "permissions:\n  contents: read",
        "runs-on: ubuntu-latest",
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "run: cargo mutants --workspace --no-shuffle --minimum-test-timeout=600",
        "name: Write hosted mutants receipt",
        '"schema": "corelink.hosted-mutants-receipt.v1"',
        '"run_id": os.environ["GITHUB_RUN_ID"]',
        '"sha": os.environ["GITHUB_SHA"]',
        '"status": os.environ["JOB_STATUS"]',
        "name: Upload hosted mutants evidence",
        UPLOAD_SHA,
        "name: mutants-evidence-${{ github.run_id }}-${{ github.run_attempt }}",
        "${{ runner.temp }}/mutants-receipt.json",
        "mutants.out/",
        "if-no-files-found: error",
        "retention-days: 30",
    )
    missing = [marker for marker in required if marker not in text]
    if missing:
        raise ValueError("missing hosted mutants evidence markers: " + ", ".join(missing))

    if text.count(UPLOAD_SHA) != 1:
        raise ValueError("hosted mutants lane must have exactly one pinned evidence upload")
    if text.count("retention-days:") != 1:
        raise ValueError("hosted mutants evidence must have exactly one retention bound")
    if "contents: write" in text or "actions: write" in text:
        raise ValueError("hosted mutants evidence lane must remain read-only")
    if any(token in text.lower() for token in ("git push", "gh issue", "deploy", "publish")):
        raise ValueError("hosted mutants evidence lane must not write or publish")

    command = "run: cargo mutants --workspace --no-shuffle --minimum-test-timeout=600"
    receipt = "name: Write hosted mutants receipt"
    upload = "name: Upload hosted mutants evidence"
    if not (text.index(command) < text.index(receipt) < text.index(upload)):
        raise ValueError("receipt upload must follow the mutation command")


def main() -> int:
    verify(WORKFLOW.read_text(encoding="utf-8"))
    print("issue #1666 hosted mutants evidence contract: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
