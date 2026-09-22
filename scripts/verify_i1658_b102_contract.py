#!/usr/bin/env python3
"""Credentialless static contract guard for issue #1658 / B-102.

This guard reads repository files only. It never dispatches the staging probe,
contacts a provider, or accepts a production target as an alternate.
"""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/issue-1658-b102-cargo-put.yml"
PROBE = ROOT / "scripts/probe_i1658_cargo_put_latency.py"
MANIFEST = ROOT / "evidence/owner-actions/B-102/hot-cargo-put-contract.json"


def require(text: str, fragment: str, label: str) -> None:
    if fragment not in text:
        raise SystemExit(f"B-102 contract missing {label}: {fragment}")


def main() -> int:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    probe = PROBE.read_text(encoding="utf-8")
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))

    if manifest.get("issue") != 1658 or manifest.get("backlog_id") != "B-102":
        raise SystemExit("manifest is not bound to issue 1658 / B-102")
    if manifest.get("environment") != "staging" or manifest.get("production_allowed") is not False:
        raise SystemExit("manifest must be staging-only and forbid production")
    if manifest.get("scope", {}).get("serial_samples") != 10:
        raise SystemExit("manifest must require ten serial samples")
    if manifest.get("scope", {}).get("concurrency_control") != [1, 4]:
        raise SystemExit("manifest must retain the bounded concurrency controls")
    if manifest.get("scope", {}).get("statistics") != ["arithmetic median", "nearest-rank p90"]:
        raise SystemExit("manifest must require arithmetic median and nearest-rank p90")

    require(workflow, "workflow_dispatch:", "manual staging trigger")
    for forbidden in ("pull_request:", "push:", "schedule:", "repository_dispatch:"):
        if forbidden in workflow:
            raise SystemExit(f"staging B-102 lane must remain manual; found {forbidden}")
    for required in (
        "environment: staging",
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main'",
        "github.ref_protected",
        "run-1658-b102-staging",
        "https://staging.corelink.humangr.com",
        "CORELINK_B102_STAGING_PAT",
        "scripts/probe_i1658_cargo_put_latency.py",
        "timeout-minutes: 10",
        "persist-credentials: false",
        "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0",
        "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
        "if: always()",
    ):
        require(workflow, required, required)
    if "corelink-api.humangr.com" in workflow or re.search(r"--token\b", workflow):
        raise SystemExit("staging lane contains a production target or token command argument")

    for required in (
        'TARGET = "https://staging.corelink.humangr.com"',
        'RETRYABLE = {429, 500, 502, 503, 504}',
        '"attempts": 1',
        'cleanup = request(base, tenant, token, "DELETE"',
        'row["hashing_phase"] = "ohandler"',
        '"ostore"',
        '"oaccounting"',
        'row["auth_source"] == "d1"',
        'row["auth_source"] in {"l1", "kv"}',
        '"p50_ms"',
        '"p90_ms"',
        'concurrent.futures.ThreadPoolExecutor(max_workers=level)',
    ):
        require(probe, required, required)
    if "urlopen" not in probe or '"retry_after"' not in probe or "for attempt" in probe:
        raise SystemExit("probe must retain retry outcomes without silently retrying")
    if "args.base.rstrip(\"/\") != TARGET" not in probe:
        raise SystemExit("probe target boundary is not fail-closed")
    if '"environment": "staging"' not in probe or '"production"' in probe:
        raise SystemExit("probe evidence boundary is not staging-only")

    print("B-102 static contract: PASS (credentialless; no staging dispatch; production forbidden)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
