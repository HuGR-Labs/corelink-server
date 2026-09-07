/** Stripe idempotency and cache billing persistence writers. */
import type { D1DatabaseLike } from "./billing_checkout";
import { HANDLED_EVENT_TYPES } from "./stripe.js";
import { recoverCheckoutLedger, upsertBillingPaid as upsertOwnedBillingPaid } from "./billing_checkout";
import type { PaidTier } from "./stripe_contract.js";

export type ClaimResult =
    | "claimed" // newly inserted — this delivery is the FIRST: process + emit.
    | "duplicate" // PK conflict — already processed: process (idempotent writes), SKIP emit.
    | "claim_error"; // the claim INSERT itself failed (D1 error) — fail-safe → treat as first.

/**
 * Durably claim a Stripe `event.id` AFTER the idempotent entitlement/billing
 * writes have SUCCEEDED (migration 0044 `stripe_webhook_events_processed`). The
 * claim outcome gates ONLY the non-idempotent analytics emit (#37,
 * process-then-claim): the writes already committed before we get here.
 *
 * Uses `INSERT OR IGNORE` on the `event_id` PRIMARY KEY and inspects
 * `meta.changes`:
 *   - changes === 1 → row newly inserted → "claimed" (first SUCCESSFUL delivery: emit).
 *   - changes === 0 → PK conflict → "duplicate" (retry after a prior success: SKIP emit).
 *
 * Ordering / fail-safe (PROCESS-then-CLAIM): the caller awaits the idempotent D1
 * upserts FIRST (returning 500 on any failure so Stripe redelivers), and only
 * then claims — so the claim row exists iff a delivery SUCCEEDED. This makes the
 * emit exactly-once on the successful delivery: a first attempt that FAILED
 * never claimed (it 500'd before reaching here), so Stripe's retry completes the
 * writes and emits; a retry after a prior SUCCESS hits the PK conflict and skips
 * the emit (no double MRR) while the idempotent writes re-run harmlessly.
 *
 * If the claim INSERT itself throws (D1 unavailable), we return "claim_error"
 * and the caller treats it as a FIRST delivery (emit): the writes already
 * succeeded, so a 200 is correct, and losing a first-time revenue emit is worse
 * than a possible double count. We never drop a genuine first emit.
 *
 * `outcome` records whether we dispatched a known event or merely acknowledged
 * an unknown one (forensics column; not read by the dedup path).
 */
export async function claimWebhookEvent(
    db: D1DatabaseLike,
    opts: {
        eventId: string;
        eventType: string;
        nowMs: number;
        correlationId: string;
    },
): Promise<ClaimResult> {
    const outcome = HANDLED_EVENT_TYPES.has(opts.eventType)
        ? "dispatched"
        : "acknowledged_unknown";
    try {
        const res = await db
            .prepare(
                `INSERT OR IGNORE INTO stripe_webhook_events_processed
                   (event_id, event_type, processed_at_ms, outcome, correlation_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)`,
            )
            .bind(opts.eventId, opts.eventType, opts.nowMs, outcome, opts.correlationId)
            .run();
        // `INSERT OR IGNORE` writes 1 row on first delivery, 0 on PK conflict.
        return (res?.meta?.changes ?? 0) > 0 ? "claimed" : "duplicate";
    } catch (e: unknown) {
        // Fail-safe: do NOT lose a first-time event. Proceed to process.
        console.warn(
            `[stripe-webhook] idempotency claim failed (processing anyway): ${(e as Error).message}`,
        );
        return "claim_error";
    }
}
// ---------------------------------------------------------------------------
// D1 writers
// ---------------------------------------------------------------------------

/**
 * Upsert a tenant_billing row on `checkout.session.completed`.
 *
 * Uses INSERT … ON CONFLICT (tenant_id) DO UPDATE so Stripe retries are safe.
 */
