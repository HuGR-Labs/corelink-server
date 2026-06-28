---
type: "BillingControl"
title: "Edge quota & tier serving"
description: "How the Worker edge resolves a tenant's served tier and enforces per-tier storage and monthly-request caps — including the launch-blocker filter that refuses paid quota on a non-active subscription row."
source_files:
  - worker/src/lib/quota.ts
checkpoint_sha: "175a91320376cd80ada9797944e118ec1ff81c63"
provenance: "AUTHORED"
tags: [launch, billing, quota, tier, money-path, worker-edge]
timestamp: "2026-06-27T00:00:00Z"
---

# Edge quota & tier serving

This is the read-side counterpart to the [money path](/launch/money-path.md): once Stripe activation has written a tenant's subscription row, *this* module is what decides which tier quota the tenant is actually SERVED on every cache request. It runs in the Worker edge, AFTER PAT auth and BEFORE the request is forwarded to the Durable Object, and it answers two questions — "what tier is this tenant?" and "are they within their storage and monthly-request caps?". Getting the tier-resolution filter exactly right is launch-critical: the same row the checkout backend writes at click-time (before any payment) must NOT be served as paid until Stripe confirms the charge. The container's CAS/quota FSM is the deeper net on mutations; this concept is the cheap edge decision that protects revenue integrity and availability at the same time. See the sibling concepts [tier model](/launch/tier-model.md) (the ceilings sold) and [money path](/launch/money-path.md) (the checkout + activation seam that populates the row read here).

# Role

`quota.ts` owns three things at the Worker edge: (1) **tier resolution** — `getTierForTenant` maps a `tenant_id` to its effective `Tier` from D1; (2) **storage enforcement** — `checkStorageQuota` compares `SUM(bytes_used)` against the tier's byte ceiling; (3) **request enforcement** — `checkRequestQuota` atomically increments a monthly counter and compares it to the tier's request ceiling. The per-tier ceilings themselves are the frozen `QUOTAS` table transcribed from the signed launch rate card. All three are positioned at the edge so a doomed or over-cap request is rejected before it costs the container plane anything; the Durable Object re-enforces the hard CAS boundary on every mutation.

# How it works

- The `QUOTAS` table is the single source of per-tier ceilings — `free` = 10 GB / 500 K req/mo up through `max` = 2 TB / 80 M req/mo. The `MAX_SAFE_INTEGER` "no cap" sentinel is per-AXIS, not per-tier: `team` uncaps only its REQUEST axis but is storage-capped at a FINITE **1 TB** (`1_099_511_627_776`, `worker/src/lib/quota.ts:88`); ONLY `enterprise` is unlimited on BOTH axes (`worker/src/lib/quota.ts:85-92`).
- **The launch-blocker filter:** tier resolution reads the canonical subscription row with `SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = 'active'` — the `= 'active'` predicate is what refuses to serve a paid tier on a `pending_checkout` (written at click-time, before payment) or `inactive` row (`worker/src/lib/quota.ts:152`).
- A confirmed, valid `active` row short-circuits and returns that tier with `d1Error: false` (`worker/src/lib/quota.ts:161-162`).
- When no active subscription is found, resolution falls through to the tenant column `SELECT tier FROM tenant WHERE tenant_id = ?1` (migration 0057, `DEFAULT 'free'`) and returns **ANY valid `Tier` value that column holds — with NO active-subscription re-check** (`worker/src/lib/quota.ts:171-180`). This is `free` for every tenant TODAY only because the column defaults to `'free'` and no in-scope code writes it to a paid value; it is NOT guarded by a subscription check (see the defense-in-depth gotcha below).
- If BOTH D1 reads fail, the resolver returns the hard-coded `free` fallback but flags `d1Error = tierSelError && tenantTierError` so callers can tell a confirmed `free` from an outage-derived one (`worker/src/lib/quota.ts:187-188`).
- Storage enforcement reads `SELECT SUM(bytes_used) AS total_bytes FROM tenant_storage_state WHERE tenant_id = ?1` and compares it to the tier ceiling, returning `ok:true` while `totalBytes < storageBytesMax` (`worker/src/lib/quota.ts:344`, `worker/src/lib/quota.ts:356`).
- A tier whose STORAGE ceiling is `MAX_SAFE_INTEGER` skips the storage SUM entirely — there is nothing to check (`worker/src/lib/quota.ts:336`). Today that is ONLY `enterprise`; `team` has a finite 1 TB storage ceiling (`worker/src/lib/quota.ts:88`) and is NOT skipped, so its storage is enforced like any capped tier (the request-axis `MAX_SAFE_INTEGER` is a separate sentinel that uncaps the monthly-request gate).
- Request enforcement is a single ATOMIC `INSERT … ON CONFLICT(tenant_id, year_month) DO UPDATE SET request_count = request_count + 1 … RETURNING request_count`, so concurrent isolates cannot race a read-modify-write (`worker/src/lib/quota.ts:523`).

# Invariants

