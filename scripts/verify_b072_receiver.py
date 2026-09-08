#!/usr/bin/env python3
"""Dependency-free static guard for B-072 receiver/schedule wiring."""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path


def fail(message: str) -> None:
    raise AssertionError(message)


def main(root: Path) -> None:
    root_config = root / "wrangler.toml"
    receiver_config = root / "apps/synthetic-pager-worker/wrangler.toml"
    scheduler = root / "worker/src/index_schedule.ts"
    receiver = root / "apps/synthetic-pager-worker/src/index.ts"
    contract = root / "apps/synthetic-pager-worker/src/contract.ts"
    workspace = root / "pnpm-workspace.yaml"
    for path in (root_config, receiver_config, scheduler, receiver, contract, workspace):
        if not path.is_file():
            fail(f"missing B-072 contract file: {path.relative_to(root)}")

    root_data = tomllib.loads(root_config.read_text())
    triggers = root_data.get("triggers", {}).get("crons", [])
    if triggers != ["0 14 * * 1"]:
        fail(f"root schedule must contain only the synthetic cron, got {triggers!r}")

    staging = root_data.get("env", {}).get("staging", {})
    if staging.get("triggers", {}).get("crons") != []:
        fail("root staging schedule is not explicitly disabled")
    staging_bindings = staging.get("services", [])
    if not any(
        item.get("binding") == "SCHEDULED_DRILL_DELIVERY"
        and item.get("service") == "corelink-synthetic-pager-staging"
        for item in staging_bindings
    ):
        fail("staging receiver service binding is missing")

    for environment in ("prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"):
        env_data = root_data.get("env", {}).get(environment, {})
        if env_data.get("triggers", {}).get("crons") != []:
            fail(f"{environment} does not explicitly disable schedules")
        if any(item.get("binding") == "SCHEDULED_DRILL_DELIVERY" for item in env_data.get("services", [])):
            fail(f"{environment} has an accidental synthetic receiver binding")

    receiver_data = tomllib.loads(receiver_config.read_text())
    if receiver_data.get("vars", {}).get("SYNTHETIC_DRILL_ENABLED") != "false":
        fail("receiver default activation is not fail-closed")
    for environment in ("staging", "prod"):
        env_data = receiver_data.get("env", {}).get(environment, {})
        if env_data.get("vars", {}).get("SYNTHETIC_DRILL_ENABLED") != "false":
            fail(f"receiver {environment} activation is not disabled")
        if env_data.get("triggers", {}).get("crons") != []:
            fail(f"receiver {environment} has an unexpected cron")

    receiver_source = receiver.read_text()
    contract_source = contract.read_text()
    scheduler_source = scheduler.read_text()
    workspace_source = workspace.read_text()
    required_fragments = {
        "production environment guard": 'environment === "prod" || environment?.startsWith("prod-")',
        "activation gate": 'env.SYNTHETIC_DRILL_ENABLED !== "true"',
        "canonical endpoint gate": "env.PAGERDUTY_EVENTS_URL !== PAGERDUTY_EVENTS_URL",
        "routing-key gate": "PAGERDUTY_SYNTHETIC_ROUTING_KEY?.trim()",
        "header/payload correlation gate": "deliveryId !== envelope.synthetic_page.dedup_key",
        "PagerDuty non-2xx guard": "!pagerDutyResponse.ok",
        "D1-before-PagerDuty path": "await persistDelivery(env, envelope)",
    }
    for label, fragment in required_fragments.items():
        if fragment not in (receiver_source + contract_source):
            fail(f"missing {label}")
    for label, fragment in {
        "scheduler deterministic delivery id": "const deliveryId = `${drill}:${controller.cron}:${controller.scheduledTime}`",
        "scheduler correlation id": "correlation_id: `PAT-CORRELATION-ID-001:${deliveryId}`",
        "scheduler retry on non-2xx": "if (!response.ok)",
    }.items():
        if fragment not in scheduler_source:
            fail(f"missing {label}")
    if "apps/synthetic-pager-worker" not in workspace_source:
        fail("receiver package is not in the pnpm workspace")

    print("B-072 receiver/schedule guard: PASS")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        main(args.root.resolve())
    except (AssertionError, OSError, tomllib.TOMLDecodeError) as error:
        print(f"B-072 receiver/schedule guard: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
