# CAA-360 pre-HN-launch audit — confirmed findings

Run wf_6cadec90-fae · 35 confirmed / 19 refuted of 54 unique · by severity: {'info': 13, 'low': 18, 'critical': 1, 'high': 1, 'medium': 2}

_Every finding below was adversarially verified (confirm/refute) — false-positives removed._


## [CRITICAL] EU-resident tenants' CAS+AC bytes survive GDPR erasure — DSR erase/verify runs only in the IAD container against corelink-cas-prod, never the separate EU-jurisdiction buckets (corelink-cas-eu / corelink-ac-eu), and the Ed25519 attestation then falsely signs VerifiedComplete

- **surface:** DSR/GDPR erasure completeness × residency (Schrems-II)
- **location:** crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:47,55,100-127 + adapter_r2_ac.rs:48,56,107-138 vs wrangler.toml:754-770 + apps/signup-worker/src/webhooks/dsr_consumer.ts:76,85-87 + apps/signup-worker/wrangler.toml:76,90-92
- **confidence:** high · foundBy REV-O1

**What it is:** CoreLink promises GDPR Article 17 (right to erasure) and physically isolates EU tenants' data: the `[env.prod-lhr]` regional Worker/container writes EU tenants' content-addressed (CAS) and action-cache (AC) bytes to DEDICATED EU-jurisdiction R2 buckets `corelink-cas-eu` / `corelink-ac-eu` reached via the EU S3 endpoint (`...eu.r2.cloudflarestorage.com`), specifically to keep EU bytes out of the US-located default buckets (wrangler.toml:756-770). That residency isolation is real. The bug is that the ERASURE path was never taught about it. When a Clerk `user.deleted` webhook fires (the PRIMARY, always-on GDPR trigger for a self-serve SMB), the signup-worker's queue consumer forwards the erase to `${CORELINK_API_BASE}/_internal/dsr/erase` over the `CORELINK_API_SVC` service binding — and BOTH of those point at `corelink-prod`, i.e. the IAD (US) Worker/container (apps/signup-worker/wrangler.toml:76 `CORELINK_API_BASE = https://corelink-api.humangr.com`, :90-92 `service = corelink-prod`). Inside that IAD container the R2 CAS erase adapter reads its OWN process env: `R2_CAS_BUCKET` defaults to `corelink-cas-prod` (adapter_r2_cas.rs:56) and `StorageEnv::from_env()` yields the US S3 endpoint, then it LIST-and-DELETEs only the region *prefixes* `[sam,iad,lhr,nrt,syd]` INSIDE that single US bucket (adapter_r2_cas.rs:47,110-123). It never opens `corelink-cas-eu`, and it never uses the EU endpoint. The AC adapter is worse by name: it computes bucket `corelink-ac-` + region ∈ {sam,iad,lhr,nrt,syd} = `corelink-ac-lhr` (adapter_r2_ac.rs:48,56,90,118) — but the EU AC bucket is `corelink-ac-eu`, which is not in the list and is on a different endpoint. So an EU tenant's CAS blobs and AC entries are NEVER deleted on the webhook-driven erasure. Then the 24h verify cron ALSO runs in IAD (dsr_verify_cron.ts:109 targets the same `CORELINK_API_BASE`), so `count_cas_remaining` / `count_ac_remaining` count objects in the US buckets, find zero, return the canonical empty hash, and the orchestrator signs an Ed25519 `VerifiedComplete` attestation for a tenant whose EU data is fully intact. The system therefore produces a cryptographically-signed FALSE certificate of erasure.

**Reproduction:** 1. Provision (or identify) a tenant with `primary_region = 'weur'` so the Worker fan-out routes it to `env.PROD_LHR` (worker/src/index.ts:2461-2540); write CAS blobs (`PUT /v1/cas/:tenant/:hash`) and AC entries through the EU host — bytes land in `corelink-cas-eu`/`corelink-ac-eu`. 2. Confirm placement: list `corelink-cas-eu` under `lhr/<derive_prefix(tdk,tenant)>/` via the EU S3 endpoint — objects present. 3. Trigger erasure: either fire the Clerk `user.deleted` webhook for that tenant, or POST the same `dsr.queued.v1` to the signup-worker's queue. 4. Observe the consumer POST `/_internal/dsr/erase` to `corelink-api.humangr.com` (IAD). 5. After completion, re-list `corelink-cas-eu` / `corelink-ac-eu` via the EU endpoint — the tenant's objects are STILL THERE. 6. Fire the verify tick (or wait for the 24h cron); observe an `erasure_attestation` row signed `VerifiedComplete` despite the surviving EU bytes.