export async function upsertBillingPaid(
    db: D1DatabaseLike,
    opts: {
        tenantId: string;
        stripeCustomerId: string;
        stripeSubscriptionId: string | null;
        sessionId: string;
        plan: string | null;
        currentPeriodEndMs: number | null;
        nowMs: number;
        checkoutCreatedAtMs: number | null;
    },
): Promise<void> {
    return upsertOwnedBillingPaid(db, opts);
}
/**
 * Update billing status + period-end on `customer.subscription.updated`.
 * Looks up by stripe_subscription_id so we can find the tenant even if the
 * webhook arrives before tenant_id metadata is available.
 *
 * TERMINAL-STATE GUARD (audit fix 4): 'canceled' is terminal for a given
 * stripe_subscription_id. Stripe webhooks are NOT ordered — a late/out-of-order
 * `customer.subscription.updated(status=active)` arriving AFTER
 * `customer.subscription.deleted` used to resurrect tenant_billing to 'paid'
 * (the canonical tier_selections gate was already safe: activation only happens
 * via checkout.session.completed). `AND status != 'canceled'` makes the mirror
 * match the canonical gate: once canceled, only a NEW checkout (the
 * upsertBillingPaid ON CONFLICT path, which carries a new subscription id) can
 * move the row forward.
 */
