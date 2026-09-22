#!/usr/bin/env python3
"""Fail-closed contract check for the unprovisioned #1700 staging target."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path

ORIGIN = "https://staging.corelink.humangr.com"
CONTRACT = Path("infra/staging/topology.json")
WORKFLOWS = (Path(".github/workflows/load-test-nightly.yml"), Path(".github/workflows/endurance-2h-nightly.yml"))
SECRETS = {
    "K6_STAGING_BYOK_CMK_ID",
    "K6_STAGING_MFA_STUB",
    "K6_STAGING_PAT",
    "K6_STAGING_STRIPE_WHSEC",
    "K6_STAGING_TEARDOWN_TOKEN",
    "K6_TARGET_IDENTITY_RECEIPT",
    "K6_TARGET_HOST",
}
RESOURCES = {"corelink-config-staging", "corelink-cas-staging", "corelink-ac-iad-staging", "corelink-chunk-iad-staging", "corelink-manifest-iad-staging", "corelink-metadata-staging", "corelink-clerk-jwks-staging", "corelink-negative-cache-staging", "corelink-dsr-erasure-staging", "corelink-dsr-erasure-dlq-staging"}


def assess(root: Path) -> list[str]:
    data = json.loads((root / CONTRACT).read_text(encoding="utf-8"))
    gaps: list[str] = []
    if data.get("schema_version") != 1 or data.get("issue") != 1700:
        gaps.append("schema-or-issue")
    if data.get("deployment_state") != "unprovisioned":
        gaps.append("deployment-must-remain-unprovisioned")
    if data.get("canonical_origin") != ORIGIN:
        gaps.append("canonical-origin")
    owner = data.get("ownership", {})
    if owner.get("owner") != "SRE Lead" or set(owner.get("approvers", [])) != {"SRE Lead", "Security Lead"}:
        gaps.append("ownership")
    budget = data.get("budget", {})
    if budget.get("enforce_before_apply") is not True or budget.get("workflow_timeout_minutes") != 145 or budget.get("max_load_run", 0) <= 0 or budget.get("max_endurance_run", 0) <= 0 or budget.get("monthly_cap", 0) <= 0:
        gaps.append("budget")
    life = data.get("lifecycle", {})
    teardown = life.get("teardown", {})
    if not (0 < life.get("lease_ttl_hours", 0) <= 24 and 0 < life.get("idle_teardown_after_hours", 0) <= 2 and life.get("r2_object_ttl_hours") == 24 and teardown.get("manual_only") is True and teardown.get("automatic") is False and teardown.get("fail_closed") is True):
        gaps.append("lifecycle-or-teardown")
    inputs = data.get("validated_inputs", {})
    if inputs.get("runner_label") != "corelink" or inputs.get("target_host") != ORIGIN or set(inputs.get("load_scenarios", [])) != {"signup", "webhook", "dsr", "cas", "byok"} or set(inputs.get("endurance_durations", [])) != {"30s", "2h"} or set(inputs.get("required_secret_names", [])) != SECRETS:
        gaps.append("validated-inputs")
    outputs = data.get("outputs", {})
    if outputs.get("target_host") != ORIGIN or outputs.get("github_environment") != "staging" or set(outputs.get("resource_names", [])) != RESOURCES:
        gaps.append("outputs")
    cf = data.get("cloudflare", {})
    if cf.get("zone_name") != "humangr.com" or cf.get("route") != "staging.corelink.humangr.com/*" or cf.get("root_worker") != "corelink-staging" or cf.get("signup_worker") != "corelink-signup-staging" or cf.get("synthetic_receiver_worker") != "corelink-synthetic-pager-staging":
        gaps.append("cloudflare-boundary")
    settings = cf.get("root_worker_settings", {})
    if settings.get("workers_dev") is not False or settings.get("crons") != [] or settings.get("observability", {}).get("enabled") is not True:
        gaps.append("worker-isolation")
    if cf.get("container", {}).get("max_instances") != 5 or cf.get("container", {}).get("instance_type") != "basic":
        gaps.append("container-budget")
    serialized = json.dumps(data, sort_keys=True)
    if re.search(r'(?i)(secret|token|password|credential)[^,}]*:', serialized) and re.search(r'(?i)(secret|token|password|credential)[^,}]*:\s*"(?!K6_|https?://)', serialized):
        gaps.append("possible-secret-value")
    for resource in data.get("outputs", {}).get("resource_names", []):
        if not isinstance(resource, str) or not resource.endswith("-staging"):
            gaps.append("non-staging-resource-name")
            break
    for path in WORKFLOWS:
        text = (root / path).read_text(encoding="utf-8")
        if "CANONICAL_TARGET='https://staging.corelink.humangr.com'" not in text or 'TARGET_HOST="${K6_TARGET_HOST%/}"' not in text or '[[ "$TARGET_HOST" != "$CANONICAL_TARGET" ]]' not in text:
            gaps.append(f"workflow-host:{path.name}")
    endurance = (root / WORKFLOWS[1]).read_text(encoding="utf-8")
    if "30s|2h" not in endurance or 'DURATION: ${{ github.event.inputs.duration || \'2h\' }}' not in endurance:
        gaps.append("workflow-duration-budget")
    if re.search(r"(?m)^\s*\[env\.staging\]\s*$", (root / "wrangler.toml").read_text(encoding="utf-8")):
        gaps.append("partial-wrangler-staging-environment")
    return sorted(set(gaps))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    gaps = assess(args.root.resolve())
    print(f"staging target contract: {'OPEN' if gaps else 'READY TO PROVISION'} ({len(gaps)} gap(s))")
    for gap in gaps:
        print(f"- {gap}")
    return 1 if gaps else 0


if __name__ == "__main__":
    raise SystemExit(main())