**Impact:** Every EU (weur/lhr) tenant that requests erasure — the exact population the EU-bucket isolation exists to protect — retains all their content-addressed cache blobs and action-cache entries (which can embed source paths, build artifacts, and derived data) in `corelink-cas-eu`/`corelink-ac-eu` forever, despite a completed, signed 'VerifiedComplete' erasure. This is a direct GDPR Art.17 violation compounded by a falsified Art.5(2)/accountability artifact (a signed certificate asserting erasure that did not happen). Blast radius = all EU tenants; regulatory exposure = GDPR fines up to 4% of global turnover, plus the signed-but-false attestation makes the failure worse in an audit than a plain miss. Note: the SELF-SERVE `POST /v1/customer/account/delete` path happens to run in-process in the LHR container and DOES hit the EU buckets — so the gap is specifically the webhook/queue + verify path (the default, always-on trigger), which masks the bug from the self-serve smoke test.

**Fix:** The erasure must run against EVERY jurisdiction bucket/endpoint a tenant could have written to, keyed by the tenant's residency — not a single IAD sweep. Concretely: (a) resolve the tenant's `primary_region` at erase time and fan the `/_internal/dsr/erase` call out to the matching regional container (PROD_LHR etc.) exactly as the data-plane fan-out does in worker/src/index.ts:2461, OR (b) make the CAS/AC erase adapters iterate an explicit `(endpoint, bucket)` residency matrix that includes `(eu-endpoint, corelink-cas-eu)` and `(eu-endpoint, corelink-ac-eu)`, building a distinct `R2S3Client` per endpoint rather than reusing the single `StorageEnv::from_env()` US client. Add `eu` to the AC/CAS sweep matrix. Critically, make `verification_hash` / `count_*_remaining` sweep the SAME matrix so the attestation cannot sign `VerifiedComplete` unless the EU buckets were actually checked-and-empty — and add a gate test that fails if any provisioned residency bucket (from wrangler `[env.prod-*]`) is absent from the erase/verify matrix (mirror the existing `residency_invariant_every_regional_env_sets_cas_region` test, but for the erase side).

**Verify verdict:** confirmed — I independently reproduced the full path in real code; the finding holds end-to-end.

TOPOLOGY (write side — EU data really lands in separate EU buckets):
- wrangler.toml [env.prod-lhr] (name "corelink-prod-lhr") sets R2_S3_ENDPOINT="https://...eu.r2.cloudflarestorage.com", R2_CAS_BUCKET="corelink-cas-eu", R2_AC_BUCKET="corelink-ac-eu" (lines 761-764); comment (755-759) states these are physically EEUR and the corelink-*-lhr buckets are ENAM/US and MUST NOT hold EU bytes.
- worker/src/region-map

---

## [HIGH] Team Member Role Not Enforced — All Clerk Sessions Get Full read-write Scope

- **surface:** worker/src/lib/clerk_auth.ts:265-275, worker/src/index.ts:2120
- **location:** worker/src/lib/clerk_auth.ts:265-275 and worker/src/index.ts:2120
- **confidence:** high · foundBy REV-S2

**What it is:** The `team_member` D1 table (migration 0074) defines four RBAC roles: `owner`, `admin`, `member`, and `viewer`. When a team member logs in via Clerk, `verifyClerkSessionAndResolveTenant` falls through to the `team_member` lookup (line 265 of clerk_auth.ts) and returns only `{ ok: true, tenantId, clerkUserId }` — the `role` column is never queried or returned. Then, in the `customer_v1` arm in index.ts (line 2120), the Worker unconditionally sets `x-corelink-scope: 'read-write'` for every Clerk session, regardless of whether the authenticated user is an owner, admin, member, or viewer. A viewer-role team member should receive read-only access, but instead receives exactly the same `read-write` scope as the tenant owner. This means all RBAC role distinctions defined in the database schema are dead: the `role` column exists and is schema-enforced with a CHECK constraint, but is never consulted at the authz layer. A user who has been granted `viewer` or `member` role gets full read-write access to the tenant's dashboard and data-plane endpoints.

