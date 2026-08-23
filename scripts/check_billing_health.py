#!/usr/bin/env python3
"""Daily LIVE billing-health check — flags revenue-affecting anomalies in D1.

WHY THIS EXISTS
---------------
On 2026-08-22 a **live** Stripe subscription (`sub_1TvRyN…`, $16/mo) was found
sitting in `past_due` for a full month, with an open invoice at two failed
payment attempts and repeated `invoice.payment_failed` webhooks landing in
production. Nothing alerted.

The pre-existing `billing-reconcile-daily.yml` could not have caught it, for two
independent reasons:

  1. It reconciles **D1 against D1** (a usage table against a Stripe *ledger*
     table). It never asks Stripe — or the materialized subscription state —
     what a subscription's STATUS is, so a dunning failure is structurally
     invisible to it.
  2. The ledger table it reads (`stripe_idempotency_keys`) held **zero rows**,
     so the comparison was empty-vs-empty and reported success every single day.
     A green check over no data proves nothing.

This checker closes that gap from the opposite direction: it reads the
**materialized subscription + webhook state** that the Stripe webhook handler
already writes into D1, and fails when that state describes lost or at-risk
revenue.

DESIGN NOTES
------------
* **No new secret.** The signal was already in D1 — nobody was looking at it.
  This uses the Cloudflare API token + D1 database id that the billing crons
  already carry, so no live Stripe key has to be minted into CI. Reading D1 is
  also strictly cheaper and safer than polling the Stripe API on a schedule.
* **Fails loud when unconfigured.** Missing credentials exit non-zero with an
  explicit `NOT CONFIGURED` verdict rather than passing. That is the whole
  lesson of the bug above: a check that silently no-ops when it cannot see
  anything is worse than no check, because it manufactures false confidence.
* **Missing tables are an ERROR, not a pass.** If the schema drifts and a table
  this check depends on disappears, that is a failure of the check's own
  premise. It must not be reported as health.

Exit codes: 0 healthy · 1 anomalies found · 2 not configured / cannot see state.
"""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request

CF_API = "https://api.cloudflare.com/client/v4"

# Stripe subscription statuses that mean revenue is lost or at risk RIGHT NOW.
# `canceled` is deliberately absent: a cancellation is a completed, intentional
# terminal state, not an anomaly to page on.
UNHEALTHY_SUBSCRIPTION_STATUSES = ("past_due", "unpaid", "incomplete_expired")

# A single `invoice.payment_failed` is ordinary (a card declines, Stripe retries
# and usually succeeds). A cluster is dunning that is not recovering.
PAYMENT_FAILED_ALERT_THRESHOLD = 3

# Look-back for the webhook-event scan. Wide enough to catch a full dunning
# cycle, narrow enough that a long-resolved incident stops paging.
LOOKBACK_DAYS = 30

# `stripe_webhook_events_processed.event_id` is written under TWO different id
# schemes, because two separately-registered Stripe endpoints both deliver into
# this one table: the signup-worker stores Stripe's own `evt_…` id, while the
# container stores a derived content hash. This SQL fragment separates them.
#
# It matters for counting. Each endpoint receives EVERY event of the types it
# subscribes to, so each scheme is already a COMPLETE view of those events.
# Summing the two therefore double-counts every event both endpoints see, and a
# cluster rule that sums will trip at half its stated threshold. Take the MAX of
# the per-scheme counts, never the sum.
CANONICAL_EVENT_ID_SQL = r"event_id LIKE 'evt\_%' ESCAPE '\'"


class NotConfigured(Exception):
    """Credentials or database id absent — the check cannot see billing state."""


