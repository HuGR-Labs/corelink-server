# Go-Live Code Review — Money / Billing Path (2026-06-23)

Read-only senior review of the Stripe webhook → materializer → D1 → quota/$-ceiling
chain. Scope per the review brief. Every finding below was verified against the
code (line refs are absolute file paths). No code was modified.

## Verdict

The core money-integrity invariants hold: HMAC-verify-before-parse, idempotency
dedup committed before materialize, audit-before-write (fail-closed), the
`subscription_state='active'` gate on both the Worker read path and the container
materializer write path, atomic check-and-accrue for the `$`-ceiling, and the
runners-vs-cache routing decouple cleanly. The recently-changed code
(`reconcile_runners`, F-001 tier map, F-008 DLQ, F-018 billing scope gate) is
sound. **No CRITICAL money-loss or wrong-grant defect found.**

The findings are a set of MED/LOW correctness + robustness gaps. The one that
matters most for launch is the **`team` tier price has no container-side mapping**
(F-MP-1): a real `team` subscription's `customer.subscription.updated` 422s in the
container materializer and silently drops the entitlement reconcile there (no DLQ,
Stripe stops retrying). It is NOT a live money-loss because the signup-worker is the
authoritative tier writer and DOES map `team` — but it is a latent landmine if the
container webhook ever becomes the sole/primary authority, and it is an asymmetry
between the two writers that violates the "two writers must agree" design intent.

---

## Findings

### F-MP-1 — `team` price-id has no container-side tier mapping → 422 + silent entitlement-drop on the container webhook  [MED]

- `crates/corelink-container/src/main.rs:93-98` — `TIER_PRICE_ENV_TABLE` maps only
  `Solo/Starter/Pro/Max`. No `STRIPE_PRICE_ID_TEAM` row (TierKind has no `Team`
  variant — `crates/corelink-tier-selection/src/tier.rs:15-78`).
- `worker/src/durable_object.ts:556` forwards `STRIPE_PRICE_ID_TEAM` to the
  container env, and the signup-worker (`apps/signup-worker/src/webhooks/stripe.ts:246,290`)
  treats `team` as a first-class `PaidTier` mapped from `STRIPE_PRICE_ID_TEAM`.
  The Worker quota table (`worker/src/lib/quota.ts:88`) and the OCI cap resolver
  (`crates/corelink-container/src/oci_cap.rs:74`) both carry a finite `team` cap.
- Exploit/bug: a tenant on a real `team` subscription generates
  `customer.subscription.updated` carrying `plan.id == STRIPE_PRICE_ID_TEAM`. In the
  container materializer, `reconcile_runners` returns `Ok(false)` (not a runners
  price), so `reconcile_tier` runs → `compute_tier(team_price, …)` →
  `TierSelectError::UnknownPlan` → `MaterializerError::InvalidPayload` → HTTP 422.
  Stripe stops retrying on 422, and the F-008 DLQ only captures `Transient`
  (`webhook_dispatch.rs:714` vs `:740` InvalidPayload), so the container's
  `tier_selections` reconcile for that event is **permanently dropped**.
- Why it is MED not CRITICAL: in `do_subscription_upsert`
  (`crates/corelink-billing-stripe-materializer/src/handler.rs:303-329`) the audit
  and the `stripe_subscriptions` row are written BEFORE `reconcile_tier` raises —
  so the subscription state is still recorded; only the entitlement reconcile is
  lost. And per the memory the LIVE Stripe webhook is the signup-worker, which maps
  `team` correctly. The container path is the secondary/defense-in-depth writer.
- Fix recommendation: either (a) add a `TierKind::Team` and a
  `("STRIPE_PRICE_ID_TEAM", "plan_team", TierKind::Team)` row so the two writers
  agree, or (b) if `team` is deliberately not a self-serve checkout tier, route a
  recognized-but-non-checkout price to `Ok(())` (no-op, 200) instead of
  `UnknownPlan` 422 — so an out-of-band `team` assignment doesn't 422 the container
  webhook. Do NOT leave it 422-ing silently.

### F-MP-2 — `UnknownPlan`/`InvalidPayload` from a real price-id is not DLQ'd → unrecoverable on the container path  [MED]

- `crates/corelink-stripe-real/src/webhook_dispatch.rs:740-756` — the
  `MaterializerError::InvalidPayload` arm returns 422 and does NOT call
  `quarantine_transient`; only the `Transient` arm (`:714-738`) and the
  unknown-variant arm (`:760-780`) do.