export async function updateBillingSubscription(
    db: D1DatabaseLike,
    opts: {
        stripeSubscriptionId: string;
        status: string;
        currentPeriodEndMs: number | null;
        nowMs: number;
    },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET status = ?1,
                 current_period_end_ms = ?2,
                 updated_at_ms = ?3
             WHERE stripe_subscription_id = ?4
               AND status != 'canceled'`,
        )
        .bind(opts.status, opts.currentPeriodEndMs, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/**
 * Update ONLY `tenant_billing.status` (leave period-end + ids untouched), keyed
 * by subscription id. Used on invoice.payment_failed so a dunning failure marks
 * the row 'past_due' without clobbering the existing current_period_end_ms.
 * Idempotent on redelivery. Same terminal-state guard as
 * updateBillingSubscription (audit fix 4): a late invoice.payment_failed after
 * subscription.deleted must not move a 'canceled' row to 'past_due'.
 */
export async function updateBillingStatus(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; status: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET status = ?1,
                 updated_at_ms = ?2
             WHERE stripe_subscription_id = ?3
               AND status != 'canceled'`,
        )
        .bind(opts.status, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/**
 * Mark billing as canceled on `customer.subscription.deleted`.
 */
export async function cancelBilling(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET status = 'canceled',
                 updated_at_ms = ?1
             WHERE stripe_subscription_id = ?2`,
        )
        .bind(opts.nowMs, opts.stripeSubscriptionId)
        .run();
}

/** Expiry is the only safe transition that releases a pending checkout. */
export async function expirePendingCheckout(
    db: D1DatabaseLike,
    opts: { tenantId: string; tier: string; sessionId: string; stripeCustomerId: string;
        checkoutCreatedAtMs: number | null },
): Promise<void> {
    await recoverCheckoutLedger(db, {
        tenantId: opts.tenantId,
        tier: opts.tier,
        sessionId: opts.sessionId,
        // A reserved crash row has no customer yet; a session_created orphan
        // can still be rebound when Stripe includes its customer id.
        stripeCustomerId: opts.stripeCustomerId,
        checkoutCreatedAtMs: opts.checkoutCreatedAtMs,
        nowMs: Date.now(),
    });
    // Resolve the correlation before clearing the ledger's Stripe identifiers.
    // The session mirror is preferred, while the ledger remains the orphan
    // recovery path when the mirror write was interrupted.
    await db.prepare(`
      UPDATE tier_selections
      SET subscription_state = 'inactive', stripe_customer_id = NULL,
          subscription_started_at_ms = NULL
      WHERE tenant_id = ?1 AND subscription_state = 'pending_checkout'
        AND correlation_id = COALESCE(
          (SELECT correlation_id FROM stripe_checkout_sessions
            WHERE session_id = ?2 AND tenant_id = ?1),
          (SELECT correlation_id FROM stripe_checkout_ownership_ledger
            WHERE session_id = ?2 AND tenant_id = ?1)
        )
        `).bind(opts.tenantId, opts.sessionId).run();
    await db.prepare(`
      UPDATE stripe_checkout_ownership_ledger
      SET state = 'abandoned', session_id = NULL, stripe_customer_id = NULL,
          updated_at_ms = ?3
      WHERE tenant_id = ?1 AND session_id = ?2 AND state = 'session_created'
    `).bind(opts.tenantId, opts.sessionId, Date.now()).run();
    await db.prepare(`
      DELETE FROM stripe_checkout_sessions
      WHERE tenant_id = ?1 AND session_id = ?2
    `).bind(opts.tenantId, opts.sessionId).run();
}

/**
 * Advance `tier_selections.subscription_state` → 'active' on a completed paid
 * checkout (GAP-6 reconciliation).
 *
 * The container persists the row as `pending_checkout` when checkout starts;
 * THIS is the only writer on the live path that flips it to `active` (the
 * in-process `corelink-tier-selection::ledger` that does so is test-only and
 * never runs in production). Without this, a paid tenant stays
 * `pending_checkout` forever, the container's `has_active_subscription` guard
 * never fires, and a returning customer can open a SECOND subscription.
 *
 * Idempotent (`ON CONFLICT DO UPDATE`) so Stripe retries converge. The INSERT
 * SELECT is deliberately fail-closed: a paid completion is accepted only when
 * the exact tenant/customer/subscription billing owner was established by the
 * checkout-session proof. `subscription_state` is only ever set to 'active'
 * here with a non-null `subscription_started_at_ms`, satisfying the
 * `subscription_started_when_active` CHECK.
 *
 * The ON CONFLICT UPDATE is guarded `WHERE subscription_state <> 'active' OR
 * tier = 'free'`, so a duplicate or out-of-order `checkout.session.completed` can
 * never downgrade an already-active PAID tier or shift its activation timestamp
 * (review hardening — Stripe can redeliver and reorder webhook events).
 *
 * The `OR tier = 'free'` half permits the proven paid owner to replace the
 * signup seed. Free is an activation, not a paid subscription, so a paid
 * activation is always allowed to overwrite it; a paid active row remains
 * protected exactly as before. A missing or mismatched checkout owner does not
 * reach this mutation and is surfaced as a failed webhook for redelivery.
 *
 * When the guard DOES fire
 * (the row was previously deactivated — state <> 'active' with
 * subscription_started_at_ms = NULL), the UPDATE rewrites
 * subscription_started_at_ms to a fresh non-null value so the re-activated row
 * still satisfies the `subscription_started_when_active` CHECK (a returning
 * customer would otherwise be locked out — the NULL timestamp left by
 * deactivateTierSelection* would violate the CHECK and throw).
 */
export async function activatePaidTierSelection(
    db: D1DatabaseLike,
    opts: {
        tenantId: string;
        tier: string;
        stripeCustomerId: string;
        stripeSubscriptionId: string;
        sessionId: string;
        nowMs: number;
        correlationId: string;
    },
): Promise<void> {
    const result = await db
        .prepare(
            `INSERT INTO tier_selections
               (tenant_id, tier, subscription_state, stripe_customer_id,
                subscription_started_at_ms, schema_version, correlation_id)
             SELECT ?1, ?2, 'active', ?3, ?4, 1, ?5
             WHERE EXISTS (SELECT 1 FROM tenant_billing
               WHERE tenant_id = ?1 AND stripe_customer_id = ?3
                 AND stripe_subscription_id = ?6 AND status = 'paid')
             ON CONFLICT (tenant_id) DO UPDATE SET
               tier                       = excluded.tier,
               subscription_state         = 'active',
               stripe_customer_id         = excluded.stripe_customer_id,
               -- Re-activation MUST write a fresh non-null timestamp: a prior
               -- cancel/payment-failure ran deactivateTierSelection* which set
               -- subscription_started_at_ms = NULL, so without this the
               -- (state='active' AND started_at IS NULL) row violates the 0039
               -- subscription_started_when_active CHECK and the write throws —
               -- locking the paying re-subscriber out. Bind ?4 (= nowMs), the
               -- same value the INSERT arm writes; NOT a COALESCE of the
               -- existing column (which is NULL after cancel).
               subscription_started_at_ms = ?4
             WHERE (tier_selections.subscription_state <> 'active'
                    OR tier_selections.tier = 'free')
               AND EXISTS (SELECT 1 FROM tenant_billing
                 WHERE tenant_id = ?1 AND stripe_customer_id = ?3
                   AND stripe_subscription_id = ?6 AND status = 'paid')`,
        )
        .bind(
            opts.tenantId,
            opts.tier,
            opts.stripeCustomerId,
            opts.nowMs,
            opts.correlationId,
            opts.stripeSubscriptionId,
        )
        .run();
    const changes = result.meta?.changes;
    if (changes !== 0 && changes !== 1) {
        throw new Error("tier activation write did not return D1 changes metadata");
    }
    await db.prepare(`
      UPDATE stripe_checkout_ownership_ledger
      SET state = 'completed', updated_at_ms = ?2
      WHERE tenant_id = ?1 AND session_id = ?3 AND state = 'session_created'
    `).bind(opts.tenantId, opts.nowMs, opts.sessionId).run();
}

/**
 * Revoke the canonical access gate: flip `tier_selections.subscription_state`
 * away from 'active' so `has_active_subscription` (tier_select_store.rs:192,
 * `… WHERE subscription_state = 'active'`) returns false. Used on
 * invoice.payment_failed (final dunning), subscription.deleted (cancel), and
 * any subscription.updated that no longer grants access.
 *
 * Keyed by `stripe_subscription_id` — the SPECIFIC subscription in the terminal
 * event — resolved to the tenant through `tenant_billing` (which maps
 * subscription id → tenant id, 0055) via a correlated subquery. This is the sole
 * revocation key (H1 fix): keying by `stripe_customer_id` would flip EVERY
 * tier_selections row sharing the tenant's SINGLE Stripe customer, and because
 * checkout reuses one `cus_…` per tenant across the cache AND runner
 * subscriptions, a runner-subscription terminal event would silently revoke the
 * tenant's ACTIVE cache tier. `tenant_billing` is the one-row-per-tenant CACHE
 * billing map and NEVER holds a runner subscription id (runner subs live in
 * `runner_billing`, 0087), so this join is a clean no-op for a runner-sub event
 * and revokes only the cache tier when the CACHE subscription ends. Subscription
 * objects always carry their own id (the callers guard on it) even when they omit
 * `customer`, so this key is both more precise AND more robust than the customer.
 *
 * `inactive` (NOT `pending_checkout`) is chosen per the 0039 CHECK so the tenant
 * (a) immediately loses access and (b) can re-subscribe — tier_select.rs:699
 * `AlreadyActive` only blocks a tenant whose state is still 'active'.
 *
 * Idempotent: a bare UPDATE with `WHERE … <> 'inactive'` is a safe no-op on
 * redelivery, and clearing `subscription_started_at_ms` keeps the
 * `subscription_started_when_active` CHECK satisfied for the now non-active
 * state. Safe no-op if no billing row maps the subscription id (e.g. an event
 * for an unknown/foreign subscription, or a runner subscription).
 */
export async function deactivateTierSelectionBySubscription(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET subscription_state = 'inactive',
                 subscription_started_at_ms = NULL
             WHERE tenant_id IN (
                       SELECT tenant_id FROM tenant_billing
                       WHERE stripe_subscription_id = ?1
                   )
               AND subscription_state <> 'inactive'`,
        )
        .bind(opts.stripeSubscriptionId)
        .run();
}