**Reproduction:** 1. Provision a tenant owner + invite a second user as `viewer` (role='viewer' in team_member table). 2. Log in as the viewer user via the Clerk dashboard. 3. Issue POST /v1/customer/keys/revoke with the viewer's Clerk session Bearer. 4. Observe the request succeeds despite the viewer's intended read-only role.

**Impact:** Any active team member, regardless of their assigned role (viewer or member), can perform write operations on behalf of the owning tenant — including deleting cache artifacts, revoking API keys, modifying team settings, and triggering billing changes. This breaks the multi-seat access-control model at its foundation.

**Fix:** 1. Change the D1 query in `clerk_auth.ts` to also select `role`: `SELECT tenant_id, role FROM team_member WHERE user_id = ?1 AND status = 'active' LIMIT 1`. 2. Return the resolved role in the `ClerkAuthResult` type (add `memberRole?: string`). 3. In the `customer_v1` arm, derive the scope from the role: owner/admin → `read-write`; viewer → `read-only`; member → decide policy. 4. Set `x-corelink-scope` from that derived value instead of hardcoding `read-write`.

**Verify verdict:** confirmed — I independently reproduced the full exploit path in the real code; the finding holds.

CORE MECHANISM (both cited facts verified):
- worker/src/lib/clerk_auth.ts:265-268 — the team_member fallback query is `SELECT tenant_id FROM team_member WHERE user_id = ?1 AND status = 'active' LIMIT 1`. Only `tenant_id` is selected; `role` is never read, and ClerkAuthResult carries no role.
- worker/src/index.ts:2120 — inside the `customer_v1` arm, `h.set('x-corelink-scope', 'read-write')` is unconditional f

---

## [MEDIUM] Email address hashes stored as plain SHA-256 in production — rainbow-table attack possible against D1 team-invite pseudonyms

- **surface:** Email pseudonymization — email_hash.rs hash_email() with EMAIL_HASH_SALT unset
- **location:** crates/corelink-container/src/email_hash.rs:47-62
- **confidence:** high · foundBy REV-S4

**What it is:** The `hash_email()` function is the single entry point for all email pseudonymization in CoreLink: it writes the hash at team-invite creation time (`customer_d1.rs`), looks it up at accept time, and uses it in DSR Art.16 rectification routes. The function has two code paths: if the environment variable `EMAIL_HASH_SALT` is set and non-empty, it computes `HMAC-SHA256(key=EMAIL_HASH_SALT, msg=normalized_email)` — a keyed hash that is computationally infeasible to reverse without the key. If `EMAIL_HASH_SALT` is absent or empty (the current production state, confirmed by secrets-checklist.md row 171: 'UNSET in prod today'), it falls back to plain `SHA-256(normalized_email)` — an unkeyed hash. SHA-256 of an email address is trivially reversible via rainbow tables or targeted pre-computation: email addresses come from a finite, enumerable space (common providers, predictable formats), and prebuilt tables for hundreds of millions of email addresses exist. An attacker with read access to the D1 `team_invites` table (e.g., via a D1 SQL injection, a misconfigured Cloudflare access control, or an insider threat) can map every stored `invited_email_hash` back to the plaintext email address using a precomputed table, completely defeating the pseudonymization intended to protect invitee privacy. The code's own comment acknowledges this risk ('an attacker without it [EMAIL_HASH_SALT] cannot rainbow-table the pseudonyms') but the salt is not set in production, so that protection does not currently exist.

**Reproduction:** 1. Confirm EMAIL_HASH_SALT is unset in the container environment (check via `wrangler secret list --env prod`). 2. Look at any row in D1 `team_invites.invited_email_hash`. 3. Compute `sha256('user@gmail.com')` and compare — if match, hash is reversible. 4. Alternatively, query 'SELECT invited_email_hash FROM team_invites LIMIT 10' and run the results through any SHA-256 rainbow table service (e.g., CrackStation) — expect hits for common email providers.

**Impact:** Privacy breach: all email addresses in D1 `team_invites` since platform launch are pseudonymized with reversible plain SHA-256. The data is PII under GDPR (email is directly identifiable). Blast radius: every user ever invited to a CoreLink workspace (tenant org members, pilot invitees) has their email address effectively exposed if D1 is read by an attacker. The platform's stated GDPR posture (DSR routes, Art.16 rectification) relies on email pseudonymization as a privacy control — that control is currently absent in production.

