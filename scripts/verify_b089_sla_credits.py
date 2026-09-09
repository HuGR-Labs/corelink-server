#!/usr/bin/env python3
"""Fail-closed repository guard for B-089.

This guard proves the repo-owned mechanics only.  It intentionally keeps B-089
parked: a live Stripe test-mode mutation and a subsequent invoice reconciliation
remain owner/provider evidence, not something a source grep can claim.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def verify(root: Path = ROOT) -> dict[str, object]:
    failures: list[str] = []

    def need(path: str, needles: tuple[str, ...]) -> None:
        target = root / path
        if not target.is_file():
            failures.append(f"missing {path}")
            return
        body = target.read_text(encoding="utf-8")
        for needle in needles:
            if needle not in body:
                failures.append(f"{path}: missing {needle}")

    need("apps/signup-worker/src/webhooks/sla_credit_cron.ts", (
        "publishClosedSlaMeasurements", "recordCanonicalSlaObservation", "handleSlaObservationIngest", "parseUtcMonth",
        "monthlyCutoffAtMs", "LIMIT ?", "state = 'pending'", "tenant_mapping_pending",
        "SLA_CREDITS_ENABLED", "provider_disabled", "Number.isSafeInteger", "BigInt",
        "sla_credit_outbox", "sla_credit_reconciliation", "db.batch",
        "Idempotency-Key", "Math.min(100, percent)", "attempts = attempts + 1",
        "x-corelink-sla-observation-key", "SLA_OBSERVATIONS_ENABLED", "stripe_reconcile_mismatch",
        "provider_recovery_reconcile_failed", "status = 'needs_review'",
    ))
    need("migrations/d1/0117_sla_credit_ledger.sql", (
        "sla_monthly_observations", "sla_monthly_measurements", "sla_credit_ledger",
        "sla_credit_outbox", "sla_credit_reconciliation", "sla_credit_audit_events",
        "UNIQUE (tenant_id, service_period)", "idempotency_key TEXT NOT NULL UNIQUE",
        "published_at_ms", "cutoff_at_ms", "next_attempt_at_ms", "lease_until_ms",
    ))
    need("apps/signup-worker/tests/sla_credit_cron.test.ts", (
        "does not starve a newer row", "mapping misses starve", "missing tenant mapping", "accepted provider object",
        "provider_disabled", "outbox and reconciliation", "canonical observation producer",
    ))
    need("apps/signup-worker/wrangler.toml", ("SLA_CREDITS_ENABLED = \"false\"", "SLA_OBSERVATIONS_ENABLED = \"false\"", "STRIPE_SECRET_KEY", "FOUR sweep families"))

    sla = root / "legal/sla/v1.0.0.md"
    if not sla.is_file():
        failures.append("missing legal/sla/v1.0.0.md")
    else:
        body = sla.read_text(encoding="utf-8")
        if "issued automatically against the next invoice" not in body:
            failures.append("executed SLA promise was silently removed")
        if "sole and exclusive remedy" not in body:
            failures.append("executed SLA exclusive-remedy clause was silently removed")

    return {
        "ok": not failures,
        "failures": failures,
        "status": "parked_until_provider_proof",
        "post_deploy_proof_remaining": [
            "apply migration 0117 to the production D1 binding",
            "run one owner-approved Stripe test-mode invoice-item mutation with the gate enabled",
            "replay the same sweep and verify one provider object for the stable idempotency key",
            "reconcile the provider object against the next invoice and retain the D1 audit rows",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    result = verify(Path(args.root).resolve())
    if args.json:
        print(json.dumps(result, indent=2))
    elif result["ok"]:
        print("B-089 SLA-credit repository guard: PASS (parked until provider proof)")
        for item in result["post_deploy_proof_remaining"]:
            print(f"POST-DEPLOY: {item}")
    else:
        print("B-089 SLA-credit repository guard: FAIL")
        for failure in result["failures"]:
            print(f"FAIL: {failure}")
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