def d1_query(account_id: str, database_id: str, token: str, sql: str) -> list[dict]:
    """Run one read-only SQL statement against D1 and return its result rows."""
    req = urllib.request.Request(
        f"{CF_API}/accounts/{account_id}/d1/database/{database_id}/query",
        data=json.dumps({"sql": sql}).encode(),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        payload = json.load(resp)
    if not payload.get("success"):
        raise RuntimeError(f"D1 query failed: {payload.get('errors')}")
    return payload["result"][0]["results"]


def table_exists(account_id: str, database_id: str, token: str, table: str) -> bool:
    rows = d1_query(
        account_id,
        database_id,
        token,
        f"SELECT name FROM sqlite_master WHERE type='table' AND name='{table}'",
    )
    return bool(rows)


def check_unhealthy_subscriptions(
    account_id: str, database_id: str, token: str
) -> list[str]:
    """Subscriptions materialized in a revenue-losing state."""
    quoted = ", ".join(f"'{s}'" for s in UNHEALTHY_SUBSCRIPTION_STATUSES)
    rows = d1_query(
        account_id,
        database_id,
        token,
        "SELECT stripe_subscription_id, tenant_id, status, materialized_at_ms "
        f"FROM stripe_subscriptions WHERE status IN ({quoted}) "
        "ORDER BY materialized_at_ms DESC LIMIT 50",
    )
    return [
        f"subscription {r['stripe_subscription_id']} is '{r['status']}' "
        f"(tenant {r['tenant_id']}, materialized_at_ms={r['materialized_at_ms']})"
        for r in rows
    ]


def check_payment_failure_clusters(
    account_id: str, database_id: str, token: str
) -> list[str]:
    """Repeated payment failures inside the look-back window = dunning not recovering.

    Counted over the processed-webhook log rather than over invoices, because the
    webhook log is what production actually receives and is written on every
    delivery — including the retries that indicate a failure is not clearing.

    Counted per id-scheme and reduced with MAX, not SUM. Two Stripe endpoints
    write this table under different id schemes, so each scheme already holds a
    complete copy of the same events; summing them reports double the real
    number and trips the threshold at half its stated value. See
    `CANONICAL_EVENT_ID_SQL` and `check_duplicate_webhook_ingestion`.
    """
    cutoff_ms = f"(strftime('%s','now') - {LOOKBACK_DAYS} * 86400) * 1000"
    rows = d1_query(
        account_id,
        database_id,
        token,
        f"SELECT SUM(CASE WHEN {CANONICAL_EVENT_ID_SQL} THEN 1 ELSE 0 END) AS canonical, "
        f"SUM(CASE WHEN NOT ({CANONICAL_EVENT_ID_SQL}) THEN 1 ELSE 0 END) AS derived "
        "FROM stripe_webhook_events_processed "
        f"WHERE event_type = 'invoice.payment_failed' AND processed_at_ms >= {cutoff_ms}",
    )
    row = rows[0] if rows else {}
    canonical = row.get("canonical") or 0
    derived = row.get("derived") or 0
    n = max(canonical, derived)
    if n >= PAYMENT_FAILED_ALERT_THRESHOLD:
        return [
            f"{n} invoice.payment_failed webhooks in the last {LOOKBACK_DAYS}d "
            f"(threshold {PAYMENT_FAILED_ALERT_THRESHOLD}) — dunning is not recovering"
        ]
    return []


def check_duplicate_webhook_ingestion(
    account_id: str, database_id: str, token: str
) -> list[str]:
    """Two live Stripe endpoints ingesting the same event under different keys.

    `stripe_webhook_events_processed` dedupes on `event_id` PRIMARY KEY. That
    only protects a retry that arrives under the SAME id scheme. When one
    endpoint stores Stripe's `evt_…` id and another stores a derived hash of the
    same delivery, the two rows do not collide, so the same Stripe event is
    processed twice and every count over this table is inflated.

    Seeing both schemes for one `event_type` is therefore evidence of a second
    live endpoint, not of a busy month. Resolving it is a Stripe dashboard
    change (retire the redundant endpoint), which is why this reports rather
    than repairs.
    """
    cutoff_ms = f"(strftime('%s','now') - {LOOKBACK_DAYS} * 86400) * 1000"
    rows = d1_query(
        account_id,
        database_id,
        token,
        "SELECT event_type, "
        f"SUM(CASE WHEN {CANONICAL_EVENT_ID_SQL} THEN 1 ELSE 0 END) AS canonical, "
        f"SUM(CASE WHEN NOT ({CANONICAL_EVENT_ID_SQL}) THEN 1 ELSE 0 END) AS derived "
        "FROM stripe_webhook_events_processed "
        f"WHERE processed_at_ms >= {cutoff_ms} "
        "GROUP BY event_type HAVING canonical > 0 AND derived > 0 "
        "ORDER BY event_type",
    )
    if not rows:
        return []
    detail = ", ".join(
        f"{r['event_type']} ({r['canonical']} canonical / {r['derived']} derived)"
        for r in rows
    )
    return [
        f"{len(rows)} event type(s) ingested under BOTH id schemes in the last "
        f"{LOOKBACK_DAYS}d — a second live Stripe endpoint is writing this table "
        f"and `event_id` dedupe does not span the two: {detail}"
    ]


def check_dead_letter_queue(account_id: str, database_id: str, token: str) -> list[str]:
    """Stripe webhooks that failed processing outright and were parked in the DLQ."""
    rows = d1_query(
        account_id, database_id, token, "SELECT COUNT(*) AS n FROM stripe_webhook_events_dlq"
    )
    n = rows[0]["n"] if rows else 0
    if n:
        return [f"{n} Stripe webhook event(s) in the dead-letter queue — billing state may be stale"]
    return []


CHECKS = (
    ("stripe_subscriptions", check_unhealthy_subscriptions),
    ("stripe_webhook_events_processed", check_payment_failure_clusters),
    ("stripe_webhook_events_processed", check_duplicate_webhook_ingestion),
    ("stripe_webhook_events_dlq", check_dead_letter_queue),
)


def main() -> int:
    account_id = os.environ.get("CF_ACCOUNT_ID", "").strip()
    database_id = os.environ.get("D1_DATABASE_ID", "").strip()
    token = os.environ.get("CF_API_TOKEN", "").strip()

    if not (account_id and database_id and token):
        missing = [
            name
            for name, value in (
                ("CF_ACCOUNT_ID", account_id),
                ("D1_DATABASE_ID", database_id),
                ("CF_API_TOKEN", token),
            )
            if not value
        ]
        print("NOT CONFIGURED — billing health is UNKNOWN, not healthy.")
        print(f"  missing: {', '.join(missing)}")
        print("  Refusing to report success while unable to see billing state.")
        return 2

    findings: list[str] = []
    for table, check in CHECKS:
        if not table_exists(account_id, database_id, token, table):
            print(f"NOT CONFIGURED — required table '{table}' is absent from D1.")
            print("  Schema drift: this check cannot verify its own premise.")
            return 2
        findings.extend(check(account_id, database_id, token))

    if findings:
        print(f"BILLING HEALTH: {len(findings)} anomaly(ies) found\n")
        for f in findings:
            print(f"  - {f}")
        print(
            "\nTriage in the Stripe dashboard. For lost or at-risk revenue: "
            "recover the payment, or cancel the subscription and void its open "
            "invoice if it is not a real customer. For duplicate ingestion: "
            "retire the redundant webhook endpoint so one handler owns the event."
        )
        return 1

    print("BILLING HEALTH: OK")
    print(f"  no subscriptions in {'/'.join(UNHEALTHY_SUBSCRIPTION_STATUSES)}")
    print(
        f"  fewer than {PAYMENT_FAILED_ALERT_THRESHOLD} invoice.payment_failed "
        f"webhooks in {LOOKBACK_DAYS}d"
    )
    print("  no event type ingested under two webhook id schemes")
    print("  Stripe webhook dead-letter queue empty")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (urllib.error.URLError, RuntimeError) as exc:
        # An unreachable or erroring D1 is also "cannot see billing state".
        print(f"NOT CONFIGURED — could not read billing state: {exc}")
        sys.exit(2)
