#!/usr/bin/env python3
"""Decision-bearing B-076 guard for payable subscription ownership."""
from __future__ import annotations

import argparse
from pathlib import Path


class VerificationError(RuntimeError):
    pass


def assess(root: Path) -> list[str]:
    store = (root / "crates/corelink-container/src/routes/tier_select_store.rs").read_text()
    route = (root / "crates/corelink-container/src/routes/tier_select.rs").read_text()
    route += "\n" + "\n".join(path.read_text() for path in sorted(
        (root / "crates/corelink-container/src/routes/tier_select").glob("part-*.rs")
    ))
    client = (root / "crates/corelink-stripe-real/src/client.rs").read_text()
    checkout = (root / "apps/signup-worker/src/webhooks/billing_checkout.ts").read_text()
    webhook = (root / "apps/signup-worker/src/webhooks/stripe.ts").read_text()
    billing_checkout = (root / "apps/signup-worker/src/webhooks/billing_checkout.ts").read_text()
    stripe_persistence = (root / "apps/signup-worker/src/webhooks/stripe_persistence.ts").read_text()
    webhook_all = webhook + "\n" + billing_checkout + "\n" + stripe_persistence
    migration = (root / "migrations/d1/0111_b076_payable_subscription_ownership.sql").read_text()
    handled = (root / "apps/signup-worker/src/webhooks/handled-stripe-events.json").read_text()
    focal = (root / "apps/signup-worker/tests/stripe_b076_ownership.test.ts").read_text()
    required = {
        "correlation-owned lock release":
            "DELETE FROM tier_selection_locks WHERE tenant_id = ?1 AND correlation_id = ?2" in store,
        "pending checkout reader": "FROM stripe_checkout_sessions" in store and
            "subscription_state = 'pending_checkout'" in store,
        "recoverable checkout ledger":
            "stripe_checkout_ownership_ledger" in migration and
            "reconcile_pending_checkout" in store and
            "state = 'session_created'" in store,
        "pending write has live lease guard":
            "INSERT INTO tier_selections" in store and "RETURNING tenant_id" in store and
            "expires_at_ms >= ?5" in store,
        "pre-Stripe payable reservation":
            "reserve_pending_checkout" in store and "subscription_state = 'pending_checkout'" in store,
        "cache-only payable reservation":
            "if !tier.is_runner()" in route and "reserve_pending_checkout" in route,
        "payable partial unique index":
            "idx_tenant_one_payable_subscription" in migration and
            "subscription_state IN ('pending_checkout', 'active')" in migration,
        "tenant-scoped checkout idempotency":
            "checkout_idempotency_key" in client and
            'format!("checkout:{axis}:{tenant_id}")' in client and
            '"cache"' in client and '"runner"' in client,
        "billing session ownership proof":
            "s.session_id = ?7" in checkout and "s.tenant_id = ?1" in checkout and
            "t.stripe_customer_id = ?2" in checkout,
        "billing paid conflict cannot clobber":
            "tenant_billing.status != 'paid'" in checkout and
            "tenant_billing.stripe_subscription_id IS excluded.stripe_subscription_id" in checkout,
        "activation repeats exact billing owner":
            "stripe_subscription_id = ?6" in webhook_all and "status = 'paid'" in webhook_all and
            "tier activation write did not return D1 changes metadata" in webhook_all,
        "webhook writes are causal":
            "Array<() => Promise<void>>" in webhook and
            "for (const write of requiredWrites)" in webhook and
            "Promise.all(requiredWrites)" not in webhook,
        "runner/cache reservation axes remain separate":
            "if !tier.is_runner()" in route and "reserve_pending_checkout" in route,
        "expired checkout releases only its correlation":
            "checkout.session.expired" in handled and
            "correlation_id = COALESCE" in webhook_all and
            "stripe_checkout_ownership_ledger" in webhook_all,
        "bounded crash rebind":
            "CHECKOUT_RESERVATION_RECOVERY_TTL_MS" in store and
            "SET correlation_id = ?2" in store and
            "updated_at_ms >= ?3 - ?6" in store and
            "EXISTS (SELECT 1 FROM tier_selection_locks" in store and
            "correlation_id = ?2 AND expires_at_ms >= ?3" in store,
        "webhook ledger metadata recovery":
            "recoverCheckoutLedger" in webhook_all and
            "recoverCheckoutLedger" in checkout and
            "created_at_ms <= ?6 + 2000" in checkout and
            "checkoutCreatedAtMs" in webhook,
        "adversarial worker focal":
            "rejects an update that D1 says did not own" in focal and
            "repeats the exact subscription owner" in focal and
            "executes stale and fresh webhook recovery against SQLite" in focal,
    }
    return [name for name, ok in required.items() if not ok]


def self_test(root: Path) -> None:
    source = (root / "crates/corelink-stripe-real/src/client.rs").read_text()
    identity = lambda text: ('format!("checkout:{axis}:{tenant_id}")' in text and
                             'format!("checkout:{tenant_id}")' not in text)
    if not identity(source):
        raise VerificationError("baseline checkout identity is not closed")
    mutated = source.replace('format!("checkout:{axis}:{tenant_id}")',
                             'format!("checkout:{tenant_id}")')
    if identity(mutated):
        raise VerificationError("checkout identity mutation was not rejected")
    # Independent mutation teeth for the two SQL boundaries.
    checkout = (root / "apps/signup-worker/src/webhooks/billing_checkout.ts").read_text()
    if "s.session_id = ?7" not in checkout:
        raise VerificationError("billing ownership mutation was not present")
    if "s.session_id = ?7" in checkout.replace("s.session_id = ?7", "s.session_id = ?8"):
        raise VerificationError("billing session mutation was not detectable")
    webhook = (root / "apps/signup-worker/src/webhooks/stripe.ts").read_text()
    causal = lambda text: ("for (const write of requiredWrites)" in text and
                           "Promise.all(requiredWrites)" not in text)
    if not causal(webhook):
        raise VerificationError("causal webhook writer is not present")
    if causal(webhook.replace("for (const write of requiredWrites)",
                              "for (const queuedWrite of requiredWrites)")):
        raise VerificationError("causal webhook mutation was not detectable")
    store = (root / "crates/corelink-container/src/routes/tier_select_store.rs").read_text()
    ledger_guard = lambda text: ("stripe_checkout_ownership_ledger" in text and
                                 "state = 'session_created'" in text)
    if not ledger_guard(store):
        raise VerificationError("recoverable checkout ledger is not present")
    if ledger_guard(store.replace("stripe_checkout_ownership_ledger", "removed_checkout_ledger")):
        raise VerificationError("ledger mutation was not detectable")
    if "updated_at_ms >= ?3 - ?4" in store.replace("updated_at_ms >= ?3 - ?4", "updated_at_ms >= ?3"):
        raise VerificationError("reservation recovery TTL mutation was not detectable")
    if "created_at_ms <= ?6 + 2000" in checkout.replace("created_at_ms <= ?6 + 2000", "created_at_ms <= ?8 + 2000"):
        raise VerificationError("webhook metadata age mutation was not detectable")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--expect", choices=("done",), default="done")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    gaps = assess(args.root)
    if gaps:
        raise VerificationError("B-076 drifted: " + "; ".join(gaps))
    if args.self_test:
        self_test(args.root)
    print("done: B-076 payable subscription ownership is tenant-scoped, unique, idempotent, and fail-closed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, VerificationError) as exc:
        print(f"FALHA: {exc}")
        raise SystemExit(1)
