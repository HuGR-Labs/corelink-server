#!/usr/bin/env python3
"""Credentialless static guard for the #1700 provider preflight workflow."""
from __future__ import annotations

from pathlib import Path


WORKFLOW = Path(".github/workflows/staging-provider-preflight.yml")
PROVIDER = Path("scripts/staging_bootstrap_provider.py")


def main() -> int:
    workflow = WORKFLOW.read_text(encoding="utf-8")
    provider = PROVIDER.read_text(encoding="utf-8")
    required_workflow = (
        "github.repository == 'HuGR-dev/corelink-server'",
        "github.ref == 'refs/heads/main' && github.ref_protected",
        'test "$EXPECTED_SHA" = "$GITHUB_SHA"',
        'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"',
        "persist-credentials: false",
        "timeout-minutes: 10",
        "scripts/staging_bootstrap_provider.py --phase preflight",
        "actions/upload-artifact@",
        "retention-days: 30",
    )
    missing = [item for item in required_workflow if item not in workflow]
    if missing:
        raise SystemExit(f"preflight workflow is missing safety guards: {missing}")
    if "pull_request:" not in workflow or "if: github.event_name == 'pull_request'" not in workflow:
        raise SystemExit("pull request checks must remain credentialless and separate")
    for forbidden in ("wrangler deploy", "secret put", "dns_records", "workers/routes"):
        if forbidden in workflow:
            raise SystemExit(f"provider workflow contains a forbidden mutation surface: {forbidden}")
    if 'choices=("preflight", "quarantine", "postflight")' not in provider:
        raise SystemExit("provider phase interface changed; re-review the read-only preflight")
    preflight = provider.split('if args.phase in {"preflight", "quarantine"}', 1)[0]
    if 'if args.phase == "postflight"' in preflight or 'args.phase in {"quarantine", "postflight"}' in preflight:
        raise SystemExit("preflight can reach post-deployment provider checks")
    if "urllib.request.urlopen" not in provider or 'method="GET"' in provider:
        raise SystemExit("provider readback request behavior changed; re-review before dispatch")
    print("staging provider preflight safety contract passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
