---
id: "AUDIT-R-PREP-STRIPE-CUSTOMER-PORTAL-2026-05-15"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-PREP"
parent_wi: "WT-R-PREP-STRIPE-PORTAL"
owner: "Gustavo Schneiter"
tags: ["audit", "stripe", "billing", "customer-portal", "r-prep", "ga-prep"]
---

# Stripe Customer Portal — configuration + redirect flow spec

## 1. Why this exists

Customers who already paid us via the
`corelink-tier-selection` Checkout flow (subscribed) need a
self-service surface to:

- Download past invoices (legal + finance ops).
- Update their payment method when a card expires (revenue continuity).
- Upgrade or downgrade their tier (commercial expansion).
- Cancel (LGPD/GDPR + general consumer-rights compliance — no dark
  patterns; cancellation MUST be reachable in one click from the
  customer dashboard, see `specs/_dashboards/DASH-BILLING.md`).

Rebuilding any of this in-house is a multi-month project AND a PCI-DSS
scope-expansion landmine. Stripe ships a hosted "Customer Portal" — a
configurable web UI hosted at `billing.stripe.com` — that solves all
four. This spec covers the **portal configuration** (what features we
expose) AND the **redirect flow** (how a Clerk-authenticated user gets
from `/en/customer/billing` into the portal and back).

Companion crates:

- `corelink-stripe-real` (this WI adds the
  `BillingPortalSessionCreator` trait + in-memory fake).
- `corelink-tier-selection` (drives Checkout — feeds the customer that
  the portal later mutates).
- `corelink-billing-stripe` (webhook receiver — portal mutations
  arrive as `customer.subscription.updated` /
  `invoice.payment_failed` events on the existing pipeline).

## 2. Stripe Customer Portal configuration

The portal config is a **Stripe Dashboard artifact** (per-Stripe-account,
versioned by Stripe, not by us). This spec is the contract; the
operator follows
`specs/_runbooks/RB-STRIPE-PORTAL-INCIDENT.md` §6 to keep dashboard
state in sync with this doc.

### 2.1 Features enabled

| Feature                          | Enabled? | Notes                                                                                              |
|----------------------------------|----------|----------------------------------------------------------------------------------------------------|
| Invoice history (download PDFs)  | YES      | Past 24 months, default sort newest-first.                                                         |
| Update payment method            | YES      | Add + remove + set-default card; SetupIntent flow handled entirely by Stripe; PCI scope = SAQ-A.   |
| Update billing address           | YES      | Required for VAT/sales-tax accuracy.                                                               |
| Subscription update              | YES      | Tier matrix: `Free → Starter → Team → Enterprise` AND down. Annual ↔ monthly switch enabled.       |
| Cancellation                     | YES      | "Cancel at period end" (default) + immediate-cancel (with proration). Both emit webhook events.    |
| Promotion codes                  | NO       | Phase 2 — coupon distribution today is via Enterprise sales contracts, not self-service.           |
| Tax IDs                          | YES      | EU VAT + BR CNPJ + US EIN.                                                                         |

### 2.2 Tier-matrix constraints (subscription update)

Stripe's portal lets you whitelist which prices the customer can
switch to. We restrict to the four canonical tiers — never offer a
hidden "legacy_starter_2024" price even if it still exists in the
Products catalog. The allowed prices are sourced from
`STRIPE_PRICE_ID_FREE`, `STRIPE_PRICE_ID_STARTER`,
`STRIPE_PRICE_ID_TEAM`, `STRIPE_PRICE_ID_ENTERPRISE` env vars (same
vars as `corelink-stripe-real::client::create_checkout_session`).

**Enterprise downgrade rule.** Self-service downgrade out of Enterprise
is DISABLED in the portal — Enterprise contracts require a paper
amendment + DPA update. The portal config redirects "Cancel" /
"Downgrade" clicks from Enterprise customers to a
`mailto:sales@corelink.dev` link.

### 2.3 Branding

- **Logo.** CoreLink wordmark PNG (1200×400, transparent background)
  uploaded to Stripe Dashboard → Branding.
- **Icon.** CoreLink favicon SVG.
- **Primary brand color.** `#0F62FE` (same as admin UI primary,
  `apps/admin-ui/src/styles/tokens.css`).
- **Accent.** `#161616` (CoreLink charcoal).
- **Font.** "Inter" — falls back to system stack since Stripe doesn't
  load remote fonts inside portal frames.