- Paid serving is granted on EITHER of two arms: (1) an `active` `tier_selections` row (subscription-gated — the launch-blocker filter), OR (2) a paid value in the `tenant.tier` column (an UNGUARDED fallback with NO active-subscription check). Arm (1) is what makes an abandoned checkout (`pending_checkout`/`inactive`) fall through and never be served paid quota (`worker/src/lib/quota.ts:152`). Arm (2) is safe TODAY only because `tenant.tier` defaults to `'free'` and no in-scope code writes it paid — it is NOT a subscription guarantee (`worker/src/lib/quota.ts:171-180`); see the defense-in-depth gotcha.
- The per-tier ceilings served at the edge are exactly the frozen `QUOTAS` rate-card values — no AXIS is silently uncapped except where the value is the explicit `MAX_SAFE_INTEGER` sentinel: `team` uncaps only its request axis (storage stays a finite 1 TB, `worker/src/lib/quota.ts:88`) and only `enterprise` is uncapped on both axes (`worker/src/lib/quota.ts:85-92`).
- The monthly request count is incremented atomically in one round trip (`INSERT … ON CONFLICT … RETURNING`), so two concurrent requests cannot both read the same pre-increment value (`worker/src/lib/quota.ts:523`).
- Tier resolution and storage enforcement share ONE failure posture: an unconfirmed (`d1Error`) tier never seeds a cap header (`storageQuotaHeaderValue` returns `null`) (`worker/src/lib/quota.ts:234`).

# Gotchas

- **F21 fail-OPEN symmetry — disclosed honestly.** If BOTH tier queries error, `getTierForTenant` returns `{ tier: 'free', d1Error: true }` (`worker/src/lib/quota.ts:187-188`); `checkStorageQuota` then SKIPS the storage SUM and returns `ok:true` for READ-style requests rather than enforce a possibly-wrong `free` cap (`worker/src/lib/quota.ts:327`). This is deliberate: combining an error-derived `free` tier with a successful storage query would 429 a paid tenant who legitimately stores > 10 GiB. The cost of the symmetry is real, though — during a *total* tier-lookup outage a tenant who IS over their real cap is not blocked at the edge on reads. The bound is that this is fail-open only; MUTATING (PUT/POST) requests fail CLOSED with a short Retry-After (`worker/src/lib/quota.ts:316-322`), and the Durable Object's CAS quota FSM remains the deeper net on every write.
- **Defense-in-depth gap — the `tenant.tier` fallback is unguarded.** Fallback arm (2) returns whatever valid `Tier` the `tenant.tier` column holds with NO `subscription_state = 'active'` re-check (`worker/src/lib/quota.ts:171-180`). Today this is safe ONLY because the column's migration-0057 default is `'free'` and no in-scope writer sets it to a paid value — the protection is a default, not an invariant. A future or rogue writer that set `tenant.tier = 'pro'` directly (bypassing `tier_selections`) would be served paid quota with NO active subscription, contradicting the money-path guarantee. The robust fix would be to either gate this arm on an active subscription too, or never store a paid value in `tenant.tier`. Tracked as a defense-in-depth seam, not an exploitable bug at HEAD.
- The monthly request gate is the *aggregate* monthly cap, NOT the container's per-second token-bucket rate limit — a slow-but-steady tenant can stay under req/s yet exceed the contracted monthly allowance, which is exactly the gap this edge check closes (`worker/src/lib/quota.ts:444`).
- `storageQuotaHeaderValue` emits `"0"` for a genuinely-unlimited tier and `null` (do-not-inject) for an unconfirmed `d1Error` tier — `"0"` is the container's unlimited sentinel and is NOT the same as absence; conflating them re-opens the legacy "fresh tenant uncapped" hole (`worker/src/lib/quota.ts:234`).

# Citations

1. `worker/src/lib/quota.ts:85-92` — the frozen `QUOTAS` per-tier storage/request ceilings (rate card); the `MAX_SAFE_INTEGER` sentinel is per-axis (`team` request-only, `enterprise` both).
1b. `worker/src/lib/quota.ts:88` — `team` has a FINITE 1 TB storage ceiling (`1_099_511_627_776`); only its request axis is `MAX_SAFE_INTEGER`.
2. `worker/src/lib/quota.ts:152` — the launch-blocker filter: `subscription_state = 'active'` refuses paid tiers on non-active rows.
3. `worker/src/lib/quota.ts:161-162` — confirmed active row returns the tier (`d1Error: false`).
4. `worker/src/lib/quota.ts:171-180` — UNGUARDED fallback to the `tenant.tier` column: returns ANY valid `Tier` value with NO active-subscription re-check (safe only by the migration-0057 `'free'` default).
5. `worker/src/lib/quota.ts:187-188` — both-queries-failed → `free` fallback flagged `d1Error: true`.
6. `worker/src/lib/quota.ts:234` — `storageQuotaHeaderValue` returns `null` on an unconfirmed tier (no cap-header injection).
7. `worker/src/lib/quota.ts:316-322` — D1-error posture: writes fail CLOSED with a short Retry-After.
8. `worker/src/lib/quota.ts:327` — F21: `checkStorageQuota` skips the storage check when `d1Error` (fail-open symmetry).
9. `worker/src/lib/quota.ts:336` — unlimited tier skips the storage SUM.
10. `worker/src/lib/quota.ts:344` — `SUM(bytes_used)` storage-cap query.
11. `worker/src/lib/quota.ts:356` — within-cap → `ok:true`.
12. `worker/src/lib/quota.ts:444` — request-quota gate (monthly aggregate, distinct from the per-second rate limit).
13. `worker/src/lib/quota.ts:523` — atomic monthly increment-and-check UPSERT (`ON CONFLICT … RETURNING`).
