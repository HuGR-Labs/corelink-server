#!/usr/bin/env python3
"""Fail closed if B-107 confuses a repository split with production evidence."""

from __future__ import annotations

import argparse
from pathlib import Path


BACKLOG = Path("BACKLOG.md")
LANE = Path(".github/workflows/issue-1667-b122-production-evidence.yml")


def b107_block(text: str) -> str:
    start = text.find("### B-107")
    if start < 0:
        raise ValueError("B-107 section is missing")
    end = text.find("\n### ", start + 1)
    return text[start:] if end < 0 else text[start:end]


def verify(backlog: str, lane: str) -> None:
    block = b107_block(backlog)
    required_backlog = (
        "id: B-107",
        "status: parked",
        "verify: manual",
        "#1825",
        "`ostore`",
        "`oaccounting`",
        "Issue 1667 B-122 production timing evidence",
        "executada em `main`",
        "`CORELINK_DOGFOOD_PAT`",
        "tres PUTs autenticados de 1 KiB",
        "60 s",
        "p50/p90/p99",
        "`serving_sha`",
        "Nenhuma fixture, teste",
        "medicao de producao",
    )
    missing = [needle for needle in required_backlog if needle not in block]
    forbidden = [needle for needle in ("status: done", "latencia de producao provada") if needle in block]
    if missing or forbidden:
        detail = []
        if missing:
            detail.append("missing B-107 boundary: " + ", ".join(missing))
        if forbidden:
            detail.append("forbidden B-107 claim: " + ", ".join(forbidden))
        raise ValueError("; ".join(detail))

    required_lane = (
        "name: Issue 1667 B-122 production timing evidence",
        "workflow_dispatch:",
        "environment: production",
        "secrets.CORELINK_DOGFOOD_PAT",
        "b107-separated",
        "--require-identities artifacts/b122/b107-separated.txt",
        "timeout 25s curl",
        "--max-time 20",
        "-X PUT",
        "--data-binary @<(head -c 1024 /dev/zero)",
    )
    missing = [needle for needle in required_lane if needle not in lane]
    forbidden = [needle for needle in ("wrangler deploy", "kubectl apply", "terraform apply") if needle in lane]
    if missing or forbidden:
        detail = []
        if missing:
            detail.append("missing production-lane control: " + ", ".join(missing))
        if forbidden:
            detail.append("forbidden production-lane mutation: " + ", ".join(forbidden))
        raise ValueError("; ".join(detail))


def expect_rejected(backlog: str, lane: str, old: str, new: str, reason: str) -> None:
    block = b107_block(backlog)
    mutated = backlog.replace(block, block.replace(old, new), 1)
    try:
        verify(mutated, lane)
    except ValueError:
        return
    raise AssertionError(f"mutation survived: {reason}")


def self_test(backlog: str, lane: str) -> None:
    verify(backlog, lane)
    expect_rejected(backlog, lane, "status: parked", "status: done", "parked state changed")
    expect_rejected(backlog, lane, "`serving_sha`", "deployed revision", "deployed identity removed")
    expect_rejected(backlog, lane, "p50/p90/p99", "median only", "tail requirement removed")
    try:
        verify(backlog, lane.replace("secrets.CORELINK_DOGFOOD_PAT", "secrets.REMOVED", 1))
    except ValueError:
        return
    raise AssertionError("mutation survived: production secret boundary removed")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    backlog = BACKLOG.read_text(encoding="utf-8")
    lane = LANE.read_text(encoding="utf-8")
    if args.self_test:
        self_test(backlog, lane)
        print("B-107 runtime evidence boundary mutations: PASS")
    else:
        verify(backlog, lane)
        print("B-107 runtime evidence boundary: PASS")


if __name__ == "__main__":
    main()
