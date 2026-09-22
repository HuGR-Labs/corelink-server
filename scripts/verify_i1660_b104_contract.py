#!/usr/bin/env python3
"""Credentialless static and adversarial contract gate for issue #1660.

This gate reads the checked-in B-104 lane only. It never dispatches Actions,
contacts a target, reads a secret, or invokes the latency probe.
"""

from __future__ import annotations

import json
import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/issue-1660-b104-authenticated-404.yml"
MANIFEST = ROOT / "evidence/owner-actions/B-104/authenticated-404-contract.json"
PROBE = ROOT / "scripts/probe-authenticated-404.sh"
RUNBOOK = ROOT / "specs/_runbooks/RB-B104-AUTHENTICATED-404.md"


def require(text: str, fragment: str, label: str) -> None:
    if fragment not in text:
        raise SystemExit(f"B-104 contract missing {label}: {fragment}")


def main() -> int:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    probe = PROBE.read_text(encoding="utf-8")
    runbook = RUNBOOK.read_text(encoding="utf-8")
    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))

    if manifest["issue"] != 1660 or manifest["backlog_id"] != "B-104":
        raise SystemExit("manifest issue identity drifted")
    if manifest["status"] != "open_external_measurement_required":
        raise SystemExit("manifest must remain open pending external evidence")
    if manifest["credentialless"] is not True or manifest["network_calls"] is not False:
        raise SystemExit("manifest must remain credentialless and network-free")
    require(workflow, "workflow_dispatch:", "manual trigger")
    for forbidden in ("pull_request:", "push:", "schedule:", "repository_dispatch:"):
        if forbidden in workflow:
            raise SystemExit(f"B-104 measurement lane must remain manual; found {forbidden}")
    for fragment, label in (
        ("environment: staging", "staging environment"),
        ("github.ref == 'refs/heads/main'", "main guard"),
        ("github.ref_protected", "protected-ref guard"),
        ('test "$CONFIRM" = run-1660-b104-staging-read-only', "exact confirmation"),
        ('test "$NO_WRITE_ACK" = staging-only-no-write', "no-write acknowledgement"),
        ('PROBE_SAMPLES: "10"', "bounded ten-sample population"),
        ('CORELINK_B104_STAGING_PAT', "staging-only PAT secret"),
        ("if: always()", "evidence upload on failure"),
        ("persist-credentials: false", "checkout credential isolation"),
        ("actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "pinned checkout"),
        ("actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a", "pinned upload"),
    ):
        require(workflow, fragment, label)
    if re.search(r"--(?:header|token|pat|secret)\s+\$\{?\{?\s*secrets", workflow, re.I):
        raise SystemExit("secret must not be passed as a command-line argument")
    if "corelink-api.humangr.com" in workflow and "production origin is forbidden" not in workflow:
        raise SystemExit("known production origin is present without an explicit rejection")
    if re.search(r"curl[^\n]*(?:PUT|POST|PATCH|DELETE)|method=\"(?:PUT|POST|PATCH|DELETE)\"", workflow, re.I):
        raise SystemExit("measurement workflow contains a forbidden mutation")

    require(probe, 'PROBE_BASE="${PROBE_BASE:-}"', "explicit target prerequisite")
    require(probe, 'PROBE_SAMPLES="${PROBE_SAMPLES:-10}"', "ten-sample default")
    require(probe, 'openssl rand -hex 32', "fresh 64-hex object keys")
    require(probe, '[ "${REQUEST_STATUS}" = "404" ]', "served authenticated 404 assertion")
    require(probe, "phase_stats()", "wall median/p90 reporting helper")
    require(probe, "for phase in wall auth wdb", "Server-Timing phase reporting")
    require(probe, "for required_phase in auth wdb origin opat ohandler total; do", "required attribution phases")
    require(probe, "staging-origin-redacted", "redacted target label")
    if '"${PROBE_BASE}"' in probe.split("printf 'B-104 authenticated 404 probe", 1)[-1].split("if ! request", 1)[0]:
        raise SystemExit("probe prints the target origin before requests")
    if re.search(r"printf[^\n]*PROBE_TOKEN[^\n]*>>?\s*\$\{?GITHUB_STEP_SUMMARY", probe, re.I):
        raise SystemExit("probe may print token material to a retained summary")
    if re.search(r"curl[^\n]*(?:--request|-X)\s*(?:PUT|POST|PATCH|DELETE)", probe, re.I):
        raise SystemExit("probe contains a forbidden mutation method")

    for fragment, label in (
        ("issue-1660-b104-authenticated-404.yml", "hosted lane reference"),
        ("`staging` environment", "staging prerequisite"),
        ("median and nearest-rank p90", "percentile contract"),
        ("Server-Timing", "phase contract"),
        ("does not retain the PAT", "secret redaction"),
    ):
        require(runbook, fragment, label)

    print("B-104 static/adversarial contract: PASS (credentialless; no dispatch)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
