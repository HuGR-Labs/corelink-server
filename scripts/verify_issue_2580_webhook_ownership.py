#!/usr/bin/env python3
"""Static writer/class census for the #2580 webhook ownership boundary."""

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"#2580 ownership census failed: {message}")


def main() -> None:
    inbox = (ROOT / "crates/corelink-container/src/webhook_inbox_d1.rs").read_text()
    audit = (ROOT / "crates/corelink-container/src/webhook_dlq_d1.rs").read_text()
    worker = (ROOT / "apps/signup-worker/src/webhooks/stripe.ts").read_text()
    batch = (ROOT / "apps/signup-worker/src/webhooks/stripe_staging_batch.ts").read_text()
    billing = (ROOT / "apps/signup-worker/src/webhooks/stripe_persistence_billing.ts").read_text()
    checkout = (ROOT / "apps/signup-worker/src/webhooks/billing_checkout.ts").read_text()
    runner = (ROOT / "apps/signup-worker/src/webhooks/stripe_persistence_runner.ts").read_text()
    tests = (ROOT / "apps/signup-worker/tests/stripe.test.ts").read_text()
    dispatcher = (ROOT / "crates/corelink-stripe-real/src/webhook_dispatch.rs").read_text()

    require("SQL_RECEIVE" in inbox and "StagingLoadTestResourceClass::WebhookInbox" in inbox,
            "Rust inbox writer is not mapped to webhook_inbox")
    require("SQL_EFFECT_INSERT" in inbox and "SQL_EFFECT_SEAL" in inbox and
            "StagingLoadTestResourceClass::WebhookEffect" in inbox,
            "Rust effect reserve/commit writers are not mapped to webhook_effect")
    require("SQL_REQUIRE_EXISTING_OWNERSHIP" in inbox and
            "commit_effect_with_ownership_context" in inbox,
            "Rust effect commit does not atomically require its reserved owner row")
    durable_dispatch = dispatcher.split("fn process_durable(", 1)[1].split("fn quarantine_transient(", 1)[0]
    require("if emit_audit(" in durable_dispatch and
            ".emit_audit_and_sli(" not in durable_dispatch,
            "Rust durable success audit bypasses the request ownership context")
    require("SQL_BILLING_AUDIT_INSERT" in audit and
            "StagingLoadTestResourceClass::BillingAudit" in audit and
            "StagingLoadTestDisposition::Retained" in audit,
            "Rust billing audit writer is not retained and attributed")
    require("INSERT OR IGNORE INTO stripe_webhook_events_processed" in batch and
            "db.batch(statements)" in batch and "ownershipInsertStatement" in batch,
            "Worker claim/writes/ownership do not share one D1 batch")
    require("WHERE changes() = 0" in batch and "webhook_inbox" in batch and
            "webhook_effect" in batch,
            "Worker replay fences or ownership classes are missing")

    # Every D1 writer callback is request-local and stages its prepared
    # statements when an admitted context exists. Ordinary None calls still
    # execute through runOrStage's compatibility branch.
    callbacks = re.findall(r"requiredWrites\.push\((.*?)\)\);", worker, re.S)
    require(callbacks, "Stripe writer callback inventory is empty")
    require(all("requestBatch" in callback for callback in callbacks),
            "a Stripe required-write callback bypasses the request batch")
    require("batch?: StripeStagingWriteBatch" in checkout and
            "batch?: StripeStagingWriteBatch" in billing and
            "batch?: StripeStagingWriteBatch" in runner,
            "a checkout, billing, or runner D1 writer lacks the optional batch path")
    require("changes() != 1" in checkout and "atomicGuard" in runner,
            "checkout or runner failure guard is not part of its transaction")
    require("commits admitted claim, billing writes, and ownership rows in one D1 batch; replay rolls back" in tests and
            "rejects a present invalid staging envelope before Stripe billing writes" in tests,
            "signed-envelope replay and invalid-before-write negative controls are missing")

    print("#2580 writer/class census: PASS")
    print("Rust D1 writers: webhook inbox, webhook effect reserve/commit, retained billing audit")
    print(f"Worker required-write callbacks: {len(callbacks)}; one request-local D1 batch with claim/replay fence")


if __name__ == "__main__":
    main()
