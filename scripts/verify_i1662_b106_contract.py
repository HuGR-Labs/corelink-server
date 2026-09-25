#!/usr/bin/env python3
"""Static, credentialless contract check for the #1662 B-106 lane.

This check deliberately reads source text only.  It is suitable for a hosted
PR gate and never dispatches the production workflow or contacts the API.
"""

from __future__ import annotations

import re
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github/workflows/issue-1662-b106-cold-warm.yml"
PROBE = ROOT / "scripts/probe_i1662_b106.py"


def require(text: str, fragment: str, label: str) -> None:
    if fragment not in text:
        raise SystemExit(f"B-106 contract missing {label}: {fragment}")


def main() -> int:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    probe = PROBE.read_text(encoding="utf-8")

    require(workflow, "workflow_dispatch:", "manual trigger")
    for forbidden in ("pull_request:", "push:", "schedule:", "repository_dispatch:"):
        if forbidden in workflow:
            raise SystemExit(f"B-106 lane must remain manual; found {forbidden}")
    require(workflow, "environment: production", "protected production environment")
    require(workflow, "github.ref == 'refs/heads/main'", "main guard")
    require(workflow, "github.ref_protected", "protected-ref guard")
    require(workflow, "test \"$CONFIRM\" = run-1662-b106-cold-warm", "exact confirmation")
    require(workflow, "if: always()", "unconditional revoke/upload cleanup")
    require(workflow, "scripts/probe_i1662_b106.py run", "probe implementation")
    require(workflow, "scripts/probe_i1662_b106.py cleanup", "revoke implementation")
    require(workflow, "CORELINK_B106_OWNER_SESSION", "protected owner session")
    require(workflow, "secrets.CORELINK_B106_DOGFOOD_TENANT", "protected dogfood tenant")
    if "inputs.tenant" in workflow:
        raise SystemExit("dogfood tenant must not be persisted in workflow-dispatch inputs")
    require(workflow, "timeout-minutes: 7", "bounded timeout")
    require(workflow, "persist-credentials: false", "checkout credential isolation")
    require(workflow, "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0", "pinned checkout")
    require(workflow, "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a", "pinned artifact upload")
    if re.search(r"--owner-session|--token\b", workflow):
        raise SystemExit("session/token must not be passed as a command-line argument")

    require(probe, "MIN_IDLE_SECONDS = 61", "61-second cold interval")
    require(probe, 'cold["auth_source"] != "d1"', "cold d1 proof")
    require(probe, 'warm["auth_source"] not in {"l1", "kv"}', "warm cache proof")
    require(probe, 'warm["colo"] != cold["colo"]', "same-colo proof")
    require(probe, 'f"{base}/cargo/{tenant}/{key}"', "tenant-bound GET")
    require(probe, 'method="GET"', "read-only data-plane method")
    require(probe, 'f"{base}/v1/customer/keys/{pat_id}/revoke"', "PAT revoke endpoint")
    require(probe, "args.state.unlink(missing_ok=True)", "state cleanup")
    require(probe, '"captured_at_epoch": time.time()', "per-request observation timestamp")
    for forbidden in (
        '"token_fingerprint"',
        '"tenant_id": tenant',
        'receipt["pat_id"]',
        '"response_body_sha256"',
        '"request_id":',
    ):
        if forbidden in probe:
            raise SystemExit(f"B-106 receipt must not disclose sensitive field: {forbidden}")
    if re.search(r'print\([^\n]*token|logger\.[a-z]+\([^\n]*token', probe, re.IGNORECASE):
        raise SystemExit("probe must never print token material")
    if re.search(r"method=\"(?:PUT|PATCH|DELETE)\"", probe):
        raise SystemExit("B-106 probe contains a forbidden data-plane mutation")

    print("B-106 static contract: PASS (credentialless; no dispatch)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
