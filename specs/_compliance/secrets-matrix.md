---
id: "SECRETS-MATRIX-2026-06-05"
type: "compliance_secrets_matrix"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-05"
updated: "2026-06-05"
sprint: "GA-cutover"
parent_wi: "WI-R2-14"
owner: "Gustavo Schneiter"
tags: ["secrets", "ga-cutover", "compliance", "secrets-matrix"]
---

# Canonical Secrets Matrix — GA-cutover reference

> **doc_status:** ACTIVE · **scope:** the compliance anchor for the production
> secrets matrix referenced by `specs/_runbooks/RB-GA-CUTOVER.md` §1.3 (final
> secrets matrix audit). The **operational source of truth** — every production
> secret row, its consumer, source, rotation cadence, and deploy surface — lives
> in `docs/internal/secrets-checklist.md`, which is the file enforced by the
> automated drift gates (`scripts/validate_secrets_matrix.py` +
> `scripts/secrets-checklist-verify.sh`, both run as `cf-deploy-prod` gates).
> This document does not duplicate that matrix; it points at it and records the
> compliance-relevant rows that gate the money path.

## Money-path price IDs (Stripe)

The Stripe plan price IDs below are **not credentials** (they are public
`price_...` identifiers), but they are **deploy-blocking**: if a plan's price ID
is absent in Cloudflare, the corresponding upgrade button resolves no price and
checkout returns a 502. They are therefore asserted present by the
`cf-deploy-prod` "Verify required prod secrets are populated" gate and mirrored
here for the GA-cutover audit. Column format matches
`docs/internal/secrets-checklist.md`.

| # | Secret | Env var | Consumers | Source | Provider URL | How to obtain | Rotation | Owner | Revoke / rotate | Deploy surface |
|---|--------|---------|-----------|--------|--------------|---------------|----------|-------|-----------------|----------------|
| 1 | Stripe price ID — Team plan | `STRIPE_PRICE_ID_TEAM` | corelink-stripe-real (`crates/corelink-stripe-real/src/client.rs` builds `STRIPE_PRICE_ID_{TIER}` per tier) + worker DO env forwarding (`worker/src/durable_object.ts`) + cf-deploy-prod required-secrets gate | Stripe | https://dashboard.stripe.com/prices | Products → Team plan → copy price ID (`price_...`); required so the Team upgrade button resolves a price (else checkout 502s) | rotate-on-compromise | SRE Lead | N/A (price ID, not secret; rotate only on plan SKU change) | cf-wrangler |
| 2 | Stripe price ID — Pro plan | `STRIPE_PRICE_ID_PRO` | corelink-stripe-real (`crates/corelink-stripe-real/src/client.rs` builds `STRIPE_PRICE_ID_{TIER}` per tier) + worker DO env forwarding (`worker/src/durable_object.ts`) + cf-deploy-prod required-secrets gate | Stripe | https://dashboard.stripe.com/prices | Products → Pro plan → copy price ID (`price_...`); required so the Pro upgrade button resolves a price (else checkout 502s) | rotate-on-compromise | SRE Lead | N/A (price ID, not secret; rotate only on plan SKU change) | cf-wrangler |

## Audit pointer

For the full matrix (≥ 100 rows: Stripe, Clerk, PagerDuty, Slack, R2, BYOK/Vault,
Dependency-Track, etc.) and the canonical Starter price row
(`STRIPE_PRICE_ID_STARTER`, #103), see `docs/internal/secrets-checklist.md`. The
GA-cutover §1.3 step verifies the live `wrangler secret list` output against that
file; the two price rows above are the money-path entries most likely to be
silently missing at first launch.