/**
 * Re-activate the canonical access gate after a DUNNING RECOVERY: flip
 * `tier_selections.subscription_state` back to 'active' when Stripe reports a
 * subscription has returned to active/trialing WITHOUT a fresh checkout session
 * (payment-method fix + automatic retry succeeds, an operator marks an invoice
 * paid, or an incomplete state resolves to trialing). Stripe fires
 * `customer.subscription.updated` with grantsAccess=true in those cases but
 * never re-runs checkout.session.completed, so without this the paying tenant is
 * stranded with subscription_state='inactive' and the quota gate denies them.
 *
 * TERMINAL-CANCEL GUARD (critical): a CANCEL and a DUNNING deactivation BOTH
 * leave `tier_selections.subscription_state='inactive'`, so that column alone
 * cannot tell a recoverable dunning lapse from a terminal cancel. Stripe
 * webhooks are unordered — a late, out-of-order `subscription.updated(active)`
 * arriving AFTER `subscription.deleted` must NOT resurrect a canceled
 * subscription (re-subscribing must go through checkout). We therefore gate the
 * re-activation on the authoritative `tenant_billing.status != 'canceled'`
 * (mirroring updateBillingSubscription's audit-fix-4 guard): the tenant is
 * resolved through `tenant_billing` BY THIS SUBSCRIPTION ID, and a canceled
 * billing row makes the re-activation a no-op. Keyed by subscription id (not
 * customer) precisely so we can join `tenant_billing` for that guard;
 * subscription objects always carry their own id.
 *
 * This NEVER inserts — it can ONLY resurrect a row a prior checkout already
 * created — so the single-activation-writer invariant holds (checkout remains
 * the sole CREATOR). Restores `subscription_started_at_ms` to a fresh non-null
 * value (the prior deactivate set it NULL) so the re-activated row still
 * satisfies the 0039 `subscription_started_when_active` CHECK. `tier` is left to
 * the separate `updateTierSelectionTierByCustomer` propagation. Idempotent: the
 * `WHERE … <> 'active'` filter makes a redelivery a no-op (and prevents shifting
 * the activation timestamp of an already-active row).
 */
