# Stripe webhook event reconciliation (signup-worker)

**Runbook for the operator.** Keeps the LIVE Stripe webhook endpoint's
`enabled_events` in lockstep with the code that dispatches them, so a
cancel / plan-downgrade / dunning failure can never be silently dropped.

## Why this exists (the bug it kills)

The **signup-worker** (`https://corelink-signup.humangr.com/webhooks/stripe`)
is the **authoritative billing + downgrade handler** — it is the one that flips
`tier_selections.subscription_state` to `inactive` on cancel and away from
`active` on terminal dunning failure. If the Stripe **dashboard** endpoint is
subscribed to *fewer* events than the code handles, those events are **never
delivered**, so:

- `customer.subscription.deleted` missing → a **canceled** customer keeps paid access.
- `invoice.payment_failed` missing → a **non-paying** (dunning-terminal) customer keeps access.
- `customer.subscription.updated` missing → a **plan-downgrade** never takes effect.

All three are **revenue leaks**. Hand-clicking the event list in the dashboard is
exactly what drifts. This makes it deterministic.

## Single source of truth

`apps/signup-worker/src/webhooks/handled-stripe-events.json`

The **same** file drives:
1. the runtime allowlist (`HANDLED_EVENT_TYPES` in `stripe.ts`), and
2. the live endpoint's `enabled_events` (via the reconcile script below).

A vitest guardrail (`tests/handled-stripe-events.test.ts`) fails CI if any
`downgrade_critical` event is dropped. **Edit the JSON — never a literal.**

The canonical set (7 events):

| Event | Role |
|---|---|
| `checkout.session.completed` | grant (upgrade) |
| `checkout.session.async_payment_succeeded` | grant (delayed methods: SEPA/ACH) |
| `checkout.session.async_payment_failed` | ack (nothing was granted) |
| `customer.subscription.created` | backfill period-end |
| `customer.subscription.updated` | **downgrade** (plan-change / status sync) |
| `customer.subscription.deleted` | **downgrade** (cancel) |
| `invoice.payment_failed` | **downgrade** (terminal dunning) |

## How to reconcile (operator, OOB)

The script **never reads or hard-codes a key** — auth is the Stripe CLI's own.
It is **dry-run by default**; it only writes with `--yes`.

```bash
# 1. DRY-RUN — see the diff (safe; no writes). Uses whatever mode the CLI is in.
scripts/ops/stripe-reconcile-webhook-events.sh

# 2. DRY-RUN against LIVE (read-only) — confirm the live drift:
scripts/ops/stripe-reconcile-webhook-events.sh --live

# 3. APPLY on LIVE. Supply a RESTRICTED live key scoped to
#    "Webhook Endpoints: write" (do NOT use a full secret key):
STRIPE_API_KEY=rk_live_… scripts/ops/stripe-reconcile-webhook-events.sh --yes
#    (a live key in STRIPE_API_KEY already selects live mode; --live is implied)
```

The script:
- prints the resolved Stripe account identity (eyeball it before applying);
- lists every `*.humangr.com` destination — **both** v1 webhook endpoints
  (`/v1/webhook_endpoints`) **and** v2 event destinations
  (`/v2/core/event_destinations`, the `thin`-payload ones) — and **warns on any
  URL carrying more than one ENABLED destination** (a stray `stripe listen`
  listener or an orphaned v2 destination; disabled ones receive nothing, so they
  are listed with a `note:` instead of raising a standing alarm — review it in the dashboard; the script never deletes). If the
  CLI/key cannot read v2 it says so loudly and **exits 3**: a run that could not
  look must never be read as "no strays found". See the 2026-08-03 correction in
  `docs/handoff/2026-07-03-REPLY-from-clw-coordinator-webhook-reconcile-DONE-and-no-stray-endpoint-exists.md`
  for why both halves of this check were previously blind;
- sets `enabled_events` to **exactly** the canonical set, then re-reads to verify.

## The container (grant-only) endpoint — second source of truth

The **container materializer** (`corelink-api.humangr.com/v1/billing/stripe-webhook`, "Corelink prd",
endpoint `we_1Tfh8P…`) materializes the **10** events in `EVENT_MATERIALIZATION_MATRIX`
(`crates/corelink-billing-stripe-materializer/src/handler.rs`). It was once subscribed to a ~236-event
catch-all; that tightening was applied and the 2026-08-03 dashboard read confirms it now sits at
exactly 10.
Its source of truth is `crates/corelink-billing-stripe-materializer/container-webhook-events.json`, pinned
to the matrix by a Rust guardrail test (`handler.rs::container_webhook_events_json_matches_the_matrix`).

Tightening 236 → 10 is **safe**: every event outside the matrix is a no-op ack in the container, so
narrowing removes only events it never acted on (over-subscription fails safe; under-subscription would
drop a grant path — which is exactly what the guardrail test prevents from drifting). Reconcile it with the
SAME tool via the overrides:

```bash
# DRY-RUN first (--live, read-only) — confirm the diff is ONLY REMOVEs (never an ADD):
scripts/ops/stripe-reconcile-webhook-events.sh --live \
  --url  https://corelink-api.humangr.com/v1/billing/stripe-webhook \
  --json crates/corelink-billing-stripe-materializer/container-webhook-events.json

# APPLY (restricted rk_live_ key):
STRIPE_API_KEY=rk_live_…  scripts/ops/stripe-reconcile-webhook-events.sh --yes --live \
  --url  https://corelink-api.humangr.com/v1/billing/stripe-webhook \
  --json crates/corelink-billing-stripe-materializer/container-webhook-events.json
```