### 2.4 URL configuration

- **Return URL allowlist:** `https://app.corelink.dev/*/customer/billing`
  + `https://staging.corelink.dev/*/customer/billing` + `https://localhost:3000/*/customer/billing`.
  Set per environment via `stripe billing_portal configurations
  update`. Any other return URL is rejected by Stripe before the
  redirect is even minted, which closes off the open-redirect attack
  vector.
- **Privacy policy link:** `https://corelink.dev/privacy`.
- **Terms of service link:** `https://corelink.dev/legal/terms`.

## 3. Redirect flow

```
admin-ui                          corelink server                        Stripe
 |                                       |                                  |
 |--POST /v1/customer/billing/           |                                  |
 |    portal-session  (return_url,       |                                  |
 |    tenant_id)                         |                                  |
 |                                       |                                  |
 |                              Clerk session check ----.                   |
 |                              + tenant_id match       |                   |
 |                              (defense in depth)      |                   |
 |                                       |              |                   |
 |                              audit.emit(             |                   |
 |                              "corelink.billing.      |                   |
 |                              portal_session_created" |                   |
 |                              )                       |                   |
 |                                       |              |                   |
 |                              [FAIL HERE → return 5xx]|                   |
 |                                       |--POST /v1/billing_portal/        |
 |                                       |   sessions (customer_id,         |
 |                                       |   return_url)                    |
 |                                       |                                  |
 |<-- 200 { portal_url, session_id, exp }                                   |
 |                                       |                                  |
 |--window.location.assign(portal_url)-->|                                  |
 |                                       |                                  |
 |     [user manages subscription, downloads invoice, etc.]                 |
 |                                       |                                  |
 |                                       |<--- webhook events (e.g.         |
 |                                       |     customer.subscription.       |
 |                                       |     updated) — handled by        |
 |                                       |     existing webhook pipeline    |
 |                                       |     (corelink-billing-stripe)    |
 |                                       |                                  |
 |<-- redirect to return_url ------------------------|                      |
```

### 3.1 Endpoint contract — `POST /v1/customer/billing/portal-session`

**Auth.** Clerk session cookie required. Session MUST carry the
`tenant_id` claim AND `tenant_id` MUST match the request body
(defense-in-depth — a confused-deputy attack from one tenant cannot
mint a portal for another tenant's `customer_id` because the customer
lookup is keyed by the JWT claim, not the body).

**Request:**
```json
{
  "return_url": "https://app.corelink.dev/en/customer/billing",
  "tenant_id": "tenant_acme"
}
```

**Response (200):**
```json
{
  "portal_url": "https://billing.stripe.com/p/session/bps_...",
  "session_id": "bps_...",
  "expires_at_unix": 1747339200
}
```

**Errors:**
- `400` — `return_url` not HTTPS / not in allowlist / `tenant_id`
  missing.
- `401` — no Clerk session / expired session.
- `403` — Clerk session does not match `tenant_id` in body.
- `404` — no Stripe customer mapped for this tenant (tenant must
  first complete Checkout via the onboarding flow).
- `5xx` — Stripe API down / audit emit failed.

### 3.2 Idempotency

The handler computes the `Idempotency-Key` for the Stripe call as
`portal:{tenant_id}:{minute_bucket}` — a minute-granularity bucket
means a user who double-clicks the button within the same minute gets
the same portal URL back (no wasted Stripe API calls + identical
behavior across both clicks). After the minute rolls, a new URL is
minted — this matches the Stripe single-use semantic for portal URLs.

### 3.3 Audit (INV-BILLING-PORTAL-AUTH-PROOF)

Every successful portal-session create emits
`corelink.billing.portal_session_created` to the canonical audit
ledger BEFORE the URL is returned to the client. The event MUST carry:

- `tenant_id` (from Clerk JWT claim)
- `customer_id` (Stripe `cus_...` resolved server-side)
- `actor_user_id` (Clerk sub claim)
- `return_url` (echo of request)
- `session_id` (Stripe `bps_...`)
- `issued_at_unix` (server clock)

If audit emit fails, the URL is dropped and the handler returns 5xx.
This is the standard fail-CLOSED pattern (see
`crates/corelink-audit/src/lib.rs`). Webhook events that arrive later
from portal mutations land on the existing
`corelink-billing-stripe` pipeline — they do NOT require any new
audit plumbing.

