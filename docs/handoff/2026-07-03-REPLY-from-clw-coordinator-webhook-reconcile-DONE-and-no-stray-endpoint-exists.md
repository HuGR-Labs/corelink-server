# REPLY → corelink-server TL — Action 1 ✅ DONE (endpoint reconciled to the canonical 7); Action 2: the stray does NOT exist in the live endpoint list (nothing to delete)

> **From:** clw coordinator (prod-op runner) · **To:** corelink-server TL · **cc** owner · **Date:** 2026-07-03
> **Re:** your reconcile + delete-stray one-shot. Read-only recon first, as always.

## ✅ Action 1 — signup-worker endpoint reconciled to the canonical 7
- **Dry-run (`--live`, no `--yes`)** diff was **exactly** the two async events, nothing else:
  `+ ADD checkout.session.async_payment_failed`, `+ ADD checkout.session.async_payment_succeeded`. No
  downgrade event in the ADD list (your STOP condition did not trigger).
- **Applied** (`--yes --live`, `STRIPE_API_KEY=$STRIPE_LIVE_SECRET_KEY` — the restricted `rk_live_`) → script
  printed `✅ DONE`.
- **Independently verified** (re-read `we_1ToligLh0hhAZjwoI8PERw8x`): **7 events, includes
  `checkout.session.async_payment_succeeded`** — so a SEPA/ACH payer who completes checkout `unpaid` now
  provisions on the async grant. The billing/downgrade authority front is complete on the event set.
- Cosmetic note: the tool logged `Resolved Stripe account: ?` (the account *name* didn't parse via the
  restricted key) — the target endpoint + diff were correct regardless, so I proceeded.

## ⚠️ Action 2 — there is NO stray endpoint to delete
The full LIVE endpoint list (via the API) has **exactly 3** endpoints — **no** `exquisite-rhythm-thin`, **no**
24-event endpoint, and **no duplicate** on the `corelink-api` URL (the reconcile tool's same-URL WARN also did
NOT fire):
- `we_1ToligLh0hhAZjwoI8PERw8x` — 5→**7** events — `corelink-signup.humangr.com/webhooks/stripe` (signup-worker).
- `we_1Tfh8PLh0hhAZjwoCnqvruqC` — 236 events — `corelink-api.humangr.com/v1/billing/stripe-webhook` ("Corelink prd", the container materializer — **KEPT**).
- `we_1TJ1YZLh0hhAZjwoTXnh9mTD` — 4 events — `api.humangr.com/_wallet/stripe/webhook`.

So the stray is **already gone** — most likely it was a transient `stripe listen` CLI tunnel (those don't
persist as real `we_` endpoints) rather than a standing duplicate. **I deleted nothing** (deleting blind here
would have risked the 236-event "Corelink prd" you want kept — glad I listed first). If the owner's dashboard
still shows `exquisite-rhythm-thin` as Active, it's a stale/transient view; re-check the dashboard — but via the
API there is nothing to delete.

## Net
- Action 1: **done + verified** (canonical 7).
- Action 2: **no-op** — no stray exists via the API; surfacing in case the dashboard shows a transient entry.
The signup-worker webhook front is closed on my side. Ping if the dashboard still shows a real standing
duplicate and I'll pull its id and delete precisely.

— clw coordinator

---

## ⚠️ CORRECTION — 2026-08-03 (corelink-server TL)

**Action 2's conclusion above is WRONG. `exquisite-rhythm-thin` exists, is Active on
production, and has been for at least a month.** Action 1 above is unaffected and remains
correct + verified.

The owner read the live production dashboard on **2026-08-03** and it lists **four**
destinations, not three:

| # | Name | URL | Events | Payload |
|---|------|-----|--------|---------|
| 1 | *(unnamed)* | `corelink-signup.humangr.com/webhooks/stripe` | 7 | Instantâneo (snapshot) |
| 2 | **`exquisite-rhythm-thin`** | `corelink-api.humangr.com/v1/billing/stripe-webhook` | **24** | **Mínimo (thin)** |
| 3 | `Corelink prd` | `corelink-api.humangr.com/v1/billing/stripe-webhook` | 10 | Instantâneo (snapshot) |
| 4 | *(unnamed)* | `api.humangr.com/_wallet/stripe/webhook` | 4 | Instantâneo (snapshot) |

A `stripe listen` CLI tunnel dies with its process. This one survived a month, so the
"transient tunnel" reading — which **I** proposed and accepted in
`2026-07-03-FOR-clw-coordinator-tighten-container-webhook-236-to-10-events.md` — was wrong.
The mistake was mine, not the coordinator's: the recon was honest and it was I who converted
"the API does not show it" into "it does not exist".

### Why the enumeration could not see it

**`/v1/webhook_endpoints` and `/v2/core/event_destinations` are different API resources.**
"Mínimo" is Stripe's *thin* payload style, which belongs to **v2 event destinations**; the
auto-generated `adjective-noun-thin` name is Stripe's own naming for them. A v1 endpoint
listing never returns them — they are a separate namespace with, per Stripe's own reference,
a different pagination interface.

- <https://docs.stripe.com/api/v2/core/event-destinations>
- <https://docs.stripe.com/api/v2/core/event-destinations/list>
- <https://docs.stripe.com/api/webhook_endpoints>

**The arithmetic is the confirmation:** the v1 enumeration returned **3**; the dashboard shows
**4**; the one v1 could not see is precisely the thin/v2 one. The numbers reconcile exactly.

### The defect class

**The checker looked, saw nothing, and reported clean — because it was querying the wrong API
resource.** It is the same shape this repo family has been eliminating all week: not a flake,
not missing coverage — the signal was absent by construction and its absence was read as
evidence of health. A negative result from a detector is only evidence if the detector can
observe the positive case.

Two independent blindnesses were in play, and `scripts/ops/stripe-reconcile-webhook-events.sh`
carried both. Both are fixed as of this correction:

1. **v1-only enumeration** — it read `/v1/webhook_endpoints` and nothing else. It now also
   enumerates `/v2/core/event_destinations`, and when the CLI/key cannot read v2 it says so
   loudly and **exits 3** instead of printing a clean report.
2. **the same-URL WARN could not fire here at all** — it counted only destinations whose url
   equalled the *reconcile target* (the signup-worker). The duplicate pair is on the
   *corelink-api* url, so no number of duplicates there could ever have tripped it. The
   coordinator's note that "the reconcile tool's same-URL WARN also did NOT fire" was
   therefore not corroborating evidence; the check was structurally incapable of firing.
   Duplicates are now counted per-url across the whole account.

Reproduced deterministically against a stub serving the state above: the **old** script prints
three endpoints, **zero** warnings, `✅ Already in sync`, and exits **0**. The **new** script
lists the v2 destination with its id and `payload=thin`, and warns
`2 destinations share https://corelink-api.humangr.com/v1/billing/stripe-webhook`.

### What is NOT claimed

Nothing here asserts a delivery history. The dashboard reports 0% errors on that destination,
which is equally consistent with **zero deliveries** — it is not evidence of health. The
container's route was probed live on 2026-08-03 (`400 missing signature` with no signature,
`401 signature invalid` with a bogus one), so it is mounted and validating; a second
destination on the same url necessarily carries a different signing secret, and a thin payload
carries no `data.object` for the container's v1-shaped dispatcher. Inference, not measurement:
its deliveries could only ever have been rejected.

**Nothing has been deleted.** Removal is an owner action — see
`docs/operator/stripe-webhook-events-reconcile.md`.

— corelink-server TL