If the dry-run shows ANY `+ ADD`, STOP — the matrix handles an event the live endpoint isn't subscribed to
(the reverse of over-subscription); investigate before applying.

## Boundaries

- The **admin lane (Claude) does not run this against LIVE** — it writes/vets the
  script; the **operator** (or the go-live coordinator) runs the `--yes --live`
  one-shot with the restricted live key.
- This never touches secrets. The webhook **signing secret** (`whsec_…`) is a
  separate concern — each destination has its own, and the signup-worker's is set
  as a worker secret.

  ⚠️ **A 0% error rate does NOT verify a signing secret.** This runbook used to
  say it did. Zero errors over zero deliveries is also 0%, so the reading is
  vacuous in exactly the case you care about — a destination nobody is exercising.
  The 2026-08-03 dashboard read showed **0% on all four destinations, including a
  stray whose deliveries could only ever have been rejected**. Verify a secret by
  observing a **successful delivery** (a 2xx against a non-zero delivery count),
  never by the absence of failures.

## Related

- `apps/signup-worker/src/webhooks/stripe.ts` — the handler.
- `scripts/ops/stripe-setup-tiers.sh` / `stripe-setup-runners.sh` — sibling
  idempotent Stripe provisioners (same safety pattern).
- Billing authority split: the signup-worker is the authoritative downgrade
  handler; the container materializer is grant-only.

## ✅ CLOSED — the stray `exquisite-rhythm-thin` v2 destination

**Status: CLOSED 2026-08-03 — the owner disabled it** (see the resolution at the end of this
section). The history below is kept because the *mechanism* recurs: a v2 destination is
invisible to a v1 listing, so the next stray will hide the same way.

Production has **four** Stripe destinations. Exactly one was stray:

| Destination | URL | Events | Payload | Verdict |
|---|---|---|---|---|
| *(unnamed)* `we_1Tolig…` | `corelink-signup.humangr.com/webhooks/stripe` | 7 | snapshot | ✅ **KEEP** — the authoritative signup-worker handler. Matches `apps/signup-worker/src/webhooks/handled-stripe-events.json` exactly. |
| `Corelink prd` `we_1Tfh8P…` | `corelink-api.humangr.com/v1/billing/stripe-webhook` | 10 | snapshot | ✅ **KEEP** — the container grant-only materializer. Its signing secret is the one bound to the container. |
| **`exquisite-rhythm-thin`** | `corelink-api.humangr.com/v1/billing/stripe-webhook` | 24 | **thin** | ❌ was the stray — **DISABLED by the owner 2026-08-03** |
| *(unnamed)* `we_1TJ1YZ…` | `api.humangr.com/_wallet/stripe/webhook` | 4 | snapshot | 🚫 **DO NOT TOUCH — a different project (hugr-wallet).** Not CoreLink's. |

**How:** Stripe Dashboard → **Developers → Webhooks** (event destinations) → open
**`exquisite-rhythm-thin`** → delete it. Confirm the URL reads
`corelink-api.humangr.com/v1/billing/stripe-webhook` and the payload style reads
**Mínimo / thin** before deleting — that is what distinguishes it from `Corelink prd`, which
sits on the *same URL* and must survive.

**Why it is safe to remove:** the container cannot consume it under any circumstances. A
signing secret is bound to one destination, and the container holds exactly one — the
`Corelink prd` one — so this destination's deliveries can only be rejected `401`. Separately,
a thin payload carries no `data.object`, which the container's v1-shaped dispatcher cannot
materialize. Removing it changes no CoreLink behaviour; it removes an undocumented,
unconsumable surface from the billing path.

**Why it lingered:** it is a **v2 event destination**, and every automated sweep here read only
v1 webhook endpoints, so it was invisible to tooling for a month while a check reported clean.
That gap is fixed in `scripts/ops/stripe-reconcile-webhook-events.sh` (it now enumerates v2,
warns on any shared URL, and exits 3 rather than reporting clean when it cannot look).

### ✅ RESOLVED 2026-08-03 — the owner **disabled** it

The owner retired `exquisite-rhythm-thin` by **disabling** it rather than deleting it. That
closes the hazard: a disabled destination receives nothing, so it cannot deliver, cannot be
rejected, and cannot double-process. It remains on the account as an inert row.

Deleting it as well is optional housekeeping, not a fix. If you ever **re-enable** it, the
duplicate returns and the sweep will say so.

**Disable vs delete matters to the tooling**, and the first version of this sweep got it
wrong: it counted duplicates by URL regardless of status, so a disabled stray raised the same
WARN forever — and the WARN's own wording ("the others can only ever be rejected") is false
for a destination that receives nothing. A permanent alarm is an alarm the operator learns to
ignore, which is the same failure mode this sweep exists to prevent. The duplicate check now
counts **enabled** destinations only; disabled ones are still listed and get a `note:` line.

**To verify the current state,** re-run the read-only sweep:

```bash
STRIPE_API_KEY=rk_live_… scripts/ops/stripe-reconcile-webhook-events.sh --live
```

Expected now: **four rows**, the `exquisite-rhythm-thin` one showing `[disabled]` with a
`note:` line, **no** `WARN: N ENABLED destinations share …`, exit `0`. (If you delete it
instead, the row simply disappears — three rows, same verdict.) An exit of `3` means the
sweep could not read v2 and **proves nothing** — fix the key/CLI and re-run.
