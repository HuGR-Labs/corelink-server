# Issue #1655 / B-089 SLA credit evidence

The hosted workflow `.github/workflows/issue-1655-b089-contract.yml` is the
credentialless evidence lane for the automatic SLA credit path. It runs the
repository guard and validates the signed evidence manifest. It has read-only
GitHub contents permission, no provider credentials, and no network or customer
mutation capability.

## What a green run proves

The guard checks the repository-owned contract across:

- UTC month eligibility, ten-business-day cutoff, tier thresholds, force-majeure
  exclusions, capped percentage and safe integer amount calculation;
- unique tenant/month ledger rows, stable provider idempotency keys, durable
  outbox leasing, bounded retries, and `needs_review` failure states;
- customer monthly observation/report surfaces and the signed SLA's automatic
  invoice credit and sole-and-exclusive-remedy clauses; and
- the explicit `SLA_CREDITS_ENABLED` and `SLA_OBSERVATIONS_ENABLED` production
  defaults, both currently `false`.

The receipt status remains `parked_until_provider_proof` (or
`repository_contract_ready_external_receipt_pending`). A green run must not be
described as a live credit, invoice, provider, or migration proof.

## Owner packet for the remaining proof

The owner/operator must attach these four receipts to B-089, without placing
credentials or full provider responses in git:

1. **Production D1 migration receipt:** apply `migrations/d1/0117_sla_credit_ledger.sql`
   to the production `BILLING_DB` binding and record binding, migration version,
   timestamp, and operator identity.
2. **Controlled Stripe test-mode receipt:** with Finance approval, enable the
   gate only for a disposable test tenant and create one invoice item through
   the configured Stripe test account. Record the redacted provider object id,
   amount, currency, service period, and stable idempotency key.
3. **Replay receipt:** replay the same closed-month sweep after the first call
   and show that the stable idempotency key resolves to one provider object and
   one ledger row. Record retry or lease recovery outcomes.
4. **Reconciliation receipt:** match that provider object to the next invoice,
   then export the redacted D1 ledger, outbox, reconciliation, and audit rows.

These actions require provider/account authority and may issue a test credit;
they are intentionally outside the hosted credentialless lane.

## Contract decision still required

Before any production enablement, Owner, Finance, and Counsel must resolve the
published contract mismatch. The executed SLA names Free, Starter, Pro, and
Enterprise, while the product ladder also sells Solo and Max. The published
Terms describe an automatic Pro credit, while `apps/docs/src/lib/pricing.ts`
marks Pro `slaCredits: false` and its pricing test enables credits only for
Enterprise. Record an executed amendment or a signed authorization and align
the tier matrix before changing either production flag.

The canonical machine-readable packet is
`evidence/i1655/sla-credit-contract-manifest.json`.