**Fix:** Set EMAIL_HASH_SALT immediately in production. The infrastructure to support it is fully built and the dual-read shim is deployed (handoff doc 2026-07-02 confirms this). The one-shot activation command is already documented: `V=$(openssl rand -hex 32) && printf '%s' "$V" | worker/node_modules/.bin/wrangler secret put EMAIL_HASH_SALT --env prod` (repeat for signup-worker for cross-lang parity, as documented). This must be done in a quiet window to avoid orphaning pending invites. Mark EMAIL_HASH_SALT as a required (not optional) secret in the secrets matrix so that future deployments without it fail fast at startup rather than silently degrading to the unsalted fallback. Consider adding a startup assertion: `if std::env::var("EMAIL_HASH_SALT").map_or(true, |v| v.is_empty()) { tracing::error!("EMAIL_HASH_SALT is unset — email pseudonyms are not privacy-protected"); }`.

**Verify verdict:** confirmed — The finding is fully confirmed by independent code review. All three evidence pillars hold:

1. VULNERABLE CODE PATH IS ACTIVE. `/Users/gustavoschneiter/Documents/HuGR/corelink-server/crates/corelink-container/src/email_hash.rs:47-56` shows the exact fallback branch — when `EMAIL_HASH_SALT` is unset, `hash_email()` returns `hex::encode(Sha256::digest(normalized.as_bytes()))`: plain SHA-256 with no server-side secret. The TypeScript twin in `apps/signup-worker/src/webhooks/clerk.ts:695-711` (the 

---

## [MEDIUM] customer.subscription.deleted downgrade writes tier_selections.tier='free' with subscription_state='active', leaving a contradictory active-free row

- **surface:** Billing/Stripe — subscription deletion materializer
- **location:** crates/corelink-billing-stripe-materializer/src/handler.rs:369-375
- **confidence:** high · foundBy REV-S5

**What it is:** When the container materializer handles customer.subscription.deleted, it calls do_subscription_upsert(env, canceled=true). This path calls mark_subscription_canceled (which updates stripe_subscriptions to status='canceled') and then calls persist_tier_change with TierKind::Free. The persist_tier_change method reads the current tier, finds it changed (e.g. from 'pro' to 'free'), and calls upsert_tier with tier='free'. The upsert_tier call uses SQL_UPSERT_TIER which always writes subscription_state='active'. So after a cancellation, the container materializer leaves tier_selections.tier='free' AND subscription_state='active'. The tier_select.rs has_active_subscription check at tier_select_store.rs:187-197 queries WHERE subscription_state='active', which means a freshly-canceled tenant who has been downgraded to 'free' by the container materializer CANNOT re-subscribe via tier-select because AlreadyActive is triggered. The signup-worker does correctly set subscription_state='inactive' via deactivateTierSelectionByCustomer, but there is a window between the container materializer writing 'active' for the free tier and the signup-worker writing 'inactive', during which the tenant cannot re-subscribe.

**Reproduction:** 1. Have a Pro tenant cancel their Stripe subscription. 2. Before the signup-worker webhook fires, observe tier_selections: tier='free', subscription_state='active'. 3. Attempt POST /v1/onboarding/tier-select for the Solo tier. 4. Observe 409 with code 'already_active'. 5. Wait for signup-worker webhook to fire (or manually set subscription_state='inactive'), then retry — succeeds.

**Impact:** Canceled tenants who try to re-subscribe immediately after cancellation may be denied by the AlreadyActive gate during the window between the container materializer writing the 'free/active' row and the signup-worker setting 'inactive'. This degrades the re-subscribe user experience and may drive churn. It also creates a state (free tier, subscription_state=active) that is semantically contradictory: free tier should be perpetually available and does not need an 'active' subscription_state.

**Fix:** The container materializer should NOT call persist_tier_change (which writes subscription_state='active') on a subscription.deleted event. The responsibility for updating tier_selections.subscription_state to 'inactive' belongs to the signup-worker (the documented authority). On subscription.deleted, the container materializer should only: (1) call mark_subscription_canceled to update stripe_subscriptions, and (2) optionally update tenant.tier for denormalization WITHOUT touching subscription_state in tier_selections. Alternatively, add a separate SQL constant SQL_DOWNGRADE_TIER that updates tier='free' and subscription_state='inactive' atomically, and use it instead of upsert_tier (which forces 'active') in the canceled=true path.

**Verify verdict:** confirmed — The finding is confirmed by independent code reading. The exploit chain is reachable and the evidence is exact.

**Confirmed path:**

1. `on_subscription_deleted` → `do_subscription_upsert(env, canceled=true)` at `handler.rs:706`
2. The `else if canceled` branch at `handler.rs:369-374` calls `self.persist_tier_change(&tenant_id, TierKind::Free, env, now_ms)`
3. `persist_tier_change` (lines 481-518) reads the current tier, sees it changed (e.g., 'pro' → 'free'), then calls `self.d1.upsert_tier(te

---

## Low/info (recorded, not launch-blocking)

- **[low]** Native CAS/AC/Bazel/Turbo plane has no per-request secret binding — the truncated 128-bit HMAC signature is the SOLE pos
- **[low]** checkout.session.completed grants entitlement from server-set metadata[tier] with no price cross-check on that event; a 
- **[low]** `sam`/`apac` macro regions are routable but point at the US default bucket — a tenant labelled sam mis-lands in US R2 (L
- **[low]** Money-path internal-auth gate (tier-select checkout) accepts a 16-char shared secret while every other privileged plane 
- **[low]** Session-exchange in-memory mint backstop never resets → permanent 429 self-DoS of a legitimate principal within an isola
- **[low]** signup.rs: JSON body deserialized before rate-limit and HMAC-verify gates (M3 violation)
- **[low]** public_attestation.rs: unauthenticated GET endpoints with no rate limiting
- **[low]** ratelimit_layer.rs: OCI plane (/v2/* + /token) per-tenant gate fail-opens for all unauthenticated traffic
- **[low]** dsr.rs: internal_auth_ok duplicated instead of shared from internal_pat — divergence risk
- **[low]** auth_rotate Allows Rotation of 'admin'-Scoped PATs — Privilege Amplification via Mint Throttle Bypass
- **[low]** Sentry Scrub Pattern Misses Non-Standard Corelink PAT Prefix Variants
- **[low]** PatError::HashError silently collapsed to HTTP 401 — D1 row corruption is indistinguishable from wrong credentials
- **[low]** OCI token key name drift — legacy HUGR_OCI_TOKEN_KEY fallback still active in production code, creating a dual-key confu
- **[low]** PAT mint endpoint response includes Argon2id PHC hash — hash exfiltration enables offline cracking of the 32-byte random
- **[low]** Webhook idempotency commit-before-materialize creates a permanent event-drop window when DLQ is not wired
- **[low]** map_billing_status collapses 'incomplete' (pending first payment) onto 'past_due', misleading customers with a false pay
- **[low]** DEBUG derive on MintResponse with plaintext PAT token
- **[low]** Production Stripe Webhook Secret Exposed in Git History
- **[info]** Tenant-isolation surface is hardened — no exploitable cross-tenant read/write path found (assurance note)
- **[info]** Moat CAS write path uses a self-derived digest, making the container's content-hash gate a tautology for _public/adapter
- **[info]** Global 10 MiB DefaultBodyLimit is silently NOT applied to internal routers merged after the .layer() call (mitigated to 
- **[info]** customer.rs: handle_keys_create and handle_team_invite use Json body extractor before auth gate (M3 violation)
- **[info]** internal_pat.rs: Argon2id PHC hash returned alongside token plaintext in mint response (M7 deferred issue)
- **[info]** customer.rs: handle_usage exposes quota/usage telemetry without billing_pii_gate — scope inconsistency
- **[info]** ratelimit_layer.rs SENTINEL set is a subset of auth_tenant::SENTINELS — _oci and _public pass through rate-limiter
- **[info]** Data-Residency Region Leaked in Error Message Body
- **[info]** Dummy Argon2id timing pad uses a truncated PHC string that causes immediate parse failure — timing parity is lost on ent
- **[info]** Non-constant-time tenant comparison on NativePatGate cache hit path — informational, tenant IDs are not secrets
- **[info]** Migration 0064 trigger_primary_region_valid_insert duplicates the CHECK constraint already present in tenant_new, creati
- **[info]** Stale PyO3 Advisories (RUSTSEC-2026-0176, 0177) — Waiver Condition Stale
- **[info]** postgres-protocol DoS Advisories (RUSTSEC-2026-0178, 0179, 0180) — Stale Waiver, Revisit Annually