export async function reactivateTierSelectionBySubscription(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET subscription_state = 'active',
                 subscription_started_at_ms = ?2
             WHERE tenant_id IN (
                       SELECT tenant_id FROM tenant_billing
                       WHERE stripe_subscription_id = ?1
                         AND status != 'canceled'
                   )
               AND subscription_state <> 'active'`,
        )
        .bind(opts.stripeSubscriptionId, opts.nowMs)
        .run();
}

/**
 * Propagate an in-place plan change (Stripe price swap on the SAME subscription)
 * to `tier_selections.tier`. checkout.session.completed is the only OTHER writer
 * of `tier`, so without this a tenant who upgrades/downgrades inside Stripe keeps
 * their old entitlement tier forever.
 *
 * Keyed by customer; only touches the `tier` column. Idempotent (writing the
 * same tier twice is a no-op). Never called with an unknown tier (caller gates
 * on a non-null PaidTier — fail-safe).
 *
 * F35 FIX: `AND subscription_state = 'active'` is added so this is a no-op on
 * rows that are already inactive/canceled. Without the filter, a concurrent
 * `customer.subscription.updated` event that both changes the price AND cancels
 * the subscription writes a stale tier onto an inactive row: the access gate
 * (quota.ts `subscription_state='active'`)
 * is unaffected, but the row carries an incorrect tier label that misleads
 * forensic/audit queries. The filter makes the tier update a no-op when
 * `deactivateTierSelectionBySubscription` wins the D1 race (or has already run),
 * preventing stale tier data on inactive rows.
 */
export async function updateTierSelectionTierByCustomer(
    db: D1DatabaseLike,
    opts: { stripeCustomerId: string; tier: PaidTier },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET tier = ?1
             WHERE stripe_customer_id = ?2
               AND subscription_state = 'active'`,
        )
        .bind(opts.tier, opts.stripeCustomerId)
        .run();
}

/**
 * Backfill `tenant_billing.current_period_end_ms` on customer.subscription.created.
 * checkout.session.completed writes the row with a null period-end (the session
 * object does not carry it); .created fires immediately after and DOES carry
 * `current_period_end`. Keyed by subscription id. Pure UPDATE — never inserts —
 * so it can only enrich an existing paid row, never create one out of band.
 * Idempotent: writing the same period-end twice is a no-op.
 */
export async function backfillPeriodEnd(
    db: D1DatabaseLike,
    opts: { stripeSubscriptionId: string; currentPeriodEndMs: number; nowMs: number },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tenant_billing
             SET current_period_end_ms = ?1,
                 updated_at_ms = ?2
             WHERE stripe_subscription_id = ?3`,
        )
        .bind(opts.currentPeriodEndMs, opts.nowMs, opts.stripeSubscriptionId)
        .run();
}
