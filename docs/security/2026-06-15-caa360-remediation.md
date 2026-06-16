# CAA-360 audit remediation — 33 confirmed findings (2026-06-15)

Source: CAA-360 workflow wf_59279b63-57b (16 agents; 50 raw → 33 confirmed / 17 refuted).
By severity: 1 HIGH, 7 MEDIUM, 16 LOW, 9 INFO. Owner directive: fix ALL, no exceptions, SOTA rigor.
Status legend: [ ] todo · [~] in-progress · [x] done (PR#).

## AUTH/PAT (high-care, mostly worker/index.ts + internal_pat + pat crate)
- [ ] **#1 HIGH** (REV-O2) — Single shared CORELINK_INTERNAL_AUTH_KEY concentrates five high-privilege internal surfaces (PAT-mint-for-any-tenant, GDPR erase, CAS erase, admin, pilots) behind one secret — no privilege separation
    - loc: `crates/corelink-container/src/main.rs:304-365 (mount sites) + crates/corelink-container/src/routes/admin.rs:380 internal_auth_key_from_env + worker/src/index.ts:559-564 (documented F4 follow-up)`
    - fix: Implement the in-code F4 plan: (1) Split the single key into per-consumer secrets — e.g. CORELINK_PAT_MINT_AUTH_KEY (signup-worker only), CORELINK_ADMIN_AUTH_KEY (operator admin/pilots), CORELINK_ERASE_AUTH_KEY (DSR/CAS erase) — each gate reads only its own key so a leak of one does not grant the others. (2) Add per-tenant authorization / an allow-scope on `/_internal/pat/mint` so a caller cannot 
- [ ] **#2 MEDIUM** (PEN-2) — Native data-plane (CAS/AC/Bazel/Turbo) proves possession with HMAC only — the per-token Argon2id secret is never verified; a leak of the single PAT_SIGNING_KEY makes every tenant's PAT forgeable
    - loc: `worker/src/index.ts:759-798 (extractAuth HMAC-only gate) + worker/src/index.ts:2076-2082 (DO forward comment); crates/corelink-container/src/routes/internal_pat.rs:20-27 (F3 decision)`
    - fix: Treat PAT_SIGNING_KEY as a single point of failure and add the documented Option-B backstop: wire crates/corelink-container/src/adapter_pat::PatVerifier (which DOES Argon2id-verify random_secret against pat_hash, as already used by the adapter routes and auth_introspect.rs:454) onto the native CAS/AC/Bazel/Turbo plane as a container-side second possession layer. To keep the hot path fast, cache th
- [ ] **#3 MEDIUM** (PEN-2) — /_internal/pat/mint is internet-reachable and mints an ANY-tenant, full-admin PAT behind one shared secret with no per-tenant authorization
    - loc: `crates/corelink-container/src/routes/internal_pat.rs:259-371 (handle_mint) + worker/src/index.ts:1358-1419 (internal route forward) + internal_pat.rs:8-18 (F4 warning)`
    - fix: Implement the three documented hardenings: (1) restrict /_internal/pat/mint to a Worker-to-Worker Service Binding so it has NO public route (remove it from the public matchRoute surface); (2) use a per-consumer secret for the mint, distinct from the secret that gates token-exchange/tenant-lookup/admin, so a signup-worker leak cannot also exercise the admin control plane (the fabric introspect path
- [ ] **#12 LOW** (REV-S1) — PAT Mint Response Exposes Argon2id Verifier Hash to Internal-Auth-Cleared Callers
    - loc: `internal_pat.rs:362-370`
    - fix: Remove the `hash` field from `MintResponse` and from the public response. Instead, have the container write the hash directly to D1 as part of the mint operation (the container already has D1 access via the D1AdminHandler pattern). The caller receives only `token_plaintext`, `pat_id`, `token_id`, and `expires_ms`. The verifier never leaves the container. If the caller must own the D1 write for arc
- [ ] **#15 LOW** (REV-S1) — PAT Mint Endpoint Internet-Reachable with Single Shared Secret (No Service Binding Restriction)
    - loc: `internal_pat.rs:1-25 (route mount), main.rs (conditional mount block)`
    - fix: Short term: Introduce a separate dedicated secret for the mint endpoint (`CORELINK_PAT_MINT_KEY`) distinct from `CORELINK_INTERNAL_AUTH_KEY`, so key rotation for mint does not affect admin/DSR and vice versa. Medium term: Restrict the `/_internal/pat/mint` route to Cloudflare service bindings only (not public internet) by using CF Access or a network policy on the container's inbound traffic. Long
- [ ] **#27 INFO** (REV-S2) — internal_auth.ts requireInternalAuth: timing-safe compare uses ctEqStr with early-return length oracle
    - loc: `worker/src/lib/internal_auth.ts:40-43`
    - fix: Replace `ctEqStr` in `requireInternalAuth` with the same padded `timingSafeEqual` approach used in `index.ts` for the `/_internal/*` gate. Concretely:
```typescript
export function requireInternalAuth(request: Request, env: Env, requestId: string): Response | null {
  const expected = env.CORELINK_INTERNAL_AUTH_KEY;
  if (!expected || expected.length < MIN_INTERNAL_AUTH_KEY_LEN) {
    return reapi
- [ ] **#29 INFO** (REV-S2) — extractAuth docstring specifies SQL-level expiry filter but actual query omits it — documentation drift creates false assurance
    - loc: `worker/src/index.ts:660-663, 814`
    - fix: Update the docstring to exactly match the actual query. Specifically: (1) Remove `pat_hash` from the documented column list; (2) Add `scope` to the documented column list; (3) Replace 'AND expires_ms > now_ms' with 'AND revoked_at_ms IS NULL' to match the SQL, and note that expiry is checked in application code after the fetch. The corrected docstring step 4 should read: 'D1 lookup by token_id — S
- [ ] **#30 INFO** (REV-S2) — CLERK_ISSUER_URL exact-pin is optional and not enforced — issuer verification falls back to weak shape-check in production
    - loc: `worker/src/lib/clerk_auth.ts:119-175`
    - fix: Set `CLERK_ISSUER_URL` as a required production secret immediately. Add it to the secrets checklist (`scripts/secrets-checklist-verify.sh`) and the secrets matrix (`scripts/validate_secrets_matrix.py`) as `code_and_secret` (not `code_only`). Consider making the Worker fail-closed (return 403) if `CLERK_ISSUER_URL` is absent in production environments (`ENVIRONMENT === 'production'`), forcing the o
- [ ] **#31 INFO** (REV-S3) — Cargo adapter: the F27 two-layer write check performs a second full Argon2id PAT verification on every PUT — doubled D1 + Argon2id latency for cache writes
    - loc: `crates/corelink-container/src/routes/cargo.rs:225-268`
    - fix: Thread the capability result from the gate layer into the adapter's request context (e.g. via a request extension) so the adapter can consume the already-verified tenant+capability without re-running Argon2id. Axum's `Extension` mechanism can carry the resolved `ResolvedTenant` from the gate middleware to the handler. This would make the 'one PAT verification' claim accurate and halve the write la
- [ ] **#33 INFO** (REV-S4) — Argon2id verify uses PHC-embedded params rather than global hasher params — correct but worth explicit audit confirmation
    - loc: `crates/corelink-pat/src/argon.rs:128-171`
    - fix: No change required. Document this pattern in the crate-level auth_model reference as a confirmed defense against PHC substitution attacks.

## QUOTA/BILLING TOCTOU (tenant_quota.rs + quota.ts + session_exchange)
- [ ] **#5 MEDIUM** (REV-O3) — Per-tenant $-ceiling TOCTOU over-admission on the fresh-row / cycle-roll path (F13 fix only covered the steady-state branch)
    - loc: `crates/corelink-container/src/tenant_quota.rs:352-380 (QuotaGuard::check, the `rolling` branch)`
    - fix: Make the roll/fresh path atomic too, instead of a read-decide-absolute-put. Option A (preferred): seed the row idempotently and then route through the SAME atomic `check_and_accrue`. e.g. on `rolling`, first issue an atomic conditional reset that only rolls if the stored anchor is still stale: `UPDATE tenant_quota SET accrued_usd_micros = 0, cycle_anchor_ms = ?now WHERE tenant_id = ?1 AND cycle_an
- [ ] **#14 LOW** (REV-S1) — FindMissingBlobs Batch Quota Cap Allows 64x Quota Dilution for Large Batches
    - loc: `bazel_v2.rs:288, bazel_v2.rs:299`
    - fix: Replace the sequential per-unit D1 check with a single bulk quota reservation call that deducts `min(n, MAX_BATCH)` units in one D1 transaction. This avoids the thundering-herd problem (single D1 write instead of N writes) while correctly charging for the actual batch size. Define `MAX_BATCH = 4096` to match the REAPI spec. If a single-call quota API is not yet available, at minimum raise `BATCH_Q
- [ ] **#16 LOW** (REV-S2) — Module-level _inMemoryMintCounts map grows without bound and persists beyond the D1 throttle window
    - loc: `worker/src/lib/session_exchange.ts:110, 193-205`
    - fix: Two changes: (1) Add a maximum map size guard to prevent unbounded growth: if `_inMemoryMintCounts.size > 50_000`, evict the oldest entries (use a FIFO insertion-order Map or an LRU). (2) Consider applying the in-memory backstop only on D1 failure paths (move the in-memory increment/check inside the `d1Failed` branch), so normal D1-healthy operation is governed solely by the D1 policy cap. This br
- [ ] **#18 LOW** (REV-S3) — Quota gate on Bazel findMissingBlobs is capped at 64 iterations — batches over 64 digests are under-charged and bypass proportional cost enforcement
    - loc: `crates/corelink-container/src/routes/bazel_v2.rs:288-305`
    - fix: Implement `QuotaGuard::check_batch(tenant, n)` as noted in the existing code comment, which issues a single D1 statement charging `n × cost` in one round-trip. This eliminates both the D1 round-trip overhead that motivated the cap AND the quota enforcement gap. Until the batch method is available, raise BATCH_QUOTA_ITERS_CAP to match FIND_MISSING_BLOB_CAP (4096) and accept the D1 latency cost for 
- [ ] **#20 LOW** (REV-S5) — Cycle-Roll Path in tenant_quota Uses Non-Atomic Absolute Write (TOCTOU Over-Spend)
    - loc: `crates/corelink-container/src/tenant_quota.rs:354-376`
    - fix: Replace the two-step read-check-write with an atomic upsert that handles both the fresh-row/cycle-roll case and the steady-state case. One approach: use a single `INSERT INTO tenant_quota ... ON CONFLICT DO UPDATE SET accrued_usd_micros = CASE WHEN <cycle_elapsed_or_new_row> THEN ?cost ELSE accrued_usd_micros + ?cost END ... RETURNING accrued_usd_micros` and then compare the returned value against
- [ ] **#22 LOW** (REV-S5) — checkout.session.completed Does Not Propagate subscription_data.metadata[tier] — tier Absent from subscription Events
    - loc: `crates/corelink-stripe-real/src/client.rs:673-684`
    - fix: Add `subscription_data[metadata][tier]` and `subscription_data[metadata][tenant_id]` to the checkout session creation form in `create_checkout_session_raw`. This propagates the tier to the subscription object so `customer.subscription.created/updated` events can read it from `metadata[tier]` directly, making `tierFromMetadata` the primary signal and reducing reliance on the env-var-dependent price
- [ ] **#25 INFO** (PEN-2) — Storage-quota enforcement fails OPEN on a D1 error in the data-plane forward path
    - loc: `worker/src/index.ts:1865-1901 (quota check) + the documented fail-open posture at index.ts:1871-1873`
    - fix: If storage cost-control is a launch concern, make the storage-quota gate fail CLOSED (429 Retry-After) on D1 errors for write/PUT verbs specifically, matching the residency gate's fail-closed posture, while keeping reads available; or assert (and test) that the container CAS quota FSM independently caps storage on the mutation path so the edge fail-open is provably redundant. At minimum, emit a CR

## DSR/RESIDENCY (dsr/adapter_d1 + residency)
- [ ] **#4 MEDIUM** (REV-O1) — DSR/GDPR erasure incomplete: `survey_responses` (tenant-linked NPS/CSAT PII incl. recipient_hash/ip_hash) is not in the DSR D1 erase-set and not in the documented retain-set
    - loc: `crates/corelink-container/src/routes/dsr/adapter_d1.rs:43-66 (TENANT_ID_TABLES / NAMESPACE_TABLES) vs migrations/d1/0046_survey_responses.sql`
    - fix: Add `"survey_responses"` to `TENANT_ID_TABLES` in adapter_d1.rs (it is `tenant_id`-keyed, so `DELETE … WHERE tenant_id = ?1` is exact), and it will then be covered by both the erase loop and the `remaining_rows` verify sweep. If product/legal instead decides survey data is retained for analytics, add it to `RETAIN_SET` with an explicit ADR reason — but it MUST be in one set or the other. Additiona
- [ ] **#10 LOW** (REV-O1) — Schrems-II / GDPR Art. 44 residency gap: EU tenant CAS bytes physically stored in the single US-located `corelink-cas-prod` bucket (R2_CAS_REGION only re-keys, does not relocate)
    - loc: `wrangler.toml:719,726-727 (env.prod-lhr) and crates/corelink-container/src/routes/cas.rs:230-242`
    - fix: Provision real per-region CAS buckets (`corelink-cas-lhr`, `corelink-cas-sam`, …) with EU/region location hints, then set `R2_CAS_BUCKET` per `[env.prod-<region>]` (e.g. `R2_CAS_BUCKET = "corelink-cas-lhr"` in prod-lhr) so EU CAS bytes physically land in the EU — mirroring how AC already uses per-region buckets. Migrate existing EU-keyed objects (`lhr/…` prefix in `corelink-cas-prod`) into the new
- [ ] **#11 LOW** (REV-O1) — DSR/GDPR erasure: `tenant_quota` (tenant-keyed spend ledger written by the live container) omitted from BOTH the erase-set and the documented retain-set
    - loc: `crates/corelink-container/src/routes/dsr/adapter_d1.rs:43-85 vs migrations/d1/0068*-tenant_quota + crates/corelink-container/src/tenant_quota.rs`
    - fix: Add `"tenant_quota"` to `TENANT_ID_TABLES` (exact `WHERE tenant_id = ?1` delete) so it is both erased and verified; OR, if it is deliberately retained, add it to `RETAIN_SET` with an ADR rationale. Pair with the same `migrations/d1/*.sql`-enumeration completeness guard test recommended for the survey gap so every tenant-keyed table is provably in exactly one of the two sets.

## STORAGE/HANDLERS (cas/ac/turbo/oci + migrations)
- [ ] **#6 MEDIUM** (REV-S1) — Pervasive Zero Audit Timestamps: now_ms = 0u64 Across All CAS, AC, Bazel, and Turbo Handlers
    - loc: `cas.rs:395, cas.rs:443, cas.rs:488, cas.rs:527; bazel_v2.rs:311-313`
    - fix: Introduce a `WallClock` collaborator (matching the admin_pilot pattern) into `CasRouteState`, `BazelRouteState`, and `TurboRouteState`. Replace all `let now_ms = 0u64;` and `const fn now_ms() -> u64 { 0u64 }` with `state.wall_clock.now_ms()`. The `WallClock` trait already exists (used in admin_pilot); it is a one-liner to add to each route state struct and wire in `build_handlers`. In tests, injec
- [ ] **#7 MEDIUM** (REV-S1) — Turbo R2 Build Failure Silently Degrades to Non-Durable InMemoryKvStore
    - loc: `turbo_v8.rs:223-231`
    - fix: Mirror the CAS pattern: introduce an `UnavailableTurboHandler` that returns HTTP 503 with a `{"error":"storage_unavailable"}` body for every request when R2KvStore build fails. Change the `Some(Err(e))` arm in `build_handlers()` to construct and return this unavailable handler instead of falling through to InMemory. The InMemory path should be reserved exclusively for the `None` case (no credentia
- [ ] **#9 LOW** (PEN-3) — AC action_digest (and CAS hash) path segment used as R2 object-key component with no charset/length validation
    - loc: `crates/corelink-container/src/routes/ac.rs:360-394 (handle_update) + crates/corelink-container/src/storage/r2_s3.rs:1031-1034 (R2AcHandler::r2_key) + :314-316 (blob_key)`
    - fix: Add a strict digest-format gate on the native AC route (and as defense-in-depth on the CAS route) BEFORE the value is used as a storage key. Concretely, in ac.rs handle_lookup/handle_update/handle_delete (and cas.rs handlers), validate action_digest/hash against the canonical digest grammar and 400 on failure, e.g.: `fn is_canonical_digest(s: &str) -> bool { s.len() == 64 && s.bytes().all(|b| b.is
- [ ] **#13 LOW** (REV-S1) — D1 Mirror Called After InMemory Commit in Admin Mutate: MutateCommitted Emitted Before Durable Write Succeeds
    - loc: `admin.rs:346-366`
    - fix: Invert the commit order: (1) Attempt the D1 write first. (2) Only if D1 write succeeds, call `self.inner.mutate(req)` to apply in-memory state and emit `MutateCommitted`. If the D1 write fails, return the error without touching in-memory state or emitting any 'committed' audit event. Alternatively, implement the 'outbox flow' referenced in the comment — a transactional outbox pattern where the D1 
- [ ] **#19 LOW** (REV-S3) — OCI: inflight_bytes ceiling check is non-atomic — race allows slight overrun of the 512 MiB cross-tenant cap
    - loc: `crates/corelink-container/src/routes/oci.rs:322-362`
    - fix: Use a `compare_exchange` loop on the AtomicU64 to atomically check-and-increment: attempt to CAS from `current` to `current + chunk_len` only if `current + chunk_len <= ceiling`; on CAS failure (another thread modified the value), retry the load. This makes the ceiling check-and-increment atomic without requiring additional locking. Alternatively, move the ceiling check INSIDE the session mutex lo
- [ ] **#21 LOW** (REV-S5) — Missing FOREIGN KEY Constraint on tenant_quota and cas_tombstone (No Referential Integrity)
    - loc: `migrations/d1/0066_tenant_quota.sql:45-70, migrations/d1/0067_cas_tombstone.sql:25-38`
    - fix: Add explicit FK declarations to both tables: `FOREIGN KEY (tenant_id) REFERENCES tenant(tenant_id)` (with `PRAGMA foreign_keys = ON` at connection time, or at minimum document that D1 does not enforce FKs and compensate in application code). More critically, add explicit DELETE steps for `tenant_quota` and `cas_tombstone` in the DSR erasure pipeline (`crates/corelink-container/src/routes/dsr/adapt

## CONFIG/HYGIENE (env/secrets/docs/dead-code)
- [ ] **#8 MEDIUM** (REV-S4) — OCI token key env var name drift — HUGR_OCI_TOKEN_KEY in prod vs CORELINK_OCI_TOKEN_KEY in code
    - loc: `crates/corelink-container/src/routes/oci.rs:140`
    - fix: Rename the Cloudflare production secret from `HUGR_OCI_TOKEN_KEY` to `CORELINK_OCI_TOKEN_KEY` in all five deployed environments (the five-env deployment matrix referenced in recent commits). Verify the OCI route mounts successfully after the rename by checking startup logs. The code constant requires no change; this is purely an operator action. Track it in task #46 (the existing launch operator g
- [ ] **#17 LOW** (REV-S3) — Pip adapter: wheel URL comes from the cached per-tenant index KV — a poisoned cached index can trigger fetches to any URL passing the two-host SSRF allowlist
    - loc: `crates/corelink-adapter-host/src/pip/wheel.rs:133-145, pip/index.rs:164-167`
    - fix: When resolving a wheel URL on a cache miss, re-fetch the PEP 691 index from the upstream origin (bypassing the KV cache) to obtain a fresh, authoritative URL before making the outbound wheel fetch. The TTL-based KV cache should be used only for serving the index to clients, not for resolving outbound fetch URLs on misses. Alternatively, enforce that all wheel URLs extracted from the KV cache must 
- [ ] **#23 LOW** (REV-H3) — Hardcoded Resend API Key in .env.local
    - loc: `.env.local:35`
    - fix: 1. Rotate the Resend API key immediately. 2. Store the key in Cloudflare Workers Secrets, never in .env.local. 3. Add pre-commit hooks to reject commits containing 're_' patterns. 4. Enable IP whitelisting in Resend settings to restrict API calls to CoreLink's Cloudflare Worker IPs only. 5. Use Resend's audit log feature to review all emails sent. 6. Consider using a dedicated 'noreply' email addr
- [ ] **#24 LOW** (REV-H4) — Inaccurate Advisory Waiver: RSA Marvin Timing Sidechannel Justification Contradicts Actual Dependency Graph
    - loc: `deny.toml:40-50 + .cargo/audit.toml:25-32`
    - fix: Correct the waiver justification in both deny.toml and .cargo/audit.toml to accurately reflect that the `rsa` crate is test-only and does NOT appear in production code. Update the comment to: "RUSTSEC-2023-0071 is waived because the `rsa` crate is ONLY pulled via optional features (test-utils in corelink-clerk, dev-dependencies in corelink-dpa-acceptance and corelink-worker) and does not appear in
- [ ] **#26 INFO** (REV-S1) — VALID_TIERS Divergence Between auth_introspect.rs and admin.rs Creates Unresolvable Tenant Tiers
    - loc: `auth_introspect.rs:109-118, admin.rs:461`
    - fix: Either: (A) Add 'team' and 'org' to `TIER_SELECTIONS_TIERS` in admin.rs as valid settable tiers (requires an additive D1 migration confirming the enum column accepts them). Or: (B) Remove 'team' and 'org' from `VALID_TIERS` in auth_introspect.rs and add a migration to remap those values to their canonical equivalents ('pro' and 'max') in D1. Do not simply document the divergence — it creates an in
- [ ] **#28 INFO** (REV-S2) — /_health/container passes through raw non-JSON error bodies to unauthenticated callers
    - loc: `worker/src/index.ts:1340-1342`
    - fix: Replace the raw-body fallback with a safe generic error body:
```typescript
} catch {
  // Non-JSON body: return a safe generic error rather than streaming
  // potentially sensitive raw error content to unauthenticated callers.
  redactedBody = JSON.stringify({ status: 'unavailable' });
  containerHeaders.set('Content-Type', 'application/json');
}
```
Alternatively, gate `/_health/container` behi
- [ ] **#32 INFO** (REV-S4) — dummy_verify_for_constant_time: ct_eq call on mismatched-length slices is dead code — not the timing anchor
    - loc: `crates/corelink-pat/src/argon.rs:217-256`
    - fix: The function is secure as-is. For documentation clarity, amend the comment to state explicitly that the ct_eq call is NOT the timing anchor — the Argon2id invocation is — and that the ct_eq is present only to create a data-flow dependency on `plaintext` to prevent compiler DCE of any future use. Alternatively, replace the dead ct_eq with a `std::hint::black_box(plaintext)` call, which is more hone