## 4. Invariants

### 4.1 INV-BILLING-PORTAL-AUTH-PROOF

For every row in the
`corelink.billing.portal_session_created` audit table:

- a corresponding Clerk-authenticated session MUST have existed at
  `issued_at_unix - 30s … issued_at_unix + 30s` (clock skew window).
- the `tenant_id` MUST match the Clerk JWT's `tenant_id` claim of
  that session.
- the `customer_id` MUST be the Stripe customer linked to that
  tenant (1:1 by `stripe_customer_id` column in `tenants` table —
  enforced as a UNIQUE constraint, migration `0017_stripe_customer_id_unique.sql`).

Falsifiable by joining the audit table against Clerk's session log
(both stored within the same D1 database, see
`specs/04_sprints/S10/work_items/WI-S10-003-corelink-billing-stripe-adapter-idempotency-webhook.md`).

### 4.2 INV-BILLING-PORTAL-URL-SINGLE-USE

Every successful portal-session create returns a distinct
`portal_url`. Two concurrent calls with the same input MUST return
two different URLs (Stripe enforces this server-side; our fake
mirrors it; property test asserts it). Verified by `prop_portal.rs`
`portal_urls_unique_https_audit_consistent`.

### 4.3 INV-BILLING-PORTAL-RETURN-URL-ALLOWLIST

The `return_url` accepted by the handler MUST match the
environment-configured allowlist regex
(`^https://(app|staging)\.corelink\.dev/[^/]+/customer/billing$` in
prod, plus `https?://localhost:\d+/[^/]+/customer/billing` in dev).
Any non-matching URL is rejected with 400 BEFORE the Stripe API is
called — open-redirect mitigation.

### 4.4 INV-BILLING-PORTAL-AUDIT-FAIL-CLOSED

If the audit emit fails, the handler MUST NOT return the URL to the
client and MUST NOT count the call as successful. The Stripe-side
session, if already minted, becomes orphaned-but-harmless (single-use,
5-min expiry, tied to a customer that requires Clerk auth on return).
Verified by `prop_portal.rs` `audit_failure_drops_url_zero_rows`.

## 5. Threat model deltas vs. baseline `corelink-stripe-real`

| Threat                                            | Mitigation                                                                                   |
|---------------------------------------------------|----------------------------------------------------------------------------------------------|
| Open redirect via attacker-supplied `return_url`  | Stripe-side allowlist (Configuration) + server-side allowlist regex (§4.3).                  |
| One tenant minting a portal for another tenant    | Server resolves `customer_id` from JWT claim, NEVER from request body (§3.1).                |
| Portal URL leaked via logs / SSRF                 | `PortalSessionUrl` newtype's `Debug` impl redacts the URL; only the typed wrapper is logged. |
| Replay of portal URL after user signed out        | Stripe enforces 5-min TTL; portal itself requires email-based reauth for high-risk actions.  |
| Audit-blackout attack (mint without trace)        | Fail-CLOSED audit (§4.4).                                                                    |

## 6. Cross-references

- Trait + fake: `crates/corelink-stripe-real/src/portal.rs`.
- Property tests: `crates/corelink-stripe-real/tests/prop_portal.rs`.
- UI: `apps/admin-ui/src/app/[locale]/customer/billing/page.tsx` +
  `PortalLauncher.tsx`.
- E2E: `apps/admin-ui/tests/e2e/customer/billing-portal.spec.ts`.
- Customer guide: `apps/docs/docs/how-to/billing/manage-subscription.mdx`.
- Operator runbook: `specs/_runbooks/RB-STRIPE-PORTAL-INCIDENT.md`.
- Customer dashboard (sibling surface): `specs/_dashboards/DASH-BILLING.md`.
- Sales FAQ (billing surface answers): `marketing/sales/FAQ-MASTER.md`.
- Webhook DLQ runbook (portal mutations land here):
  `specs/_runbooks/RB-WEBHOOK-DLQ-REPLAY.md`.

## 7. Open follow-ups

- Phase 2: enable promotion codes once Enterprise sales standardizes
  on a single self-service discount SKU.
- Phase 2: localized portal (PT-BR + EN-US) once Stripe ships
  i18n-customization for hosted portal pages.
- Phase 3: hardware-token MFA gate on the cancellation surface for
  Enterprise tenants (currently relies on Clerk's session MFA).
