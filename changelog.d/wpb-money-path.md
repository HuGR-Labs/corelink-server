### Fixed

- **B-090 — the live Stripe/Clerk/DSR worker no longer deploys from an
  unpinned `npm install`.** `.github/workflows/signup-worker-deploy.yml` ran
  `npm install --legacy-peer-deps` on the premise that "`npm ci` cannot run —
  the lockfile is gitignored". The premise was false: `apps/signup-worker` is a
  pnpm workspace member with a fully-resolved importer in the **committed**
  `pnpm-lock.yaml`. Every deploy of the money-path handler was re-resolving its
  whole dependency tree on the self-hosted Mac with the Cloudflare deploy
  credentials in the environment, and shipping a graph `pnpm-audit.yml` never
  scanned. The lane now runs `pnpm install --frozen-lockfile --ignore-scripts`
  from the repo root, matching `admin-ui-deploy.yml` and `cf-deploy-prod.yml` —
  which pins the graph **and** makes it the same graph the audit lane sees. No
  `package-lock.json` is committed; the repo's pnpm-only convention is intact.

- **B-079 — an unpriced rate-limit tier no longer receives the Enterprise
  rate.** `refill_rate_for_tier`'s forward-compatibility wildcard in
  `crates/corelink-ratelimit/src/tier.rs` fell back to
  `(ENTERPRISE_REFILL_RPS, ENTERPRISE_BURST)` — 10 000 rps, 1000× Free and 50×
  the system's own default — for any `Tier` variant the crate had not been
  taught about, while its sibling `tier_for_billing_label` fell back to `Team`.
  Two paths that both mean "we do not know this tenant's plan", answering 50×
  apart. The wildcard now resolves to the Team default, which is what
  `RateLimitConfig::canonical()` already gives an unresolved tenant: the
  availability property the old arm defended is kept, the free ride is not.

- **B-076 (partial) — a tier-select lock can no longer be released by a
  request that does not hold it.** `release_lock` in
  `crates/corelink-container/src/routes/tier_select_store.rs` was
  `DELETE … WHERE tenant_id = ?1`, discarding the `correlation_id` that
  `acquire_lock` writes. With a 60 s TTL and a live Stripe call inside the
  window, a request that overran its lease released whichever lock was on the
  row — including a *later* request's, mid-orchestration. The statement is now
  scoped by holder (`RELEASE_TIER_SELECTION_LOCK_SQL`) and the
  `TierSelectStore::release_lock` signature carries the `correlation_id`.

- **B-076 (partial) — a superseded Stripe subscription is no longer erased in
  silence.** `tenant_billing` holds one row per tenant and
  `upsertBillingPaid` (`apps/signup-worker/src/webhooks/stripe.ts`) replaces
  `stripe_subscription_id` unconditionally on conflict. When the previous
  subscription is still live in Stripe, that overwrite is the moment it stops
  existing for the platform: nothing keys on it, no cancellation path can reach
  it, and it keeps charging. The write is deliberately unchanged — refusing it
  would strand a customer who has already paid — but it is now preceded by a
  read that emits a stable `ORPHANED_SUBSCRIPTION` line naming the tenant and
  both subscription ids, so the orphan is reconcilable instead of invisible.