- The design intent (`tier.rs:9-11`) is that `UnknownPlan` 422 = "operator gets
  paged via the dashboard." That is the correct posture for a genuinely malformed
  event — but a 422 caused by a *config drift* (a real, valid price-id that the
  container's env map simply doesn't know, e.g. F-MP-1, or a price-id env var that
  was set in the Worker but not forwarded/typo'd) is operationally indistinguishable
  from a malformed event and is silently, permanently dropped on this path.
- Fix recommendation: emit a Sev1/error-level structured audit (not just the
  `MaterializerInvalid` audit at INFO-equivalent severity) AND/OR quarantine
  `InvalidPayload` to the DLQ as well so an operator can replay after fixing the
  price map. At minimum, alert on the `MaterializerInvalid` audit-outcome count.

### F-MP-3 — `reconcile_runners` swallows a missing `plan.id` and falls through to the cache path  [LOW]

- `crates/corelink-billing-stripe-materializer/src/handler.rs:369-377` — when
  `data.object.plan.id` is absent, `reconcile_runners` returns `Ok(false)` ("let
  the cache path raise its own error"). The cache `reconcile_tier`
  (`handler.rs:422-432`) then raises `InvalidPayload("missing data.object.plan.id")`.
- This is correct for a cache subscription but means a RUNNERS subscription whose
  event shape omits `plan.id` (or carries the price under a different key, e.g.
  `items.data[0].price.id` rather than `plan.id`) is misclassified as a cache event
  and 422s. Stripe's modern subscription object nests the price under
  `items.data[].price.id`; `plan.id` is the legacy location. Both the runners AND
  the cache resolver read ONLY `data.object.plan.id`
  (`handler.rs:371-374` and `:425-427`).
- Exploit/bug: not a security issue, but a real-event-shape robustness gap — if the
  live Stripe API version sends `items.data[].price.id` and not the legacy
  `plan.id`, EVERY container-side reconcile (cache and runners) 422s. Verify the
  pinned Stripe API version still populates `data.object.plan.id` for
  subscription.updated; the signup-worker should be checked for the same assumption.
- Fix recommendation: read the price id from `plan.id` with a fallback to
  `items.data[0].price.id` (and `…plan.id`), in BOTH resolvers, behind one helper.

### F-MP-4 — `$`-ceiling charges a flat op cost on CAS reads that 404/410  [LOW]

- `crates/corelink-container/src/routes/cas.rs:660-664` (read) charges
  `gate.check` BEFORE the tombstone gate (`:674`) and the R2 read (`:701`). A read
  that resolves to 404 (absent) or 410 (tombstoned) has already accrued the flat
  `cost_per_op` micro-dollars against the tenant's monthly ceiling.
- This is an over-charge of the tenant, never an under-charge / money-loss for us,
  and the cost model is explicitly documented as a coarse preventive tripwire, not
  precise metering (`tenant_quota.rs:78-89`). So it is acceptable-by-design, logged
  here as a known asymmetry: a tenant probing many absent/erased hashes burns
  `$`-ceiling budget. Worth a doc note; not a launch blocker.
- Fix recommendation (optional): move the `$`-charge AFTER the tombstone gate for
  reads, or refund on 404/410, if precise read accounting ever matters.

### F-MP-5 — `i64::MAX` saturation on byte length silently caps an over-`i64` write  [LOW]

- `crates/corelink-container/src/byte_accounting.rs:655` (`i64::try_from(req.bytes.len()).unwrap_or(i64::MAX)`)
  and the `mat_ms`/`now` casts in `billing_d1_http.rs:148,294`. The global 10 MiB
  body limit (`main.rs:459`) makes an over-`i64` CAS body unreachable in practice,
  and a `now_ms` over `i64::MAX` is the year-292M; both are defensive saturations
  that are unreachable. Noted for completeness — saturating to `i64::MAX` on the
  byte path would, if ever reached, reserve a near-infinite cap (rejecting the write
  as OverCap), which is fail-safe. No action required.

### F-MP-6 — `LeasedQuotaStore` crash-loss tail under-charges (by design) — confirm the operator accepts it  [LOW / informational]

- `crates/corelink-container/src/tenant_quota.rs:396-441` documents the five lease
  invariants. The up-front debit means a container crash loses at most the unused
  lease tail → the tenant is slightly UNDER-charged (`$0.016` worst case on a `$5`
  ceiling), never over-served. Verified the refill path is fail-CLOSED
  (`charge` at `:578-620`: inner `check_and_accrue`/`seed_checked_accrue` `Ok(false)`
  ⇒ propagate reject, `Err` ⇒ propagate 503; the partial-lease halving never serves
  for free because every chunk includes `cost_micros` and the loop floors at
  `cost_micros`). This is correct and safe; flagged only so the under-charge tail is
  an explicit, accepted business choice.

---

## What was verified clean (positive assertions)

- **HMAC verify precedes JSON parse** (`webhook_dispatch.rs:554-599`); 401 on
  bad/replayed/future-dated sig before any envelope parse; 5-min replay tolerance;
  constant-time compare via `subtle` (per module doc + `classify_verify_err`).
- **Idempotency ordering**: dedup row committed at step 5 BEFORE materialize
  (`:606-646`); a duplicate short-circuits to 200 with NO dispatch
  (`:607-623`); the native `D1IdempotencyStore` / `try_record_event` uses
  `INSERT … ON CONFLICT DO NOTHING RETURNING event_id` so insert-vs-replay is
  detected atomically (`billing_d1_http.rs:276-305`, `d1.rs:164`). No double-dispatch.
- **F-008 DLQ**: a `Transient` materialize failure quarantines the (HMAC-verified)
  event so a Stripe-retry `AlreadyProcessed` short-circuit doesn't permanently drop
  state; best-effort, never masks the 500 (`webhook_dispatch.rs:714-738,784-825`);
  regression test at `:1009-1075`.
- **Audit-before-write fail-CLOSED**: every state mutation emits the billing audit
  BEFORE the D1 write and returns `Transient` if the audit fails — no orphan state
  (`handler.rs:297-329, 388-411, 466-503`; test `audit_failure_aborts_d1_write_fail_closed`).
- **subscription_state='active' gate (no paid tier for unpaid)**: enforced in THREE
  places consistently — the Worker read path
  (`worker/src/lib/quota.ts:150-159`, `AND subscription_state = 'active'`), the
  container materializer write path (`handler.rs:135-137,444-461,385-387` —
  `subscription_status_grants_access` only `active|trialing`; a recognized plan with
  `past_due/unpaid/…` does NOT reach the `'active'` upsert, for BOTH cache tier and
  runners seed), and the OCI cap resolver (`oci_cap.rs:122-149`). Regression tests
  `subscription_updated_non_granting_status_does_not_grant_active` and
  `runners_price_with_non_granting_status_does_not_seed` pin it.
- **Runners vs cache routing (no cross-grant)**: `reconcile_runners` returns
  `Ok(true)` ONLY when the price resolves to a runners entitlement, suppressing the
  cache reconcile; a cache price resolves `None` → falls through to tier; a runners
  price never touches `tier_selections` and a cache price never touches
  `runners_entitlement` (`handler.rs:339-413`; tests
  `runners_subscription_seeds_entitlement_not_tier`,
  `cache_price_still_routes_to_tier_when_runners_resolver_present`). The two price
  env tables are disjoint by construction (`main.rs:93-113`); runners has NO literal
  fallback (a runners price must be a real Stripe id), cache has the `plan_{tier}`
  fallback only when the env is unset.
- **$-ceiling TOCTOU closed**: steady-state, cycle-roll, and brand-new-tenant first
  op all route through atomic single-statement D1 (`check_and_accrue` /
  `seed_checked_accrue` / `roll_if_stale` overrides in
  `tenant_quota.rs:1021-1136`); the fresh-row first op is ceiling-checked
  (red-team #6) so a fat first batch can't bypass the cap.
- **Byte-accounting TOCTOU closed**: reserve-before-PUT, single-statement atomic
  check-and-accrue, per-`(tenant,hash)` shard lock serializing write-vs-delete,
  fail-CLOSED on indeterminate/transport (`byte_accounting.rs:300-432,648-736`).
  Tier-downgrade reconcile on conflict (rt-nuclear #16) is correct
  (`:347-378`, `COALESCE(NULLIF(?5,0), …)`).
- **F-018 billing scope gate**: every billing/PII customer surface (overview,
  billing, audit, portal) requires `requires_cache_write`; a `cas:r` token → 403
  (`routes/customer.rs:312-344,403-411,506-511,560-565,605-611`; 5 tests).
- **Webhook routing arm**: `/v1/billing/stripe-webhook` is a pure forwarder to the
  `_system` DO with the raw body UNREAD (signature-preserving), strips client-trust
  headers, never sets a tenant header (`worker/src/index.ts:771-773,1867-1905`).
- **Worker request-quota** is an orthogonal axis (monthly request COUNT, atomic
  UPSERT, fail-open on D1 error per documented posture) — no double-charge with the
  container `$`-ceiling (cost in dollars) (`worker/src/lib/quota.ts:437-541`).
