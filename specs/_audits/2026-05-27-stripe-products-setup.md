---
id: "AUDIT-2026-05-27-STRIPE-PRODUCTS-SETUP"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
tags: ["audit", "stripe", "products", "phase-0", "pricing"]
---

# Stripe Products + Prices setup SEAL

Phase 0.C wired Stripe Checkout but lacked `STRIPE_PRICE_PRO_MONTHLY`. This SEAL
documents the programmatic creation of the CoreLink Pro Stripe product and
its monthly + annual recurring prices via the Stripe REST API.

## §1 Products created

| Name | ID | Description |
|---|---|---|
| CoreLink Pro | `prod_UayszOXZRnN8Uf` | Multi-tenant content-addressable build cache. 500 GB / 20M req/month. Cancel anytime. |

URL: `https://corelink-docs.humangr.com/pricing`

## §2 Prices

| Lookup Key | ID | Amount | Currency | Interval | Nickname |
|---|---|---|---|---|---|
| `pro_monthly_usd_v1` | `price_1TbmuZLh0hhAZjwoL8PxhFhk` | $25.00 | USD | month | Pro Monthly USD |
| `pro_annual_usd_v1`  | `price_1TbmuaLh0hhAZjwoggmtSNL5` | $250.00 | USD | year  | Pro Annual USD |

Annual = 10x monthly ⇒ 2 months free per pricing-benchmarks §5.

## §3 Mode

**test** (`STRIPE_SECRET_KEY` starts with `sk_test_`). Production cutover will
re-create equivalent product/prices in live mode and update Pages secrets.

## §4 Pre-flight verification

- `livemode: false` confirmed in product response.
- `lookup_keys[]=pro_monthly_usd_v1&lookup_keys[]=pro_annual_usd_v1` query
  returned empty before creation ⇒ no duplicates.
- `CoreLink Pro` name not present in existing product list before creation
  (only `CoreLink Starter` was registered).

## §5 Wiring next steps

- `STRIPE_PRICE_PRO_MONTHLY=price_1TbmuZLh0hhAZjwoL8PxhFhk` registered as
  Cloudflare Pages secret on project `corelink-admin-ui` (see §6).
- `STRIPE_PRICE_PRO_ANNUAL=price_1TbmuaLh0hhAZjwoggmtSNL5` registered the
  same way.
- `/api/checkout/session` route already consumes `STRIPE_PRICE_PRO_MONTHLY`;
  follow-up issue will extend it with a `plan=annual|monthly` switch and
  read `STRIPE_PRICE_PRO_ANNUAL` accordingly.

## §6 Pages secrets

Applied via `npx wrangler@latest pages secret put ... --project-name=corelink-admin-ui`:

- `STRIPE_PRICE_PRO_MONTHLY` ← `price_1TbmuZLh0hhAZjwoL8PxhFhk`
- `STRIPE_PRICE_PRO_ANNUAL`  ← `price_1TbmuaLh0hhAZjwoggmtSNL5`

## §7 DCO sign-off

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
