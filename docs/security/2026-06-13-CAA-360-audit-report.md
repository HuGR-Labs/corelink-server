# CAA-360 Adversarial Assurance — CoreLink security/quality audit

**Date:** 2026-06-13 · **Raw:** 49 · **Confirmed:** 37 · **Refuted (FP):** 12

**By severity (confirmed):** {"medium": 9, "low": 16, "info": 11, "high": 1}

---

## Synthesized report

I'll synthesize the final CAA-360 launch-risk report from the 37 confirmed findings. Let me produce the complete Markdown document.

# CAA-360 Final Launch-Risk Report — CoreLink

**Assessment:** CoreLink Adversarial Assurance, 360° (CAA-360)
**Methodology:** PTES lifecycle + OWASP/ASVS depth + MITRE ATT&CK TTPs, with adversarial verification (every finding independently re-read against the real code; 12 candidate findings refuted as false-positives during verification).
**Scope:** Multi-tenant content-addressable cache + storage-governance platform on Cloudflare (edge Worker [TS] → Durable Object → native Rust container → R2 + D1 + KV). Surfaces: native CAS/AC, Bazel REAPI v2, Turborepo, sccache, OCI; Stripe billing; Clerk auth; PAT (HMAC) tenant auth; internal-auth (worker↔container); DSR/GDPR erasure.

---

## 1. Executive Summary

**Is CoreLink launch-safe? — Yes, conditionally.** After a brutal, evidence-anchored 360° sweep, CoreLink has **no confirmed Critical and only one confirmed High** finding, and that High is a *documentation-only* error (a wrong code comment in a migration), not a live, attacker-reachable breach. The adversarial verification round was decisive: several findings that *looked* critical on first pass (arbitrary cross-tenant CAS read, the "any-string-accepted" auth bypass, multi-instance rate-limit fan-out) were **disproven** by the actual code — CAS is content-addressed with read- and write-side hash re-verification, Durable Objects are pinned per-tenant via `idFromName(tenantId)`, and the Worker strips all client-supplied trust headers before re-injecting D1-sourced values. So the steady-state production posture (with all secrets bound and the UUID-tenant model) holds.

**The honest top risks** cluster in three places, and they are the launch gate. **First, the tenant-isolation prefix is keyed on a public, low-entropy value in production today** because the secret `R2_TDK_HEX` is unset (Finding 1, medium) — this is the secret-HMAC tenant-prefix design being switched *off*, leaving a latent same-millisecond co-mingling risk and a poisonable Action Cache. **Second, several production secrets are "optional" and fail *open* silently** rather than fail-closed: `PAT_SIGNING_KEY` (Finding 18) collapses PAT possession to a guessable token_id if unbound, and `ERASURE_SALT_KEY` (Finding 9) makes GDPR pseudonyms re-identifiable if unbound — both are launch-day operator secrets that no gate currently *forces*. **Third, the production Action Cache silently overwrites a proven result with divergent bytes** (Finding 5, medium) — the 409 DivergentBody guard that exists in the in-memory handler was dropped on the R2 path, enabling intra-tenant/intra-org AC cache poisoning. Surrounding these are a cluster of consistent, mechanical defense-in-depth gaps: a single shared internet-reachable internal-auth secret that can mint any-tenant admin PATs (Finding 4), an open-redirect on the Stripe success_url (Findings 6/10), a real GDPR data-residency gap where EU CAS bytes land in the US bucket (Finding 7), and an env-contract drift that would silently no-op the residency fix (Finding 8). A migration (0064) destroys the `tenant.primary_region` enforcement triggers based on a factually-wrong SQLite comment (Findings 33/36) — fixable before that migration reaches prod.

**Bottom line:** none of these are turn-key remote compromises in steady state, but a cluster of them share one root cause — **secrets and invariants that fail open and silently instead of failing closed and loudly.** The pre-launch remediation list below closes that class. Set the four secrets, make them mandatory in the gate, add the AC divergence guard, fix the migration triggers, and lock the success_url allowlist; then launch.

### Counts table

| Severity | Count |
|---|---|
| Critical | 0 |
| High | 1 |
| Medium | 9 |
| Low | 16 |
| Info | 11 |
| **Total confirmed** | **37** |
| (Refuted as false-positive during verification) | 12 |

> Note on Finding 36 (High): this is a *documentation* defect that is the maintenance-twin of Findings 33's operational consequence. Its "High" reflects the criticality of the privacy invariant it endangers; the *exploitable* residual (lost `primary_region` UPDATE-immutability DB backstop) is medium-grade and operator/code-bug-reachable, not unauthenticated-remote. It is treated as fix-before-the-0064-migration-reaches-prod.

---

## 2. Findings

Ordered Critical → High → Medium → Low → Info. There are **no Critical findings.**

---

### [HIGH] F36 — Migration 0064 documents incorrect SQLite trigger semantics, silently destroying `tenant.primary_region` enforcement

**What it is.** CoreLink keeps each tenant's data in its declared jurisdiction by enforcing a `primary_region` column on the `tenant` D1 table. Migration 0028 added three triggers to police this: `trg_tenant_primary_region_required` (reject NULL on insert), `trg_tenant_primary_region_valid_insert` (reject out-of-set regions on insert), and — most importantly — `trg_tenant_primary_region_immutable` (reject any UPDATE that *changes* a tenant's region after creation). Migration 0064 rebuilds the `tenant` table using SQLite's "12-step" pattern (create `tenant_new`, copy rows, `DROP TABLE tenant`, `ALTER TABLE tenant_new RENAME TO tenant`) to widen the `tier` CHECK constraint. Its comment block (lines 78–86) asserts the three triggers "are name-bound to `tenant`, not the physical object… survive the DROP + RENAME swap… We do NOT drop + recreate them." **This claim is factually false.** Per the official SQLite docs (`lang_droptable.html`): "Every trigger associated with a table is removed when the table is dropped." `DROP TABLE tenant` destroys all three triggers; the subsequent rename produces a fresh, trigger-less `tenant` table. No later migration (0065–0068) re-creates them.

**Why it matters / Impact.** The rebuilt `tenant_new` table *does* preserve the inline column constraint `primary_region TEXT NOT NULL CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr'))`, so the NULL-guard and valid-set-on-INSERT enforcement survive at the column level. **But there is no column-level equivalent for the immutability trigger** — only a `BEFORE UPDATE` trigger can prevent a *valid* region value from changing to a *different* valid value. After 0064, a direct `UPDATE tenant SET primary_region = 'wnam' WHERE tenant_id = X` is no longer rejected by D1. The privacy model (`crates/corelink-privacy/src/residency/migration.rs:6-10`) treats `primary_region` as monotonic, mutable only via a dual-approval + 30-day-cooldown ticket path, and the adversarial test `crates/corelink-privacy/tests/residency_region_adversarial.rs:122-136` explicitly relies on "the D1 trigger rejects the UPDATE." The app-layer `TenantCtx` immutability only protects in-memory request objects, not durable writes. So the loss is the **D1-layer backstop for INV-REGION-NO-CROSS-LEAK** — a GDPR/LGPD data-residency invariant the codebase rates CRITICAL. Compounding it, the comment seeds a *recurring* bug class: any future engineer copying the "triggers survive the rebuild" pattern silently drops enforcement again.

**How it's exploited / reproduction.** Not a network-reachable attack — the residual requires direct D1 write access (the `CLOUDFLARE_API_TOKEN` operator path) or a future code bug. Reproduced empirically on sqlite3 3.43.2:
```
CREATE TABLE tenant (id TEXT PRIMARY KEY, primary_region TEXT);
CREATE TRIGGER trg_req BEFORE INSERT ON tenant FOR EACH ROW
  WHEN NEW.primary_region IS NULL
  BEGIN SELECT RAISE(ABORT,'region required'); END;
CREATE TABLE tenant_new (id TEXT PRIMARY KEY, primary_region TEXT);
INSERT INTO tenant_new SELECT * FROM tenant;
DROP TABLE tenant;                       -- destroys trg_req
ALTER TABLE tenant_new RENAME TO tenant; -- does NOT recreate it
SELECT count(*) FROM sqlite_master WHERE type='trigger';   -- => 0
INSERT INTO tenant (id, primary_region) VALUES ('t1', NULL); -- => SUCCEEDS
```
Trigger count is 0 and the NULL insert succeeds. Post-0064, `UPDATE tenant SET primary_region=...` against D1 is no longer aborted.

**Evidence.**
- `migrations/d1/0064_tenant_tier_max.sql:83-86` — the false comment: *"they survive the DROP + RENAME swap on the same connection. We do NOT drop + recreate them … D1 trigger bindings follow the table name."*
- `migrations/d1/0064_tenant_tier_max.sql:210` — `DROP TABLE tenant;`
- `migrations/d1/0064_tenant_tier_max.sql:212` — `ALTER TABLE tenant_new RENAME TO tenant;`
- `migrations/d1/0064_tenant_tier_max.sql:214-261` — all 9 indexes re-created; **zero triggers** re-created.
- `migrations/d1/0028_*.sql:53-59` — `trg_tenant_primary_region_immutable` (no DDL equivalent exists for UPDATE-immutability).
- ADR-0064 §4 repeats the same incorrect claim.
- `scripts/check_migrations_additive.py` — has no awareness of trigger preservation in a DROP+RENAME rebuild.

**CVSS.** Documentation-defect class; the exploitable residual is `AV:L/AC:L/PR:H/UI:N/S:C/C:N/I:H/A:N` ≈ 5.8 (Medium) given it requires privileged/direct-DB or a future code-bug. Reported HIGH to reflect the criticality of the privacy invariant whose DB-layer enforcement is silently lost and the recurring-bug-class risk the false comment seeds.

**Fix.** Re-create all three 0028 triggers verbatim immediately after `ALTER TABLE tenant_new RENAME TO tenant` in 0064 (or in a new additive migration 0069 applied before 0064 reaches prod):
```sql
CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_required
BEFORE INSERT ON tenant FOR EACH ROW WHEN NEW.primary_region IS NULL
BEGIN SELECT RAISE(ABORT,'primary_region is required (WI-S14-002: INV-REGION-NO-CROSS-LEAK)'); END;

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_immutable
BEFORE UPDATE OF primary_region ON tenant FOR EACH ROW
WHEN OLD.primary_region IS NOT NULL AND OLD.primary_region != NEW.primary_region
BEGIN SELECT RAISE(ABORT,'primary_region is immutable post-INSERT'); END;

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_valid_insert
BEFORE INSERT ON tenant FOR EACH ROW
WHEN NEW.primary_region IS NOT NULL
  AND NEW.primary_region NOT IN ('wnam','enam','weur','sam','apac','afr')
BEGIN SELECT RAISE(ABORT,'primary_region must be one of: wnam,enam,weur,sam,apac,afr'); END;
```
Correct the comment in 0064 and ADR-0064 §4 to state that `DROP TABLE` destroys associated triggers and they are explicitly re-created below. Extend `scripts/check_migrations_additive.py`: any migration containing both `DROP TABLE X` and `RENAME TO X` must also contain a `CREATE TRIGGER` for every trigger that existed on `X`.

---

### [MEDIUM] F1 — Cross-tenant CAS/AC blob co-residence via raw-padded 16-char prefix fallback (`R2_TDK_HEX` unset in production)

**What it is.** CoreLink physically isolates each tenant's cache objects in R2 by prefixing every object key with a per-tenant string. The canonical, secure design (`tenant-path::derive_prefix`) is HMAC-SHA256(secret TDK, tenant_uuid) truncated to 16 url-safe base64 chars — an unpredictable, ~96-bit, collision-resistant namespace keyed under a secret. The R2 key is `<region>/<tenant_prefix_16>/<digest>`, and that prefix is the *only* thing keeping tenant A's blobs from sharing storage addresses with tenant B's (the route-layer `tenant == auth.0` checks only stop a caller from *naming* another tenant — they never inspect the physical prefix derived for the caller's own tenant). **The bug:** `R2CasHandler`/`R2AcHandler` only call the secure `derive_prefix` when a Tenant Derivation Key is configured. When the TDK is absent, the code falls into a dev/test branch that takes the tenant string, `truncate(16)`s it and pads to 16 chars — a **public, non-secret prefix of the tenant_id**. The TDK is loaded from `R2_TDK_HEX`, which the repo's own CHANGELOG documents as *currently-unset in production*. Production tenant_ids are hyphenated UUIDv7 lowercase, so the first 16 chars are `tttttttt-tttt-7r` — the full 48-bit creation-millisecond timestamp + the literal version nibble `7` + exactly one random nibble. The whole prefix is therefore a function of (creation millisecond, 4 random bits).

**Why it matters / Impact.** Two tenants created in the same millisecond share 15 of 16 chars and have a 1-in-16 chance of a *full* prefix collision — they then resolve to the **same R2 key space**. For the Action Cache (key = `<region>/<prefix>/<action_digest>`, no content verification on read), a colliding pair read and write each other's build-action results — cross-tenant AC poisoning into the victim's CI. This converts the documented ~2⁻⁹⁶ design target into an effective ~4-bit-entropy same-millisecond collision. The impact is bounded but real: a latent accidental co-mingling risk at scale plus a poisonable AC namespace whenever two same-ms tenants collide. (The adversarial round corrected the original "critical" framing: CAS read/poison is *not* achievable because CAS re-verifies the content hash on both write and read — see "How it's exploited" — so the realistic vector is AC poisoning under a narrow collision, and the attacker cannot target a specific victim because they cannot observe or control the victim's exact creation ms.)

**How it's exploited / reproduction.**
1. Confirm prod state: `R2_TDK_HEX` unset → `R2CasHandler/R2AcHandler.tdk = None` → raw-padded fallback live.
2. Two tenants with UUIDv7 ids sharing the first 16 chars (e.g. `0190abcd-1234-75ab-…` and `0190abcd-1234-75cd-…`) both yield prefix `0190abcd-1234-75`.
3. AC variant (the genuine vector): as tenant A `PUT /v1/ac/A/<action_digest>` (honest bytes), then as tenant B `PUT /v1/ac/B/<action_digest>` (malicious ActionResult). Both resolve to the same R2 key; B overwrites/poisons A's entry, and A's next build consumes B's bytes as a trusted cache hit.
4. CAS read/poison does **not** work: `r2_s3.rs:439` re-verifies the returned bytes hash to the requested digest on read, and the write path verifies bytes hash to the claimed digest — so CAS degrades only to a known-digest existence oracle.

**Evidence.**
- `crates/corelink-container/src/storage/r2_s3.rs:325-333` (CAS, active prod branch): `None => { let mut p = tenant.to_owned(); p.truncate(16); while p.len() < 16 { p.push('0'); } p }` then `R2S3Client::blob_key(&self.cas_region, &prefix, digest)`.
- `r2_s3.rs:680-687` — identical fallback for AC.
- `r2_s3.rs:921-936` — `load_tdk_from_env` returns `None` when `R2_TDK_HEX` empty/absent.
- `CHANGELOG.md:37` — *"(currently-unset, so no-op today) … R2_TDK_HEX"*.
- `worker/src/durable_object.ts:490` — `R2_TDK_HEX: this.env.R2_TDK_HEX ?? ""` (empty → None).
- `crates/corelink-core/src/types/tenant.rs:10` — text form is canonical UUIDv7 hyphenated lowercase.
- Cross-tenant guards that only compare caller==self: `cas.rs:213`, `ac.rs:192`, `r2_s3.rs:722`, `:813`.
- CAS content re-verification on read: `r2_s3.rs:439`. AC returns bytes with no verification: `r2_s3.rs:761-773`.

**CVSS.** `AV:N/AC:H/PR:L/UI:N/S:C/C:H/I:H/A:N` ≈ 8.0 by raw vector, **downgraded to Medium** in the verified verdict: AC:H is real (needs same-ms + 1/16 nibble collision, blind to the attacker), the CAS-read/poison vector is refuted, and only AC poisoning under the narrow collision holds. A legitimate isolation weakness — the secret-HMAC TDK that exists precisely to remove the public/predictable prefix is off in prod.

**Fix.** Make the TDK mandatory on the production storage path. In `build_r2_cas_handler_from_env`/`build_r2_ac_handler_from_env`, when storage credentials are present but `load_tdk_from_env()` returns `None`, **fail closed** — return `Some(Err("R2_TDK_HEX required for production tenant prefixing"))` so the route does not mount. Gate the raw-padded fallback behind `#[cfg(test)]` and always HMAC the full tenant id under the secret TDK via `derive_prefix`. Set `R2_TDK_HEX` (`openssl rand -hex 32`) as a container secret on all five regional deployments before serving real tenant traffic. Enabling the TDK re-keys all objects — plan a migration / accept a cold cache.

---

### [MEDIUM] F4 — Single shared internal-auth secret gates operator-grade `/_internal/pat/mint`, reachable from the public internet — a leak yields any-tenant admin-PAT minting

**What it is.** `/_internal/pat/mint` can mint a PAT for **any** `tenant_id` and **any** scope (including full `admin` = `SCOPE_ADMIN_ALL`), with both values taken straight from the JSON request body and no authorization beyond a single shared-secret header. That secret, `CORELINK_INTERNAL_AUTH_KEY`, also gates the onboarding/tier-select money path and the `internal` Worker arm, and is **shared with the separate signup-worker**. Critically, `/_internal/*` is matched and served by the *public* edge Worker (bound to `corelink-api.humangr.com/*`), gated only by a constant-time compare of the caller-supplied `x-corelink-internal-auth` header against the secret. The compare itself is correct and fail-closed — but the secret is a single, internet-reachable, operator-grade credential. There is no WAF rule, CF Access policy, or IP allowlist on `/_internal/` anywhere in the repo.

**Why it matters / Impact.** Anyone who presents the secret to `https://corelink-api.humangr.com/_internal/pat/mint` can mint a full-admin PAT for an arbitrary tenant UUID and then act as that tenant — full multi-tenant compromise contingent on one secret. Sharing the secret with the lower-trust signup-worker doubles the leak surface (and `apps/signup-worker/src/lib/corelink-internal.ts` performs a plain `fetch()` to the public URL in at least the DSR path, so the secret travels a public HTTP route, not only a Service Binding). This is a "blast radius after compromise" + privilege-concentration finding, not an active zero-day — the precondition is a secret leak (insecure `.env.local` backup, compromised signup-worker, accidental log).

**How it's exploited / reproduction.** Precondition: attacker obtains `CORELINK_INTERNAL_AUTH_KEY`.
1. `curl -s -o /dev/null -w "%{http_code}" https://corelink-api.humangr.com/_internal/pat/mint -X POST` → 401 (confirms route is live and key set).
2. `curl -X POST .../_internal/pat/mint -H 'x-corelink-internal-auth: <secret>' -H 'content-type: application/json' -d '{"tenant_id":"<victim-uuid>","principal_id":"00000000-0000-0000-0000-000000000001","scopes":"admin","ttl_seconds":31536000}'` → `200 {"token_plaintext":"corelink_pat_...", ...}`.
3. Use that PAT as `Authorization: Bearer` against any victim-tenant route. The mint never checks the caller is authorized for that specific `tenant_id`.

**Evidence.**
- `worker/src/index.ts:509-510` — `/_internal/` routes matched (routeKind "internal") on the public Worker.
- `worker/src/index.ts:1194-1235` — sole gate is constant-time compare of `x-corelink-internal-auth` vs `env.CORELINK_INTERNAL_AUTH_KEY`.
- `crates/corelink-container/src/routes/internal_pat.rs:296-297` — `let tenant_id = TenantId(req.tenant_id); let principal_id = PrincipalId(req.principal_id);` (tenant taken verbatim from body).
- `internal_pat.rs:274-275` — `"admin" => PatScopes::from_u64(SCOPE_ADMIN_ALL)`.
- `internal_pat.rs:14-16` — *"The Worker AND signup-worker both carry the same secret."*
- `apps/signup-worker/wrangler.toml:22-24` + `apps/signup-worker/src/lib/corelink-internal.ts:40,65` — shared secret, public-URL `fetch()`.
- No WAF/CF-Access/IP-allowlist on `/_internal/` exists in any config file (grep-confirmed).

**CVSS.** Conditional chain (secret leak → public endpoint → any-tenant admin PAT). Medium — real, high-consequence, but requires the secret-leak precondition; the constant-time check and write-only-secret posture are intact.

**Fix.** (1) Separate secrets per consumer (the `auth_introspect` route already models this with a dedicated `FABRIC_INTROSPECT_AUTH_KEY`) so a leak of one cannot exercise the mint. (2) Strongly prefer **not** exposing `/_internal/pat/mint` through the public edge Worker — restrict it to a Worker-to-Worker Service Binding from the signup-worker (no public route). (3) Add per-tenant authorization to the mint (prove authorization for the specific `tenant_id` via the verified Clerk session) rather than letting the shared secret authorize minting for all tenants. (4) Ensure short-TTL rotation and never log the secret.

---

### [MEDIUM] F5 — Production Action-Cache handler (`R2AcHandler`) silently overwrites a proven AC result with divergent bytes (missing 409 DivergentBody guard → AC poisoning)

**What it is.** The Action Cache maps an `action_digest` (hash of the build *action*, not its result) to a result payload. Because the key is the hash of the action and not the bytes stored, AC bytes are **not** self-verifying. REAPI/Bazel semantics therefore require: once a result is proven and cached for an `action_digest`, it must never be silently replaced with different bytes; a divergent re-PUT must be rejected with 409. The codebase knows this — the in-memory handler returns `AcHandlerError::DivergentBody` (mapped to HTTP 409 in `routes/ac.rs`) and the canonical worker handler implements `UpdateResultMismatch`. **But the only AC handler mounted in production — `R2AcHandler::update` — does not perform the check.** It probes existence purely to set the `durable` boolean, then **unconditionally PUTs** the new bytes, overwriting any existing result. The route's 409 arm is dead code on the prod path.

**Why it matters / Impact.** Anyone able to write a tenant's AC (any `cas:rw`/`read-write` PAT for that tenant — a low-trust CI job token, a contractor's runner, a compromised teammate's token, a stolen build token) can replace a legitimate cached `ActionResult` with a poisoned one. Every subsequent build in that tenant/org hitting the same `action_digest` then receives the attacker's outputs *as if* they were the trusted, reproducible build output — supply-chain compromise of every build that hits the poisoned digest. It spans both the native `/v1/ac/...` route and the Bazel REAPI v2 surface (which is handed the same `ac_update` trait object). It is **not** cross-tenant (the `tenant==caller` checks and per-tenant prefix hold), and the attacker must already hold a write-capable credential — so blast radius is one tenant/org at a time, but the REAPI AC-immutability guarantee the product advertises is silently absent. CAS is unaffected (it verifies content_hash on write and read).

**How it's exploited / reproduction.**
1. Authenticate to the AC route as tenant T with `cas:rw`.
2. `PUT /v1/ac/T/<action_digest>` with honest bytes → 201.
3. `GET /v1/ac/T/<action_digest>` → 200, honest bytes.
4. `PUT /v1/ac/T/<action_digest>` again with *different* (malicious) bytes. Expected (per InMemory/canonical handler + route map): **409 Conflict**, original preserved. Actual (R2AcHandler in prod): **200 OK**, entry overwritten.
5. `GET /v1/ac/T/<action_digest>` → returns the malicious bytes; the victim's next build consumes the attacker's output digests as a cache hit.

**Evidence.**
- `crates/corelink-container/src/storage/r2_s3.rs:857-878` — `let pre_existed = matches!(... self.client.get(&key), Ok(Some(_)));` then unconditional `self.client.put(&key, req.result_payload)`; `pre_existed` only sets the `durable` field.
- Contrast `crates/corelink-handler-ac/src/handler.rs:336-347` — InMemory handler: `if *existing != req.result_payload { return Err(AcHandlerError::DivergentBody { ... }) }` ("NEVER overwrite a proven AC result").
- `crates/corelink-container/src/routes/ac.rs:282-285` — maps `DivergentBody` → 409 (dead code on the R2 path).
- `crates/corelink-container/src/routes/ac.rs:113-150` — R2AcHandler mounted in prod when `StorageEnv::from_env().is_some()`.
- `crates/corelink-container/src/routes.rs:341` — `bazel_v2::build_handlers_from(..., ac_lookup, ac_update)` passes the same trait objects to REAPI.

**CVSS.** `AV:N/AC:L/PR:L/UI:N/S:U/C:N/I:H/A:N` ≈ 5–6 (Medium). Genuine AC-integrity / build-supply-chain break, intra-tenant, gated behind an existing write credential.

**Fix.** Make `R2AcHandler::update` enforce the divergent-body invariant before PUT: read-and-compare; if `Some(existing)` and `existing != req.result_payload`, return `DivergentBody` (no PUT, audit the mismatch); if equal, idempotent no-op (`durable=false`); only PUT when absent (`durable=true`); fail closed (`Internal`) on an ambiguous GET error rather than blind-overwriting:
```rust
let existing = tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)));
match existing {
    Ok(Some(b)) if b != req.result_payload =>
        return Err(AcHandlerError::DivergentBody { tenant: req.tenant, action_digest: req.action_digest }),
    Ok(Some(_)) => return Ok(AcUpdateResponse::new(req.action_digest, false)),
    Ok(None) => { /* PUT, durable=true */ }
    Err(e) => return Err(AcHandlerError::Internal(e)),
}
```
Add a regression test mirroring `update_divergent_body_returns_conflict_and_preserves_original` against `R2AcHandler`. The GET→compare→PUT is non-atomic against concurrent writers — close that with an R2 conditional PUT (`If-None-Match`) or a D1 `ac_meta` row carrying the result hash as the canonical guard.

---

### [MEDIUM] F7 — CAS storage ignores tenant residency: EU/regional tenant blobs are written to a single US (IAD) R2 bucket (Schrems-II / GDPR Art. 44 violation)

**What it is.** CoreLink deploys 5 regional PoPs (sam/iad/lhr/nrt/syd) so each tenant's data can stay in its jurisdiction — each region gets its own AC bucket (`corelink-ac-<region>`) and chunk bucket (`corelink-chunk-<region>`), selected at container start. The **CAS** store (the actual cached file/blob bytes — the largest, most sensitive payload) is the exception: a single global bucket `corelink-cas-prod` physically homed in IAD (US), with the region encoded by a process-global `R2_CAS_REGION` defaulting to the hardcoded `"iad"`. That region is baked into the CAS handler once at boot and used for every tenant's key; the code never reads the tenant's `primary_region` on the CAS write path. `R2_CAS_REGION` is not set in *any* wrangler environment, including the EU env (`[env.prod-lhr].vars` sets `R2_AC_REGION="lhr"`/`R2_CHUNK_REGION="lhr"` but omits CAS), and the file itself documents `corelink-cas-prod` as "IAD-only global bucket, shared by all regions."

**Why it matters / Impact.** An EU customer routed to `lhr.corelink-api.humangr.com` who pushes a cache artifact has the *actual bytes* written to a US bucket under an `iad/` key prefix, while only the lighter AC metadata stays in the EU bucket. CAS content routinely embeds proprietary source, credentials baked into build outputs, and PII in test fixtures — an undisclosed cross-border transfer for a platform that markets regional residency. Residency *is* promised in writing (OBJECTION-HANDLING.md, COMPETITIVE-MATRIX.md's INV-REGION-NO-CROSS-LEAK, CAIQ-V4 DSP-16.1 "Y"), and the nightly verifier `scripts/verify-lgpd-residency.py` fixtures expect `cas-sam`/`cas-weur` buckets that physically do not exist. Blast radius: all CAS content for all non-IAD tenants. This is a compliance/contractual exposure (GDPR Art. 44 / Schrems-II / LGPD), **not** a tenant-isolation or auth break — prefixes remain per-tenant; the residency *guarantee* the architecture exists to provide is defeated.

**How it's exploited / reproduction.** (Compliance path, not an attack.) An EU/BR tenant is routed to corelink-prod-lhr/sam; pushes any CAS/Bazel/Turborepo blob; the container (started without `R2_CAS_REGION`) falls back to `env_or("R2_CAS_REGION","iad")` and keys the object under `cas_region="iad"` in `corelink-cas-prod` (US). Verify by reading `[env.prod-lhr].vars` (no `R2_CAS_REGION`), the DO forward-list (omits it), `cas.rs:126` (default `"iad"`), `r2_s3.rs:335` (single global `self.cas_region`).

**Evidence.**
- `crates/corelink-container/src/routes/cas.rs:125-126` — `env_or("R2_CAS_BUCKET","corelink-cas-prod")` / `env_or("R2_CAS_REGION","iad")`.
- `crates/corelink-container/src/storage/r2_s3.rs:307-335` — `R2S3Client::blob_key(&self.cas_region, &prefix, digest)`; no residency/`primary_region` check.
- `wrangler.toml:558` — *"CAS bucket = corelink-cas-prod (IAD-only global bucket, shared by all regions)"*.
- `wrangler.toml:697-704` — `[env.prod-lhr].vars` sets AC/CHUNK region but **no** `R2_CAS_REGION`; same for sam/nrt/syd.
- No `cas-<region>` bucket defined anywhere (AC has full per-region bindings).

**CVSS.** No CVSS confidentiality/integrity impact *between tenants* (none); regulatory/contractual severity. Medium.

**Fix.** Make CAS residency-aware like AC/chunk. (1) Give each regional env its own CAS region/bucket var (e.g. `R2_CAS_REGION="lhr"`, bucket `corelink-cas-lhr`) **and add `R2_CAS_REGION`/`R2_CAS_BUCKET` to the DO `container.start({env})` forward-list** (see F8). (2) Better, derive the CAS storage region per-write from the tenant's `primary_region` (already in D1 via migration 0028). Until enforced, remove the residency claim from marketing/DPAs for CAS content. Add a residency invariant test that fails if a non-IAD regional env lacks a CAS region binding.

---

### [MEDIUM] F8 — Worker→container env-contract gap: `R2_CAS_REGION` / `R2_CAS_BUCKET` / `R2_AC_BUCKET_PREFIX` / `R2_TURBO_BUCKET` are read by the container but not forwarded by `durable_object.ts`

**What it is.** The Durable Object boots the container and is the **only** channel for process-env to reach it (`container.start({ env: {...} })`). Cross-referencing every `env::var`/`env_or`/`non_empty_env` the container reads against that forward-list reveals four variables the container reads but the Worker does **not** forward: `R2_CAS_REGION`, `R2_CAS_BUCKET`, `R2_AC_BUCKET_PREFIX`, `R2_TURBO_BUCKET`. The forward-list even carries a comment claiming the 2026-06-13 audit "completed the env contract … every var the container reads via `env::var` MUST be forwarded" — yet these four are missing. Because `env_or()` treats an absent var as "use the default," any operator override is silently dropped with no error — the exact `ERASURE_SALT_KEY`-class incident the comment references.

**Why it matters / Impact.** Latent today (the defaults happen to match the intended prod values: `corelink-cas-prod` / `iad` / `corelink-ac-` / `corelink-turbo-prod`). The damage is **future**: any operator override of CAS region/bucket, AC prefix, or turbo bucket silently no-ops. The highest-impact case is the residency fix for F7 — an operator who sets `R2_CAS_REGION` to redirect EU CAS writes deploys successfully, sees no error, and believes residency is fixed while the container keeps using `"iad"`. That turns a *known* compliance gap into an *unknown* one masked by an apparently-applied fix. It also undermines the "env contract is complete" claim a reviewer might trust.

**How it's exploited / reproduction.** `grep -n "R2_CAS_REGION\|R2_CAS_BUCKET\|R2_AC_BUCKET_PREFIX\|R2_TURBO_BUCKET" worker/src/durable_object.ts` → no output. `grep -n 'R2_CAS_REGION\|R2_CAS_BUCKET' crates/corelink-container/src/routes/cas.rs` → lines 125-126. Behaviorally: set `R2_CAS_REGION` in a regional env, deploy, exec into the container, `printenv R2_CAS_REGION` → empty.

**Evidence.**
- `worker/src/durable_object.ts:444-506` — forward-list; comment at 484-489 claims completeness; the four keys are absent.
- `crates/corelink-container/src/routes/cas.rs:125-126` — reads `R2_CAS_BUCKET`/`R2_CAS_REGION`.
- `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:68` — reads `R2_AC_BUCKET_PREFIX`.
- `crates/corelink-container/src/storage/r2_kv.rs:178` — reads `R2_TURBO_BUCKET`.

**CVSS.** n/a (operational-integrity / latent-misconfig); Medium as a force-multiplier on the F7 compliance gap.

**Fix.** Add the missing keys to the `container.start({env})` object: `R2_CAS_REGION: this.env.R2_CAS_REGION ?? "", R2_CAS_BUCKET: this.env.R2_CAS_BUCKET ?? "", R2_AC_BUCKET_PREFIX: this.env.R2_AC_BUCKET_PREFIX ?? "", R2_TURBO_BUCKET: this.env.R2_TURBO_BUCKET ?? "",`. Then *mechanize* the contract: a CI check that greps every container `env::var`/`env_or`/`non_empty_env` name and asserts it appears in the forward-list — exactly what the audit comment promised but never enforced.

---

### [MEDIUM] F9 — DSR erasure pseudonymization salt falls back to a predictable non-secret value when `ERASURE_SALT_KEY` is unset, enabling re-identification of the Stripe pseudonym

**What it is.** When a GDPR erasure runs, the surviving Stripe-side record is a pseudonym: the customer email becomes `erased-<hash>@deleted.invalid`, where `hash = SHA-256(subject_id || erasure_salt)`. The unlinkability of that pseudonym depends entirely on the secrecy of `erasure_salt`. The salt is produced by `deriveErasureSalt(dsr_id, ERASURE_SALT_KEY)`: with the secret it is `HMAC-SHA256(key, dsr_id)` (unlinkable); **without it, it falls back to `SHA-256("erasure-salt:" + dsr_id)`** — and `dsr_id` is itself a deterministic public function of the Clerk user id. So with no secret, the salt — and the whole Stripe pseudonym — is recomputable by anyone who knows the (non-secret) Clerk user id and `subject_id` (= tenant_id). `ERASURE_SALT_KEY` is an unprovisioned launch-day operator secret, so the predictable-salt fallback is the **live behavior at launch**.

**Why it matters / Impact.** GDPR pseudonymization (Art. 4(5), Recital 26) must be cryptographically unlinkable. An adversary who obtains the redacted Stripe customer dump (breach at Stripe, rogue insider, legal process) and knows/guesses a target's Clerk user id can recompute `erased-<hash>@deleted.invalid` offline and confirm "this person was a CoreLink customer and was erased" — exactly the re-identification the salt exists to prevent, a breach-notification-grade privacy harm. Scope is capped at the Stripe pseudonym because D1 hard-deletes the primary PII rows — hence medium, not high.

**How it's exploited / reproduction.** Pure offline computation once the Stripe dump is in hand: `dsr_id = UUID(SHA-256("corelink-dsr-v1:" + clerk_user_id))`; `salt = SHA-256("erasure-salt:" + dsr_id)`; `marker = SHA-256(subject_bytes || salt)`; `pseudo_email = "erased-" + marker[0:16] + "@deleted.invalid"`; match against the dump.

**Evidence.**
- `apps/signup-worker/src/webhooks/clerk.ts:153-157` — the no-secret fallback `SHA-256("erasure-salt:" + dsrId)`.
- `clerk.ts:121-130` — `dsr_id = SHA-256("corelink-dsr-v1:" + clerk_user_id)` (deterministic, public).
- `crates/corelink-container/src/routes/dsr/adapter_stripe.rs:73-82` — `SHA-256(subject_id || erasure_salt)` → `erased-{short}@deleted.invalid`.
- `worker/src/durable_object.ts:476-479` — comment: "If absent the container falls back to a PREDICTABLE non-secret salt."
- `ERASURE_SALT_KEY` has zero hits in `docs/internal/secrets-checklist.md` — no gate.

**CVSS.** `AV:N/AC:L/PR:H/UI:N/S:U/C:L/I:N/A:N` adjacent (requires a secondary breach of the Stripe dump); Medium given the compliance-defect nature.

**Fix.** (1) Make `ERASURE_SALT_KEY` a *required* prod secret — add it to `docs/internal/secrets-checklist.md` and `scripts/secrets-checklist-verify.sh` so `validate_secrets_matrix.py` enforces it at deploy. (2) Make the fallback fail-closed in prod: in `deriveErasureSalt`, if `key` is absent and `ENVIRONMENT=prod`, throw rather than silently returning the deterministic hash; return 500 so Svix retries (the erasure obligation stays alive while the missing secret is an operator error, not a silent downgrade). (3) Provision a 32+-byte random `ERASURE_SALT_KEY` via `wrangler secret put` before DSR goes live.

---

### [MEDIUM] F18 — HMAC fast-fail gate is silently skipped when `PAT_SIGNING_KEY` is unset

**What it is.** PAT authentication in the Worker's `extractAuth()` includes an HMAC-SHA256 fast-fail (Step 3) verifying the `hmac_sig` segment against `PAT_SIGNING_KEY` — the *primary cryptographic possession proof* (it ensures the caller physically holds the signing key, not just a `token_id` from a partial leak). But the entire check is wrapped in `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0) { ... }`. When the secret is unbound, the guard is false, the HMAC block is **silently skipped**, and execution falls straight to the D1 lookup. `PAT_SIGNING_KEY` is declared `optional` (`PAT_SIGNING_KEY?: string`) with no boot-time validation.

**Why it matters / Impact.** In any environment without `PAT_SIGNING_KEY` (staging, CI, a new regional worker before secrets are provisioned, dev), a structurally-valid PAT with a *known* `token_id` is accepted if that `token_id` exists in D1 and is unexpired — the `random_secret` and `hmac_sig` are never checked. Authentication collapses to "knowing a valid token_id," granting full read-write access to that tenant's CAS/AC/Turbo cache. The code's own comment (lines 647-651) makes the model explicit: "PAT_SIGNING_KEY is bound in prod, so this HMAC-SHA256 check IS the cryptographic possession gate." When the key is absent, that model inverts. In production all 5 regional workers have the key set, so this is a staging/misconfiguration/secret-rotation risk — but the *silent opt-out design* is the defect.

**How it's exploited / reproduction.** In an env where `PAT_SIGNING_KEY` is unset: obtain any live 16-char Crockford-b32 `token_id` (build logs, leaked CI artifact, PR diff); craft `Authorization: Bearer corelink_pat_<TOKEN_ID>.<43 b64url chars>.<22 b64url chars>`; issue any authenticated request. `parsePat()` succeeds, the HMAC block is skipped, the D1 lookup matches by `token_id` (`SELECT tenant_id, expires_ms, scope FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL`), expiry passes, and `extractAuth` returns `ok:true` with the victim's `tenant_id`.

**Evidence.**
- `worker/src/index.ts:663-672` — `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0) { ...verifyPatHmac...; if (!hmacOk) return { ok:false, reason:'invalid_pat_hmac' }; }` — no else, no fail-closed.
- `worker/src/index.ts:70` — `PAT_SIGNING_KEY?: string;` (optional).
- `worker/src/index.ts:688` — D1 lookup verifies only `token_id` existence + non-revocation, not `random_secret`/`hmac_sig`.

**CVSS.** Medium — full cache read/write for any known token_id, but gated behind two preconditions (key unset in the target env *and* a leaked token_id).

**Fix.** Make `PAT_SIGNING_KEY` required and enforced at startup. Remove the `?` from the Env interface. Add a fail-closed guard at the top of `extractAuth()`: `if (!env.PAT_SIGNING_KEY || env.PAT_SIGNING_KEY.length < 32) { return { ok:false, reason:'signing_key_not_configured' }; }`, mapped to HTTP 503 so operators are alerted. Add it to the secrets matrix as a required prod secret for all 5 regional workers.

---

### [MEDIUM] F28 — DSR erasure route accepts a single-character `CORELINK_INTERNAL_AUTH_KEY` (no minimum-length enforcement)

**What it is.** `build_state_from_env()` for the DSR erase/verify route mounts the route as long as `CORELINK_INTERNAL_AUTH_KEY` is non-empty — a single character suffices. Every other route sharing this key (`admin.rs`, `internal_pat.rs`, `admin_pilot.rs`) enforces a 16-char minimum and refuses to mount below it. The DSR route — the *most destructive* internal route (it drives real GDPR erasure across D1, R2 CAS, R2 AC, KV, and Stripe via the Wave-1 adapters wired in #254) — has the *weakest* mount guard.

**Why it matters / Impact.** An operator who accidentally sets a short key (`CORELINK_INTERNAL_AUTH_KEY=x`) during a misconfigured deploy creates a trap: the admin and PAT-mint routes refuse to mount, but the DSR erase/verify routes mount successfully. An attacker with internal-network access (compromised co-tenant process on the CF Containers host, or a malicious Worker handler) who guesses or brute-forces the trivial key (95⁴ ≈ 81M for 4 chars, trivially enumerable with no rate limit on this path) can trigger **irreversible** tenant data erasure for any tenant UUID. The constant-time auth check itself is correct, so the weak key is the whole exposure.

**How it's exploited / reproduction.** Set `CORELINK_INTERNAL_AUTH_KEY=abcd`; boot the container; admin and internal_pat routes warn "too short" and do not mount, but DSR mounts. `POST /_internal/dsr/erase` with `x-corelink-internal-auth: abcd` and a valid `DsrQueuedV1` body passes `internal_auth_ok` (`ct_eq("abcd","abcd")==1`, len 4==4) and runs the Wave-1 erasure adapters against the specified tenant.

**Evidence.**
- `crates/corelink-container/src/routes/dsr.rs:213-217` — `let internal_auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?; if internal_auth_key.is_empty() { return None; }`.
- `crates/corelink-container/src/routes/admin.rs:378` and `internal_pat.rs:399` — `if key.len() < 16 { ...warn...; return None; }`.
- DSR Wave-1 real adapters wired: `dsr.rs:48-52` (adapter_d1, adapter_r2_ac, adapter_r2_cas, adapter_stripe).

**CVSS.** Medium — irreversible data destruction, but gated behind operator misconfiguration + internal-network reach (route is not on the public ingress).

**Fix.** Align with the rest of the codebase — replace `is_empty()` with `len() < 32` (DSR is destructive; match the PAT-signing-key standard), with a warning log and fail-closed:
```rust
if internal_auth_key.len() < 32 {
    tracing::warn!("CORELINK_INTERNAL_AUTH_KEY too short (< 32 chars); /_internal/dsr/* NOT mounted (fail-CLOSED)");
    return None;
}
```
Consider raising all routes sharing this key to a 32-char floor (NIST 128-bit minimum for symmetric auth secrets).

---

### [LOW] F2 — Secure `derive_prefix` path is unreachable when `R2_TDK_HEX` is unset (fail-open, no alarm)

**What it is.** The architectural root-cause of F1. The secure `derive_prefix(tdk, uuid)` lives *only* inside the `Some(tdk)` match arm. The `None` arm (taken whenever `R2_TDK_HEX` is unset — current prod) doesn't even attempt `Uuid::try_parse`; it jumps straight to the raw-padded fallback for every tenant. The posture is binary and brittle: TDK present → safe HMAC prefix; TDK absent → *every* tenant silently degrades to the public-prefix scheme with **no `tracing::error` and no fail-closed**. In-code comments imply the fallback is for test fixtures, but it fires for a perfectly valid production UUID tenant purely because the secret is unset.

**Why it matters / Impact.** Defense-in-depth + observability gap: a missing-secret operational state silently downgrades tenant-isolation prefixing with no signal to the operator. The posture is *inconsistent* — sibling code (`cas_erase.rs:620-626`, DSR `adapter_r2_cas.rs:164`, `adapter_r2_ac.rs:155`) DOES fail closed on a missing TDK; the primary CAS/AC data plane does not. (The verification round refuted the original "same end-state, HIGH" framing: production tenant ids being distinct UUIDs means the *practical* cross-tenant co-residence is the narrow same-ms case of F1, not an arbitrary break — so this entry is low, scoped to the silent-degraded-mode + posture-inconsistency.)

**How it's exploited / reproduction.** Start the container with R2/CF/D1 creds set but `R2_TDK_HEX` unset (documented prod reality): `routes/cas.rs:121` sees `StorageEnv::from_env().is_some()`, builds the handler with `tdk=None`, mounts it logging "R2S3 (real storage)" with **no TDK warning**; `r2_key` uses the raw-padded branch for every request.

**Evidence.**
- `crates/corelink-container/src/storage/r2_s3.rs:308-333` — `Some(tdk)` arm calls `derive_prefix`; `None` arm has no `derive_prefix`, no UUID parse.
- `r2_s3.rs:594-609` / `:901-915` — `build_*_handler_from_env` constructs with `tdk_bytes=None`, no error log.
- Fail-closed siblings: `cas_erase.rs:620-626`, `main.rs:357-368`, `adapter_r2_cas.rs:164`, `adapter_r2_ac.rs:155`.

**CVSS.** n/a; Low (defense-in-depth / observability).

**Fix.** Treat "storage creds present" and "TDK present" as one invariant for the real handler. In both `build_r2_*_handler_from_env`, require the TDK and return `Err` on `None` (route does not mount → fail closed). Collapse `r2_key` to a single path that always HMACs the full tenant id; gate the raw-padded branch behind `#[cfg(test)]` with an explicit fake TDK. Add a startup assertion + structured error log so the degraded mode is loud. (Same code change as F1's fix.)

---

### [LOW] F3 — Native data-plane does not re-verify the PAT in the container; the "Argon2id second layer" comment is false

**What it is.** Native cache/data-plane routes (CAS read/write, AC, `/v1/users/me`, Bazel REAPI v2, Turbo v8, customer portal, audit) determine the acting tenant entirely from the `x-corelink-tenant-id` header the Worker injects. The container's `AuthTenant` extractor only checks that value is non-empty and not a sentinel — **no cryptographic PAT verification**. The sole possession check happens once, at the edge Worker (HMAC fast-fail + D1 existence/expiry); the Worker deliberately skips the Argon2id verify for CPU budget. The Worker's comment (index.ts:1789-1792 and 583-590) asserts "the DO performs the Argon2id + scope verify against the D1 PAT store" — but no such middleware exists. Argon2id is wired only to the cache *adapters* (cargo/brew/npm/pip/oci), not the native plane.

**Why it matters / Impact.** The entire possession guarantee for the highest-traffic surface rests on a single control — the Worker HMAC gate — with no container-side backstop. If `PAT_SIGNING_KEY` were unbound (see F18), possession collapses to knowing a valid token_id. The false comment is itself a hazard: a future change loosening the Worker gate "because the DO re-verifies anyway" would silently remove the only possession control. (Verification: not exploitable in steady state — the container listens only on the DO's internal TCP port 50051, all traffic passes the Worker's HMAC gate when the key is bound; this is a single-layer + false-comment defense-in-depth gap, hence low.)

**How it's exploited / reproduction.** Requires `PAT_SIGNING_KEY` unbound on a Worker (the F18 precondition) + a known token_id. With the key bound, the same forged PAT is rejected at the HMAC gate — confirming the single point of failure.

**Evidence.**
- `crates/corelink-container/src/auth_tenant.rs:25-36` — reads `x-corelink-tenant-id`, checks non-empty/non-sentinel only (no crypto).
- `worker/src/index.ts:663-672` — HMAC skipped when key unbound.
- `worker/src/index.ts:1789-1792` — false "DO performs Argon2id" comment; `routes.rs` wires `adapter_pat::PatVerifier` only to cargo/brew/npm/pip/oci.

**CVSS.** n/a; Low.

**Fix.** (1) Add a container-side middleware/extractor that runs `adapter_pat::PatVerifier::verify` against the raw Authorization header on the native routes and asserts the resolved tenant equals the Worker-supplied `x-corelink-tenant-id` (fail-closed) — the Option-B model already proven for the adapters. (2) Until that lands, fail closed in the Worker when `PAT_SIGNING_KEY` is unbound on a PAT-gated route (F18). (3) Fix the false comment.

---

### [LOW] F6 — Stripe Checkout `success_url` has no destination-domain allowlist (only an `https://` prefix check) — latent open-redirect / session-id exfil

**What it is.** When a paid checkout is created, CoreLink passes a client-influenced `success_url`/`cancel_url` to Stripe (Stripe redirects there post-payment, appending the Checkout session id). The only validation is `starts_with("https://")` — **no allowlist of permitted destination hosts**. In the current deployment the success_url is server-built by the trusted Worker and the route sits behind the DO + internal-auth gate, so a remote attacker cannot trivially supply an arbitrary URL — this is a defense-in-depth gap one mis-wire away from an open redirect + session-id leak (and the money-path-audit-27 memory already flags a success_url allowlist as backlog).

**Why it matters / Impact.** If a future caller, Worker bug, or internal-auth weakness lets a client control `success_url`, an attacker submits `https://attacker.tld/x?s={CHECKOUT_SESSION_ID}`; the check passes (it is https); Stripe redirects the victim post-payment to the attacker, carrying the session id — phishing continuation + session-id exposure. No direct tenant-data/cross-tenant/money impact; bounded by the server-built URL today.

**How it's exploited / reproduction.** Reach `POST /v1/onboarding/tier-select` with `success_url = https://attacker.example/landing?session_id={CHECKOUT_SESSION_ID}` and a paid tier; `authorize_and_validate` accepts it; `StripeCheckoutCreator` forwards it; the resulting Checkout redirects post-payment to the attacker domain. Pre-condition: the caller can influence `success_url`.

**Evidence.** `crates/corelink-container/src/routes/tier_select.rs:344-348`:
```rust
if tier.is_paid()
    && (!body.success_url.starts_with("https://") || !body.cancel_url.starts_with("https://"))
{ return Err(TierSelectHttpError::BadRequest); }
```

**CVSS.** Low (exploitation requires success_url to become client-controllable).

**Fix.** Constrain `success_url`/`cancel_url` to an explicit allowlist of CoreLink-owned origins; parse the URL and assert `host ∈ {known origins}`, reject userinfo/`@`/non-standard ports/embedded credentials; keep the https check as an additional constraint. (See F10 for the verified directly-reachable variant of this same control gap.)

---

### [LOW] F10 — Stripe Checkout `success_url` open-redirect — directly reachable by an authenticated user (scheme-only check)

**What it is.** The verified, *directly-reachable* form of F6. The Worker forwards the original request body unchanged (`new Request(request, { headers })` rebuilds only headers), so an authenticated user holding a valid Clerk session (any self-serve account, `azp ∈ CLERK_AZP_ALLOWLIST`) can POST directly to `POST /v1/onboarding/tier-select` on the public Worker (`corelink-api.humangr.com/*`) with an arbitrary `https://` `success_url`, bypassing the admin-ui's correct server-side URL derivation. The container accepts it (scheme-only check) and Stripe redirects post-payment to the attacker host.

**Why it matters / Impact.** A reflected open-redirect anchored on the trusted `checkout.stripe.com` payment domain — a high-credibility phishing primitive (Stripe even appends the real `{CHECKOUT_SESSION_ID}`). Severity is Low because: (1) the attacker must be authenticated; (2) the redirect only fires *after* the attacker completes their *own* real payment; (3) the exfiltrated session id is the attacker's own, display-only (`upgraded/page.tsx` explicitly never treats it as proof of payment); (4) tier activation is via the Stripe webhook, not the redirect — a hijacked redirect cannot affect billing. So: one authenticated user can redirect their own post-payment flow to a custom https URL, exfiling their own display-only session id. No cross-tenant exposure, no money-path bypass.

**How it's exploited / reproduction.**
1. Register a free account; obtain a Clerk session JWT (`azp = https://corelink-app.humangr.com`).
2. `POST https://corelink-api.humangr.com/v1/onboarding/tier-select` with `Authorization: Bearer <jwt>` and `{"tier":"pro","success_url":"https://attacker.example/steal?s={CHECKOUT_SESSION_ID}","cancel_url":"https://attacker.example/cancel"}`.
3. Worker verifies the JWT, forwards the body unchanged; container accepts (https prefix passes); Stripe checkout created with the attacker `success_url`.
4. Attacker completes payment; Stripe redirects to `https://attacker.example/steal?s=cs_live_...`.
Negative control: `http://...` returns 400, proving the scheme prefix is the only gate.

**Evidence.**
- `crates/corelink-container/src/routes/tier_select.rs:344-348` — scheme-only check.
- `tier_select.rs:406-407` → `tier_select_checkout.rs:186-192` — URLs flow unmodified to Stripe.
- `worker/src/index.ts:1322` — `new Request(request, { headers })` (body passed verbatim).
- `wrangler.toml:266` — `corelink-api.humangr.com/*`.
- Correct (protected) admin-ui path that is *not* the only caller: `apps/admin-ui/src/app/api/checkout/session/route.ts:112-125,158-159`.

**CVSS.** `AV:N/AC:L/PR:L/UI:R/S:C/C:N/I:L/A:N` ≈ 4.7 → Low.

**Fix.** Same as F6 — replace the scheme-only check with a host allowlist pinned to CoreLink origins, or (simplest/safest) derive `success_url`/`cancel_url` *server-side* in the container from a trusted origin (append only a fixed path + the `{CHECKOUT_SESSION_ID}` placeholder), ignoring the body fields entirely. Also fix the aspirational-but-unenforced comment at `tier_select.rs:341`.

---

### [LOW] F15 — DSR `build_state_from_env`: weak minimum-key-length enforcement (`is_empty` only)

**What it is.** The same DSR mount-guard asymmetry as F28, recorded as the original detailed finding: DSR's `build_state_from_env` mounts on any non-empty key, while `internal_pat`/`admin` enforce `< 16`. A 1–15-char key mounts the DSR erase/verify routes while the other key-sharing routes refuse to mount. This and F28 are the same defect; treat them as one fix (raise the DSR floor to ≥16, preferably 32).

**Why it matters / Impact.** Identical to F28 — operator-misconfiguration trap on the most destructive internal route, enabling unauthorized irreversible GDPR erasure if a short key is set and reached from the internal network.

**How it's exploited / reproduction.** Set `CORELINK_INTERNAL_AUTH_KEY=test`; DSR mounts while PAT-mint/admin do not; `POST /_internal/dsr/erase` with `x-corelink-internal-auth: test` + valid `DsrQueuedV1` body passes auth and erases.

**Evidence.** `crates/corelink-container/src/routes/dsr.rs:213-217` (`is_empty()` only) vs `internal_pat.rs:399` / `admin.rs:378` (`< 16`).

**CVSS.** `AV:N/AC:H/PR:N/UI:N/S:U/C:N/I:H/A:H` ≈ 7.4 in the misconfig scenario; Low overall (precondition = operator misconfig, internal-only reachability).

**Fix.** Replace `is_empty()` with `len() < 16` (or `< 32`) + warning, fail-closed. (Unified with F28.)

---

### [LOW] F17 — Inaccurate security documentation creates false assurance about PAT auth depth

**What it is.** Two comments in `index.ts` claim Argon2id verification is performed by "the Rust DO (corelink-worker Tower middleware)" as a second defense layer. Both are false: the TS Durable Object performs no crypto and forwards directly to the container (`proxyToContainer`); the container's `corelink-server` binary does not even depend on `corelink-worker`; native CAS/AC/Turbo routes trust `x-corelink-tenant-id` directly. The correct posture is actually documented elsewhere in the same function (lines 646-662) — so the file contains both a wrong claim and a correct one.

**Why it matters / Impact.** Audit blind spot: a reviewer relying on the documented Argon2id second-factor would deprioritize hardening the HMAC-only gate, underestimating blast radius if `PAT_SIGNING_KEY` is absent (F18) or HMAC is bypassed. No direct exploit — a wrong comment cannot be attacked — but it misrepresents the security boundary in load-bearing documentation.

**How it's exploited / reproduction.** `grep -n "argon\|Argon\|verify\|pat_hash\|PatVerifier" worker/src/durable_object.ts` → zero hits; the DO at 331-332 calls `proxyToContainer` with no intervening auth.

**Evidence.** `worker/src/index.ts:586`, `:1789-1791` (false claims); `worker/src/durable_object.ts:331-332` (no auth before proxy); Argon2id only in `adapter_pat.rs:253-258` (adapter routes).

**CVSS.** n/a; Low.

**Fix.** Correct both comments to state accurately that the native plane has no container-side Argon2id re-verify (Argon2id is wired only for OCI/adapter paths) and that possession rests on the Worker HMAC gate; track wiring Argon2id onto the CAS path as a TODO. (Pairs with F3's fix.)

---

### [LOW] F20 — Session-exchange mint throttle fails open on D1 error — abuse window on control-plane outage

**What it is.** `checkMintThrottle()` enforces a per-principal fixed-window rate limit (10 mints/60s) on PAT minting via `POST /v1/session/exchange`, backed by D1. On a D1 error the catch block logs and returns `null` → the caller proceeds with the mint (documented fail-open). During a D1 outage the throttle is fully disabled, so a still-valid Clerk session can loop-mint PATs, each running a real Argon2id hash (m=65536, t=3, p=4) on the shared `_system` DO container.

**Why it matters / Impact.** DoS on the `_system` DO container via Argon2id CPU exhaustion during a D1 outage window — degrading all tenants routing through that DO. Bounded by outage duration + Clerk session TTL. Gated behind two preconditions: a valid Clerk session (authenticated user) AND a D1 outage (which an app-layer attacker cannot force). No data/isolation/money impact.

**How it's exploited / reproduction.** With a valid Clerk JWT and a D1 outage in progress, loop `POST /v1/session/exchange`; each call skips the throttle (D1 error caught), reaches `/_internal/pat/mint`, runs Argon2id. Without an outage, the throttle fires at the 11th request (429), blocking the loop.

**Evidence.** `worker/src/lib/session_exchange.ts:118-135` — catch block returns `null` ("D1 unavailable — fail OPEN (allow)"); Argon2id cost confirmed at `crates/corelink-pat/src/argon.rs:44-81`.

**CVSS.** n/a; Low.

**Fix.** Prefer fail-closed (503) on D1 throttle errors for this non-latency-critical path. If fail-open must stay, add a module-level in-memory rate limiter in the Worker isolate as a local backstop (`if (inMemoryCounter.get(principalId) > MAX_BURST) return 429`).

---

### [LOW] F21 — Partial D1 outage can cause incorrect quota enforcement (false-positive 429) for high-tier tenants

**What it is.** Quota uses two sequential D1 queries: `getTierForTenant()` (tier from `tier_selections`+`tenant`) and `checkStorageQuota()` (current `bytes_used`). Their failure modes are **asymmetric**: `getTierForTenant()` falls back to `'free'` (the most restrictive, 10 GiB) on D1 error, while `checkStorageQuota()` fails *open* on D1 error. Under a partial outage where the tier queries fail but the storage query succeeds, a paid-tier tenant (e.g. solo, 40 GiB stored) gets `tier='free'` + actual `bytes_used` → `40 GiB > 10 GiB` → false-positive 429.

**Why it matters / Impact.** Paying tenants are incorrectly blocked (HTTP 429) during partial/asymmetric D1 instability (rolling maintenance, regional failover, capacity events). Reliability/correctness issue, not a security bypass — `getTierForTenant` returning the most restrictive tier is actually fail-*closed* for paying tenants, contradicting the file's own "fail-open" header comment. No data exfil, no billing bypass, not attacker-controlled.

**How it's exploited / reproduction.** Tenant on `solo` with 40 GiB stored; tier-table queries throw while `tenant_storage_state` succeeds; `getTierForTenant` returns `'free'`; `checkStorageQuota('free')` reads 40 GiB > 10 GiB cap → `ok:false` → 429.

**Evidence.** `worker/src/lib/quota.ts:111-136` (tier lookup falls through to hardcoded `'free'` on D1 error); `quota.ts:198-201` (storage query fails open). Header comment at line 7 ("fail-open on D1 errors") contradicts the tier path's actual fail-closed behavior.

**CVSS.** n/a; Low.

**Fix.** Make the failure modes symmetric: propagate a `d1Error` flag from `getTierForTenant()`; if the tier lookup errored, skip the storage check entirely (fail open). Or treat an error-derived `'free'` distinctly from a confirmed `'free'`.

---

### [LOW] F23 — Brew non-content-addressed paths (tag manifests) cached into `PUBLIC_NAMESPACE` without pre-store integrity verification

**What it is.** The Homebrew adapter proxies to ghcr.io and caches responses in the multi-tenant `PUBLIC_NAMESPACE` (shared across all tenants). For content-addressed paths (`.../blobs/sha256:<64-hex>`) it correctly verifies the sha256 before storing. For *tag*-addressed manifest paths (`.../manifests/8.5.0`, `.../manifests/latest`) `expected_sha256()` returns `None` and the code skips all integrity verification (`None => false`), storing the bytes as-is and serving them to every tenant.

**Why it matters / Impact.** If ghcr.io returns a corrupt or tampered manifest while the cache is being populated, the bad bytes are cached once and served to all brew-proxy tenants until purged. Homebrew clients use manifests to resolve which blob digest to pull, so a poisoned manifest can cause cross-tenant install failures (DoS) or, worst case, redirect pulls to wrong-version blobs. **Severity is Low because exploitation requires infrastructure-level network position** (DNS hijack of ghcr.io / TLS MITM of the container's egress / a compromised ghcr.io CDN node) — a valid PAT alone gives no control over the upstream response; the PAT only triggers the fetch. The cross-tenant scope (shared `PUBLIC_NAMESPACE`) is real but the trigger is not tenant-controllable, and blob-level poisoning *is* protected by the sha256 check.

**How it's exploited / reproduction.** With an infra-level MITM position against the container→ghcr.io channel, any request for `.../manifests/latest` on a cache miss fetches the attacker-controlled bytes; `expected_sha256` returns `None`, the `None => false` branch stores them under `PUBLIC_NAMESPACE`; subsequent tenants receive the corrupted manifest from cache.

**Evidence.**
- `crates/corelink-adapter-host/src/brew/bottle.rs:185-206` — `None => false` branch skips integrity check, stores as-is.
- `crates/corelink-container/src/routes/brew.rs:92-99` — `put` ignores `tenant_id`, always uses `PUBLIC_NAMESPACE`.

**CVSS.** `AV:N/AC:H/PR:H/UI:N/S:C/C:L/I:L/A:N` ≈ 4.9 → Low (infra position required).

**Fix.** For tag-addressed manifests, parse and store the `Docker-Content-Digest`/`ETag` returned by ghcr.io; at minimum log the sha256 of every fetched body in the audit row. Longer term, do not promote mutable tag manifests to `PUBLIC_NAMESPACE` — serve them pass-through or store under a per-tenant namespace with a short TTL.

---

### [LOW] F24 — Authenticated tenant can flood their own container memory via concurrent 100 MiB Turborepo PUT bodies (self-DoS)

**What it is.** The Turbo PUT route accepts bodies up to 100 MiB (`TURBO_BODY_LIMIT_BYTES`, intentionally larger than the global 10 MiB limit for large artifacts). Axum buffers the entire body in memory before the handler runs, and the body is read as `Bytes` (no streaming to R2). There is no per-tenant concurrency or rate limit; the ADR-0068 quota gate only enforces a monthly dollar ceiling.

**Why it matters / Impact.** **Self-DoS only.** The original "cross-tenant" framing is refuted: the Worker routes each tenant to `env.CORELINK_SERVER.idFromName(resolvedTenantId)` — one DO and one container per tenant (durable_object.ts: "Per-tenant pinning: DO ID = idFromName(tenantId) — never cross-tenant"). A tenant flooding concurrent 100 MiB PUTs OOMs **their own** container only; other tenants are in separate DO+container instances and unaffected. `max_instances=5` is a platform ceiling, not a per-tenant fan-out. Real impact: an authenticated tenant can cycle their own build cache and generate operator-alert noise.

**How it's exploited / reproduction.** With a `cas:rw` PAT, send N (e.g. 20) concurrent `PUT /v8/artifacts/<hash>?teamId=...` each with a 100 MiB body (~2 GiB held simultaneously in the attacker's own container); container OOMs and restarts; only the attacker's own Turbo cache is disrupted.

**Evidence.** `crates/corelink-container/src/routes/turbo_v8.rs:97` (`TURBO_BODY_LIMIT_BYTES = 100*1024*1024`), `:239-241` (DefaultBodyLimit layer on all turbo routes), `:308` (`body: Bytes` full buffer); per-tenant DO routing `worker/src/index.ts:1758`.

**CVSS.** `AV:N/AC:L/PR:L/UI:N/S:U/C:N/I:N/A:L` — only the attacker's own resources → Low.

**Fix.** Enforce a per-tenant in-flight PUT concurrency limit (e.g. 4) rejecting excess with 429, ideally at the Worker/DO layer; stream the Turbo PUT body directly to R2 rather than buffering fully in memory.

---

### [LOW] F25 — OCI upload-session buffer is process-local and unbounded — unfinalized sessions accumulate memory with no cross-session cap

**What it is.** `OciMoatStore` buffers all in-flight OCI blob uploads in a `Mutex<HashMap<String, Vec<u8>>>` in process memory (explicitly non-durable). There is no cap on the number of open sessions per tenant or in total, and no inactivity timeout. The PATCH handler collects the entire request body with `to_bytes(body, usize::MAX)` before the per-session oversize check (5 GiB) fires.

**Why it matters / Impact.** An authenticated tenant can open many sessions via `POST /v2/repo/blobs/uploads/` without finalizing, and feed each a large PATCH body — memory scales as N × chunk-size up to N × 5 GiB, contributing to OOM of the OCI DO container (cross-tenant for tenants sharing that OCI DO). Per-session size *is* bounded (the 5 GiB oversize check cancels a runaway session), so the real vector is the *number* of sessions. Requires a valid `cas:rw` PAT; service-availability concern only.

**How it's exploited / reproduction.** With a `cas:rw` PAT, exchange at `/token` for a push bearer; loop `POST .../blobs/uploads/` to open N sessions (each inserts an empty `Vec`); `PATCH .../blobs/uploads/<uuid>` with a large body per session (buffered via `to_bytes(_, usize::MAX)`), never finalizing; heap grows toward OOM.

**Evidence.** `crates/corelink-container/src/routes/oci.rs:123-127,158-165` (unbounded `uploads` HashMap, no count guard); `crates/corelink-adapter-host/src/oci/server/handlers.rs:305` (`to_bytes(body, usize::MAX)`); `crates/corelink-adapter-host/src/oci/push/upload.rs:121-138` (oversize check fires *after* `append_chunk`).

**CVSS.** n/a; Low.

**Fix.** Add a per-tenant cap on open sessions (e.g. 4) in `open_upload()`; add an inactivity timeout via a background cleanup task; enforce a total upload-buffer size limit; long-term migrate to R2 multipart upload (consistent with the R2KvStore pattern).

---

### [LOW] F34 — `invoice.payment_failed` missing fallback revocation path when the Stripe invoice lacks `customer`

**What it is.** The `invoice.payment_failed` webhook handler revokes the canonical access gate (`tier_selections.subscription_state → 'inactive'`) **only** when the invoice payload contains a `customer` field. If `customer` is absent, the tier-selections row stays `active` even on a terminal failure (dunning exhausted, `next_payment_attempt === null`). The analogous `customer.subscription.updated` and `customer.subscription.deleted` handlers both have a subscription-id fallback (`deactivateTierSelectionBySubscription`); `invoice.payment_failed` does not — it still writes `tenant_billing.status='past_due'` (secondary mirror) but skips the canonical gate.

**Why it matters / Impact.** A tenant with a terminal payment failure whose `invoice.payment_failed` payload omits `customer` retains indefinite paid-tier access (since `getTierForTenant` reads `tier_selections WHERE subscription_state='active'`) — served without payment. Blast radius is limited by the rarity of customer-less invoice payloads (Stripe's Invoice object normally always includes `customer`, and the attacker cannot control the HMAC-signed payload), and a later `subscription.updated`/`deleted` event (which has the fallback) would eventually revoke. Defensive inconsistency, not an externally-triggerable bypass.

**How it's exploited / reproduction.** Craft an `invoice.payment_failed` with `next_payment_attempt: null`, `subscription: 'sub_x'`, and no top-level `customer`; sign with the test webhook secret; POST. Result: `tenant_billing.status='past_due'` (correct) but `tier_selections.subscription_state` stays `'active'` (bug).

**Evidence.** `apps/signup-worker/src/webhooks/stripe.ts:1226-1233` — bare `if (typeof stripeCustomerId === 'string' && stripeCustomerId) { deactivateTierSelectionByCustomer(...) }` with no else; contrast `:1116-1123` and `:1269-1274` (ternary fallback to `deactivateTierSelectionBySubscription`).

**CVSS.** n/a; Low.

**Fix.** Replace the bare `if` with the same ternary fallback used by the subscription handlers:
```typescript
requiredWrites.push(
    typeof stripeCustomerId === "string" && stripeCustomerId
        ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
        : deactivateTierSelectionBySubscription(db, { stripeSubscriptionId }),
);
```
Add a unit test covering `invoice.payment_failed` with `next_payment_attempt:null`, `subscription` present, no `customer`.

---

### [LOW] F35 — `updateTierSelectionTierByCustomer` updates tier on inactive/canceled subscriptions — stale tier data in the access-gate table

**What it is.** `updateTierSelectionTierByCustomer` issues `UPDATE tier_selections SET tier = ?1 WHERE stripe_customer_id = ?2` with **no `subscription_state` filter**. On a `customer.subscription.updated` event that both changes the price (→ a known tier) and no longer grants access (e.g. `status:'canceled'`), the tier-update and the `deactivateTierSelectionByCustomer` write are pushed to `requiredWrites` and run concurrently via `Promise.all` (two independent D1 requests). The row ends up `subscription_state='inactive'` (correct) but with a freshly-written stale `tier`.

**Why it matters / Impact.** **No access-control bypass** — `getTierForTenant` gates on `subscription_state='active'`, so an inactive row's tier grants nothing. The impact is data quality: inactive/canceled rows carry incorrect tier labels, misleading forensic/audit queries and any analytics that count tenants by tier without also filtering `subscription_state='active'` (e.g. an operator scanning `tier='pro'` sees canceled tenants as Pro). The drift view (`stripe_tier_drift_view`) is unaffected (it only includes active rows).

**How it's exploited / reproduction.** Tenant on active `solo`; Stripe fires `customer.subscription.updated` with `status:'canceled'` and a new price mapping to `max`; both writes run; row ends `tier='max'`, `subscription_state='inactive'`; `SELECT tier FROM tier_selections WHERE tenant_id=?` returns `'max'` for an inactive tenant.

**Evidence.** `apps/signup-worker/src/webhooks/stripe.ts:711-723` (bare UPDATE, no state filter), called at `:1090-1101` before the deactivation at `:1116-1124`, both flushed via `Promise.all` at `:1308`.

**CVSS.** n/a; Low.

**Fix.** Add `AND subscription_state = 'active'` to the WHERE clause so the tier update is a no-op on already-inactive rows:
```sql
UPDATE tier_selections SET tier = ?1
WHERE stripe_customer_id = ?2 AND subscription_state = 'active'
```

---

### [LOW] F37 — Secret material exposed via automatic `#[derive(Debug)]` in Vault authentication

**What it is.** In the `corelink-byok` Vault KMS integration, several structs derive `Debug` while holding plaintext secrets: `AuthSource` (`Token(String)`, `AppRole { role_id, secret_id }`, `Kubernetes { role, sa_jwt }`, `Static(String)`), `CachedToken { token: String }`, `AuthInner`, and the public `VaultAuth`. A derived `Debug` renders these verbatim. The module doc claims tokens are never logged, and indeed **no current code path Debug-formats these types** (exhaustively grep-verified — every `{:?}` hit is on unrelated types, and the original finding's "panic backtraces print local-variable Debug" premise is factually false: Rust backtraces print frames, not Debug reprs). So there is no reachable leak today; this is a latent hardening gap that violates the crate's own established, *tested* redaction discipline.

**Why it matters / Impact.** If a future change adds `tracing::debug!("{:?}", vault_auth)` (or similar), Vault tokens / AppRole secrets / K8s JWTs would land in logs shipped to observability platforms — enabling direct Vault access and DEK extraction for BYOK-Vault tenants. The crate elsewhere does this correctly: `byok_core/types.rs:73-77` (Dek redacts to `[REDACTED]`), `byok_azure/entra.rs:50-64` (`SecretString` redacting Debug *with* an enforcing test). The Vault auth structs are the inconsistency.

**How it's exploited / reproduction.** No current PoC (no Debug-format path exists). Latent: a future `tracing::debug!(auth = ?vault_auth, ...)` would emit `Token("hvs_...")` / `AppRole { secret_id: "hvs_..." }` into logs.

**Evidence.** `crates/corelink-byok/src/byok_vault/auth.rs:45-59` (`#[derive(Debug)] enum AuthSource`), `:73-77` (`CachedToken { token: String }`), `:80-91` (`VaultAuth`/`AuthInner`). Contrast the redacting precedent at `byok_core/types.rs:73-77` and `byok_azure/entra.rs:61-64` (+ test `entra.rs:400-406`).

**CVSS.** n/a; Low (defense-in-depth, no reachable leak today).

**Fix.** Replace the raw derives with custom redacting `Debug` impls for `AuthSource`/`CachedToken`/`AuthInner`/`VaultAuth` (e.g. `AuthSource::Token(_) => "AuthSource::Token(<redacted>)"`), or wrap `token`/`secret_id`/`sa_jwt` in the existing `SecretString` newtype. Add a test asserting no secret substring appears in the Debug output, mirroring `secret_string_debug_redacts`.

---

### [INFO] F11 — Audit-analytics per-tenant rate limiter is in-memory/per-instance, resets on cold start

**What it is.** `build_state()` (the prod router) constructs an `InMemoryTokenBucketRateLimiter` for the analytics query routes (`/v1/audit/analytics/timeline`, `.../event-count`, 10/min/tenant). Token counts live only in the container process RAM; on a cold-start (5-min idle timeout) the bucket resets to a full burst. The original "spread load across multiple instances" angle is refuted — per-tenant `idFromName(tenantId)` routing means one DO → one container → one limiter per tenant; there is no fan-out.

**Why it matters / Impact.** A tenant can wait out the 5-min idle timeout to force a container recycle and get a fresh 10-token burst — a ~2× rate bypass on a read-only analytics aggregate (Neon Postgres shadow), bounded and not cross-tenant, not billing/auth. Info-level hardening.

**Evidence.** `crates/corelink-container/src/routes/audit_analytics/state.rs:90` (`InMemoryTokenBucketRateLimiter::new`), `routes.rs:388` (prod mount); per-tenant routing `worker/src/index.ts:1758`.

**Fix.** Back the limiter with a durable per-tenant store (the `ratelimit_buckets` D1 table) using the atomic `INSERT … ON CONFLICT … DO UPDATE … RETURNING count` pattern proven in `session_exchange.ts::checkMintThrottle`, or move the gate to the DO-backed Worker layer.

---

### [INFO] F12 — Per-tenant $-ceiling charges one flat op cost per request regardless of batch fan-out (quota-bypass-by-batching)

**What it is.** ADR-0068's per-tenant monthly $-ceiling charges a flat `DEFAULT_COST_PER_OP_MICROS = 1000` ($0.001) per HTTP request via one `gate.check()` call. Bazel `findMissingBlobs` accepts a *batch* of digests (capped at `FIND_MISSING_BLOB_CAP = 4096`) and performs one backend CAS existence check per digest, yet accrues only one flat op cost for the whole batch.

**Why it matters / Impact.** A tenant drives up to 4096× more backend work per accrued dollar than the flat model assumes, weakening the ADR-0068 cost-blast-radius bound. The $5 ceiling still fires (at ~5000 requests/month), so blast radius is the operator's own cost exposure, not another tenant. The authors explicitly designed this as a "coarse preventive tripwire, NOT precise metering" — an honest, documented approximation.

**Evidence.** `crates/corelink-container/src/routes/bazel_v2.rs:506` (single `quota_reject`) → `:525` (per-digest backend loop); `crates/corelink-container/src/tenant_quota.rs:89` (flat cost); `tenant_quota.rs:79-88` + ADR-0068 (documented coarse-tripwire intent).

**Fix.** Post-launch metering refinement: scale cost by `digests.len()` (or byte-weighted) for batch endpoints, and/or add an explicit per-request digest-count 413 gate.

---

### [INFO] F13 — $-ceiling can over-shoot under concurrency (TOCTOU over-admit)

**What it is.** The ceiling guard reads accrued spend, checks `baseline + cost > budget`, then accrues — but the read-for-decision is intentionally **not** serialized with the atomic accrual write. Under high concurrency, many requests read the same pre-accrual baseline, all pass, and all proceed, admitting the tenant slightly past budget. The accrual itself is atomic (never under-counts).

**Why it matters / Impact.** Bounded over-admission proportional to in-flight concurrency (a few extra ops past the cap). The authors document the trade-off explicitly and chose the safe direction (never under-count). Not an isolation/correctness break. Info.

**Evidence.** `crates/corelink-container/src/tenant_quota.rs:277-283` (the documented note), `:284-285` (projected check), `:316-318` (accrue) — not in one transaction.

**Fix.** Accept as-is for a coarse tripwire. For exact enforcement, make the check + accrual one atomic statement: `UPDATE tenant_quota SET accrued = accrued + ?delta WHERE accrued + ?delta <= budget RETURNING accrued`, reject when no row returned.

---

### [INFO] F14 — DSR routes forward verbose serde_json / engine error messages to the caller

**What it is.** The DSR erase/verify handlers return raw deserialization and erasure-engine error detail to the HTTP caller (`format!("invalid body: {e}")`, `format!("erasure failed: {e}")`), exposing field names, expected types, byte offsets, enum variants, and the internal error taxonomy.

**Why it matters / Impact.** Schema/error-taxonomy disclosure — but only to a caller already holding `CORELINK_INTERNAL_AUTH_KEY` (the Worker 401s unauthenticated callers before they reach the handler), and the `DsrQueuedV1` schema is already documented in an inline comment in the open-source codebase. Post-compromise information-disclosure hardening item; no independent exploit. Info.

**Evidence.** `crates/corelink-container/src/routes/dsr.rs:322,341,361,379`.

**Fix.** Return opaque fixed strings ("invalid request body" / "erasure failed") and log the detail server-side via `tracing::warn!/error!` with a structured `error = %e` field.

---

### [INFO] F16 — admin `handle_mutate`: Json extractor parses body before handler auth check

**What it is.** `handle_mutate` takes `Json(body): Json<AdminMutateBody>` as an axum extractor, which deserializes the full body before the handler's `internal_auth_ok` check runs — so an unauthenticated caller forces body allocation + JSON deserialization before the 403.

**Why it matters / Impact.** Pre-auth deserialization cost, but bounded by the global 10 MiB `DefaultBodyLimit` (the route is covered) and the route is reachable only via the DO's internal TCP port, not the public internet. The `AdminMutateBody` schema is small/non-recursive, so even a max body is bounded CPU. Info-level M3-pattern consistency gap.

**Evidence.** `crates/corelink-container/src/routes/admin.rs:619,625`; 10 MiB limit applied in `main.rs:299-301`.

**Fix.** Accept `body: Bytes`, run `internal_auth_ok` first, then `serde_json::from_slice` — matching the M3 pattern already used by `dsr.rs`/`internal_pat.rs`.

---

### [INFO] F19 — Health endpoints disclose deployment environment without authentication

**What it is.** `/health`, `/_health`, `/api/health` (unauthenticated) return `{"status":"ok","env": env.ENVIRONMENT}` — e.g. `{"status":"ok","env":"prod-lhr"}`.

**Why it matters / Impact.** Zero information uplift: the regional topology is already encoded in the hostname the attacker dialed (`sam.corelink-api.humangr.com` already reveals SAM), and `ENVIRONMENT` carries no secret. `workers_dev=false` means the "dev" default is never publicly reachable. Cosmetic minimal-disclosure hygiene. Info (CVSS 0.0).

**Evidence.** `worker/src/index.ts:1116`.

**Fix.** Drop the `env` field: `JSON.stringify({ status: statusLiteral })`. Serve environment detail only on an authenticated/internal endpoint if needed.

---

### [INFO] F22 — `timingSafeEqual` in `durable_object.ts` uses a zero HMAC key and has length-branch timing asymmetry

**What it is.** The exported `timingSafeEqual` helper HMACs both inputs (with an all-zero key) before XOR-comparing — and on a length mismatch runs one SHA-256 while same-length inputs run two HMACs, creating a distinguishable length-equality timing signal. The comment "consume constant work" is false (one SHA-256 ≠ two HMAC-SHA256). Critically, the function is **exported but never imported in production** — production constant-time comparisons use `crypto.subtle.timingSafeEqual` directly; this helper exists only for tests.

**Why it matters / Impact.** No current exploit path (dead code from a security standpoint). Latent: if a future developer imports it to compare a secret, the length-branch asymmetry leaks a length oracle and the zero key offers no confidentiality. Info.

**Evidence.** `worker/src/durable_object.ts:116-127` (length branch + zero key), `:142` (exported, unused in prod).

**Fix.** Mark test-only / remove the export, or fix both defects (random per-isolate key + run both HMACs regardless of length), or replace with `crypto.subtle.timingSafeEqual` on UTF-8 bytes.

---

### [INFO] F26 — OCI surface bypasses the `x-corelink-scope` header gate by design (single-layer vs double-layer enforcement)

**What it is.** All non-OCI adapters receive a server-trusted `x-corelink-scope` header (Worker-injected from D1) and gate on it before adapter code runs. OCI is forwarded raw (two-leg pass-through): no `x-corelink-scope`, so per-op authorization rests solely on the adapter's bearer-scope check (`scope.allows(repo,action)`) plus the `/token` downscope. The compensating controls are solid (full Option-B PAT re-verify at `/token`, `restricted_to_read()` downscope, HMAC-signed bearer on every op), but OCI has one fewer defense-in-depth layer than the other five surfaces, and the architecture exception is documented only in code, not in `auth_model.md`.

**Why it matters / Impact.** Not currently exploitable. A future bug in OCI bearer scope enforcement would have no backstop layer (unlike the header gate on other surfaces). Info / documentation gap.

**Evidence.** `crates/corelink-container/src/routes/oci.rs:375-379`; Worker deliberately omits the header at `worker/src/index.ts:1411`; `grep "OCI" specs/03_architecture/auth_model.md` → empty.

**Fix.** Document the OCI two-leg pass-through exception and its compensating controls in `auth_model.md`. Consider injecting `x-corelink-scope` on `/token` (credential exchange, where the Worker has the Basic-auth PAT) to restore one backstop layer without disrupting the two-leg flow.

---

### [INFO] F27 — `PatVerifier` `can_write` bit discarded by non-OCI adapter resolvers; write enforcement relies on `x-corelink-scope` header trust

**What it is.** `PatVerifier.verify_capability()` returns `(tenant_id, can_write)`. OCI uses the `can_write` bit to downscope the minted bearer (true end-to-end enforcement). Cargo/brew/npm/pip call `verify()`, which discards the bit (`.map(|(tenant, _can_write)| tenant)`); their per-op write enforcement instead reads the Worker-injected `x-corelink-scope` header. The D1-sourced `can_write` truth is never cross-checked against the header.

**Why it matters / Impact.** Not exploitable today — the Worker strips client-supplied `x-corelink-scope` (`stripClientTrustHeaders`) and re-sets it from D1 (`auth.scope`), so a forged scope cannot survive to the container. Exploitation would require a Worker-side bug (failing to strip, or setting the wrong value). Defense-in-depth gap: four surfaces are single-layer (header trust) vs OCI's two-layer. Info.

**Evidence.** `crates/corelink-container/src/adapter_pat.rs:274-278` (`verify` discards `can_write`); `routes/cargo.rs:72-78` (resolver calls `verify`), `:149-158` (gate reads header); strip+set at `worker/src/index.ts:1769,1782`.

**Fix.** Thread `can_write` into the cargo/brew/npm/pip resolvers (mirroring OCI's `ResolvedPat`) and require `scope_ok_from_header AND can_write_from_pat_reverify`, eliminating the header-trust dependency for write enforcement and making all surfaces uniformly two-layered.

---

### [INFO] F29 — `internal_pat.rs` documents a 32-byte minimum for `CORELINK_INTERNAL_AUTH_KEY` but enforces only 16

**What it is.** `build_state_from_env` doc says "Must be at least 32 bytes (ASCII)" while the code enforces `if auth_key.len() < 16`. A 16–31-char key mounts the PAT-mint route despite the doc-promised 32-byte floor.

**Why it matters / Impact.** Doc/code inconsistency for the route that mints PATs. The secrets-checklist correctly instructs `openssl rand -hex 32` (64 chars), so operators following the runbook are unaffected; exploitation requires an operator to misread both the code and the checklist. Info.

**Evidence.** `crates/corelink-container/src/routes/internal_pat.rs:390-399` (doc says 32, code checks `< 16`).

**Fix.** Change `< 16` to `< 32` and align the warning; consider bumping `admin.rs` to 32 for consistency.

---

### [INFO] F30 — Stale comment claims `rsplitn` but code uses `splitn` in the signup token parser

**What it is.** `parse_and_verify_pilot_token` comments "We use rsplitn so the `<env>` field cannot smuggle an underscore," but the code calls `body.splitn(4, '_')`. The actual security comes from the strict env allowlist (`{"staging","prod"}`) + u64 timestamp validation, not the split direction.

**Why it matters / Impact.** No current impact (the code is correct). Risk: a future developer "fixing" the code to `rsplitn` to match the comment would invert segment order, fail the `parts.first() != Some("pilot")` check, and reject *all* valid tokens — a DoS on pilot signups. Info.

**Evidence.** `crates/corelink-container/src/routes/signup.rs:260-263`.

**Fix.** Correct the comment to describe `splitn(4)` and credit the env allowlist + timestamp validation as the actual security mechanism.

---

### [INFO] F31 — `dummy_verify_for_constant_time` returns early on an empty dummy PHC without performing Argon2id work

**What it is.** The function that pads a PAT lookup-miss to match a genuine Argon2id verify (preventing a token-enumeration timing oracle) returns early if the lazily-initialized dummy PHC is empty, skipping the Argon2id work. The empty-PHC state only arises if Argon2id hashing itself fails at startup — unreachable on the Cloudflare/Linux deployment platform (getrandom always available; salt/params statically valid).

**Why it matters / Impact.** No attacker-triggerable path on the actual platform (the entropy/hash failure that produces an empty PHC cannot be induced remotely). If it ever occurred, it would create a token_id existence oracle (token_ids are non-secret lookup keys, no cryptographic secret leaked, not a forgery path). Info.

**Evidence.** `crates/corelink-pat/src/argon.rs:224-230` (early return on empty PHC); `:178-197` (empty-PHC fallback).

**Fix.** Replace the early-return with a hardcoded known-valid dummy PHC so Argon2id-equivalent work always runs, or synthesize the delay via `ARGON2_EXPECTED_DURATION`, removing the dependency on dummy-PHC availability.

---

### [INFO] F32 — OCI config doc says "32 bytes (raw, not hex)" but loading passes the raw string without hex-decoding; plus a stale env-var name

**What it is.** `config.rs` doc says the OCI token signing key "MUST be ≥32 bytes (raw, not hex)," and `sanity_check` measures the raw *string* length ≥32 (the string bytes become the HMAC key directly). An operator who interprets "raw, not hex" as "use `openssl rand -hex 16` (32 hex chars = 16 raw bytes)" would pass the check with only 128 bits of entropy. The doc also names the wrong env var (`HUGR_OCI_TOKEN_KEY`; the actual is `CORELINK_OCI_TOKEN_KEY`).

**Why it matters / Impact.** Pure documentation ambiguity. The secrets-checklist correctly instructs `openssl rand -hex 32` (64 chars, 256-bit string entropy fed to HMAC-SHA256), so an operator following the runbook is fine. No code-path vuln; reduced entropy only under an active misconfiguration that contradicts the checklist. Info.

**Evidence.** `crates/corelink-adapter-host/src/oci/config.rs:73` (doc, wrong env var), `:184-185` (string-length check); actual env var at `crates/corelink-container/src/routes/oci.rs:97`.

**Fix.** Fix the env-var name to `CORELINK_OCI_TOKEN_KEY`; clarify the doc ("≥32 characters of key material; `openssl rand -hex 32` → 64 chars → 256-bit key, correct"); optionally hex-decode at load time and validate the *decoded* byte length.

---

### [INFO] F33 — Migration 0064 trigger-destruction (operational twin of F36)

**What it is.** The operational consequence half of the F36 documentation defect: after 0064 applies, the three `trg_tenant_primary_region_*` triggers are gone. The NULL-guard and valid-set-on-INSERT triggers are redundant with the preserved column constraint `primary_region TEXT NOT NULL CHECK (...)`, so their loss is non-impactful for inserts. **The one genuinely lost guard is `trg_tenant_primary_region_immutable`** — there is no column-level equivalent, so a direct `UPDATE tenant SET primary_region` against D1 is no longer aborted.

**Why it matters / Impact.** Loss of the D1-layer residency-immutability backstop (INV-REGION-NO-CROSS-LEAK), reachable only via direct/privileged D1 write or a future code bug (no code path issues such an UPDATE today). Recorded as Info here because the *exploitable residual* is constrained and the full severity is carried by F36; this entry documents the operational fact and shares F36's fix.

**Evidence.** Same as F36: `migrations/d1/0064_tenant_tier_max.sql:210,212,214-261`; `0028:53-59`; empirically reproduced on sqlite3 3.43.2 (trigger count → 0 after DROP+RENAME).

**Fix.** Re-create the three 0028 triggers after the RENAME (see F36); fix the comment + ADR-0064 §4; teach `check_migrations_additive.py` to require `CREATE TRIGGER` on any table rebuild.

---

## 3. Remediation Order

### Fix before launch (gate)

These close the "fail-open / silent-degraded secret" class and the two integrity breaks reachable in steady state:

1. **F1 + F2 — Set `R2_TDK_HEX` on all 5 regional containers AND make the storage handler fail-closed without it** (gate the raw-padded fallback behind `#[cfg(test)]`, HMAC the full tenant id). One code change closes both. *Plan the cache re-key / cold cache.*
2. **F18 — Make `PAT_SIGNING_KEY` mandatory + fail-closed (503)** at Worker boot/extractAuth; remove the optional `?`; verify it on all 5 workers.
3. **F9 — Make `ERASURE_SALT_KEY` a required prod secret, fail-closed when absent in prod**, and provision it before DSR goes live.
4. **F5 — Add the AC divergent-body guard (409) to `R2AcHandler::update`** + regression test + atomic conditional PUT.
5. **F28 + F15 — Raise the DSR mount-guard key floor to ≥16 (preferably 32)**, fail-closed; consider unifying all key-sharing routes at 32.
6. **F36 + F33 — Re-create the three `tenant.primary_region` triggers in 0064 (or a new 0069) and fix the false comment** — *before migration 0064 is applied to prod.* Mechanize the additive-migration trigger check.
7. **Mechanize the secrets gate:** add `R2_TDK_HEX`, `PAT_SIGNING_KEY`, `ERASURE_SALT_KEY`, `CORELINK_OCI_TOKEN_KEY` to `scripts/secrets-checklist-verify.sh` + `validate_secrets_matrix.py` as required prod secrets so a missing one reds CI.
8. **F10 + F6 — Lock the Stripe `success_url`/`cancel_url` to a CoreLink-owned host allowlist (or derive server-side).** Low individually, but it is on the money path and the fix is trivial.

### Fix shortly after launch (hardening / compliance)

9. **F7 + F8 — CAS data-residency:** add the env-contract forwarding (F8) *first* (so any fix takes effect), then make CAS residency-aware per `primary_region`; until then, remove the residency claim for CAS content from marketing/DPAs.
10. **F4 — De-concentrate the internal-auth secret:** separate per-consumer secrets, remove `/_internal/pat/mint` from the public edge Worker (Service-Binding only), add per-tenant authorization.
11. **F3 + F17 — Wire `PatVerifier` onto the native CAS/AC/Bazel/Turbo plane (Option-B extension) and fix the false Argon2id comments.**
12. **F20, F21 — Symmetrize the D1 fail-open/closed behavior** (session-exchange throttle; quota tier vs storage).
13. **F23, F24, F25 — Adapter/transport DoS + integrity hardening** (brew tag-manifest integrity; per-tenant Turbo PUT concurrency; OCI session caps/timeouts).
14. **F34, F35 — Stripe webhook consistency** (payment_failed fallback; tier-update active-only filter).
15. **F37 — Redact the Vault auth `Debug` impls.**

### Cleanup backlog (info — schedule opportunistically)

F11, F12, F13, F14, F16, F19, F22, F26, F27, F29, F30, F31, F32 — documentation corrections, coarse-tripwire refinements, dead-code hygiene, and defense-in-depth alignment. None block launch.

---

## 4. Surface Coverage

| # | Surface | Status | Findings |
|---|---|---|---|
| 1 | Tenant isolation / R2 key-prefix derivation (CAS + AC) | **Has findings** | F1 (M), F2 (L) |
| 2 | AuthN/AuthZ full-chain — PAT possession (native plane) | **Has findings** | F3 (L), F17 (L), F18 (M) |
| 3 | AuthN/AuthZ — internal-auth gate / privilege concentration | **Has findings** | F4 (M), F28 (M), F15 (L), F29 (I) |
| 4 | Cache poisoning — Action Cache integrity / REAPI v2 | **Has findings** | F5 (M) |
| 5 | Money path — Stripe checkout (`success_url`, webhooks) | **Has findings** | F6 (L), F10 (L), F34 (L), F35 (L) |
| 6 | DSR / GDPR — residency, erasure salt, erasure routes | **Has findings** | F7 (M), F9 (M), F14 (I), F30 (I), F31 (I) |
| 7 | ENV-contract (Worker↔container) / schema-migration integrity | **Has findings** | F8 (M), F36 (H), F33 (I) |
| 8 | DoS / quota ($-ceiling, rate-limit, body limits) | **Has findings** | F11 (I), F12 (I), F13 (I), F20 (L), F21 (L), F24 (L), F25 (L) |
| 9 | Cache adapters (cargo/brew/npm/pip/OCI/sccache) | **Has findings** | F23 (L), F26 (I), F27 (I), F32 (I) |
| 10 | Admin / internal mutation routes | **Has findings** | F16 (I) |
| 11 | Worker / DO helpers, health, secret-handling hygiene | **Has findings** | F19 (I), F22 (I), F37 (L) |

**No surface came back clean** — every one of the 11 surfaces carries at least one finding. That said, **10 of the 11 surfaces are dominated by Low/Info defense-in-depth and hardening items**; only three surfaces (1 — tenant isolation prefixing, 4 — AC integrity, 6 — DSR/residency) and the migration/env-contract surface (7) carry a Medium-or-above that gates launch, and all of those are closed by the eight pre-launch fixes above. The recurring root cause across surfaces 1, 2, 3, and 6 is identical — **secrets and invariants that fail open and silently**; the secrets-gate mechanization (remediation #7) plus the per-handler fail-closed changes neutralize that class wholesale.

---

## Full confirmed findings (chewed)

### [1] MEDIUM — Cross-tenant CAS/AC blob co-residence via raw-padded 16-char prefix fallback (R2_TDK_HEX unset in production)

**surface:** Tenant isolation / R2 key-prefix derivation (CAS + AC durable storage)

**location:** crates/corelink-container/src/storage/r2_s3.rs:307-336 (R2CasHandler::r2_key) and :666-690 (R2AcHandler::r2_key)

**cvss_reasoning:** CVSS 3.1 AV:N/AC:H/PR:L/UI:N/S:C/C:H/I:H/A:N ≈ 8.0 (High/Critical). Network attack vector; low privilege (any authenticated self-serve tenant); AC:H because the attacker needs a tenant whose UUIDv7 first-16-char prefix collides with the victim (achievable by harvesting tenant_ids — which leak the creation ms — and registering to land in a colliding bucket, ~1/16 per same-ms pair). Scope changed (S:C): the storage layer's isolation boundary is breached, affecting a different security authority (other tenant). C:H + I:H (read AND poison another tenant's CAS/AC). Treated as critical given it is a core multi-tenant isolation break on the money/data path and launch-blocking.

**explanation:** CoreLink physically isolates each tenant's cache objects in R2 by prefixing every object key with a per-tenant string: the canonical design (tenant-path::derive_prefix) is HMAC-SHA256(secret TDK, tenant_uuid) truncated to 16 url-safe base64 chars — an unpredictable, collision-resistant (~96-bit) namespace keyed under a secret. The R2 object key is `<region>/<tenant_prefix_16>/<digest>`, and that prefix is the ONLY thing that keeps tenant A's blobs from sharing storage addresses with tenant B's (the route-layer `tenant == auth.0` check only stops a caller from naming ANOTHER tenant in the URL; it does nothing about the physical R2 prefix derived for the caller's OWN tenant). The bug: R2CasHandler/R2AcHandler only call the secure derive_prefix when a Tenant Derivation Key is configured (self.tdk = Some). When the TDK is absent, the code falls back to a 'dev/test' branch that takes the tenant string, truncate(16)s it and pads to 16 chars — using a PUBLIC, NON-SECRET prefix of the tenant_id as the namespace. The TDK is loaded from env var R2_TDK_HEX, and the repo's own CHANGELOG/secrets state it is 'currently-unset, so no-op today' in production (CHANGELOG.md:37; durable_object.ts forwards R2_TDK_HEX: this.env.R2_TDK_HEX ?? "", and load_tdk_from_env returns None for an empty/absent value). Therefore production runs the raw-padded fallback for every request. Production tenant_ids are canonical hyphenated UUIDv7 lowercase strings (corelink-core/src/types/tenant.rs; signup.rs uses Uuid::now_v7()). The first 16 chars of a hyphenated UUIDv7 are tttttttt-tttt-7r — the entire 48-bit creation-millisecond timestamp (12 hex digits) plus the literal version nibble 7 plus exactly ONE random nibble. So the tenant's whole R2 prefix is a function of (creation millisecond, 4 random bits). Two tenants created in the same millisecond collide on 15 of 16 chars and have a 1-in-16 chance of a FULL prefix collision; tenants created near each other in time collide on the dominant timestamp portion. A full prefix collision means the two tenants share the SAME R2 key space. For CAS (content-addressed), tenant A can fetch tenant B's private blob by GETting a digest under A's own tenant, and a write deduplicates over / overwrites B's blob. For the Action Cache, the key is <region>/<prefix>/<action_digest>; two colliding tenants read and write each other's build-action results (cross-tenant AC poisoning into the victim's CI).

**attack_scenario:** An attacker registers many CoreLink accounts (self-serve SMB signup) to harvest tenant_ids; UUIDv7 leaks the exact creation millisecond, so the attacker can target a victim whose account was created in the same ms (or brute-force by registering rapidly to land in a colliding millisecond / random-nibble bucket). Once the attacker holds a tenant whose first-16-char UUIDv7 prefix equals the victim's, both tenants resolve to the same R2 prefix. The attacker then (a) READS the victim's private CAS blobs by requesting digests under the attacker's own authenticated tenant — every auth check passes because the attacker is acting as itself; isolation depended solely on the R2 prefix, which now collides — or (b) POISONS the victim's Action Cache by PUTting attacker-chosen result bytes for a known action_digest, which the victim's next build fetches as a cache hit and trusts.

**reproduction:** 1. Confirm prod state: R2_TDK_HEX unset (CHANGELOG.md:37 'currently-unset, so no-op today'; durable_object.ts:490 forwards ?? ""; load_tdk_from_env returns None on empty) => R2CasHandler/R2AcHandler.tdk = None.
2. Two tenants with UUIDv7 ids sharing the first 16 chars, e.g. A=0190abcd-1234-75ab-89de-0123456789ab, B=0190abcd-1234-75cd-aaaa-bbbbbbbbbbbb. r2_key(A,H) and r2_key(B,H) both = iad/0190abcd-1234-75/H (truncate(16) of both is 0190abcd-1234-75). Verified with a python sim: a[:16]==b[:16] for same-ms UUIDv7s.
3. As tenant A (valid PAT, x-corelink-tenant-id=A), PUT /v1/cas/A/<digest> with bytes X. R2 object written at iad/0190abcd-1234-75/<digest>.
4. As tenant B (valid PAT for B), GET /v1/cas/B/<digest>. Route check tenant(B)==auth.0(B) passes; r2_key(B,digest) resolves to the SAME key; R2 returns tenant A's bytes X. Cross-tenant read achieved.
5. AC variant: PUT /v1/ac/A/<action_digest> then GET /v1/ac/B/<action_digest> returns A's cached result (cross-tenant AC read/poison).

**impact:** Full cross-tenant confidentiality AND integrity break of the durable cache for any pair of tenants whose UUIDv7 first-16-char prefix collides. Blast radius: private source artifacts / build outputs in CAS (corelink-cas-prod) and Action Cache results (corelink-ac-iad) leak between tenants; an attacker can also overwrite/poison a victim's cache entries, turning the shared cache into a supply-chain injection vector into the victim's builds. Collision probability is far higher than the documented ~2^-96 design target because the namespace is no longer an HMAC but a public, low-entropy timestamp+1-nibble prefix (effective entropy ~4 bits for same-ms signups; the 48-bit timestamp is attacker-observable from the tenant_id itself). Launch-blocking.

**evidence:** r2_s3.rs:325-333 (CAS, active prod branch): `None => { // No TDK — dev/test mode: use padded tenant.\n let mut p = tenant.to_owned();\n p.truncate(16);\n while p.len() < 16 { p.push('0'); }\n p }` then `R2S3Client::blob_key(&self.cas_region, &prefix, digest)` (:335) => `format!("{region}/{tenant_prefix}/{digest}")` (:251-252). Identical fallback for AC at r2_s3.rs:680-687. TDK gating: `let tdk = tdk_bytes.map(TenantDerivationKey::from_bytes);` (:293) with load_tdk_from_env returning None when R2_TDK_HEX empty/absent (:921-936, doc: 'Returns None when not set, causing R2CasHandler to use the raw-padded fallback (dev/test mode)'). Prod-unset: CHANGELOG.md:37 '(currently-unset, so no-op today) gaps — R2_TDK_HEX'; worker/src/durable_object.ts:490 `R2_TDK_HEX: this.env.R2_TDK_HEX ?? ""`. Tenant text form: corelink-core/src/types/tenant.rs:10 'text form is canonical UUIDv7 hyphenated lowercase'. Route/handler cross-tenant checks (cas.rs:213 `if tenant != auth.0`, ac.rs:192, r2_s3.rs:722 `if req.tenant != req.caller_tenant`) all compare the caller against ITSELF and never inspect the physical R2 prefix.

**recommendation:** Make the TDK mandatory on the production storage path: when storage credentials are present but load_tdk_from_env() returns None, FAIL CLOSED — refuse to construct R2CasHandler/R2AcHandler (fall back to InMemory or return an explicit build error so the route does not mount) instead of silently keying by a public UUID prefix. In build_r2_cas_handler_from_env / build_r2_ac_handler_from_env change `let tdk_bytes = load_tdk_from_env();` to require Some and `return Some(Err("R2_TDK_HEX required for production tenant prefixing".into()))` on None. Remove the production-reachable raw-padded fallback from r2_key (gate it behind #[cfg(test)]), and always HMAC the FULL tenant id under the secret TDK via derive_prefix. Set R2_TDK_HEX (openssl rand -hex 32) as a container secret on all five regional deployments before serving real tenant traffic (docs/internal/secrets-checklist.md row 143). Note: enabling the TDK re-keys all objects, so plan a migration / accept a cold cache.

**confidence:** high

**verify:** I independently read every cited line. The mechanics are exactly as reported: the raw-padded 16-char fallback (r2_s3.rs:325-333 / 680-687) is live in prod because R2_TDK_HEX is documented-unset (CHANGELOG.md:37-38, durable_object.ts:490), and tenant isolation is keyed solely on the resulting R2 prefix with no secondary check (cross-tenant guards only compare caller==self). So the finding is REAL, not a test-only artifact or unreachable path: confirmed.

However the claimed CRITICAL rests on an inflated exploit model that the code refutes. (1) The fallback truncates the hyphenated UUIDv7 TEXT, so the 16-char prefix is full-48-bit-timestamp + version + only 4 random bits; a collision needs same-millisecond creation AND a 1/16 nibble match, and the attacker cannot control a specific victim's creation ms nor get feedback, so targeted exploitation is essentially blind. (2) CAS is content-addressed with read-side hash re-verification (r2_s3.rs:439) and write-side hash verification, so arbitrary cross-tenant CAS read and CAS poisoning (the finding's vector (a)) are not achievable — it reduces to a known-digest existence oracle. (3) Only AC (no content verification, r2_s3.rs:761-773) is genuinely poisonable, and only under the same narrow collision.

It is still a legitimate isolation weakness worth fixing (the secret-HMAC TDK design exists precisely to remove the public/predictable prefix, and it is off in prod) plus a latent accidental co-mingling risk at scale, so I confirm it but adjust severity to medium and flag the overstated CAS-read/poison and targeted-attack claims.

---

### [2] LOW — Secure derive_prefix path is unreachable when R2_TDK_HEX is unset (fail-open, no alarm) — defense mis-placed as optional enhancement

**surface:** Tenant isolation / R2 key-prefix derivation

**location:** crates/corelink-container/src/storage/r2_s3.rs:308-333 (Some(tdk) vs None match arms) and :594-609 / :901-915 (build_*_from_env)

**cvss_reasoning:** CVSS 3.1 AV:N/AC:H/PR:L/UI:N/S:C/C:H/I:H/A:N. Same exploit consequences as the primary finding; scored High rather than Critical because this entry is the architectural root-cause / defense-in-depth observation (fail-open + no alarm) rather than an independent exploit. It is what makes the primary finding silently reachable in prod whenever the operator has not set the secret.

**explanation:** The secure code path that calls derive_prefix(tdk, uuid) lives ONLY inside the Some(tdk) match arm (r2_s3.rs:309-314). The None arm (taken whenever R2_TDK_HEX is unset — the current production state) does NOT even attempt Uuid::try_parse; it jumps straight to the raw-padded fallback for every tenant. The security posture is therefore binary and brittle: with the TDK present, prod tenants (hyphenated UUIDv7) hit derive_prefix and are safe; with the TDK absent, EVERY tenant silently degrades to the insecure public-prefix scheme with no tracing::error and no fail-closed. The in-code comments ('Non-UUID tenant (test mode)') imply the fallback is only for test fixtures, but the None-arm fallback is wholly independent of whether the tenant is a valid UUID — a perfectly valid production UUID tenant takes the insecure path purely because the secret is unset. Collision-resistant prefixing must be a hard precondition of the production R2 handler, not an optional enhancement that fails open.

**attack_scenario:** Same end-state as the primary finding (cross-tenant CAS/AC co-residence). This entry documents WHY the insecure branch is reachable in production: the secure derive_prefix is unreachable whenever R2_TDK_HEX is unset, which is the documented current production reality, and nothing fails closed or logs an error to flag the degraded mode — so the operator has no signal that tenant isolation has silently dropped to a public-prefix scheme.

**reproduction:** Static: inspect r2_s3.rs:307-336. With tdk=None (R2_TDK_HEX unset), r2_key('0190abcd-1234-75ab-89de-0123456789ab', digest) never calls Uuid::try_parse and returns prefix '0190abcd-1234-75' (truncate(16)+pad), confirming the secure derive_prefix path is bypassed for a valid UUID tenant whenever the secret is missing. build_r2_cas_handler_from_env (:594-609) proceeds to construct the handler with tdk_bytes=None and logs no error.

**impact:** Defense-in-depth failure that converts a missing-secret operational gap into a silent multi-tenant isolation break with no alarm. Same data/integrity blast radius as the primary finding; called out separately because the fix (fail-closed when creds present but TDK missing, always HMAC the full id) is a distinct code change from merely setting the secret.

**evidence:** r2_s3.rs:308-333 — the Some(tdk) arm contains `if let Ok(uid) = Uuid::try_parse(tenant) { derive_prefix(tdk, uid).to_string() } else { ...raw-padded... }`, while the `None => { ...raw-padded... }` arm has NO derive_prefix and NO UUID parse. load_tdk_from_env doc (:919-920): 'Returns None when not set, causing R2CasHandler to use the raw-padded fallback (dev/test mode).' No tracing::error or fail-closed accompanies the None path in build_r2_cas_handler_from_env (:594-609) / build_r2_ac_handler_from_env (:901-915).

**recommendation:** Treat 'storage credentials present' and 'TDK present' as a single invariant for the real handler. In both build_r2_*_handler_from_env, require the TDK and return Err on None (handler not constructed => route does not mount => fail closed). Collapse r2_key to a single path that always HMACs the full tenant id under the TDK; delete the production-reachable raw-padded branch (keep it only under #[cfg(test)] with an explicitly fake TDK). Add a startup assertion plus a structured error log if creds exist but R2_TDK_HEX is missing so the degraded mode is loud, not silent.

**confidence:** high

**verify:** The finding's FACTUAL premise is correct, but its severity framing ("same end-state as cross-tenant CAS/AC co-residence", HIGH) is overclaimed and does not hold up against the real production config.

WHAT IS TRUE (verified):
1. The data-plane CAS handler mounts the real R2CasHandler whenever storage creds are present, independent of TDK: routes/cas.rs:121 `if StorageEnv::from_env().is_some()` → cas.rs:135 build_r2_cas_handler_from_env → cas.rs:138 mounts it ("R2S3 (real storage)"). Same for AC (ac.rs:127).
2. build_r2_cas_handler_from_env (r2_s3.rs:594-609) and build_r2_ac_handler_from_env (:901-916) call load_tdk_from_env(), which returns None silently when R2_TDK_HEX is unset (:921-937) — no error log, no fail-closed. The handler is constructed with tdk_bytes=None.
3. With tdk=None, r2_key (r2_s3.rs:325-333) takes the `None =>` raw-padded branch: NO derive_prefix, NO UUID parse — prefix = first-16-chars-padded of the tenant id.
4. R2_TDK_HEX is in fact currently UNSET in prod (CHANGELOG.md:37: "currently-unset, so no-op today"), so the secure derive_prefix path IS unreachable on the data plane today, exactly as claimed.
5. The posture is INCONSISTENT: cas_erase (cas_erase.rs:620-626, main.rs:357-368) and the DSR R2 adapters (adapter_r2_cas.rs:164, adapter_r2_ac.rs:155) DO fail closed / return Err on missing TDK; the primary CAS/AC read+write data plane does NOT.

WHY IT IS NOT HIGH (the overclaim refuted):
- The tenant id fed to r2_key is auth.0, the Worker-resolved, D1-backed, HMAC-gated PAT tenant_id (worker/src/index.ts:571-575,687-690), and the path :tenant is checked against it with a 403 on mismatch (cas.rs:213, cas.rs:282). A tenant cannot forge or choose another tenant's id.
- Production tenant ids are server-generated random v4 UUIDs (apps/signup-worker/src/webhooks/clerk.ts:686 `const tenantId = crypto.randomUUID()`). Even in the raw-padded branch, truncate(16) of a UUID = first 16 chars "xxxxxxxx-xxxx-x" → ~48 bits of per-tenant entropy that remain DETERMINISTICALLY DISTINCT across tenants. A cross-tenant prefix COLLISION would need two distinct random UUIDs to share their first 16 chars (~2^48 targeted, and the attacker cannot pick their UUID). So there is NO practical cross-tenant CAS/AC co-residence — the "same end-state as the primary finding" claim is unsubstantiated.

WHAT THE BUG ACTUALLY IS (correctly scoped): a defense-in-depth + observability regression. Per the secrets checklist (docs/internal/secrets-checklist.md:207) the TDK provides "per-tenant CAS encryption" and unpredictable/non-enumerable prefixes (the keyed layer of INV-TENANT-ISOLATION layer 5). Unset → prefixes become PREDICTABLE (derivable from a known UUID) and unkeyed, and the degraded mode is SILENT with no alarm and inconsistent vs sibling fail-closed adapters. That is a real hardening + alerting gap worth fixing (require TDK when creds present; fail closed + structured error so the degraded mode is loud), but it is not an auth bypass or a tenant-isolation break in the current config. Severity: low (defense-in-depth / observability), not high.

---

### [3] LOW — Native data-plane (CAS/AC/users/bazel/turbo/customer/audit) does NOT re-verify the PAT in the container — possession rests on a single Worker-side HMAC gate, and the code comment claiming an Argon2id second layer is false

**surface:** AuthN/AuthZ full-chain — PAT possession / container data-plane trust model

**location:** worker/src/index.ts:1789-1792 (claim) + crates/corelink-container/src/auth_tenant.rs:21-37 (actual enforcement) + crates/corelink-container/src/routes.rs (no PAT middleware on native routes)

**explanation:** CoreLink's native cache/data-plane routes (CAS read/write, AC, /v1/users/me, Bazel REAPI v2, Turborepo v8, the customer portal, audit) determine WHICH tenant a request acts as entirely from the `x-corelink-tenant-id` HTTP header that the edge Worker injects. Inside the container this is read by the `AuthTenant` extractor (auth_tenant.rs), which only checks the value is non-empty and not a sentinel — it performs NO cryptographic verification of the caller's PAT. The actual possession check for these routes happens ONLY once, at the edge Worker, in two steps: (1) an HMAC-SHA256 fast-fail over the PAT (which requires the server `PAT_SIGNING_KEY`), and (2) a D1 lookup that the `token_id` exists and is unexpired/unrevoked. The Worker deliberately SKIPS the Argon2id verify of the PAT's `random_secret` against the stored hash for CPU-budget reasons (index.ts:646-662). The Worker's own forwarding comment at index.ts:1789-1792 asserts 'the DO performs the Argon2id + scope verify against the D1 PAT store' — but there is no such middleware: grep of crates/corelink-container/src for Argon2id/verify_with_hash on the native plane finds it ONLY in adapter_pat.rs, which is wired exclusively to the cache ADAPTERS (cargo/brew/npm/pip/oci), not to the native CAS/AC/users/bazel/turbo/customer/audit routes. So the documented second defensive layer for the native plane does not exist. The practical consequence: the entire possession guarantee for the highest-traffic surface depends on a single control — the Worker's HMAC gate — which is itself only active when `PAT_SIGNING_KEY` is bound (index.ts:663). If `PAT_SIGNING_KEY` were ever unbound or empty in an environment, the HMAC step is skipped entirely and possession collapses to merely knowing a valid 16-char `token_id` that exists in D1 (the `random_secret` and `hmac_sig` are never checked anywhere on the native path), which is a guessable/low-entropy identifier compared to the 256-bit secret it is supposed to gate.

**attack_scenario:** Primary (real, narrow): an operator misconfiguration or secret-rotation gap leaves `PAT_SIGNING_KEY` unbound on one env/region Worker. On that Worker the HMAC fast-fail is skipped; an attacker who learns or guesses any live `token_id` (a non-secret 16-char Crockford-b32 value that appears in logs/correlation, error correlation, or is brute-forceable at 2^80 but leaks via the D1 'pat_not_found' vs success distinction) can mint a request bearing `Bearer corelink_pat_<token_id>.<any-43-char-b64>.<any-22-char-b64>` and be resolved to that token's tenant, then read/write that tenant's CAS/AC data — with NO Argon2id check anywhere to stop it. Secondary (defense-in-depth): because the comment falsely asserts a container-side Argon2id layer, a future change that (e.g.) loosens the Worker HMAC gate 'because the DO re-verifies anyway' would silently remove the ONLY possession control on the native plane.

**reproduction:** 1. In a test/staging Worker, leave `PAT_SIGNING_KEY` unset (the guard at index.ts:663 makes the HMAC step a no-op). 2. Seed a D1 `pat` row for victim tenant T with token_id `ABCDEFGHJKMNPQRS`, any pat_hash, scope `read-write`, revoked_at_ms NULL, expires_ms in the future. 3. Send `GET /v1/cas/blobs/<digest>/<size>` with header `Authorization: Bearer corelink_pat_ABCDEFGHJKMNPQRS.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.AAAAAAAAAAAAAAAAAAAAAA` (95/96-char canonical shape, arbitrary secret+sig). 4. Observe extractAuth parses the shape (parsePat ok), skips HMAC (key unbound), D1 lookup by token_id succeeds, expiry passes → resolvedTenantId=T, forwarded with `x-corelink-tenant-id: T`; container `AuthTenant` accepts it and serves tenant T's data. No Argon2id ever runs. 5. With `PAT_SIGNING_KEY` bound the same request is rejected at the HMAC gate — confirming the single point of failure.

**impact:** Blast radius = full cross-tenant read/write of the native cache/data plane for ANY tenant whose token_id is known, but ONLY in an environment where PAT_SIGNING_KEY is unbound (a misconfiguration, not the steady state). In the steady state (key bound, as in prod per CLAUDE.md) the HMAC gate holds and this is a latent single-layer-of-defense / inaccurate-invariant issue rather than an active breach. The reason it is not Low: the native plane is the primary surface, the protective comment is actively false (inviting a future regression), and the failure mode (one missing secret on one of 5 regional Workers) is realistic given the documented secret-rotation/multi-region complexity.

**evidence:** auth_tenant.rs:25-36 — `let raw = parts.headers.get("x-corelink-tenant-id")...; if raw.is_empty() || SENTINELS.contains(&raw) { return Err(...UNAUTHORIZED...) } Ok(AuthTenant(raw.to_owned()))` (no crypto). index.ts:663-672 — `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0) { const hmacOk = await verifyPatHmac(...); if (!hmacOk) return {ok:false,...}; }` (HMAC entirely skipped when key unbound). index.ts:1789-1792 — `// Keep raw Authorization on the forwarded request: the DO performs the Argon2id + scope verify against the D1 PAT store (the possession check the Worker skips...)` — contradicted by routes.rs where Argon2id (adapter_pat::PatVerifier) is wired ONLY to cargo/brew/npm/pip/oci adapters, and cas.rs/users.rs/bazel_v2.rs/turbo_v8.rs use `crate::auth_tenant::AuthTenant` with no verifier.

**recommendation:** Two concrete changes: (1) Make the native-plane possession self-verifying like the adapters: add a Tower middleware in the container (or per-route extractor) that runs `adapter_pat::PatVerifier::verify(raw_pat)` against the raw Authorization header and ASSERTS the resolved tenant equals the Worker-supplied `x-corelink-tenant-id` (fail-CLOSED 401/403 on mismatch). This is the Option-B model already proven for the cache adapters — extend it to cas/ac/users/bazel/turbo/customer/audit so a single misconfigured Worker cannot grant native-plane access. (2) Until (1) lands, FAIL CLOSED in the Worker when `PAT_SIGNING_KEY` is unbound for any PAT-gated route (replace the `if (key)` opt-in at index.ts:663 with a hard 503 when the key is absent on a route that needs it), and FIX the false comment at index.ts:1789-1792 to state accurately that the native plane currently has no container-side Argon2id re-verify.

**confidence:** high

**verify:** The finding is structurally real on all three code-level claims, but the claimed severity of "medium" overstates the exploitability under production conditions. Here is the full breakdown:

**Claim 1: Native data-plane routes have no container-side PAT possession re-verify — CONFIRMED.**

`crates/corelink-container/src/routes/cas.rs`, `ac.rs`, `bazel_v2.rs`, `turbo_v8.rs`, and `users.rs` all use the `crate::auth_tenant::AuthTenant` extractor. `auth_tenant.rs:24-35` reads only the `x-corelink-tenant-id` HTTP header and checks it against known sentinel values — no HMAC, no Argon2id, no D1 lookup. By contrast, cargo/brew/npm/pip/oci adapters (routes.rs:435-535) are gated by `crate::adapter_pat::PatVerifier::from_env()` which runs the full three-step pipeline: HMAC fast-reject → D1 lookup → Argon2id verify (`adapter_pat.rs:214-278`). The asymmetry is real and documented.

**Claim 2: The comment at index.ts:1789-1792 is false — CONFIRMED.**

The comment at index.ts:1789-1792 states: "the DO performs the Argon2id + scope verify against the D1 PAT store." A search of `durable_object.ts` finds zero Argon2id invocations. The JS DO is a lifecycle proxy that forwards requests to the container via `getTcpPort(50051)`. The Rust container's native routes (`auth_tenant.rs`) perform no such check. An older comment block at index.ts:583-590 makes the same false claim: "The Rust DO (corelink-worker Tower middleware) performs the Argon2id verify on the raw token forwarded via the Authorization header." There is no Tower middleware implementing this. The comment propagates a false mental model of the security architecture.

**Claim 3: HMAC is skipped when PAT_SIGNING_KEY is unbound — CONFIRMED.**

index.ts:663: `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0)` — the HMAC gate is opt-in. If the secret is absent from a Worker deployment, the gate is silently skipped and only the D1 existence + expiry check remains as the possession control for native routes.

**Why severity is LOW, not MEDIUM:**

The finding's attack narrative depends on two preconditions that significantly reduce real-world risk:

1. **The container is not internet-accessible.** The Rust container listens on port 50051; it is only reachable via Cloudflare's internal `getTcpPort` fetcher called from the Durable Object (`durable_object.ts:215, 329`). An external attacker cannot send HTTP directly to the container — all traffic must pass through the Worker, which does perform the HMAC gate (when `PAT_SIGNING_KEY` is bound) and the D1 lookup. The attack surface is the Worker misconfiguration scenario, not an open container port.

2. **The token_id enumeration oracle is weak in practice.** The token_id is a 16-character Crockford-b32 string (80-bit entropy). Even if `PAT_SIGNING_KEY` is absent, an attacker still needs a valid live token_id that exists in D1 and is not expired/revoked. The D1 `pat_not_found` vs. success timing distinction theoretically creates an oracle, but exploiting this at 80-bit entropy space requires the token_id to leak via logs or correlation headers — a secondary vulnerability.

Under production conditions with `PAT_SIGNING_KEY` bound (which is the documented and expected state), the HMAC gate is active and forging a request to the native plane requires compromising the signing key. The gap is a defense-in-depth failure and a false comment, not an exploitable path under normal deployment.

**What is real and actionable:**

- The false comment at index.ts:1789-1792 (and 583-590) should be corrected to accurately state the architecture.
- The `if (key)` opt-in at index.ts:663 should be hardened to fail-CLOSED (503) when `PAT_SIGNING_KEY` is absent on PAT-gated routes rather than silently skipping the possession gate.
- As defense-in-depth, wiring `PatVerifier` on the native CAS/AC/Bazel/Turbo routes (as the adapters already do) would eliminate the entire class of "single-Worker-gate failure" and make the comment true. This is the "Option-B extension" recommended in the finding and is the correct long-term fix.

Severity adjusted to LOW: real architecture/comment gap + one misconfiguration risk, but not exploitable in the current production deployment without `PAT_SIGNING_KEY` being unset AND a token_id leaking.

---

### [4] MEDIUM — Single shared internal-auth secret (CORELINK_INTERNAL_AUTH_KEY) is the sole gate for operator-grade /_internal/pat/mint, reachable from the public internet through the edge Worker — a leak yields full cross-tenant admin-PAT minting

**surface:** AuthN/AuthZ full-chain — internal-auth gate / privilege concentration

**location:** worker/src/index.ts:1194-1255 (public /_internal/* arm) + crates/corelink-container/src/routes/internal_pat.rs:243-317 (mint derives tenant_id from request body)

**explanation:** The `/_internal/pat/mint` route can mint a PAT for ANY `tenant_id` and ANY scope (including full `admin`, which maps to SCOPE_ADMIN_ALL) — the tenant and scope are taken straight from the JSON request body (internal_pat.rs:131-141, 296-297) with no further authorization beyond a single shared-secret header check. That same secret, `CORELINK_INTERNAL_AUTH_KEY`, also gates the onboarding/tier-select money path and the `internal` Worker arm. Crucially, the `/_internal/*` route is matched and served by the PUBLIC edge Worker (index.ts matchRoute maps `/_internal/*` → routeKind 'internal'), gated only by a constant-time compare of the caller-supplied `x-corelink-internal-auth` header against the secret (index.ts:1204-1235). The constant-time compare itself is correct and fail-closed. But this means the secret is a single, internet-reachable, operator-grade credential: anyone who can present it to `https://<worker-host>/_internal/pat/mint` can mint a full-admin PAT for an arbitrary tenant UUID and then act as that tenant (or, with scope `admin`, obtain cache rw for any tenant they name). The secret is shared across the edge Worker AND the separate signup-worker (per internal_pat.rs:13-16), widening the surface from which it could leak (logs, a compromised signup-worker, a misconfigured wrangler secret dump).

**attack_scenario:** An attacker obtains `CORELINK_INTERNAL_AUTH_KEY` via any side channel — a leaked `.env.local` backup (CLAUDE.md notes real values live only there and 'must be backed up'), a compromised signup-worker that carries the same secret, an accidental log of the header, or an insider. They then POST `https://corelink-api.humangr.com/_internal/pat/mint` with header `x-corelink-internal-auth: <secret>` and body `{"tenant_id":"<any victim tenant uuid>","principal_id":"<any uuid>","scopes":"admin","ttl_seconds":31536000}`. The Worker validates the secret, forwards to the `_system` DO with the server-set internal-auth header, the container mints a year-long full-admin PAT for the victim tenant and returns the plaintext. The attacker now holds a valid PAT for the victim and can read/write all their cache data and (via admin scope = cache superset) their CAS/AC.

**reproduction:** 1. Acquire the value of CORELINK_INTERNAL_AUTH_KEY (out-of-band; this finding is about blast radius given a leak, not a way to obtain it). 2. `curl -XPOST https://<worker>/_internal/pat/mint -H 'x-corelink-internal-auth: <secret>' -H 'content-type: application/json' -d '{"tenant_id":"<victim-uuid>","principal_id":"00000000-0000-0000-0000-000000000001","scopes":"admin","ttl_seconds":31536000}'`. 3. Receive `{token_plaintext: corelink_pat_..., ...}`. 4. Use that PAT as `Authorization: Bearer` against any victim-tenant route — it resolves to the victim tenant. The mint never checks that the caller is authorized FOR that specific tenant_id; the shared secret authorizes minting for ALL tenants.

**impact:** Full multi-tenant compromise contingent on one secret. Because the mint takes tenant_id from the body with no per-tenant authorization, the secret is effectively a master key over every tenant's data and over the billing/onboarding mutation path (same secret gates tier_select). Sharing it with the signup-worker doubles the leak surface. This is privilege concentration: there is no second factor, no per-tenant scoping, and no separation between 'mint for tenant X' and 'mint for any tenant'.

**evidence:** internal_pat.rs:131-141 `pub struct MintRequest { pub tenant_id: Uuid, pub principal_id: Uuid, pub scopes: String, ... }` and :296-297 `let tenant_id = TenantId(req.tenant_id); let principal_id = PrincipalId(req.principal_id);` — tenant taken verbatim from the body. :274-285 `"admin" => PatScopes::from_u64(SCOPE_ADMIN_ALL)`. index.ts:1194-1235 the public 'internal' arm gates solely on `crypto.subtle.timingSafeEqual(fixed, expectedBytes) && lenEqual` of the caller header vs `env.CORELINK_INTERNAL_AUTH_KEY`. internal_pat.rs:13-16 doc: 'The Worker AND signup-worker both carry the same secret.'

**recommendation:** (1) Separate secrets per consumer (the auth_introspect route already models this with a DEDICATED FABRIC_INTROSPECT_AUTH_KEY — do the same so the mint secret, the onboarding/tier-select secret, and the fabric secret are distinct; a leak of one cannot exercise the others). (2) Strongly prefer NOT exposing `/_internal/pat/mint` through the public edge Worker at all — restrict it to a Worker-to-Worker Service Binding from the signup-worker (no public route), so the mint surface is unreachable from the internet even with the secret. (3) Add per-tenant authorization to the mint: require the caller to prove authorization for the specific `tenant_id` (e.g. the verified Clerk session already resolved in session_exchange.ts), rather than letting the shared secret authorize minting for ALL tenants. (4) Rotate-on-leak: ensure the secret is short-TTL-rotatable and never logged.

**confidence:** high

**verify:** The finding is confirmed as a real architectural concern, but the claimed severity vector deserves precise framing.

What is real and verified in code:

1. PUBLIC REACHABILITY: `/_internal/` routes are reachable from the public internet. The route parser in `worker/src/index.ts:509-510` (`if (path.startsWith("/_internal/"))`) matches all paths starting with `/_internal/` and assigns `routeKind: "internal"`. The main worker is bound to `corelink-api.humangr.com/*` (wrangler.toml:266) with `workers_dev = false` but no WAF rule, CF Access policy, or IP allowlist restricting `/_internal/` at the zone or edge level. A request from any IP on the public internet to `https://corelink-api.humangr.com/_internal/pat/mint` reaches the `internal` arm of the Worker handler.

2. SINGLE SECRET GATE: The only protection at the Worker layer (index.ts:1195-1234) is a constant-time comparison of the caller's `x-corelink-internal-auth` header against `env.CORELINK_INTERNAL_AUTH_KEY`. The comparison is technically correct (padded ct_eq + length bit). But the architectural fact stands: one secret == full access to this surface.

3. SHARED SECRET ACROSS CONSUMERS: The comment in `internal_pat.rs:14-16` explicitly states "The Worker AND signup-worker both carry the same secret." The signup-worker's `wrangler.toml:23` and `apps/signup-worker/src/lib/corelink-internal.ts:40,65` confirm it. Same secret, two separate deployed Workers with distinct blast radii if either is compromised.

4. ARBITRARY TENANT TARGETING: The Rust handler in `internal_pat.rs:296-297` takes `tenant_id` verbatim from the request body: `let tenant_id = TenantId(req.tenant_id); let principal_id = PrincipalId(req.principal_id);`. There is no server-side check that the caller has any relationship to the specified tenant. Anyone with the secret can mint for any tenant UUID.

5. ADMIN SCOPE AVAILABLE: `internal_pat.rs:274-275` maps the string `"admin"` to `PatScopes::from_u64(SCOPE_ADMIN_ALL)`, which is the union of all scope bits: `SCOPE_CACHE_RW | SCOPE_ADMIN_TENANT_R | SCOPE_ADMIN_TENANT_W | SCOPE_ADMIN_TOKENS | SCOPE_ADMIN_BILLING | SCOPE_ADMIN_AUDIT | SCOPE_ADMIN_USERS`.

What the finding overstates (severity calibration):

The finding claims severity "medium" and the agent's own recommendation treats it as a significant architectural gap. This calibration is broadly correct, but the actual exploit requires FIRST obtaining `CORELINK_INTERNAL_AUTH_KEY`, which is a Cloudflare Worker secret (write-only from CF's perspective, never readable via the CF API). The threat is a conditional one:

- Leak vector A: `.env.local` backup. CLAUDE.md notes real values live there and "must be backed up." If a backup is stored insecurely (unencrypted cloud sync, email, etc.), the secret leaks.
- Leak vector B: A compromised signup-worker deployment (e.g. via a malicious dependency, a CF account takeover, or a misconfigured wrangler deploy pipeline) could expose the secret.
- Leak vector C: Accidental logging. `internal_pat.rs` explicitly redacts the key in Debug (`[REDACTED]`), and the Worker does not log it, so this vector is low probability but not zero (e.g. a future logging change, error reporting SDK, etc.).
- Leak vector D: The secret is the SAME for all consumers. A leak from the lower-trust signup-worker path unlocks the higher-privilege mint endpoint.

The signup-worker itself actually does have a Service Binding (`CORELINK_API_SVC` in `apps/signup-worker/wrangler.toml:69-71`), and in the `clerk.ts` webhook at line 780-782 it prefers the Service Binding over a public fetch when `env.CORELINK_API_SVC` is present. However the corelink-internal.ts library (used from DSR consumer) still does a plain `fetch()` to `env.CORELINK_API_BASE` (the public URL). This means the shared secret travels over a public HTTP path in at least some code paths, not exclusively through a Service Binding.

The finding does NOT claim the secret is already leaked or that the constant-time check is broken — both of those are correct. This is a "blast radius after compromise" and "defense-in-depth gap" finding, not an active zero-day. The adjusted severity stays at medium: the conditional chain (secret leak → public endpoint accessible → any-tenant admin PAT) is real, but requires a precondition.

There is no Cloudflare WAF rule, CF Access policy, or IP allowlist covering `/_internal/` in any file in this repository. The sole layer of protection for this cross-tenant-impersonation-capable endpoint is the application-level shared secret.

Evidence summary:
- Public route registration: `worker/src/index.ts:509-510`
- Worker auth gate (correct but sole gate): `worker/src/index.ts:1194-1235`
- Arbitrary tenant_id from body: `crates/corelink-container/src/routes/internal_pat.rs:296-297`
- Admin scope available to any authenticated caller: `internal_pat.rs:274-275`
- Shared secret in signup-worker: `apps/signup-worker/wrangler.toml:22-24`, `apps/signup-worker/src/lib/corelink-internal.ts:40,65`
- No zone-level WAF block on /_internal: grep of all scripts and config confirmed no such rule exists in the codebase

---

### [5] MEDIUM — Production Action-Cache handler (R2AcHandler) silently overwrites a proven AC result with divergent bytes — missing the 409 DivergentBody guard → AC cache poisoning on the native AC + Bazel REAPI surfaces

**surface:** Cache poisoning — Action Cache (AC) integrity / REAPI v2

**location:** crates/corelink-container/src/storage/r2_s3.rs:802-887 (R2AcHandler::update)

**explanation:** CoreLink's Action Cache (AC) maps an action_digest (a hash of the BUILD ACTION, not of its result) to a result payload. Because the key is the hash of the action and NOT of the bytes stored, the bytes are NOT self-verifying the way CAS bytes are — there is no way to detect after the fact whether the stored result is the real one. REAPI/Bazel semantics therefore require that once a result is proven and cached for an action_digest, it must NEVER be silently replaced with different bytes; a divergent re-PUT must be rejected. The codebase clearly knows this: the in-memory handler (corelink-handler-ac/src/handler.rs:336-347) returns `AcHandlerError::DivergentBody` (mapped to HTTP 409 in routes/ac.rs:282-285) when the same (tenant, action_digest) receives different bytes, preserving the original; and the canonical (not-yet-wired) worker handler implements the same as `UpdateResultMismatch` (corelink-worker/src/reapi/ac/handler/methods.rs:408,424-438). But the ONLY AC handler actually mounted in production — `R2AcHandler::update` — does NOT perform this check. It probes existence purely to set the `durable` boolean (`pre_existed = matches!(... Ok(Some(_)))`, r2_s3.rs:857-860) and then unconditionally `put`s the new bytes, overwriting any existing result. WHY it matters: anyone able to write to a tenant's AC (any `cas:rw`/`read-write` PAT for that tenant — e.g. a low-trust CI job token, a malicious or compromised teammate, or a stolen build token) can replace a legitimate cached ActionResult with a poisoned one. Every subsequent build in that tenant/org that hits the same action_digest will then receive the attacker's outputs AS IF they were the trusted, reproducible build output — the classic AC poisoning attack the 409 guard exists to stop. HOW exploited: PUT the action_digest once with honest bytes, then PUT the same action_digest with malicious bytes; the second PUT returns 200/201 and the poisoned result is served to all readers, instead of the expected 409 Conflict.

**attack_scenario:** Tenant T runs CI with a self-serve `read-write` PAT. A second, lower-trust actor inside the same tenant/org (a contractor CI job, a compromised runner, or a teammate) also holds a `cas:rw` PAT for T. They observe (or guess) the action_digest of a hot, frequently-rebuilt action (deterministic — same toolchain+inputs produce the same action_digest across the org). They PUT that action_digest with a malicious ActionResult (e.g. pointing at trojaned output blobs / a backdoored compiled artifact). The production R2AcHandler overwrites the genuine entry. From then on, every developer/CI build that resolves that action_digest gets a cache HIT serving the attacker's outputs, with no 409 and no integrity error, until the entry is evicted.

**reproduction:** 1. Authenticate to the container AC route as tenant T with a `cas:rw` scope (x-corelink-tenant-id: T, x-corelink-scope: cas:rw — as injected by the Worker after PAT resolution). 2. PUT /v1/ac/T/<action_digest> with body = honest ActionResult bytes → 201 Created. 3. GET /v1/ac/T/<action_digest> → 200, honest bytes (cache populated). 4. PUT /v1/ac/T/<action_digest> AGAIN with body = different (malicious) bytes. EXPECTED (per routes/ac.rs map_err + InMemory/canonical handler): 409 Conflict 'divergent body', original preserved. ACTUAL (R2AcHandler in prod): 200 OK, the entry is overwritten. 5. GET /v1/ac/T/<action_digest> → 200, now returns the MALICIOUS bytes. The 409 arm in routes/ac.rs:282 is dead code on the prod path because R2AcHandler never returns DivergentBody.

**impact:** Within-tenant (and within-org, since teams share a tenant) Action-Cache poisoning across BOTH the native `PUT/GET /v1/ac/...` route and the Bazel REAPI v2 surface (routes.rs:341 builds the bazel handlers from the SAME ac_update trait object). A poisoned ActionResult causes trusted builds to consume attacker-controlled outputs — supply-chain compromise of every build that hits the poisoned action_digest. It is NOT a cross-tenant break (the route's tenant==auth.0 gate and the handler's req.tenant==req.caller_tenant check hold), so blast radius is one tenant/org at a time, but for a multi-team org tenant the integrity guarantee the product advertises (REAPI AC immutability of proven results) is silently absent. CAS bytes are unaffected (CAS verifies content_hash on both write and read), so the poisoning is confined to the AC result-payload layer.

**evidence:** R2AcHandler::update (the production handler) — NO divergence check, blind overwrite:
```rust
// r2_s3.rs:857-878
let pre_existed = matches!(
    tokio::task::block_in_place(|| handle.block_on(self.client.get(&key))),
    Ok(Some(_))
);
let result = tokio::task::block_in_place(|| {
    handle.block_on(self.client.put(&key, req.result_payload))   // unconditional overwrite
});
match result {
    Ok(()) => { /* emit UpdateCommitted */ Ok(AcUpdateResponse::new(req.action_digest, !pre_existed)) }
```
Contrast — the InMemory handler the route's 409 arm was designed around DOES guard (handler.rs:336-347):
```rust
if let Some(existing) = g.get(&key) {
    if *existing != req.result_payload {
        return Err(AcHandlerError::DivergentBody { tenant: req.tenant, action_digest: req.action_digest });
    }
```
And the route maps that to 409 (routes/ac.rs:282-285) — dead code on the R2 path.

**recommendation:** Make R2AcHandler::update enforce the divergent-body invariant before PUT, matching the InMemory/canonical semantics. Replace the existence-only probe with a read-and-compare: if the GET returns Some(existing) and existing != req.result_payload, return Err(AcHandlerError::DivergentBody { tenant, action_digest }) (no PUT, audit the mismatch); if existing == req.result_payload, idempotent no-op (durable=false); only PUT when absent (durable=true). Concretely:
```rust
let existing = tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)));
match existing {
    Ok(Some(b)) if b != req.result_payload => {
        // emit CorrectnessViolation/UpdateResultMismatch audit
        return Err(AcHandlerError::DivergentBody { tenant: req.tenant, action_digest: req.action_digest });
    }
    Ok(Some(_)) => return Ok(AcUpdateResponse::new(req.action_digest, false)), // idempotent
    Ok(None) => { /* PUT, durable=true */ }
    Err(e) => return Err(AcHandlerError::Internal(e)), // fail-closed, don't blind-overwrite on ambiguous state
}
```
Add a regression test mirroring `update_divergent_body_returns_conflict_and_preserves_original` against R2AcHandler. Note the GET→compare→PUT is non-atomic against concurrent writers; if strict atomicity is required, gate on R2 conditional PUT (If-None-Match) or a D1 ac_meta row with the result hash as the canonical guard.

**confidence:** high

**verify:** I independently reproduced the exact vulnerable path. Every claim in the finding checks out against the real code:

1. PRODUCTION HANDLER BLIND-OVERWRITES — confirmed. crates/corelink-container/src/storage/r2_s3.rs:857-864: the R2AcHandler::update only probes existence (`let pre_existed = matches!(... self.client.get(&key)), Ok(Some(_)))`) and then unconditionally PUTs: `handle.block_on(self.client.put(&key, req.result_payload))`. There is NO read-and-compare; a divergent body for an existing (tenant, action_digest) silently replaces the proven result. The `pre_existed` flag is used only to compute the `durable` response field, never to reject the write.

2. CANONICAL INVARIANT EXISTS AND IS ENFORCED ELSEWHERE — confirmed. crates/corelink-handler-ac/src/handler.rs:336-347: InMemoryAcHandler::update guards `if *existing != req.result_payload { ... return Err(AcHandlerError::DivergentBody {...}) }` with the explicit comment "NEVER overwrite a proven AC result." So the no-silent-overwrite property is a deliberate, documented invariant — dropped on the R2 path.

3. ROUTE 409 ARM IS DEAD ON THE R2 PATH — confirmed. crates/corelink-container/src/routes/ac.rs:282-286 maps AcHandlerError::DivergentBody → 409 Conflict, but R2AcHandler::update can only ever return Ok or AcHandlerError::Internal, so that arm is unreachable when the prod handler is wired.

4. R2AcHandler IS THE PRODUCTION HANDLER — confirmed. crates/corelink-container/src/routes/ac.rs:113-150: when StorageEnv::from_env().is_some() (true in prod, where R2 creds are set), build_handlers() returns the R2AcHandler behind BOTH the AcLookupHandler and AcUpdateHandler trait objects; InMemory is only the dev/no-creds fallback (lines 153-160).

5. BOTH THE NATIVE AC AND BAZEL REAPI v2 SURFACES SHARE IT IN PROD — confirmed. routes.rs:319 `let (ac_lookup, ac_update) = ac::build_handlers();` then routes.rs:341 `bazel_v2::build_handlers_from(cas_read, cas_write, ac_lookup, ac_update)` passes the SAME trait objects into the Bazel REAPI router, matching the comment in bazel_v2.rs:141-142. So both surfaces inherit the blind overwrite.

The finding is real and well-evidenced. I am DOWNGRADING severity from the claimed HIGH to MEDIUM for two honest reasons that the finding's own narrative concedes but its severity does not fully discount:

(a) NOT cross-tenant. r2_s3.rs:813-828 denies `req.tenant != req.caller_tenant` (CrossTenantDenied), the route (ac.rs:237-239) denies `tenant != auth.0`, and r2_key (r2_s3.rs:666-689) derives a per-tenant prefix via the TDK, so a different tenant gets a different key. There is no tenant-isolation break here — poisoning is strictly WITHIN one tenant's namespace.

(b) The attacker must already hold a valid write-capable credential. The update route enforces scope.can_write() / cas:rw (ac.rs:243-245). So the actor is, by definition, already authorized to write the tenant's cache. The invariant being broken is the AC-integrity property "a proven result is immutable once written" — a build-supply-chain / defense-in-depth guard — not a privilege-boundary crossing. The realistic blast radius is: any same-tenant actor who can write the cache (e.g. a contractor CI job, a compromised runner, a lower-trust teammate sharing the org's cas:rw token) can replace a hot action_digest's ActionResult so subsequent cache HITS serve attacker-chosen output digests to the whole org until eviction. That is a genuine and worth-fixing integrity regression (CVSS ~ AV:N/AC:L/PR:L/UI:N/S:U/C:N/I:H/A:N ≈ 5–6), but the privilege precondition and intra-tenant scope put it squarely at MEDIUM, not HIGH.

The recommended fix (read-compare-before-PUT returning DivergentBody, fail-closed on ambiguous GET, plus a regression test mirroring update_divergent_body_returns_conflict_and_preserves_original against R2AcHandler, with the noted GET→compare→PUT race best closed by an R2 conditional PUT / If-None-Match or a D1 ac_meta result-hash guard) is correct and proportionate.

---

### [6] LOW — Stripe Checkout success_url has no destination-domain allowlist (only an https:// prefix check) — open-redirect / session-id exfil if a crafted success_url reaches the checkout creator

**surface:** Money path — checkout success_url validation

**location:** crates/corelink-container/src/routes/tier_select.rs:344-348 (authorize_and_validate redirect check)

**explanation:** When a paid tier checkout is created, CoreLink passes a client-influenced `success_url`/`cancel_url` to Stripe; Stripe redirects the buyer there after payment and appends the Checkout session id (success_url commonly contains `{CHECKOUT_SESSION_ID}`). The only validation applied is that both URLs start with `https://` (tier_select.rs:345). There is NO allowlist of permitted destination hosts. If a request with an attacker-chosen `https://evil.example/...` success_url reaches this code path, the post-payment redirect (carrying the Stripe session id) lands on the attacker's domain. In the current deployment the success_url is built by the trusted Worker from a fixed origin, and this route sits behind the Durable Object + constant-time internal-auth gate, so a remote attacker cannot trivially supply an arbitrary success_url — which is why this is LOW, not higher. But the control as written is defense-in-depth that is one mis-wire away from an open redirect + session-id leak, and the surrounding code (and prior money-path audit notes referenced in repo memory) already flagged a success_url allowlist as a backlog item.

**attack_scenario:** If any future caller, a Worker bug, or an internal-auth weakness lets a client control the success_url field on the tier-select request, an attacker submits `success_url=https://attacker.tld/x?s={CHECKOUT_SESSION_ID}`. The check passes (it is https), Stripe creates the session, and after the victim pays, Stripe redirects the victim's browser to attacker.tld with the Checkout session id in the query — enabling phishing continuation and exposure of the session id.

**reproduction:** 1. Reach POST /v1/onboarding/tier-select (behind the DO + internal-auth) with a body where success_url = `https://attacker.example/landing?session_id={CHECKOUT_SESSION_ID}` and a paid tier. 2. Observe authorize_and_validate (tier_select.rs:344) accepts it because it only checks `starts_with("https://")`. 3. The StripeCheckoutCreator forwards it to Stripe; the resulting Checkout redirects post-payment to the attacker domain. (Pre-condition: the caller can influence success_url — today the Worker fixes it, so this is a latent control gap rather than a live exploit.)

**impact:** Open redirect to an arbitrary https host plus disclosure of the Stripe Checkout session id to that host, for any buyer whose checkout used the crafted URL. No direct tenant-data or cross-tenant impact; bounded by the fact that success_url is presently server-built. Severity is low because exploitation requires the success_url to become client-controllable, which the current trusted-Worker wiring prevents.

**evidence:** tier_select.rs:344-348:
```rust
if tier.is_paid()
    && (!body.success_url.starts_with("https://") || !body.cancel_url.starts_with("https://"))
{
    return Err(TierSelectHttpError::BadRequest);
}
```
The only constraint is the `https://` prefix; there is no host/domain allowlist on success_url or cancel_url.

**recommendation:** Constrain success_url/cancel_url to an explicit allowlist of CoreLink-owned origins (e.g. `https://humangr.com/corelink/`, the app origin) rather than any https URL. Parse the URL and assert host ∈ {known origins} (and reject userinfo/`@`, non-standard ports, and embedded credentials). Keep the https check as an additional constraint. Example: after parsing, `if !ALLOWED_REDIRECT_ORIGINS.iter().any(|o| url.origin_matches(o)) { return Err(BadRequest); }`.

**confidence:** medium

**verify:** The finding is real but the severity and the framing require significant qualification. Here is what the code actually shows:

**The vulnerable path exists, but only for authenticated, paying users attacking themselves.**

The container's `authorize_and_validate` in `crates/corelink-container/src/routes/tier_select.rs:344-348` validates `success_url` only with `starts_with("https://")`:

```rust
if tier.is_paid()
    && (!body.success_url.starts_with("https://") || !body.cancel_url.starts_with("https://"))
{
    return Err(TierSelectHttpError::BadRequest);
}
```

No domain allowlist. Confirmed.

The Worker's onboarding arm at `worker/src/index.ts:1322` uses `new Request(request, { headers: ... })` which forwards the original body unchanged — only headers are rewritten. The `success_url` from the body is passed through to the container verbatim. Confirmed.

**Normal flow (admin-ui → Worker → container):** The admin-ui's `/api/checkout/session/route.ts:157-159` constructs `success_url` server-side: `const origin = originFromRequest(req); const success_url = \`${origin}/\${locale}/upgraded?session_id={CHECKOUT_SESSION_ID}\``. The `originFromRequest` function trusts `x-forwarded-host`/`x-forwarded-proto` from Cloudflare's proxy layer — headers that Cloudflare sets and that a browser cannot spoof in transit to Cloudflare Pages. The route.ts comment at line 115 explicitly says: "Never trust user-supplied origin from the JSON body — that would let an attacker direct Stripe's post-checkout redirect anywhere." So in the normal flow, `success_url` is fully server-controlled.

**Direct attacker path (browser → Worker → container):** The Worker's `/v1/onboarding/tier-select` endpoint is reachable by any caller with a valid Clerk JWT whose `azp` is in `CLERK_AZP_ALLOWLIST` = `[https://humangr.com, https://corelink-app.humangr.com]`. An authenticated malicious user (a real tenant on the platform) CAN bypass the admin-ui and POST directly to the Worker with `success_url: "https://attacker.tld/?"`. The Worker forwards this body to the container, and the container accepts it. Stripe creates the session and redirects the attacker's browser to `attacker.tld` after payment with `?cs_...` in the query.

**Why this is low, not medium/high:**

1. **The attacker must be authenticated AND must pay.** They must hold a valid Clerk JWT for `humangr.com` or `corelink-app.humangr.com` (an actual registered user) AND they must go through the actual payment flow to trigger the Stripe redirect. There is no unauthenticated path.

2. **The exfiltrated data is the attacker's own session ID.** The `CHECKOUT_SESSION_ID` that Stripe appends to the `success_url` belongs to the attacker's own checkout. They already have it — Stripe's API gives it to the session creator. There is no cross-tenant session exposure.

3. **The session ID has no privilege.** `apps/admin-ui/src/app/[locale]/upgraded/page.tsx:14-16` explicitly documents: "The presence of a `session_id` in the URL is NOT proof of payment and is NEVER treated as such here. Anyone can craft `…/upgraded?session_id=anything`." The session ID on the success URL is display-only, never used for authentication or tier activation.

4. **Tier activation is via Stripe webhook, not the redirect.** The `checkout.session.completed` webhook (handled in `crates/corelink-billing-stripe`) activates the tier, not anything in the `success_url` redirect. A hijacked redirect cannot affect billing state.

5. **The "phishing continuation" is low-probability.** The attacker must spend real money (subscribe) to get Stripe to redirect to their URL, lending false legitimacy to a phishing page. The total impact is: one authenticated user can redirect their own post-payment flow to a custom HTTPS URL, exfiling their own display-only session ID. This is a defense-in-depth gap, not an exploitable vulnerability with meaningful blast radius.

**The code comment at `tier_select.rs:139` ("The Worker builds this from a trusted origin — never user-supplied") is correct for the admin-ui flow but is not enforced as a container-level invariant.** It is aspirational documentation, not a security gate. The fix — adding a domain allowlist — would be the correct defense-in-depth improvement. Severity stays low because the attacker has no ability to harm other tenants or bypass payment; they can only control where their own browser lands after their own payment, with no material credential or data exposure.

---

### [7] MEDIUM — CAS content-addressed storage ignores tenant residency — EU/regional tenant blobs are written to a single US (IAD) R2 bucket (Schrems-II / GDPR data-residency violation)

**surface:** DSR/residency — container CAS write path vs tenant.primary_region

**location:** crates/corelink-container/src/routes/cas.rs:125-126 (region selection) + crates/corelink-container/src/storage/r2_s3.rs:307-335 (r2_key uses the single global self.cas_region) + wrangler.toml:558 + wrangler.toml:697-704 (prod-lhr sets R2_AC_REGION=lhr but no R2_CAS_REGION)

**explanation:** CoreLink advertises and deploys 5 regional points of presence (sam/iad/lhr/nrt/syd) so that a tenant's data can stay in its jurisdiction — each region gets its own Action-Cache bucket (corelink-ac-<region>) and chunk bucket (corelink-chunk-<region>), selected at container start from R2_AC_REGION / R2_CHUNK_REGION. The content-addressable store (CAS), which holds the actual cached file/blob bytes, is the EXCEPTION: it is a SINGLE global bucket 'corelink-cas-prod' physically homed in IAD (US), and the key prefix that encodes the storage region is a single process-global value R2_CAS_REGION that defaults to the hardcoded string "iad" (cas.rs:126: `let region = crate::storage::env_or("R2_CAS_REGION", "iad")`). That region is baked into the CAS handler ONCE at container boot (r2_s3.rs: `cas_region` field) and then used for EVERY tenant's blob key via `R2S3Client::blob_key(&self.cas_region, &prefix, digest)` (r2_s3.rs:335) — the code NEVER reads the tenant's `tenant_primary_region` (D1, migration 0028) on the CAS write path. Critically, R2_CAS_REGION is not set in ANY wrangler environment, including the EU env: `[env.prod-lhr].vars` (wrangler.toml:697-704) sets `R2_AC_REGION = "lhr"` and `R2_CHUNK_REGION = "lhr"` but deliberately omits any CAS region, and the file even documents this at line 558: 'CAS bucket = corelink-cas-prod (IAD-only global bucket, shared by all regions)'. The net effect: an EU customer who signs up, is routed to the lhr.corelink-api.humangr.com worker, and pushes cache artifacts has the ACTUAL BYTES of those artifacts written to a US-located R2 bucket under an `iad/` key prefix, while only its lighter-weight Action-Cache metadata stays in the EU bucket. For an SMB platform that markets regional residency, this is an undisclosed cross-border transfer of customer content — a Schrems-II / GDPR Art. 44 data-residency gap.

**attack_scenario:** This is a compliance/data-governance exposure rather than an attacker-triggered exploit. Path: (1) An EU-based tenant selects/relies on EU residency and is routed to the corelink-prod-lhr worker (lhr.corelink-api.humangr.com). (2) The tenant pushes a Bazel/Turborepo/CAS artifact (any blob write through the native R2CasHandler). (3) The container, started by the prod-lhr DO with no R2_CAS_REGION set, writes the blob to bucket `corelink-cas-prod` (US/IAD) at key `iad/<tenant_prefix>/<digest>`. (4) The tenant's source-derived content (which can contain proprietary code, secrets baked into build outputs, PII in test fixtures) now physically resides in the US, outside the promised jurisdiction, with no contractual basis disclosed. A regulator audit, a customer DPA review, or a Schrems-II challenge surfaces the violation.

**reproduction:** 1. Read wrangler.toml [env.prod-lhr].vars (lines 697-704): note R2_AC_REGION="lhr", R2_CHUNK_REGION="lhr", and the ABSENCE of R2_CAS_REGION. 2. Read worker/src/durable_object.ts container.start({env}) forward-list (lines 444-506): confirm R2_CAS_REGION is NOT in the forwarded env keys, so even a manually-set value cannot reach the container. 3. Read crates/corelink-container/src/routes/cas.rs:126 — region defaults to "iad". 4. Read crates/corelink-container/src/storage/r2_s3.rs:335 — every CAS key uses self.cas_region (the global). 5. Conclude: any tenant on any regional worker writes CAS blobs to corelink-cas-prod under prefix iad/. No tenant.primary_region is ever consulted on the CAS write path.

**impact:** Blast radius = ALL CAS blob content for ALL non-IAD-region tenants (the entire EU/SAM/NRT/SYD customer base once those regions onboard). The actual cached bytes — the largest and most sensitive payload (build outputs, which routinely embed source, credentials, and PII) — leave the promised jurisdiction and sit in a US bucket. This is a GDPR Art. 44 cross-border-transfer / Schrems-II violation and a breach of any residency commitment in the customer DPA. Regulatory fines, loss of EU customers, and contract/DPA breach. It does NOT break tenant isolation (prefixes are still per-tenant), but it defeats the residency guarantee that the multi-region architecture exists to provide.

**evidence:** cas.rs:125-126: `let bucket = crate::storage::env_or("R2_CAS_BUCKET", "corelink-cas-prod");` / `let region = crate::storage::env_or("R2_CAS_REGION", "iad");`  —  r2_s3.rs:307-335: `fn r2_key(&self, tenant: &str, digest: &str) -> String { let prefix = ...; R2S3Client::blob_key(&self.cas_region, &prefix, digest) }` (self.cas_region is the single global, set once at build)  —  wrangler.toml:558: `#   - CAS bucket = corelink-cas-prod (IAD-only global bucket, shared by all regions)`  —  wrangler.toml:697-704 [env.prod-lhr].vars sets R2_AC_REGION="lhr"/R2_CHUNK_REGION="lhr" with NO R2_CAS_REGION.

**recommendation:** Make CAS residency-aware like AC/chunk. Two mutually-reinforcing changes: (1) In wrangler.toml, give each regional env its own CAS region/bucket var (e.g. R2_CAS_REGION="lhr", and a per-region CAS bucket corelink-cas-lhr) — and add R2_CAS_REGION + R2_CAS_BUCKET to the durable_object.ts container.start({env}) forward-list (see separate finding) so the value actually reaches the container. (2) Better still, derive the CAS storage region per-write from the tenant's primary_region (already in D1 via migration 0028) rather than a process-global, so a single global CAS bucket can still partition by region in the key prefix correctly per tenant. Until residency is enforced, the residency claim must be removed from marketing/DPAs for CAS content. Add a residency invariant test that fails if a non-IAD regional env lacks a CAS region binding.

**confidence:** high

**verify:** I independently reproduced every cited code path and the finding holds in full.

CONFIRMED FACTS:
1. cas.rs:125-126 — CAS handler region is a process-global env var defaulting to "iad": `let region = crate::storage::env_or("R2_CAS_REGION", "iad");`. There is no per-tenant or per-write region derivation.
2. r2_s3.rs:307-335 — `r2_key()` derives the object key with `R2S3Client::blob_key(&self.cas_region, &prefix, digest)` where `self.cas_region` is the single global set once at construction (r2_s3.rs:296 `cas_region: cas_region.into()`). The container CAS write/read path contains ZERO residency/primary_region/cross-region checks (grep for residency|primary_region|RegionMismatch|cross_region in cas.rs and r2_s3.rs returns nothing).
3. wrangler.toml:558 explicitly documents the design: "CAS bucket = corelink-cas-prod (IAD-only global bucket, shared by all regions)".
4. wrangler.toml:697-704 — [env.prod-lhr].vars sets R2_AC_REGION="lhr" and R2_CHUNK_REGION="lhr" but NOT R2_CAS_REGION; and the CAS_BUCKET binding (line 712) points at corelink-cas-prod. Same for sam(594), nrt(830), syd(948). NO per-region CAS bucket exists anywhere (no cas-sam/cas-weur/cas-lhr/cas-nrt) while AC has full per-region bindings (AC_BUCKET_SAM/IAD/LHR/NRT).
5. worker/src/durable_object.ts:502-505 forwards R2_AC_BUCKET, R2_AC_REGION, R2_CHUNK_BUCKET, R2_CHUNK_REGION to the container but NOT R2_CAS_REGION/R2_CAS_BUCKET — so even if a regional env set them, they would not reach the container. (worker/src/index.ts:101-103 likewise declares R2_AC_REGION/R2_CHUNK_REGION in the Env type but no CAS region.)

THE DIVERGENCE THAT MAKES THIS REAL: residency IS promised in writing. OBJECTION-HANDLING.md:205 "WEUR region pin... we assume EU-origin personal data does not leave the EU"; COMPETITIVE-MATRIX.md:89 "region pinning is structural, not configuration" (INV-REGION-NO-CROSS-LEAK CRITICAL); CAIQ-V4 DSP-16.1 answers residency-enforced = "Y". The nightly verifier scripts/verify-lgpd-residency.py validates objects by URI prefix and its own fixtures expect cas-sam/cas-weur objects (lines defining `{"uri": "cas-sam/abc123", "region": "sam"}` and `{"uri": "cas-weur/eu-blob", "region": "weur"}`) — buckets that physically do not exist. So the compliance model assumes per-region CAS while the deployed reality writes all CAS to a single US bucket. The 30k property test and corelink-privacy/replica-worker enforce residency in a layer the container CAS write path does not traverse.

WHY MEDIUM, NOT HIGH: This is a genuine, code-backed data-residency / Schrems-II / LGPD exposure against explicit DPA/marketing claims — EU/BR tenant CAS content (which can carry PII in fixtures, proprietary build outputs) physically resides in the US with no disclosed legal basis. But it is NOT a security exploit: there is no auth bypass, no cross-tenant read/write (each tenant's blobs still land under its own TDK-derived prefix), no secret leak, no money-path break. CVSS 3.1 confidentiality/integrity impact between tenants is none; the impact is regulatory/contractual, not a triggerable attack. The reporter's "high" overstates the security severity; the correct calibration for a real compliance-control gap with no tenant-isolation or auth break is medium. Confidence is high — every claim is verified against the real code and config.

---

### [8] MEDIUM — Worker→container env-contract gap: R2_CAS_REGION / R2_CAS_BUCKET / R2_AC_BUCKET_PREFIX / R2_TURBO_BUCKET are read by the container but NOT forwarded by durable_object.ts — silently un-settable (the exact bug class the 2026-06-13 audit comment claims to have closed)

**surface:** ENV-CONTRACT — durable_object.ts container.start({env}) forward-list vs container env::var reads

**location:** worker/src/durable_object.ts:444-506 (forward-list) vs crates/corelink-container/src/routes/cas.rs:125-126, crates/corelink-container/src/routes/dsr/adapter_r2_cas.rs:77, crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:68

**explanation:** The Cloudflare Worker Durable Object boots the native container and is the ONLY channel by which process-env reaches it: container.start({ env: {...} }) (durable_object.ts:444). Cross-referencing every `env::var`/`env_or`/`non_empty_env` the container actually reads against that forward-list shows four variables the container reads but the Worker does NOT forward: R2_CAS_REGION and R2_CAS_BUCKET (read at cas.rs:125-126 and dsr/adapter_r2_cas.rs:77), R2_AC_BUCKET_PREFIX (dsr/adapter_r2_ac.rs:68), and R2_TURBO_BUCKET. The forward-list even contains a comment (durable_object.ts:484-489) claiming the 2026-06-13 audit completed the env contract — 'every var the container reads via env::var MUST be forwarded ... the class of bug that hid the ERASURE_SALT_KEY gap' — yet these four are missing, so the claim is incomplete. The practical danger: a Cloudflare worker var/secret is set-but-never-read by the container. Because container env_or() treats an absent var as 'use the default', the container silently falls back to its hardcoded default ("iad" / "corelink-cas-prod" / "corelink-ac-") and the operator's intended override is dropped with NO error — identical to the ERASURE_SALT_KEY incident the comment references. R2_CAS_REGION is the load-bearing one: it is the very knob a future fix for the residency finding would use, and it would silently no-op.

**attack_scenario:** No external attacker; this is an operational-integrity / latent-misconfig defect that becomes a compliance incident. Path: (1) An operator, attempting to remediate the CAS residency gap, runs `wrangler secret put R2_CAS_REGION` (or adds R2_CAS_REGION="lhr" to [env.prod-lhr].vars). (2) Deploy succeeds, no warning. (3) The DO never forwards R2_CAS_REGION into container.start({env}). (4) The container still reads the default "iad". (5) EU blobs keep landing in the US bucket while the operator believes residency is fixed — a worse state than the known gap, because it is now an UNKNOWN gap masked by an apparently-applied fix.

**reproduction:** 1. Enumerate container env reads: `grep -rn 'env::var\|env_or\|non_empty_env' crates/corelink-container/src/ | grep -oE '"[A-Z_0-9]+"' | sort -u`. 2. For each, check presence in the durable_object.ts forward-list: `grep '<VAR>: this.env' worker/src/durable_object.ts`. 3. Observe R2_CAS_REGION, R2_CAS_BUCKET, R2_AC_BUCKET_PREFIX, R2_TURBO_BUCKET return no match → read but not forwarded. 4. (Behavioural) set R2_CAS_REGION in a regional env, deploy, exec into the container and `printenv R2_CAS_REGION` → empty.

**impact:** Latent in prod TODAY because the un-forwarded vars currently rely on defaults that happen to match (corelink-cas-prod / iad / corelink-ac-). The real damage is FUTURE: any operator override of CAS region/bucket, AC prefix, or turbo bucket silently no-ops, with the highest-impact case being the residency fix for the CAS finding above — making this gap a force-multiplier on a HIGH-severity compliance bug. Also undermines confidence in the documented 'env contract is complete' claim, which a reviewer might trust.

**evidence:** durable_object.ts:484-490 comment claims completeness then forwards R2_TDK_HEX/SIGNUP_TOKEN_KEY/... but the file has no `R2_CAS_REGION:`/`R2_CAS_BUCKET:`/`R2_AC_BUCKET_PREFIX:`/`R2_TURBO_BUCKET:` line.  Container reads: cas.rs:126 `env_or("R2_CAS_REGION", "iad")`; cas.rs:125 `env_or("R2_CAS_BUCKET", "corelink-cas-prod")`; dsr/adapter_r2_ac.rs:68 `env_or("R2_AC_BUCKET_PREFIX", DEFAULT_AC_BUCKET_PREFIX)`.

**recommendation:** Add the missing keys to the container.start({env}) object in durable_object.ts, mirroring the existing pattern: `R2_CAS_REGION: this.env.R2_CAS_REGION ?? "", R2_CAS_BUCKET: this.env.R2_CAS_BUCKET ?? "", R2_AC_BUCKET_PREFIX: this.env.R2_AC_BUCKET_PREFIX ?? "", R2_TURBO_BUCKET: this.env.R2_TURBO_BUCKET ?? "",`. Then mechanize the contract: add a CI check that greps every container env::var name and asserts it appears in the durable_object forward-list (so this class of drift fails the build), which is exactly what the 2026-06-13 audit comment promised but did not enforce.

**confidence:** high

**verify:** The finding is confirmed by direct code inspection. The `container.start({env})` block in `worker/src/durable_object.ts` (lines 444-506) does NOT forward `R2_CAS_REGION`, `R2_CAS_BUCKET`, `R2_AC_BUCKET_PREFIX`, or `R2_TURBO_BUCKET`. A `grep` over the entire file for all four names returns no hits (exit 1). The container crates independently verify they read these vars: `crates/corelink-container/src/routes/cas.rs:125-126` calls `env_or("R2_CAS_BUCKET", "corelink-cas-prod")` and `env_or("R2_CAS_REGION", "iad")`; `crates/corelink-container/src/routes/dsr/adapter_r2_ac.rs:68` calls `env_or("R2_AC_BUCKET_PREFIX", DEFAULT_AC_BUCKET_PREFIX)`; `crates/corelink-container/src/storage/r2_kv.rs:178` calls `non_empty_env("R2_TURBO_BUCKET").unwrap_or_else(|| "corelink-turbo-prod".to_owned())`. The comment at lines 484-489 of durable_object.ts explicitly claims to "complete the env contract (2026-06-13 audit): every var the container reads via env::var MUST be forwarded" — but the four vars are absent from the env block that immediately follows. The forward-list added `R2_AC_BUCKET`, `R2_AC_REGION`, `R2_CHUNK_BUCKET`, `R2_CHUNK_REGION` (lines 502-505) and `R2_TDK_HEX` (line 490), but missed the CAS and Turbo bucket vars. The wrangler.toml also has no declaration of these four names. Severity stays at medium (not higher): the defaults are hardcoded to the prod values today (`"corelink-cas-prod"`, `"iad"`, `"corelink-turbo-prod"`), so no live breakage currently. The real risk is the silent-failure trap the finding describes: an operator who sets `R2_CAS_REGION` to redirect CAS writes to an EU bucket will deploy successfully, see no error, and believe residency is fixed — while the container continues to use the hardcoded `"iad"` default, making a known compliance gap into an unknown one. No cross-tenant isolation break or auth bypass is involved, so CRITICAL/HIGH is not warranted.

---

### [9] MEDIUM — DSR erasure pseudonymization salt falls back to a PREDICTABLE non-secret value when ERASURE_SALT_KEY is unset, enabling re-identification of the Stripe-side pseudonym

**surface:** DSR/GDPR — erasure salt derivation (signup-worker) + container Stripe pseudonymize adapter

**location:** apps/signup-worker/src/webhooks/clerk.ts:138-158 (deriveErasureSalt fallback) + crates/corelink-container/src/routes/dsr/adapter_stripe.rs:73-83 (redaction_values uses the salt) + worker/src/durable_object.ts:476-479

**explanation:** When a GDPR erasure runs, the surviving record on Stripe's side is a pseudonym: the customer's real email is overwritten with `erased-<hash>@deleted.invalid`, where the hash = SHA-256(subject_id || erasure_salt) (adapter_stripe.rs:73-82). The unlinkability of that pseudonym depends entirely on the secrecy of erasure_salt. The salt is produced in the signup-worker by deriveErasureSalt(dsr_id, ERASURE_SALT_KEY): if the secret key is set it is HMAC-SHA256(key, dsr_id) (unlinkable), but if ERASURE_SALT_KEY is unset/empty it falls back to plain SHA-256("erasure-salt:" + dsr_id) (clerk.ts:153-157) — and dsr_id itself is a deterministic function of the Clerk user id (deterministicDsrId). So with no secret, the salt, and therefore the Stripe pseudonym, is fully recomputable by anyone who knows (or can enumerate/guess) the Clerk user id and the subject_id (= tenant_id, which is not secret). This defeats the 'unlinkable pseudonym' property GDPR pseudonymization is meant to provide. ERASURE_SALT_KEY is an operator launch-day secret that is not yet provisioned (apps/signup-worker/wrangler.toml:84 'New secret (operator): wrangler secret put ERASURE_SALT_KEY'), so the predictable-salt fallback is the LIVE behavior at launch. Mitigating context that caps this at medium: the D1 erasure HARD-DELETES the real PII rows (the tenant row carrying email_hash/clerk_user_id is removed, adapter_d1.rs:202), so the residual re-identifiable artifact is limited to the Stripe-side pseudonym, not the primary datastore.

**attack_scenario:** (1) ERASURE_SALT_KEY is not set (launch-day default). (2) A tenant exercises right-to-erasure; the Stripe customer email is redacted to erased-<hash>@deleted.invalid. (3) An adversary who obtains the dump of redacted Stripe customers and knows/guesses a target's Clerk user id computes dsr_id = deterministicDsrId(clerk_user_id), salt = SHA-256("erasure-salt:"+dsr_id), then hash = SHA-256(subject_id||salt), and matches `erased-<hash[..16]>@deleted.invalid` to confirm 'this redacted Stripe customer is that specific person' — re-identifying an erased subject and confirming they were a CoreLink customer. The pseudonym was supposed to make exactly this impossible.

**reproduction:** 1. Confirm ERASURE_SALT_KEY unset (apps/signup-worker/wrangler.toml:84 lists it as a pending operator secret). 2. Read clerk.ts:138-158 — fallback branch is SHA-256 over `erasure-salt:${dsrId}` with no secret. 3. Read clerk.ts:170-177 — dsrId = deterministicDsrId(clerkUserId) (deterministic, non-secret). 4. Read adapter_stripe.rs:73-82 — pseudo_email derived from SHA-256(subject_id||salt). 5. Given a known clerkUserId+tenantId, recompute the pseudo_email offline and match it against redacted Stripe records.

**impact:** Re-identification of GDPR-erased subjects on the Stripe billing surface for the window before ERASURE_SALT_KEY is provisioned (i.e. every erasure performed at/after launch until the operator sets the secret). Confirms 'X was a CoreLink customer and has been erased' — a privacy harm and a defect in the pseudonymization control the platform claims (the very purpose of the salt). Scope is limited to Stripe pseudonyms because primary PII is hard-deleted; hence medium not high.

**evidence:** clerk.ts:153-157: `const d = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(`erasure-salt:${dsrId}`)); return bytesToHex(d);` (the no-secret fallback).  adapter_stripe.rs:74-80: `h.update(subject_id.as_bytes()); h.update(erasure_salt); let marker_hex = hex::encode(h.finalize()); ... let pseudo_email = format!("erased-{short}@deleted.invalid");`.  durable_object.ts:476-479 comment: 'If absent the container falls back to a PREDICTABLE non-secret salt'.

**recommendation:** Make the predictable-salt path fail-closed for production: in deriveErasureSalt, if the deployment is a prod environment and ERASURE_SALT_KEY is unset, refuse to enqueue/emit the erasure (or hard-error) rather than silently using SHA-256(dsr_id). Provision ERASURE_SALT_KEY as part of the launch secrets gate (add it to scripts/secrets-checklist-verify.sh / validate_secrets_matrix.py as a REQUIRED prod secret so CI/launch-readiness fails if it is missing). Optionally raise the floor by drawing the salt from crypto.getRandomValues per-DSR (truly random, stored only as long as needed) instead of any deterministic derivation.

**confidence:** medium

**verify:** The finding is confirmed. All three cited code paths are real and the exploit chain is reachable in production.

EVIDENCE:

1. apps/signup-worker/src/webhooks/clerk.ts:138-158 — `deriveErasureSalt` is a two-branch function. When `ERASURE_SALT_KEY` is set it does `HMAC-SHA256(key, dsr_id)` (cryptographically unlinkable). When absent or empty it falls back to `SHA-256("erasure-salt:" + dsr_id)` — a completely keyless, deterministic computation.

2. The `dsr_id` itself (clerk.ts:121-130) is `SHA-256("corelink-dsr-v1:" + clerk_user_id)` coerced into UUID shape. This is a deterministic public function of the Clerk user ID, which is a stable, non-secret identifier.

3. crates/corelink-container/src/routes/dsr/adapter_stripe.rs:73-82 — `redaction_values` computes `SHA-256(subject_id_bytes || erasure_salt)` and formats the first 16 hex chars as `erased-{short}@deleted.invalid`. With the fallback salt, every input to this hash is reconstructable from the target's Clerk user ID alone — no secret knowledge required.

4. worker/src/durable_object.ts:476-479 explicitly documents the fallback risk in code comments: "If absent the container falls back to a PREDICTABLE non-secret salt."

5. `ERASURE_SALT_KEY` has zero hits in `docs/internal/secrets-checklist.md` — no gate prevents a production deploy without this secret being provisioned.

ATTACK PATH:

An adversary who obtains the Stripe customer dump (breach, rogue insider, legal subpoena) and knows a target's Clerk user ID (a stable, non-secret identifier) can:
- Compute `dsr_id = UUID(SHA-256("corelink-dsr-v1:" + clerk_user_id))` using public inputs.
- Compute `salt = SHA-256("erasure-salt:" + dsr_id)` — the exact no-secret fallback path in `deriveErasureSalt`.
- Compute `marker = SHA-256(tenant_id_bytes || salt)` matching the Rust adapter's `redaction_values`.
- Check whether `erased-{marker[:16]}@deleted.invalid` appears in the Stripe customer list.
- A match confirms (a) this individual was a CoreLink customer and (b) they exercised a GDPR right to erasure — precisely the re-identification the pseudonymization was designed to prevent.

SEVERITY RATIONALE (medium, not lower):

The attack requires prior access to the Stripe customer dump, which is a non-trivial prerequisite. However, GDPR pseudonymization must be cryptographically unlinkable by design. Failing this requirement is a material compliance defect under GDPR Article 4(5) and Recital 26 — pseudonymization that can be reversed without a secret is, legally, not pseudonymization at all. An adversary scenario does not require system compromise; it covers any path to the Stripe data (breach at Stripe, rogue employee, legal process). The data leaked is confirmation of account membership and erasure history — sensitive enough to constitute a GDPR breach notification event if exploited. Medium is correct; not high, because it is not a direct system compromise or financial loss path, and requires a secondary breach precondition.

RECOMMENDATION (concise):

(1) Make ERASURE_SALT_KEY a required production secret: add it to `docs/internal/secrets-checklist.md` and `scripts/secrets-checklist-verify.sh`. The `validate_secrets_matrix.py` gate will then enforce it at every deploy.
(2) Make the fallback fail-closed in production: in `deriveErasureSalt`, if `key` is absent and the environment is production (detectable via an existing env var like `ENVIRONMENT=prod`), throw an error rather than silently returning the deterministic hash. Return a 500 from the webhook handler so Svix retries — the erasure obligation stays alive while the missing secret is an operator error, not a silent pseudonymization downgrade.
(3) Provision `ERASURE_SALT_KEY` as a high-entropy random secret (32+ bytes, `crypto.getRandomValues`) via `wrangler secret put ERASURE_SALT_KEY` before DSR processing goes live in production.

---

### [10] LOW — Stripe Checkout success_url / cancel_url open-redirect — client-controlled redirect target, only an `https://`-prefix check (no host allowlist)

**surface:** worker↔container boundary / money-path (tier-select checkout)

**location:** crates/corelink-container/src/routes/tier_select.rs:344-348

**cvss_reasoning:** CVSS 3.1: AV:N/AC:L/PR:L/UI:R/S:C/C:N/I:L/A:N ≈ 4.7 (Medium-low). Network attack vector, low complexity; PR:L because a valid Clerk session is required; UI:R because the victim must follow the Stripe→attacker redirect; Scope Changed because the trust is borrowed from Stripe's checkout domain; integrity Low (redirect/phishing), no confidentiality or availability impact on CoreLink data. Rated low severity overall given the authentication requirement and Stripe mediation.

**explanation:** When a logged-in customer starts a paid-plan checkout, the browser calls `POST /v1/onboarding/tier-select` with a JSON body that includes `success_url` and `cancel_url` — the URLs Stripe will redirect the buyer's browser to after the hosted-checkout page finishes (success) or is abandoned (cancel). The CoreLink edge Worker authenticates the Clerk session, resolves the tenant, and forwards the request to the container WITH THE ORIGINAL BODY UNCHANGED (`new Request(request, { headers })` preserves the body — worker/src/index.ts:1322). Inside the container, the only validation applied to these two URLs is that they begin with the literal string `https://` (tier_select.rs:345). There is NO check that the host is a CoreLink-owned origin (e.g. corelink-app.humangr.com). The validated URLs are then handed straight to Stripe as the checkout session's `success_url`/`cancel_url` (tier_select_checkout.rs:186-192 → StripeRealClient::create_checkout_session). Because Stripe blindly trusts whatever success/cancel URLs the API caller supplies, an attacker who controls the request body can make Stripe's hosted checkout redirect the victim to ANY https host they choose (e.g. `https://corelink-app.phishing.example/welcome`). WHY IT MATTERS: this is a reflected open-redirect anchored on the trusted Stripe checkout domain — a high-credibility phishing primitive. The redirect originates from `checkout.stripe.com` (a domain users trust during a payment), lands on an attacker page, and Stripe even appends the real `{CHECKOUT_SESSION_ID}` if the URL template asks for it, lending the landing page legitimacy. HOW EXPLOITED: the attacker crafts a tier-select link/flow for a victim (or for themselves to seed a convincing 'upgrade complete' page) whose success_url points at attacker infrastructure; after the Stripe page, the browser is bounced to the attacker host. It is authenticated (needs a valid Clerk session for some tenant) and Stripe-mediated, which is why this is low — but it is a real, code-confirmed defect, and the repo's own memory (money-path-audit-27: 'success_url allowlist' backlog) already flags it as an unclosed item.

**attack_scenario:** 1) Attacker authenticates to CoreLink with a valid Clerk session (any self-serve account). 2) Attacker (or a phishing page driving the victim's authenticated browser) issues `POST /v1/onboarding/tier-select` with body `{"tier":"pro","success_url":"https://corelink-app.attacker.example/upgraded?s={CHECKOUT_SESSION_ID}","cancel_url":"https://corelink-app.attacker.example/cancel"}`. 3) The edge Worker verifies the Clerk session and forwards the body unchanged with the trusted internal-auth + tenant headers. 4) The container's `authorize_and_validate` passes because both URLs start with `https://`. 5) The container creates a real Stripe checkout session whose success/cancel URLs are attacker-controlled and returns the `checkout.stripe.com` hosted URL. 6) When the buyer completes (or cancels), Stripe redirects the browser from the trusted Stripe domain to the attacker host — a credible payment-flow phishing/redirect.

**reproduction:** Preconditions: a valid Clerk session for a CoreLink tenant; STRIPE_* price ids configured (paid tier). Steps: (a) From the authenticated browser, send `POST https://corelink-app.humangr.com/v1/onboarding/tier-select` with `Authorization: Bearer <clerk session jwt>` and body `{"tier":"solo","success_url":"https://evil.example/win","cancel_url":"https://evil.example/lose"}`. (b) Observe the 200 response contains a `checkout.stripe.com/...` URL. (c) Open it, complete the test-mode payment. (d) Stripe redirects the browser to `https://evil.example/win` — an origin CoreLink does not own. Negative control: change `success_url` to `http://evil.example/win` (no TLS) and confirm the container returns 400 (the only check that fires), proving the sole gate is the scheme prefix, not the host.

**impact:** Blast radius: any authenticated self-serve tenant can obtain a Stripe-hosted checkout link that redirects to an arbitrary https origin. No cross-tenant data read/write, no secret leak, no money diversion (Stripe still bills the correct CoreLink account/price). Concrete harm = phishing/social-engineering: a redirect chain that begins on the trusted `checkout.stripe.com` payment domain and ends on attacker infrastructure, optionally carrying the real checkout-session id to fake an 'upgrade succeeded' landing page and harvest credentials or push a follow-on scam. Reputational/abuse risk to the brand during the money path; not a confidentiality/integrity breach of tenant data.

**evidence:** tier_select.rs:344-348 — `if tier.is_paid()
    && (!body.success_url.starts_with("https://") || !body.cancel_url.starts_with("https://"))
{
    return Err(TierSelectHttpError::BadRequest);
}` — the ONLY validation of the two redirect URLs is the `https://` prefix; no host/origin allowlist. The values flow unmodified: tier_select.rs:406-407 `&req.success_url, &req.cancel_url` → tier_select_checkout.rs:186-192 `CheckoutSessionRequest::new(TenantId::new(tenant_id), tier_kind, String::new(), success_url, cancel_url)` → Stripe. The Worker forwards the body unchanged: worker/src/index.ts:1322 `const onbAugmented = new Request(request, { headers: (...) })` (only headers are rebuilt; the body — and thus success_url/cancel_url — is passed through verbatim).

**recommendation:** Replace the scheme-only check with a host allowlist pinned to the CoreLink dashboard origin(s). Concretely, in `authorize_and_validate` parse each URL and require the host to be in a configured allowlist (e.g. an env-sourced `CORELINK_DASHBOARD_ORIGIN`, defaulting to `corelink-app.humangr.com`), or — simplest and safest — DERIVE the success/cancel URLs server-side from the trusted origin instead of accepting them from the body at all (append only a fixed path + the Stripe `{CHECKOUT_SESSION_ID}` placeholder). Example: `let host = url::Url::parse(&body.success_url).ok().and_then(|u| u.host_str().map(str::to_owned)); if !ALLOWED_REDIRECT_HOSTS.contains(host.as_deref().unwrap_or("")) { return Err(TierSelectHttpError::BadRequest); }` applied to both URLs (and keep the existing https-scheme assertion). This closes the open-redirect while preserving the existing flow.

**confidence:** high

**verify:** The finding is real and the attack path is confirmed by code inspection, but the original "low" severity rating is appropriate — possibly even slightly generous, as explained below.

CONFIRMED VULNERABLE PATH:
The container's `authorize_and_validate` in `crates/corelink-container/src/routes/tier_select.rs:344-348` validates redirect URLs only for the `https://` scheme prefix. No host allowlist exists. The Worker at `worker/src/index.ts:1322` constructs `new Request(request, { headers: (...) })` — this only rebuilds headers; the request body (containing `success_url` and `cancel_url`) is forwarded verbatim from the client. A caller with a valid Clerk session JWT can POST directly to `https://corelink-api.humangr.com/v1/onboarding/tier-select` (the public Worker hostname, confirmed in `wrangler.toml:266`) with arbitrary `success_url`/`cancel_url` values that start with `https://`, bypassing the UI-layer protections entirely.

SECONDARY RISK (residual): `apps/admin-ui/src/app/[locale]/onboarding/actions.ts:124-136` exports `createCheckoutSessionAction` as a Next.js Server Action. Its `CheckoutSessionRequest` type includes `success_url` and `cancel_url` as direct fields. Although no current UI component calls this action in a way that passes client-supplied URLs, the exported Server Action is a callable surface that accepts and forwards these fields verbatim.

WHY THE PRIMARY UI PATH DOES NOT HELP: The `/api/checkout/session` Next.js route (`apps/admin-ui/src/app/api/checkout/session/route.ts:158-159`) correctly derives redirect URLs server-side from `originFromRequest(req)` and explicitly notes "Never trust user-supplied origin from the JSON body." But this route is not the only caller of the backend. The backend endpoint `POST /v1/onboarding/tier-select` is publicly addressable.

SEVERITY ASSESSMENT — confirmed LOW (not escalated):
This is a classic open-redirect, but the threat model context matters for severity:
1. The attacker must be authenticated (valid Clerk session — any self-serve account). This eliminates unauthenticated abuse.
2. The redirect target is a Stripe-hosted `checkout.stripe.com` URL — not the CoreLink domain itself. The open-redirect capability applies AFTER Stripe processes the payment; Stripe redirects to the attacker-controlled URL only after the buyer completes (or cancels) the Stripe checkout. The browser never leaves `checkout.stripe.com` during payment.
3. The attacker cannot redirect the checkout FLOW itself — they can only control where Stripe sends the browser AFTER the payment is complete. This means no payment data is exposed via the redirect.
4. The practical phishing risk: a victim lands on `attacker.example` after completing a real Stripe payment. The attacker could display a fake "your subscription is active" page, fish for credentials ("log in again"), or log the `{CHECKOUT_SESSION_ID}` from the query string. The session ID in the URL is low-value (cannot be used to reverse-charge or access payment data without Stripe API keys).
5. This is not a tenant-isolation break, not a money-path bypass, and not a secret leak. It is a post-payment redirect abuse requiring a cooperating victim who uses the attacker's modified checkout flow.

LOW is the correct severity. The comment in the Rust source (`tier_select.rs:341`) that says "the Worker builds these from a trusted origin" is aspirational/misleading — the Worker does not enforce this — but the real-world exploit requires a phishing scenario that is only credible against a small number of authenticated users and yields limited data. The fix (derive URLs server-side in the container from a trusted allowlist, or simply hardcode the success/cancel paths) is straightforward and should be applied, but this is not a CRITICAL or HIGH finding.

---

### [11] INFO — Production audit-analytics per-tenant rate limiter is in-memory/per-instance, not durable — resets on cold start and is not shared across container instances

**surface:** DoS / quota (rate-limit)

**location:** crates/corelink-container/src/routes/audit_analytics/state.rs:87-103

**explanation:** The container exposes internal analytics query routes (`/v1/audit/analytics/timeline` and `.../event-count`) that run aggregate queries against the per-tenant Postgres shadow. To stop a single tenant from hammering those aggregates, each route is guarded by a per-tenant token-bucket rate limiter configured at 10 queries/minute. The problem is HOW that limiter is wired in production: `build_state()` (the function `main.rs` calls for the real router) constructs an `InMemoryTokenBucketRateLimiter`. 'In-memory' means the token counts live only in that one container process's RAM. CoreLink runs on Cloudflare Containers fronted by a Durable Object, where the container is ephemeral: it cold-starts, can be recycled, and the DO may route to more than one instance. Every time the container restarts, every tenant's bucket is reset to full burst capacity, and if requests land on a different instance they see a fresh bucket. So the '10/min' cap is not a global guarantee — an attacker (any tenant with a valid PAT, since the tenant comes from the Worker-injected `x-corelink-tenant-id`) can exceed it by forcing/awaiting recycles or by spreading load across instances. Why it matters but is only LOW: the analytics queries themselves are bounded — the timeline handler enforces a max-bucket-cardinality gate and the underlying aggregate is O(1)-ish over a bounded span, and the surface is internal (DO-forwarded), not the raw public internet. So the realistic blast radius is extra Postgres aggregate load, not an isolation or money break.

**attack_scenario:** A tenant holding a valid PAT issues analytics timeline/event-count queries in a tight loop. Within a single warm instance they are capped at 10/min, but because the bucket is per-process in-memory, the cap does not survive container recycles and is not shared if the DO fans out to multiple container instances — so sustained query pressure above the intended global cap reaches the per-tenant Postgres shadow.

**reproduction:** 1. Authenticate as any tenant (obtain a normal cas:rw PAT via the normal flow). 2. Send >10 GET requests/min to `/v1/audit/analytics/timeline?from=..&to=..` for that tenant. 3. Observe the 11th in a window returns 429 on the SAME warm instance. 4. Trigger a container cold start (or let one occur) / let the DO route to a second instance, and observe the counter is reset to full burst — the global 10/min is not enforced across the limiter's lifecycle. Code evidence: `build_state` always builds `Arc::new(InMemoryTokenBucketRateLimiter::new(...))`; there is no D1/durable-backed RateLimiter impl wired (grep for a `ratelimit_buckets`-backed limiter returns only docs/DSR erase-set, never a store).

**impact:** Excess analytics-query load against a tenant's Postgres shadow beyond the intended 10/min ceiling. Bounded by the per-query cardinality gate and the internal-only surface; no cross-tenant data exposure, no money path. Operational/cost nuisance, not an isolation break.

**evidence:** crates/corelink-container/src/routes/audit_analytics/state.rs:90 — `let rate_limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryTokenBucketRateLimiter::new(\n        rl_audit,\n        rl_metrics,\n        audit_analytics_rate_limit_config(),\n    ));` and routes.rs:388 `let audit_analytics_state = audit_analytics::build_state(shadow_factory);` (the prod mount). The docstring at state.rs:81-85 itself notes shadow/Neon is the only deferred prod swap, not the limiter store.

**recommendation:** Back the analytics rate limiter with a durable per-tenant store (the `ratelimit_buckets` D1 table already referenced in the DSR erase-set / tenant_quota docs) using the same atomic `INSERT … ON CONFLICT … DO UPDATE … RETURNING count` fixed-window pattern proven in `session_exchange.ts::checkMintThrottle`, so the cap is global across instances and survives recycles. Alternatively move this gate to the edge Worker (Durable-Object-backed) which has a single point of state per tenant.

**confidence:** high

**verify:** The finding is factually correct on the core claim (in-memory rate limiter, cold-start reset) but materially wrong on the multi-instance angle, which significantly reduces severity.

WHAT IS CONFIRMED:
- `state.rs:90` constructs `InMemoryTokenBucketRateLimiter` — confirmed exactly as cited.
- `routes.rs:388` mounts it in production without swapping to a durable backend — confirmed.
- `main.rs` production boot path swaps only the `shadow_factory` (Neon Postgres), never the rate limiter — confirmed.
- On container cold-start (idle timeout = 5 min per `durable_object.ts`), the in-memory bucket resets, giving a tenant a fresh 10-token burst.

WHAT IS WRONG (the multi-instance claim):
- `index.ts:1758` routes each tenant to `idFromName(resolvedTenantId)` — each tenant gets its own DO instance.
- `durable_object.ts:5` explicitly documents "Per-tenant pinning: DO ID = idFromName(tenantId)".
- Each DO holds exactly one `state.container` binding (durable_object.ts:12).
- Therefore, per-tenant there is ONE DO → ONE container → ONE in-memory rate limiter. There is NO fan-out to multiple parallel container instances per tenant; the `max_instances = 5` cap in `wrangler.toml:40` is a platform ceiling, not a fan-out multiplier.
- Cross-tenant isolation is preserved by the DO topology itself.

REAL RESIDUAL RISK (cold-start only):
A tenant rate-limited to 10 queries/min could wait for the 5-minute idle timeout to expire, causing the container to be destroyed and restarted, then immediately issue another 10 queries against Postgres. This allows a burst of up to 20 queries per cold-start cycle instead of 10 — a 2x bypass. Given that:
- Analytics queries hit Neon Postgres (aggregate over a shadow table), not the primary data path
- The burst is bounded (10 tokens, not unlimited)
- The container must actually idle for 5+ minutes for the reset to occur
- The endpoint is customer-facing analytics only, not a billing or auth surface

This is a real but very mild gap. The blast radius is: a determined tenant could double their analytics query rate by intentionally cycling container cold starts. The Neon shadow is a read-only aggregate tier with ≤5min lag — not the canonical chain, not a billing surface, not cross-tenant. The practical impact is mild Postgres load, not data exposure or billing bypass.

Severity adjustment: Downgraded from "low" to "info". The finding documents a design imperfection (the rate limiter was always intended to eventually be durable, per the crate docs referencing `ratelimit_buckets` D1 table), but the actual exploitable scenario requires deliberate container cycling and only delivers a 2x rate burst on a read-only analytics endpoint. It is not a tenant-isolation break, not a money-path issue, and not a secret leak. "Info" / hardening backlog is the correct classification.

---

### [12] LOW — Per-tenant $-ceiling charges one flat op cost per request regardless of batch fan-out (findMissingBlobs / batch reads) — quota-bypass-by-batching

**surface:** DoS / quota ($-ceiling) + business-logic

**location:** crates/corelink-container/src/routes/bazel_v2.rs:506 (handle_find_missing quota_reject) / crates/corelink-container/src/tenant_quota.rs:89 (DEFAULT_COST_PER_OP_MICROS)

**explanation:** ADR-0068 adds a per-tenant monthly dollar ceiling (default $5/mo) to bound cost blast radius on cheap third-party infra. The guard charges a FLAT cost per 'op' — `DEFAULT_COST_PER_OP_MICROS = 1000` ($0.001) — and the route calls `gate.check(tenant)` exactly once per HTTP request. The Bazel REAPI `findMissingBlobs` endpoint, however, accepts a BATCH of digests in one request body and performs a backend CAS existence check for EACH digest, yet it accrues only ONE flat op cost for the whole batch. The body is capped at 10 MiB, which still allows on the order of thousands of digest entries in a single request. So a tenant can drive far more backend work (and real R2/Postgres cost) per accrued dollar than the flat model assumes — the $5 ceiling that is supposed to trip at ~5000 ops can be stretched to many multiples of the intended backend work. Why LOW: the code authors explicitly designed this as a 'coarse preventive tripwire, NOT precise metering' (documented at tenant_quota.rs:76-89), the body cap bounds the per-request fan-out, and it bounds blast radius rather than breaking isolation or directly stealing money. It is an honest gap against the ADR's stated goal of bounding cost, not a hidden bug.

**attack_scenario:** A tenant near or wanting to exceed their economic budget sends Bazel `findMissingBlobs` POSTs each carrying a large array of digests (up to the 10 MiB body limit). Each request accrues only $0.001 against the monthly ceiling but causes the backend to perform one CAS existence check per digest, multiplying real backend cost per accrued dollar.

**reproduction:** 1. Authenticate as a tenant with a cas:rw PAT. 2. POST `/bazel/v2/<instance>/findMissingBlobs` with a body containing N (e.g. several thousand) valid 64-hex digests, sized just under 10 MiB. 3. Observe in `handle_find_missing` (bazel_v2.rs:506) that `quota_reject` is called once for the whole request — one flat `cost_per_op_micros()` accrual — while `find_missing` (bazel_v2.rs:525) iterates a backend CAS read per digest. 4. Repeat; the $-ceiling accrues at 1 op/request irrespective of N.

**impact:** A tenant can perform substantially more backend work (R2/Postgres reads, container CPU) per accrued dollar than the flat $0.001/op model assumes, weakening the ADR-0068 cost-blast-radius bound. No cross-tenant impact; bounded by the 10 MiB body cap and the internal flat-cost tradeoff the ADR explicitly accepts.

**evidence:** bazel_v2.rs:506 `if let Some(resp) = quota_reject(&state, &tenant).await {` (single charge) followed by bazel_v2.rs:525-526 `.find_missing\n        .find_missing(&instance, &p, &tenant, now_ms(), &digests)` (per-digest backend work for the whole batch). tenant_quota.rs:89 `pub const DEFAULT_COST_PER_OP_MICROS: i64 = 1_000;` charged once per `QuotaGuard::check` call (tenant_quota.rs:241).

**recommendation:** Scale the accrued cost by the batch cardinality / payload size for batch endpoints: charge `cost_per_op_micros() * digests.len()` (or a byte-weighted cost) in `handle_find_missing` and any future batch read/write arms, rather than a single flat op. Cap the number of digests per `findMissingBlobs` request explicitly (a `BatchTooLarge`-style 413 on count) so per-request fan-out is bounded independently of the byte limit.

**confidence:** high

**verify:** The finding accurately describes the code behavior. In `/crates/corelink-container/src/routes/bazel_v2.rs:506`, `quota_reject(&state, &tenant).await` is called once before `find_missing` runs its per-digest loop. `quota_reject` delegates to `QuotaGate::check(tenant)` in `routes.rs:219`, which charges exactly one flat `cost_micros` ($0.001) via `self.guard.check(tenant, self.cost_micros)`. Then `find_missing.rs:131` executes a sequential `for digest in digests` loop doing one CAS existence check per digest, with no further quota accrual. The `FIND_MISSING_BLOB_CAP = 4096` bound (enforced in `parse_find_missing_request` in `find_missing.rs:192`) means a single request can fan out to 4096 CAS reads for $0.001 charged against the monthly ceiling.

However, this is explicitly a documented design decision, not an undetected bug. `tenant_quota.rs:79-88` states: "Every billable data-plane op (CAS/AC read+write, Bazel REAPI, Turbo, sccache) is charged the SAME flat cost regardless of byte size. It is a **preventive tripwire, NOT precise metering** ... Operators tune the per-op cost via `COST_PER_OP_MICROS_ENV` and the per-tenant ceiling via the `tenant_quota.monthly_budget_usd_micros` column." ADR-0068 (created 2026-06-12) further confirms: "precise per-byte metering is a separate, post-launch concern."

The finding is confirmed as real code behavior. The severity remains "low" because: (1) there is no tenant isolation break, auth bypass, data leak, or money-path fraud; (2) the $5/month hard ceiling still fires (a tenant burning 4096-digest batches continuously hits the tripwire at ~5000 requests/month rather than ~5000*4096 ops), limiting blast radius to the operator's cost exposure rather than another tenant's data; (3) a per-request digest count cap of 4096 bounds the per-request fan-out independently; (4) the system's own documentation explicitly accepts this as a known coarse approximation. The gap is a business-logic metering precision gap — the quota meter undercounts real backend work for large batches — but it does not enable cross-tenant attacks or revenue loss beyond the already-capped per-tenant $5/month ceiling. The recommendation (scale cost by `digests.len()` and/or add a separate count-based 413 gate) remains valid as a post-launch metering refinement, consistent with ADR-0068's own stated future work.

---

### [13] INFO — $-ceiling can over-shoot under concurrency: ceiling read is not serialized with the atomic accrual (TOCTOU over-admit)

**surface:** DoS / quota ($-ceiling) — concurrency

**location:** crates/corelink-container/src/tenant_quota.rs:276-293

**explanation:** The monthly $-ceiling guard reads the tenant's current accrued spend, checks `baseline + cost > budget`, and only THEN accrues the cost. The accrual write is atomic (the DB does `accrued = accrued + delta`, which correctly prevents lost updates / under-counting). But the read-for-the-ceiling-decision is deliberately NOT serialized with that write. Under high concurrency, many requests can each read the same pre-accrual `accrued` value, all individually conclude they are under the ceiling, and all proceed — so the tenant can be admitted slightly past the budget before the accrued counter catches up. The authors explicitly document this (tenant_quota.rs:276-283): they chose to guarantee the SAFE direction (never under-count spend) and accept a small over-shoot because the cap is a coarse preventive tripwire, not exact metering, and a precise cap would need a full DB transaction. I record this as INFO, not a vulnerability: the chosen tradeoff is correct for a cost tripwire and the over-shoot is bounded by in-flight concurrency, not unbounded.

**attack_scenario:** A tenant sitting just under their monthly ceiling fires a burst of concurrent billable requests. Each reads the same accrued baseline before any of them has committed its accrual, so several are admitted that, summed, exceed the ceiling — a small bounded over-spend beyond the cap.

**reproduction:** 1. Seed a tenant at accrued just below budget (e.g. budget $5, accrued $4.999). 2. Issue many concurrent billable requests so their `store.get` calls all observe the same $4.999 baseline before any `accrue` commits. 3. Each passes `projected <= budget` and is served, accruing afterward; the committed total ends marginally above $5. Code path: tenant_quota.rs:260 (get) → 284-285 (projected check) → 316-318 (accrue) are not in one transaction.

**impact:** Bounded over-admission past the monthly $-ceiling proportional to in-flight concurrency (a few extra ops), increasing realized cost slightly beyond the intended tripwire. No isolation or correctness break; accrual itself never under-counts.

**evidence:** tenant_quota.rs:277-283 comment: `NOTE: this read-for-ceiling is intentionally NOT serialized with the accrual write — a precise cap would need a DB transaction. The cap is a coarse preventive tripwire …` and tenant_quota.rs:284-285 `let projected = baseline_accrued.saturating_add(cost);\n        if projected > state.monthly_budget_usd_micros {`.

**recommendation:** Accept as-is for a coarse tripwire (documented). If exact enforcement is ever required, make the ceiling check and accrual one atomic DB statement — e.g. a conditional `UPDATE … SET accrued = accrued + ?delta WHERE accrued + ?delta <= budget RETURNING accrued` and reject when no row is returned — so the decision and the increment are serialized on the same row.

**confidence:** high

**verify:** The TOCTOU race is real and confirmed by direct code inspection. At `/Users/gustavoschneiter/Documents/HuGR/corelink-server/crates/corelink-container/src/tenant_quota.rs:260-293`, the `check()` method: (1) reads `baseline_accrued` from D1 via `get()` (line 260-268), (2) computes `projected = baseline_accrued + cost` and gates on it (lines 284-285), and (3) then calls the atomic DB-side `accrue()` increment (line 316-320). Steps 1-2 and step 3 are not serialized. Two concurrent requests that both arrive before either commits their accrual will each see the same `baseline_accrued`, both pass the ceiling check, and both commit their delta — potentially pushing the total accrued beyond the budget by up to `N_concurrent * cost_per_op_micros`. The atomic increment on the write side prevents lost-update (under-counting), but does NOT prevent over-admission relative to the ceiling. Crucially, the code explicitly documents this at lines 277-283: 'NOTE: this read-for-ceiling is intentionally NOT serialized with the accrual write — a precise cap would need a DB transaction. The cap is a coarse preventive tripwire…so a small over-shoot under extreme concurrency is acceptable.' This is a real, code-confirmed race, but it is an intentional, documented architectural choice. Severity remains 'info': (a) the design intent is a coarse tripwire, not precise billing enforcement; (b) the over-shoot is bounded by the concurrency window (at $0.001/op default cost and $5 cap, exploiting this to double the cap would require ~5000 simultaneous requests all hitting the tripwire at once — unrealistic for SMB use); (c) the accrual side correctly accumulates all spend atomically, so the DB always reflects the true total after the window closes; (d) this is not a tenant-isolation break, auth bypass, or money-path vulnerability — it is a bounded spend-cap imprecision. The recommendation to use a conditional UPDATE-RETURNING pattern for exact enforcement is sound if the product ever needs hard caps, but it is not a security vulnerability. Confirmed as real / info.

---

### [14] INFO — DSR Routes: Verbose serde_json and Engine Error Messages Forwarded to Caller

**surface:** POST /_internal/dsr/erase and POST /_internal/dsr/verify (crates/corelink-container/src/routes/dsr.rs)

**location:** crates/corelink-container/src/routes/dsr.rs:322,341,361,379

**cvss_reasoning:** CVSS 3.1: AV:N/AC:L/PR:H/UI:N/S:U/C:L/I:N/A:N — Requires high privilege (internal auth key). Confidentiality impact is Low (schema/error taxonomy disclosure, not customer PII). Score: 2.7 (Low).

**explanation:** The DSR (Data Subject Request) erase and verify handlers format and return raw error detail from `serde_json` deserialization failures and the internal erasure engine directly to the HTTP caller. Specifically: `format!("invalid body: {e}")` exposes serde_json's error messages (which include field names, expected types, byte offsets, and enum variant names from the Rust struct definitions), and `format!("erasure failed: {e}")` / `format!("verification failed: {e}")` expose the internal error chain from the erasure engine. These messages can reveal internal schema structure (field names, types, enum variants), the presence/absence of specific backends, and the internal error taxonomy of the erasure worker. While these routes are internal-only (require `CORELINK_INTERNAL_AUTH_KEY`), an attacker who obtains or guesses the key, or an authorized-but-malicious internal caller, gains an information advantage for further probing. Additionally, in a future scenario where the auth gate is misconfigured or bypassed, this information leaks without any authentication at all. The DSR route is particularly sensitive since it handles GDPR right-to-erasure requests — exposing its internal error taxonomy is a privacy-adjacent concern.

**attack_scenario:** 1. Attacker obtains or brute-forces the CORELINK_INTERNAL_AUTH_KEY (or this finding is exploited post-auth-bypass). 2. Sends a POST to /_internal/dsr/erase with a malformed JSON body (e.g., wrong field types, extra fields, missing required fields). 3. Server responds with HTTP 400 and a body such as 'invalid body: missing field `dsr_id` at line 1 column 2' or 'invalid body: unknown variant `tenant_id`, expected one of `erase`, `verify` at line 3 column 8'. 4. Attacker iterates to map the full DsrQueuedV1 struct schema and understand the erasure engine's error taxonomy. 5. This schema map aids in crafting a valid-looking but pathological request (e.g., a DSR that targets a different tenant's data if cross-tenant isolation has any gap).

**reproduction:** 1. Obtain CORELINK_INTERNAL_AUTH_KEY. 2. POST to /_internal/dsr/erase with header X-Corelink-Internal-Auth: <key> and body '{"wrong_field": 1}'. 3. Observe response body contains serde_json field-level error detail. 4. Repeat with progressively correct bodies to reconstruct the DsrQueuedV1 schema. 5. POST to /_internal/dsr/erase with valid schema but an invalid dsr_id pointing to a non-existent record; observe engine error detail in 500 response.

**impact:** Information disclosure of internal schema structure and erasure engine error taxonomy to any caller possessing the internal auth key. In a compound scenario (auth key compromise + schema enumeration), this could assist in crafting targeted erasure requests to probe cross-tenant isolation in the DSR pipeline.

**evidence:** dsr.rs:322: `Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid body: {e}")).into_response()`
dsr.rs:341: `Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("erasure failed: {e}")).into_response()`
dsr.rs:361: `Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid body: {e}")).into_response()`
dsr.rs:379: `Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("verification failed: {e}")).into_response()`

**recommendation:** Replace all four error-forwarding responses with opaque, fixed strings. Log the error detail server-side at the warn/error level with a structured field, but never forward it to the caller:
```rust
// Instead of:
Err(e) => return (StatusCode::BAD_REQUEST, format!("invalid body: {e}")).into_response()
// Use:
Err(e) => {
    tracing::warn!(error = %e, "dsr: invalid body");
    return (StatusCode::BAD_REQUEST, "invalid request body").into_response();
}
// And for engine errors:
Err(e) => {
    tracing::error!(error = %e, "dsr: erasure engine error");
    (StatusCode::INTERNAL_SERVER_ERROR, "erasure failed").into_response()
}
```

**confidence:** high

**verify:** The code at the cited locations is exactly as described. Lines 322, 341, 361, and 379 of `/crates/corelink-container/src/routes/dsr.rs` forward raw serde_json deserialization errors and engine error strings directly to the HTTP caller. The finding is real.

However, the claimed severity of "low" should be downgraded to "info" for the following reasons:

**Auth gate is a hard prerequisite.** The `/_internal/dsr/*` routes are publicly reachable at `corelink-api.humangr.com/_internal/dsr/erase` (matched by the `/_internal/` prefix arm in the worker router at `worker/src/index.ts:509`), but the edge Worker performs a constant-time `CORELINK_INTERNAL_AUTH_KEY` check before forwarding the request to the Durable Object/container (`index.ts:1203-1234`). An unauthenticated request receives HTTP 401 from the Worker and never reaches the Rust handler. To trigger any of these four error responses, the attacker must already possess the `CORELINK_INTERNAL_AUTH_KEY` shared secret.

**Schema is not a meaningful secret.** The `DsrQueuedV1` struct fields (`dsr_id`, `tenant_id`, `subject_id`, `erasure_salt_hex`, `queued_at_ms`, `legal_hold`) are documented in an inline comment at `dsr.rs:61-76`. The struct uses plain `#[derive(Deserialize)]` without `deny_unknown_fields`, and the file comment explicitly states "Unknown fields are accepted and ignored." The serde error messages (`missing field 'dsr_id' at line 1 column 2`) expose field names that are already visible in the open-source codebase.

**Exploit chain premise is weak.** The attack narrative claims schema enumeration "aids in crafting a valid-looking but pathological request" for cross-tenant exploitation. But tenant isolation is not enforced by struct shape — it is enforced by UUID parsing and the orchestrator's `tenant_id` binding. An attacker holding the internal auth key can already supply any `tenant_id` value; knowing the field name does not materially advance a cross-tenant attack.

**The recommendation is still sound as a hardening measure.** Opaque error messages + server-side structured logging is best practice for any internal endpoint, even auth-gated ones, because it: (a) limits information available to an attacker who steals the auth key, (b) prevents internal implementation details from leaking into logs that might be aggregated in less-controlled systems (e.g., if the caller logs response bodies), and (c) follows consistent defense-in-depth. The fix is trivially cheap and should be applied, but it is a hygiene/hardening change, not a live-exploitable vulnerability. Severity is correctly "info" (not "low").

---

### [15] LOW — DSR build_state_from_env: Weak Minimum Key Length Enforcement (is_empty Only, vs. 16-char Minimum Elsewhere)

**surface:** POST /_internal/dsr/erase, POST /_internal/dsr/verify (crates/corelink-container/src/routes/dsr.rs:213-217)

**location:** crates/corelink-container/src/routes/dsr.rs:213-217

**cvss_reasoning:** CVSS 3.1: AV:N/AC:H/PR:N/UI:N/S:U/C:N/I:H/A:H — Attack Complexity is High (attacker must guess a short key AND know the route is mounted with a weak key, a misconfiguration scenario). Integrity and Availability are both High (irreversible data erasure). Score: 7.4 (High) in the misconfiguration scenario; scored Low overall because the precondition (operator misconfiguration to a short key) reduces realistic exploitability.

**explanation:** The `build_state_from_env()` function for the DSR route mounts the route as long as `CORELINK_INTERNAL_AUTH_KEY` is non-empty — a single character is sufficient. Every other route that uses this key enforces a minimum length of 16 characters: `internal_pat::build_state_from_env` checks `internal_auth_key.len() < 16`, and `admin::build_handlers` similarly requires at least 16 chars. The DSR route only checks `is_empty()`, which means an operator who sets `CORELINK_INTERNAL_AUTH_KEY=x` would mount the DSR route successfully while all other routes refuse to mount. The 1-character key provides effectively zero brute-force resistance. The DSR route governs GDPR account erasure — unauthorized triggering could erase tenant data irreversibly from R2, D1, KV, Stripe, and other backends. The inconsistency also creates a false sense of security: operators who see the DSR route mounted assume it meets the same strength bar as all other auth-gated routes.

**attack_scenario:** 1. Operator misconfigures CORELINK_INTERNAL_AUTH_KEY to a short value (e.g., a 4-character test key) during initial deployment. 2. internal_pat, admin, and auth_introspect routes refuse to mount (key too short). 3. The DSR route mounts successfully (only checks is_empty). 4. An attacker who knows or guesses the 4-character key can POST to /_internal/dsr/erase with a crafted DsrQueuedV1 payload, triggering irreversible data erasure for any tenant's account.

**reproduction:** 1. Set CORELINK_INTERNAL_AUTH_KEY=test (4 chars) in the environment. 2. Start the container. 3. Observe in logs that /_internal/pat/mint and admin routes are NOT mounted (key too short). 4. Observe that /_internal/dsr/erase IS mounted (only is_empty check passed). 5. POST to /_internal/dsr/erase with X-Corelink-Internal-Auth: test and a valid DsrQueuedV1 body. 6. Observe the erasure is executed.

**impact:** An operator mistake (short key) or a key brute-force attack enables unauthorized triggering of irreversible tenant data erasure across all configured backends (R2 CAS objects, D1 rows, KV entries, Stripe customer records). This is a data integrity / permanent data loss risk.

**evidence:** dsr.rs:213-217:
```rust
pub fn build_state_from_env() -> Option<DsrRouteState> {
    let internal_auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;
    if internal_auth_key.is_empty() {
        return None;
    }
```
For comparison, internal_pat.rs enforces:
```rust
if internal_auth_key.len() < 16 {
    warn!("CORELINK_INTERNAL_AUTH_KEY too short ...");
    return None;
}
```

**recommendation:** Align the DSR minimum key length check with the rest of the codebase. Replace the `is_empty()` check with a `< 16` (or `< 32` for a higher bar, given DSR's destructive nature) check, with a warning log:
```rust
if internal_auth_key.len() < 16 {
    tracing::warn!(
        "CORELINK_INTERNAL_AUTH_KEY too short (< 16 chars); \
         /_internal/dsr/{{erase,verify}} NOT mounted (fail-CLOSED)"
    );
    return None;
}
```

**confidence:** high

**verify:** The finding is real and the code evidence holds exactly as cited. At `crates/corelink-container/src/routes/dsr.rs:213-217`, `build_state_from_env` checks only `is_empty()` before accepting `CORELINK_INTERNAL_AUTH_KEY`. In contrast, `internal_pat.rs:399` and `admin.rs:378` both enforce `len() < 16` with a tracing warning. This is a verified inconsistency: a key of 1-15 characters would be rejected by the PAT-mint and admin routes (they refuse to mount), but accepted by the DSR erase/verify routes. At request time, `internal_auth_ok` in `dsr.rs:100-115` performs a constant-time comparison against whatever key was loaded — there is no secondary length floor — so a 4-char key would authenticate a request carrying that 4-char value in `x-corelink-internal-auth`. The DSR erase endpoint (`POST /_internal/dsr/erase`) triggers irreversible tenant data deletion via `InMemoryErasureWorker`. Severity is kept at LOW because exploitation requires a prior operator misconfiguration (accidentally setting a short key), it is not a default-configuration or remote-only attack vector, and the DSR routes are internal-only (not exposed on the public ingress). However, the asymmetry is an obvious defense-in-depth gap: the most destructive internal route has the weakest mount guard. The recommended fix (replace `is_empty()` with `len() < 16`, matching the rest of the codebase) is correct and should be applied. The poc: set `CORELINK_INTERNAL_AUTH_KEY=abcd` in the environment; the PAT-mint and admin routes will refuse to mount (warn + None), but the DSR router will mount; `POST /_internal/dsr/erase` with `x-corelink-internal-auth: abcd` and a valid `DsrQueuedV1` JSON body will pass auth and trigger erasure.

---

### [16] INFO — admin handle_mutate: Json Extractor Parses Body Before Handler Auth Check (Mitigated by Body Limit Coverage)

**surface:** POST /v1/admin/mutate (crates/corelink-container/src/routes/admin.rs:616-647)

**location:** crates/corelink-container/src/routes/admin.rs:619,625

**cvss_reasoning:** CVSS 3.1: AV:N/AC:H/PR:N/UI:N/S:U/C:N/I:N/A:L — Attack Complexity is High (internal network reach required), Availability impact is Low (bounded by 10 MiB limit, only CPU cost). Score: 3.7 (Low). Reported as Info given the effective mitigations.

**explanation:** The `handle_mutate` handler uses `Json(body): Json<AdminMutateBody>` as an axum extractor parameter. In axum's extraction pipeline, typed extractors run in parameter order before the handler body executes. The `Json` extractor reads and deserializes the request body to produce the `AdminMutateBody` value. Only then does the handler's first statement at line 625 call `internal_auth_ok`. This means any unauthenticated caller can force the server to perform body allocation and JSON deserialization for this endpoint before auth is checked. However, two mitigating factors substantially reduce the risk: (1) the admin route is part of `build_with_factory` and therefore IS covered by the 10 MiB `DefaultBodyLimit` layer — unbounded allocation is prevented; (2) the admin route is reachable only via the Durable Object's internal TCP channel, not directly from the internet. The finding is noted for completeness and to maintain the M3 pattern consistently across all mutation endpoints.

**attack_scenario:** An attacker on the internal network (or with access to the DO's TCP port) sends a POST to /v1/admin/mutate with a 10 MiB JSON body and no auth header. The server reads and deserializes the full body before returning 403 Forbidden. With high concurrency, this could generate non-trivial serde_json CPU load on the container, but the 10 MiB cap bounds the per-request cost.

**reproduction:** 1. POST to /v1/admin/mutate without X-Corelink-Internal-Auth. 2. Include a 9 MiB JSON body with valid AdminMutateBody shape. 3. Observe the server returns 403 (not 413), confirming the body was parsed before the auth check fired. 4. If the body is > 10 MiB, the server returns 413 before parsing (the limit IS active for this route).

**impact:** Low. Pre-auth deserialization cost per request is bounded by the 10 MiB limit. Internal-only reachability further constrains who can send requests. Primarily a defense-in-depth gap: the correct M3 pattern (auth before body) is not uniformly applied.

**evidence:** admin.rs:616-631:
```rust
async fn handle_mutate(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    Json(body): Json<AdminMutateBody>,  // body parsed here by axum extractor
) -> impl IntoResponse {
    // Auth check AFTER body is already deserialized:
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        ...
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
```

**recommendation:** Refactor `handle_mutate` to accept `body: Bytes` and call `internal_auth_ok` before deserializing, matching the M3 pattern used by dsr.rs and internal_pat.rs:
```rust
async fn handle_mutate(
    State(state): State<AdminRouteState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if !internal_auth_ok(state.internal_auth_key.as_ref(), &headers) {
        tracing::warn!(...);
        return (StatusCode::FORBIDDEN, "forbidden").into_response();
    }
    let body: AdminMutateBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid body").into_response(),
    };
    ...
}
```

**confidence:** high

**verify:** The finding is real and the evidence holds up on independent code review.

At `crates/corelink-container/src/routes/admin.rs:619`, the axum `Json(body): Json<AdminMutateBody>` extractor is a function parameter that axum resolves — including reading and deserializing the full request body — before the handler body begins executing. The `internal_auth_ok` check at line 625 therefore only runs AFTER full deserialization. This is confirmed verbatim in the code.

The mitigating factors claimed by the original finding are also verified:

1. The global `DefaultBodyLimit::max(10 * 1024 * 1024)` (10 MiB) is applied in `main.rs:299-301` as a `.layer(...)` over the entire composed router returned by `routes::build_with_factory(shadow_factory)`, which includes `admin::router(admin_state)`. This means axum's limit middleware will reject bodies over 10 MiB before the extractor is called, bounding the per-request deserialization cost.

2. The admin route only listens on TCP port 50051 inside the Cloudflare Durable Object. The container is not directly reachable from the public internet — an attacker would need DO-level access to reach this endpoint. This dramatically reduces the realistic threat surface.

3. The body itself is a small, structurally simple `AdminMutateBody` JSON object (a handful of optional string fields). Even a maximal 10 MiB payload of valid-but-large JSON would produce bounded, non-explosive serde_json CPU cost (no recursive/deeply-nested schema).

Severity assessment: The ordering pattern is a real anti-pattern (other routes like `dsr.rs` and `internal_pat.rs` correctly accept `Bytes` and check auth first), but the practical exploitability is extremely low given network-isolation, the 10 MiB cap, and the simple schema. The `info` classification is accurate — this is a defense-in-depth improvement opportunity, not an exploitable vulnerability.

The recommendation in the finding (refactor to accept `Bytes`, check auth, then deserialize) is sound and follows the established M3 pattern already used elsewhere in the codebase. It should be applied for consistency and to eliminate even the theoretical pre-auth CPU cost, but it is not urgent.

---

### [17] LOW — Inaccurate Security Documentation Creates False Assurance About PAT Auth Depth

**surface:** Worker source documentation (worker/src/index.ts), Architecture comments

**location:** worker/src/index.ts:583-590, worker/src/index.ts:1789-1792

**explanation:** Two separate code comments in `index.ts` claim that Argon2id verification is performed by 'the Rust DO (corelink-worker Tower middleware)' as a second defense layer after the Worker's D1 existence check. These claims are factually incorrect and create false assurance during security reviews, audits, and threat modeling exercises.

Comment 1 (lines 583-590): 'The Rust DO (corelink-worker Tower middleware) performs the Argon2id verify on the raw token forwarded via the Authorization header. The Worker provides the existence + expiry gate (this function) which eliminates the 'any string accepted' vulnerability. Argon2id + scope checks are the DO's second defence layer.'

Comment 2 (lines 1789-1792): 'Keep raw Authorization on the forwarded request: the DO performs the Argon2id + scope verify against the D1 PAT store (the possession check the Worker skips under its cpu_ms budget).'

Reality: the TypeScript Durable Object (`durable_object.ts`) performs no cryptographic verification. It forwards all requests directly to the container via `proxyToContainer()`. The production container binary (`corelink-server`, `crates/corelink-container`) does not depend on `corelink-worker` (the crate containing the Tower middleware with Argon2id). For CAS/AC/Turbo routes, the container's `AuthTenant` extractor trusts the Worker-set `x-corelink-tenant-id` header directly. No Argon2id happens.

This misleads security reviewers and creates a documented security property that doesn't exist in the deployed system.

**attack_scenario:** Indirect: a security reviewer relying on the documented Argon2id second-factor would not prioritize strengthening the Worker's HMAC-only gate, underestimating the blast radius if PAT_SIGNING_KEY is absent or if HMAC is bypassed. This creates an audit blind spot rather than a direct exploit path.

**reproduction:** Audit path: 1. Read index.ts:583-590 — documentation claims Argon2id is in 'the Rust DO'. 2. Search durable_object.ts for Argon2id: no results. 3. Search corelink-container/Cargo.toml for 'corelink-worker': no results. 4. Search crates/corelink-container/src/routes/cas.rs for Argon2id: no results. The documented second layer does not exist in the deployed code paths.

**impact:** False sense of security regarding PAT authentication depth. If the HMAC check is the only gate (when PAT_SIGNING_KEY is present) or no gate (when absent), the actual blast radius from a stolen token_id is larger than documented. Security teams and auditors building threat models based on these comments will underestimate risk.

**evidence:** worker/src/index.ts:586: `The Rust DO (corelink-worker Tower middleware) performs the Argon2id verify on the raw token forwarded via the Authorization header`

worker/src/index.ts:1789-1791: `// Keep raw Authorization on the forwarded request: the DO performs the // Argon2id + scope verify against the D1 PAT store (the possession check // the Worker skips under its cpu_ms budget).`

worker/src/durable_object.ts:331-332: `const resp = await proxyToContainer(request, fetcher); return resp;` — no Argon2id before proxy.

**recommendation:** Correct the comments immediately. Accurate version for index.ts:583-590: 'NOTE: Argon2id verification is NOT performed in the current request path for CAS/AC/Turbo. The Worker provides a PAT HMAC fast-fail (when PAT_SIGNING_KEY is bound) + D1 existence/expiry gate. The container trusts x-corelink-tenant-id set by the Worker. Argon2id is only wired for OCI two-leg and adapter-host paths (PatVerifier.verify_capability). Adding Argon2id with a per-token-id cache to the CAS path is tracked as a TODO.' Additionally, make PAT_SIGNING_KEY required (not optional) in all non-dev environments.

**confidence:** high

**verify:** The inaccurate documentation is real and confirmed by reading the code directly.

CONFIRMED INACCURACY: worker/src/index.ts:583-590 (the JSDoc header for `extractAuth`) states "The Rust DO (corelink-worker Tower middleware) performs the Argon2id verify on the raw token forwarded via the Authorization header." This is factually false for the CAS/AC/Turbo/Bazel request paths. `durable_object.ts` contains zero references to Argon2id, pat_hash, or any PAT verification — it proxies directly to the container via `proxyToContainer` at line 332.

Similarly, the forwarding comment at worker/src/index.ts:1789-1791 states "the DO performs the Argon2id + scope verify against the D1 PAT store." No such verification occurs in the DO layer.

ACTUAL ARCHITECTURE: The Rust-side `PatVerifier` with real Argon2id IS wired (crates/corelink-container/src/adapter_pat.rs:214-268) — but only for the adapter surfaces (OCI, cargo/sccache, brew, npm, pip), as explicitly Option B. CAS, AC, Turbo, and Bazel routes trust the Worker-injected `x-corelink-tenant-id` header with no Argon2id second factor.

PARTIALLY MITIGATING FACTOR: The correct security posture IS documented within the same `extractAuth` function at lines 646-662: "We deliberately do NOT additionally Argon2id-verify … The HMAC gate already binds possession to the signing key." The JSDoc header (lines 583-590) and the forwarding comment (1789-1791) contradict this correct statement. So a reader who reads the entire function will find both a wrong claim and a correct one.

SEVERITY DOWNGRADE: The original reporter claimed "medium." This is a documentation bug, not an exploitable vulnerability. There is no direct attack path — a miscreant cannot exploit a wrong comment. The real risk is an indirect audit blind spot (someone reading only the JSDoc header might believe Argon2id is active on the CAS path when it is not). "Low" is appropriate: real impact exists only if a future security reviewer relies on the false claim to deprioritize strengthening the HMAC-only gate, and only if PAT_SIGNING_KEY is absent or compromised. It is not "info" because it misrepresents the actual security boundary in load-bearing documentation.

poc_or_disproof: No exploit PoC is possible for a documentation-only finding. Disproof attempt was run — searching durable_object.ts for all auth-related terms (argon, Argon, verify, pat_hash, PatVerifier) returned zero hits, confirming the DO performs no PAT verification. The false claim in the JSDoc at line 586 stands unambiguously contradicted by the actual code.

---

### [18] MEDIUM — HMAC Fast-Fail Gate Is Silently Skipped When PAT_SIGNING_KEY Is Unset

**surface:** Worker PAT auth middleware (worker/src/index.ts extractAuth)

**location:** worker/src/index.ts:663-672

**explanation:** The PAT authentication in `extractAuth()` includes an HMAC-SHA256 fast-fail step (Step 3) that verifies the `hmac_sig` segment of the PAT against the `PAT_SIGNING_KEY` secret. This check is the primary cryptographic possession proof — it ensures the caller physically holds the signing key rather than just knowing a `token_id` from a partial leak.

However, the entire HMAC check is wrapped in an optional guard: `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0) { ... }`. When `PAT_SIGNING_KEY` is not bound (the secret is not set via `wrangler secret put`), the guard evaluates to false and the entire HMAC verification block is skipped. Execution falls directly to the D1 lookup.

The consequence: in any environment without `PAT_SIGNING_KEY` (staging, CI runners, new regional deployments before secrets are provisioned, or dev), a structurally valid PAT (correct format, 95 or 96 chars, correct prefix, matching Crockford-b32 and base64url charsets) with a KNOWN `token_id` will be accepted if the `token_id` exists in D1 and has not expired. The `random_secret` and `hmac_sig` segments are never checked.

The `PAT_SIGNING_KEY` is declared as `optional` (`PAT_SIGNING_KEY?: string`) in the `Env` interface and in the `CLAUDE.md` documentation (`.env.local` keys). There is no validation at Worker boot that the key is present. A misconfiguration, accidental secret rotation failure, or incorrect deployment could silently remove the HMAC gate in production.

**attack_scenario:** 1. Attacker identifies that a CoreLink environment is running without PAT_SIGNING_KEY bound (e.g., staging, or a regional worker that hasn't had secrets provisioned). 2. Attacker acquires a valid `token_id` through any means: observing a PAT in build logs, extracting from a misconfigured CI artifact, or D1 enumeration. 3. Attacker crafts: `Authorization: Bearer corelink_pat_<token_id>.<43 valid base64url chars>.<22 valid base64url chars>`. 4. The Worker's extractAuth: format check passes; HMAC check skipped (no key); D1 lookup succeeds; expiry check passes → returns tenant_id. 5. Attacker accesses that tenant's CAS/AC/Turbo cache with full read-write scope.

**reproduction:** In a staging environment without PAT_SIGNING_KEY: 1. Obtain a token_id of any non-expired PAT from D1 (e.g., via an admin query or a leaked token). 2. Construct: `Bearer corelink_pat_<TOKEN_ID>.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.AAAAAAAAAAAAAAAAAAA` (43 + 22 A chars). 3. `curl -H 'Authorization: Bearer corelink_pat_...' https://staging.corelink-api.humangr.com/v1/cas/blobs/sha256:abc123/0`. 4. Observe the request is forwarded to the DO (not rejected with 401) and returns a response from the correct tenant's CAS namespace.

**impact:** In environments without PAT_SIGNING_KEY: authentication reduces to knowing a valid token_id. Complete read-write access to any tenant's cache for which a token_id is known. In production (assuming PAT_SIGNING_KEY is set): no immediate impact, but the silent opt-out design creates a misconfiguration risk. A failed secret rotation or a new regional deployment that omits the secret would silently degrade auth.

**evidence:** worker/src/index.ts:663-672: `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0) { const hmacOk = await verifyPatHmac( env.PAT_SIGNING_KEY, parsed.hmacPreimage, parsed.hmacSigBytes, ); if (!hmacOk) { return { ok: false, reason: 'invalid_pat_hmac' }; } }` — the entire HMAC block is silently skipped when the key is absent.

worker/src/index.ts:70: `PAT_SIGNING_KEY?: string;` — declared optional in the Env interface.

**recommendation:** Make PAT_SIGNING_KEY required and enforce it at startup. Remove the optional `?` from `PAT_SIGNING_KEY?: string` in the Env interface. Add a fail-CLOSED guard at the top of `extractAuth()`: `if (!env.PAT_SIGNING_KEY || env.PAT_SIGNING_KEY.length < 32) { return { ok: false, reason: 'signing_key_not_configured' }; }`. Map `signing_key_not_configured` to HTTP 503 in the caller so operators are alerted immediately. Alternatively, add a startup check in the Worker's fetch handler that returns 503 for all requests if the key is absent.

**confidence:** high

**verify:** The finding is confirmed. Independent code reading at worker/src/index.ts:663-672 reproduces the exact vulnerable path: the HMAC-SHA256 possession check is wrapped in `if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0)` with no else-branch and no fail-CLOSED default. When PAT_SIGNING_KEY is absent (staging, misconfigured regional worker, dev environment), the entire cryptographic possession gate is silently skipped.

The code's own inline comment at lines 647-651 makes the security model explicit: "PAT_SIGNING_KEY is bound in prod, so this HMAC-SHA256 check IS the cryptographic possession gate — forging a PAT requires that key, not merely a stolen/guessed token_id." When the key is absent, that model inverts: the D1 lookup by token_id (line 688) becomes the sole gate, and it does NOT verify the random_secret or hmac_sig segments — it only checks token existence and non-revocation. An attacker who obtains any valid token_id (16 Crockford b32 chars, observable in logs/CI artifacts/build outputs) can authenticate as that tenant by presenting any format-valid random_secret (43 b64url chars) and hmac_sig (22 b64url chars), gaining full read-write access to that tenant's CAS/AC/Turbo cache.

Severity is maintained at medium rather than escalated to high because the precondition is non-trivial: (a) PAT_SIGNING_KEY must be unset in the targeted environment, AND (b) the attacker must know a valid token_id. Both conditions narrow the realistic attack surface. In production all 5 regional workers have PAT_SIGNING_KEY set per the secrets-checklist (docs/internal/secrets-checklist.md row 141, wrangler.toml lines 238-242). The risk is most acute in staging/dev environments or if a new regional worker is deployed without the secret being provisioned before traffic is served. The Env interface declaring it as optional (`PAT_SIGNING_KEY?: string` at line 70) with no startup guard is the root cause — the fix is to add a fail-CLOSED guard at the top of extractAuth() returning a non-2xx (503) when the key is absent.

The poc: in an environment where PAT_SIGNING_KEY is unset, issue `Authorization: Bearer corelink_pat_<any_known_16char_token_id>.<43 valid b64url chars>.<22 valid b64url chars>`. parsePat() succeeds (format valid), HMAC block is skipped (no key), D1 lookup by token_id succeeds, expiry passes, extractAuth returns ok=true with the victim tenant's tenant_id.

---

### [19] INFO — Health Endpoints Disclose Deployment Environment Without Authentication

**surface:** Worker health routes (worker/src/index.ts:1114-1124)

**location:** worker/src/index.ts:1116

**explanation:** The public health endpoints `/health`, `/_health`, and `/api/health` are accessible without any authentication and return a JSON response that includes the literal value of `env.ENVIRONMENT`. This is the Worker's `ENVIRONMENT` binding from `wrangler.toml`, which can contain values like `'prod'`, `'staging'`, `'prod-lhr'`, `'prod-sam'`, etc.

This discloses the deployment environment name and — in the case of regional workers — the geographic region of the Worker serving the request. An attacker can use this to determine which environments exist (prod, staging), which regions are deployed, and potentially to target a specific environment for further attacks.

For example, `GET /health` returns `{"status":"ok","env":"prod-lhr"}`, confirming that a London (LHR) regional production instance is running and discoverable. The multi-region architecture becomes enumerable from the public internet.

**attack_scenario:** 1. Attacker sends `GET /health` to `https://corelink-api.humangr.com/health`. 2. Response: `{"status":"ok","env":"prod"}`. 3. Attacker probes regional endpoints (sam.corelink-api.humangr.com, lhr.corelink-api.humangr.com, etc.) and gets `{"status":"ok","env":"prod-sam"}`, `{"status":"ok","env":"prod-lhr"}`, confirming the multi-region topology. 4. Attacker can direct attacks at specific regions or use environment info to tailor exploits.

**reproduction:** 1. `curl https://corelink-api.humangr.com/health` → `{"status":"ok","env":"prod"}` 2. `curl https://sam.corelink-api.humangr.com/health` → `{"status":"ok","env":"prod-sam"}`

**impact:** Low severity: reveals deployment environment names and geographic topology without authentication. Primarily useful for reconnaissance. No direct data or credential exposure.

**evidence:** worker/src/index.ts:1116: `const body = JSON.stringify({ status: statusLiteral, env: env.ENVIRONMENT });`

**recommendation:** Remove `env.ENVIRONMENT` from the public health response. A simple `{"status":"ok"}` (or `{"status":"SERVING"}`) is sufficient for health monitoring tools. If environment details are needed for internal debugging, serve them only on an authenticated endpoint or restrict to requests with the internal-auth header. Change to: `const body = JSON.stringify({ status: statusLiteral });`

**confidence:** high

**verify:** The code at worker/src/index.ts:1116 is exactly as cited and is reachable without authentication on both /health and /api/health routes (confirmed by matchRoute at lines 377-384). The `env.ENVIRONMENT` binding is a non-secret wrangler var set to string values: "prod", "staging", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd" (wrangler.toml lines 203, 250, 439, 580, 698, 816, 934).

The finding is REAL as a code fact. However, the severity claimed ("low") is still overstated — this should be classified "info":

1. The regional topology is already publicly exposed at the DNS / URL layer. Hitting sam.corelink-api.humangr.com already tells an attacker they are in SAM; the health response confirms "prod-sam" — the same information already encoded in the hostname. There is zero information uplift.

2. The ENVIRONMENT strings are enumerable labels with no operational secrets — no IPs, no internal service names, no credentials, no stack details beyond what subdomain routing already reveals.

3. workers_dev = false is set globally and per-env (wrangler.toml lines 26, 577, 695, 813, 931), so the "dev" default in [vars] (line 203) is never reachable from public endpoints.

4. The regional hostnames themselves are already public (DNS CNAMEs, the product's multi-region feature is a selling point). The finding adds no exploitable attack chain — it cannot enable tenant isolation bypass, credential theft, billing fraud, or BOLA.

The recommendation (remove env field from the unauthenticated response) is still worth doing as a hygiene measure (defense-in-depth, minimal-disclosure principle), but this is an informational/cosmetic issue. CVSS 3.1: AV:N/AC:L/PR:N/UI:N/S:U/C:N/I:N/A:N = 0.0. Adjusted severity: info.

---

### [20] LOW — Session Exchange Mint Throttle Fails Open on D1 Error — Abuse Window on Control Plane Outage

**surface:** Session exchange throttle (worker/src/lib/session_exchange.ts:checkMintThrottle)

**location:** worker/src/lib/session_exchange.ts:118-135

**explanation:** The `checkMintThrottle()` function in `session_exchange.ts` implements a per-principal fixed-window rate limit on PAT minting via `POST /v1/session/exchange`. The throttle is backed by D1's `session_exchange_throttle` table. When D1 is unavailable (network error, database failure), the catch block logs the error and returns `null`, which the caller interprets as 'proceed with the mint'. This is the documented 'fail-open' posture.

The risk: during a D1 control-plane outage (even partial), the throttle gate is fully disabled. A still-valid Clerk session can loop-mint unbounded PATs during the outage window. Each mint involves a container Argon2id hash computation and a D1 PAT row insert (for the pat table) — the pat table insert might also fail during a D1 outage, but if the D1 outage affects only the throttle table's write path while the pat table's write path remains functional, an attacker can exhaust the container's Argon2id CPU budget.

The per-window throttle cap is 10 mints per 60 seconds per principal. This is deliberately generous (covers retries + multiple devices). During a D1 outage, the cap is entirely removed.

**attack_scenario:** 1. Attacker holds a valid Clerk session (legitimate authenticated user or stolen session). 2. D1 outage occurs (or attacker triggers one via resource exhaustion on the D1 endpoint). 3. Attacker loops POST /v1/session/exchange in rapid succession. Each call: verifies Clerk JWT (fast), skips throttle (D1 error caught), calls container /_internal/pat/mint (Argon2id + D1 insert). 4. Attacker generates dozens of short-lived PATs (each an Argon2id hash cost on the shared container). 5. Container CPU is saturated. Other tenants' requests degrade (DoS on the _system DO's container).

**reproduction:** In a test environment: 1. Make the session_exchange_throttle table temporarily unavailable (DROP or lock the table). 2. With a valid Clerk JWT, POST /v1/session/exchange in a tight loop. 3. Observe no 429 responses. 4. Monitor container CPU — each mint triggers Argon2id hashing.

**impact:** During D1 outage: denial of service on the shared _system DO's container via Argon2id CPU exhaustion. Impact is bounded by D1 outage duration and the Clerk session TTL, but on a shared container this affects all tenants routing through the _system DO. After outage recovery, the throttle counter resets (window may have rolled), allowing another burst.

**evidence:** worker/src/lib/session_exchange.ts:118-135: `} catch { // D1 unavailable — fail OPEN (allow). The limit still applies whenever the // counter is readable. Logged without PII (principalId is opaque). console.error(`[${requestId}] session exchange throttle store error; allowing`); return null; }`

**recommendation:** Consider fail-CLOSED on D1 throttle errors for the session exchange endpoint specifically, returning a 503 rather than allowing through. The session exchange is not a latency-critical path (it's used at session start, not on every cache operation), so a 503 on D1 error is preferable to unbounded minting. If fail-open must be preserved for availability, add a secondary in-memory rate limiter (module-level Map in the Worker isolate) as a local backstop: `if (inMemoryCounter.get(principalId) > MAX_BURST) { return 429; }`. The in-memory limiter resets on isolate cold-start but provides protection within a single isolate's lifetime.

**confidence:** medium

**verify:** The fail-open behavior is confirmed at worker/src/lib/session_exchange.ts:120-124. The catch block unconditionally returns null (allow) whenever the D1 INSERT/UPDATE throws, regardless of error type. The PAT mint operation genuinely runs Argon2id with OWASP 2024 parameters (m=65536 KiB, t=3, p=4) per crates/corelink-pat/src/argon.rs:44-81 — each mint is a real CPU cost on the shared container.

However, the attack chain has two hard prerequisites that cap the severity at low:

1. The attacker must hold a valid, unexpired Clerk session JWT. This means they are an authenticated, enrolled user — not an anonymous attacker. Stolen sessions are a separate attack and orthogonal to this finding.

2. D1 must be experiencing an outage. D1 is a managed Cloudflare-global service; an application-layer attacker cannot force a D1 outage to create the window. Without a D1 failure, the throttle fires normally (10 mints/60 s per principalId) and blocks the loop.

If both preconditions are simultaneously true, the attacker can loop POST /v1/session/exchange rapidly. Each call: Clerk JWT verify (fast, edge-local) succeeds, throttle is skipped (D1 error caught), mint request reaches the _system DO container, Argon2id runs. Sustained looping could saturate the container's CPU, degrading all tenants sharing that DO. There is no secondary in-memory rate limiter and no Cloudflare WAF rate-limit binding on this path.

The impact is a DoS on the _system DO container (CPU saturation), not a data breach, tenant isolation bypass, or money-path compromise. No cross-tenant data is exposed. The blast radius is container CPU during a D1 outage window — a window in which the system is already degraded.

The reporter's severity of low is accurate and is not understated. The fix suggestions (fail-CLOSED 503 or module-level in-memory backstop) are reasonable defense-in-depth improvements but are not urgent given the double-gated preconditions. The intentional fail-open trade-off is documented in the code comment and mirrors the getTierForTenant precedent, which is a known deliberate posture in this codebase.

---

### [21] LOW — Partial D1 Outage Can Cause Incorrect Quota Enforcement (False-Positive 429) for High-Tier Tenants

**surface:** Worker quota middleware (worker/src/lib/quota.ts:getTierForTenant, checkStorageQuota)

**location:** worker/src/lib/quota.ts:91-137, worker/src/lib/quota.ts:177-213

**explanation:** The Worker's quota enforcement uses two sequential D1 queries per request: (1) `getTierForTenant()` reads the tenant's subscription tier from `tier_selections` + `tenant` tables; (2) `checkStorageQuota()` reads the tenant's current `bytes_used` from `tenant_storage_state`. These two functions have ASYMMETRIC failure modes: `getTierForTenant()` falls back to 'free' on any D1 error, while `checkStorageQuota()` fails open (returns `ok:true`) on D1 error.

The dangerous scenario: a partial D1 outage where `getTierForTenant()`'s queries fail (returns 'free' tier with 10 GB limit) but `checkStorageQuota()`'s query succeeds (returns the actual bytes_used). A 'max' tier tenant (2 TB limit) with 50 GB stored would receive: tier='free' (from error fallback) + bytes_used=53_687_091_200 (actual, from successful query). The storage check compares 53 GB vs the free tier's 10 GB cap and returns `ok:false` with 429, incorrectly blocking the tenant.

This is a data consistency hazard under partial outage: the tier lookup and the storage lookup are served from different D1 queries that can independently succeed or fail. A tenant is falsely rate-limited to their 'free' quota during a brief D1 instability affecting only one of the two query paths.

**attack_scenario:** Not directly exploitable by an attacker; this is a reliability/correctness issue triggered by infrastructure conditions. However, a targeted D1 rate-limit exhaustion attack on the `tier_selections` query specifically (if query-level isolation were possible) could trigger false-positive quota blocks for paying tenants.

**reproduction:** 1. Set up a 'max' tier tenant with 50 GB stored in tenant_storage_state. 2. In the D1 database, temporarily make SELECT on tier_selections/tenant fail (e.g., drop the table temporarily, or mock at test level). 3. Send a PAT-authenticated CAS request. getTierForTenant() fails → returns 'free'. checkStorageQuota() succeeds → returns 53 GB. Worker compares 53 GB > 10 GB (free tier limit) → returns 429. 4. Paying tenant is incorrectly blocked.

**impact:** False-positive quota enforcement (HTTP 429) for tenants on paid tiers (solo/starter/pro/max) during partial D1 outages. Service disruption for paying customers. Low probability (requires partial, asymmetric D1 failure), but real in practice during rolling D1 maintenance, regional failover, or capacity events.

**evidence:** worker/src/lib/quota.ts:111-136: `try { tierSel = await db.prepare('SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = \'active\' LIMIT 1').bind(tenantId).first(); } catch { /* D1 error — fall through to tenant.tier lookup */ } ... try { tenantTier = await db.prepare('SELECT tier FROM tenant WHERE tenant_id = ?1 LIMIT 1').bind(tenantId).first(); } catch { /* D1 error — fall through to hard default */ } return 'free'; // hardcoded fallback`

worker/src/lib/quota.ts:198-201: `} catch { // D1 error → fail open. return { ok: true }; }` — storage query fails open.

**recommendation:** Make the failure modes symmetric: if `getTierForTenant()` encounters a D1 error, ALSO fail the storage quota check open (return `ok:true`). The simplest fix is to propagate the D1 error state from `getTierForTenant()` to the caller: `const { tier, d1Error } = await getTierForTenantWithStatus(env.CONFIG_DB, resolvedTenantId); if (d1Error) { /* skip quota check entirely, fail open */ }`. Alternatively, treat a 'free' tier derived from an error condition differently from a confirmed 'free' tier and skip the storage check in the error case.

**confidence:** medium

**verify:** The finding is confirmed at the code level. The asymmetric failure mode is real and demonstrable from the source.

EXACT VULNERABLE PATH (worker/src/lib/quota.ts):

1. getTierForTenant (lines 91-137): If both D1 queries throw (lines 111-113 catch block for tier_selections, lines 127-129 catch block for tenant), the function falls through to the hard default at line 136 and returns 'free' — the most restrictive tier (10 GiB storage cap). The file header at lines 6-8 describes this as "fail-open on D1 errors" but the tier-lookup path is actually fail-CLOSED: it silently downgrades the tenant to 'free'.

2. checkStorageQuota (lines 177-213): Called immediately after getTierForTenant at index.ts:1651 with the resolved tier. If the storage query SUCCEEDS (lines 191-201) and the tenant's actual bytes_used exceeds the 'free' ceiling (10 * 1,073,741,824 bytes), the function returns ok:false at line 209, triggering a 429 at index.ts:1652-1672. The storage query's own failure path (line 198-200) correctly fails open — but that only helps if the storage query also fails.

THE SPECIFIC SCENARIO THAT CAUSES THE FALSE-POSITIVE 429:
- D1 is healthy enough for tenant_storage_state queries but not for tier_selections / tenant queries (e.g., selective query timeout, table lock, or transient per-query failure on sequential calls within the same request).
- getTierForTenant returns 'free' due to caught exceptions.
- checkStorageQuota queries tenant_storage_state successfully and finds, say, 50 GiB for a solo-tier tenant.
- 50 GiB > 10 GiB (free cap) → ok:false → HTTP 429 sent to a paying tenant.

SEVERITY ASSESSMENT:
This is a reliability/availability issue, not a security bypass. It requires a partial and asymmetric D1 failure mode — D1 errors are typically instance-level, so full outages cause both code paths to fail open (storage check at line 198-200 returns ok:true). The scenario where tier tables fail but storage tables succeed is technically possible (different query complexity, query-level timeouts, etc.) but not the normal outage pattern. No attacker-controlled exploitation path exists: a hypothetical D1-exhaustion attack would need query-level isolation that D1 does not expose per-tenant. The impact is false-positive 429 (denial of service for paying tenants), not data exfiltration or billing bypass. Low severity is the correct rating.

INTERNAL COMMENT INCONSISTENCY (supporting evidence):
The file header (line 7) says "Enforcement is fail-open on D1 errors" and line 87 says "Fail-open: D1 errors return 'free' (the most restrictive tier)." These two sentences contradict each other: returning the most restrictive tier is fail-CLOSED for paying tenants, not fail-open. The storage check IS truly fail-open (line 199). The mismatch in documented intent vs. actual behavior for the tier-lookup half is the root of the issue.

RECOMMENDATION (confirmed correct from the finding):
The simplest fix is to propagate whether 'free' was the result of an error vs. a confirmed lookup. If getTierForTenant encountered D1 errors on both queries, checkStorageQuota should be skipped (return ok:true) to make both failure paths consistently fail-open. Alternatively, the tier-lookup could re-throw on D1 errors and let the caller catch and treat it as fail-open — but that changes the public API signature. The recommendation in the finding (add a d1Error flag to the return value) is sound and minimal.

---

### [22] INFO — timingSafeEqual in durable_object.ts Uses a Zero HMAC Key and Has Length-Branch Timing Asymmetry

**surface:** Durable Object helper (worker/src/durable_object.ts:timingSafeEqual)

**location:** worker/src/durable_object.ts:112-139

**explanation:** The `timingSafeEqual()` function exported from `durable_object.ts` is designed to compare two strings in constant time. It uses HMAC-SHA256 to pre-process both inputs before the XOR comparison, which is a valid approach to constant-time comparison. However, the implementation has two weaknesses:

1. ZERO HMAC KEY: The HMAC key is `new Uint8Array(32)` — all 32 bytes are zero. This is a publicly known 'key'. The HMAC is used here not for secrecy but for timing indistinguishability (any input → fixed-length 32-byte output). For this specific use (timing equalization, not authentication), a zero key is technically sufficient. However, using a zero key is an anti-pattern: if this function were ever adapted for use in an authentication comparison, the zero key would make the HMAC trivially forgeable.

2. LENGTH-BRANCH TIMING ASYMMETRY: For different-length strings (lines 116-119), the function runs `SHA-256(aBytes)` (hashing only input `a`) to consume 'constant work', then returns `false`. For same-length strings, it runs `HMAC(zero_key, aBytes)` + `HMAC(zero_key, bBytes)` (two HMAC operations). The timing for length-mismatched inputs differs from same-length inputs (one hash vs two HMACs), potentially creating a distinguishable timing signal based on whether lengths match.

Critically, this function is EXPORTED but NOT imported in any production code path. The security-sensitive internal-auth comparison in `index.ts` uses `crypto.subtle.timingSafeEqual()` directly on byte buffers (not this helper). The function is dead code from a security perspective.

**attack_scenario:** No current exploit path since the function is not used in production security comparisons. Theoretical future risk: if a developer imports and uses this function to compare a secret value, the zero key means the HMAC provides no confidentiality protection — an attacker who can observe the comparison timing could distinguish length mismatches from content mismatches. The length-branch asymmetry (one SHA-256 vs two HMACs) leaks length-equality information via timing.

**reproduction:** 1. Import timingSafeEqual from durable_object.ts. 2. Call `await timingSafeEqual('secret_value', 'a')` — observe fast return (length mismatch path: 1x SHA-256). 3. Call `await timingSafeEqual('secret_value', 'wrong_length_x')` — observe slightly slower return (same length: 2x HMAC). The timing delta can reveal whether a guessed string has the correct length.

**impact:** Minimal in current codebase (function is not called in production). Latent risk: if adopted for security-sensitive comparisons in future code without reviewing the zero-key design, could enable timing attacks. The exported symbol name `timingSafeEqual` implies it is safe to use for authentication comparisons, which is a misleading contract given the zero-key and length-branch issues.

**evidence:** worker/src/durable_object.ts:116-119: `if (aBytes.length !== bBytes.length) { // Consume constant work to prevent length-based timing await crypto.subtle.digest('SHA-256', aBytes); return false; }`

worker/src/durable_object.ts:121-127: `const key = await crypto.subtle.importKey( 'raw', new Uint8Array(32), // ZERO KEY { name: 'HMAC', hash: 'SHA-256' }, false, ['sign'] );`

worker/src/durable_object.ts:142: `export { timingSafeEqual };` — exported but not imported by any production module.

**recommendation:** 1. Remove the export or mark it as test-only. 2. If the function is intended for production use, replace the zero key with a module-level random nonce (`const TIMING_KEY = crypto.getRandomValues(new Uint8Array(32))`) generated once per isolate, making the HMAC key opaque to external observers. 3. Fix the length-branch asymmetry: run BOTH HMAC operations regardless of length, then check length equality as a separate constant-time step. 4. Alternatively, use `crypto.subtle.timingSafeEqual()` directly on UTF-8-encoded bytes (same as `index.ts` does) rather than the HMAC-based wrapper.

**confidence:** high

**verify:** The finding is confirmed as technically accurate on all cited points, but the severity of "info" is correct and I would not raise it.

Evidence verified:

1. Zero HMAC key: `worker/src/durable_object.ts:123` — `new Uint8Array(32)` (all zeros) is passed as the HMAC key. This means the "keying" provides no secret, so an attacker who can observe HMAC outputs for chosen inputs could reconstruct them independently. However, for the actual equality comparison (XOR of two HMAC outputs under the same key), a zero key still produces a constant-time result for equal-length inputs — the MAC is still collision-resistant for this use case.

2. Length-branch timing asymmetry: `durable_object.ts:116-119` — when lengths differ, the code runs one SHA-256 digest and returns false. When lengths match, it runs two HMAC-SHA256 signs. The work is asymmetric: the SHA-256 path is faster than two HMAC paths, so an observer CAN distinguish "lengths differ" from "lengths match" via timing. The comment says "consume constant work to prevent length-based timing" but this claim is false — SHA-256 ≠ two HMAC-SHA256 in execution time.

3. Not used in production: Verified exhaustively. `index.ts` imports only `CoreLinkServer` from `durable_object.ts` (line 23) — not `timingSafeEqual`. All production constant-time comparisons in `index.ts` use `crypto.subtle.timingSafeEqual` directly (lines 962, 1223), which is the Cloudflare Workers runtime's native implementation. The custom `timingSafeEqual` export exists solely for test access, as the comment at line 141 states: "Keep the export so tests can call it directly". Tests in `worker/tests/durable_object.test.ts` import and exercise it directly.

Severity assessment: "info" is correct. There is zero current exploit path because the function is never invoked on any production security-sensitive comparison. The defects (zero key + length asymmetry) are latent design flaws in dead/test-only code. They would only matter if a future developer imports and uses this function for a secret comparison — at which point the length-oracle leak could become a real timing side-channel. The recommendation to fix both issues is sound, but the present risk is informational only.

---

### [23] LOW — Brew non-content-addressed paths (tag manifests) cached into PUBLIC_NAMESPACE without pre-store integrity verification

**surface:** routes/brew.rs + crates/corelink-adapter-host/src/brew/bottle.rs

**location:** crates/corelink-adapter-host/src/brew/bottle.rs:185-206

**explanation:** The Homebrew brew adapter proxies all requests to ghcr.io and caches the response bytes in the multi-tenant PUBLIC_NAMESPACE (shared across ALL tenants). For OCI content-addressed paths whose last segment matches `sha256:<64-hex>` (e.g. `/v2/homebrew/core/curl/blobs/sha256:abc...`), the code correctly computes the sha256 of the fetched bytes and rejects any mismatch before storing. However, for tag-addressed manifest paths (e.g. `/v2/homebrew/core/curl/manifests/8.5.0`), `expected_sha256()` returns `None`, and the code at bottle.rs:185-206 falls into the `None => false` branch, skipping all integrity verification. These bytes go directly into PUBLIC_NAMESPACE and are served to EVERY tenant that requests the same canonical path. If ghcr.io returns a corrupt or tampered response (CDN edge fault, transient error, DNS poisoning), the corrupted bytes are cached once and served to all CoreLink tenants until the entry is manually purged. An authenticated tenant can deliberately trigger caching of a tag-manifest path, racing a transient upstream error into the shared cache.

**attack_scenario:** 1. Attacker holds a valid CoreLink PAT (any tier). 2. Attacker sends a brew proxy GET for a tag-addressed manifest: GET /brew/<tenant>/v2/homebrew/core/git/manifests/latest. 3. If ghcr.io is simultaneously serving a corrupt or injected response (transient CDN fault, short DNS hijack), the corrupted bytes pass through the None branch, skipping the sha256 integrity block. 4. The corrupted bytes are stored in PUBLIC_NAMESPACE under cas_key_for(canonical_path). 5. All subsequent tenants requesting the same manifest path receive the corrupted cached bytes until the entry expires or is purged. This is a cross-tenant cache-poisoning attack.

**reproduction:** 1. Stand up a CoreLink dev instance with the brew adapter wired to a local HTTP server masquerading as ghcr.io. 2. Mint a PAT with cas:rw scope. 3. Send GET /brew/<tenant>/v2/homebrew/core/curl/manifests/8.5.0 (a tag path, NOT a sha256 digest path). 4. The local ghcr.io mock returns corrupt bytes for this request. 5. Observe in the audit log that verified_sha256 = false. 6. Confirm the corrupt bytes are stored in PUBLIC_NAMESPACE. 7. Make the same request from a different tenant PAT; observe the corrupt bytes are returned from cache (no upstream re-fetch occurred).

**impact:** All CoreLink brew-proxy tenants receive the poisoned manifest until the cache entry is purged. Homebrew clients use manifests to determine which blob digest to pull; a crafted corrupt manifest could redirect pulls to wrong-version blobs or cause build failures across all tenants. In a worst-case supply-chain scenario, a specially crafted manifest could direct clients to fetch attacker-controlled blob digests. Blast radius: all brew-proxy customers across all tenants.

**evidence:** ```rust
// crates/corelink-adapter-host/src/brew/bottle.rs:185-206
let verified_sha256 = match expected_sha256(&canonical) {
    Some(expected) => {
        let computed = sha256_hex(&bytes);
        if computed != expected {
            // ... audit + return Err ...
        }
        true
    }
    None => false,  // <-- tag paths skip ALL integrity checking, stored as-is
};
// bottle.rs:88-93 (expected_sha256)
let hex_digest = last.strip_prefix("sha256:")?;  // None for tags like "8.5.0"
```

**recommendation:** For tag-addressed manifest paths, parse the `Docker-Content-Digest` or `ETag` response header returned by ghcr.io and store it alongside the cache entry. At minimum, log the sha256 of every fetched body (tag-addressed and content-addressed) in the audit row. Longer term, consider refusing to promote tag-addressed manifests to PUBLIC_NAMESPACE since tags are mutable by nature and lack the immutability guarantees of content-addressed blobs. Store tag manifests under per-tenant namespaces with a short TTL, or serve them pass-through without caching.

**confidence:** high

**verify:** The vulnerability is real and confirmed by code, but the claimed severity and attack prerequisite are significantly overstated.

**What the code actually does (confirmed):**

`crates/corelink-adapter-host/src/brew/bottle.rs:185-206` confirms the `None` branch: when `expected_sha256(&canonical)` returns `None` (which it does for tag-addressed paths like `.../manifests/latest` or `.../manifests/8.5.0`), the fetched bytes are stored without any sha256 integrity check, with `verified_sha256 = false`.

`crates/corelink-container/src/routes/brew.rs:80-99` confirms the PUBLIC_NAMESPACE storage: `BrewMoatStore::get` and `BrewMoatStore::put` both ignore the `tenant_id` parameter entirely and use `PUBLIC_NAMESPACE` (`"_public"`) for all operations. This means a corrupted tag-manifest stored by one fetch is indeed served cross-tenant.

**Where the finding overstates the attack:**

The claimed attack path states "Attacker holds a valid CoreLink PAT" and sends a brew proxy GET. The PAT gates client access to the caching service; it does NOT give the attacker any ability to control what the upstream (ghcr.io) returns. The container fetches from ghcr.io over HTTPS (`BREW_UPSTREAM_DOMAIN = "https://ghcr.io"`). For a corrupted response to be returned, the attacker must have **network-position** — DNS hijacking of ghcr.io, or TLS MITM against the container's outbound connection. A Cloudflare Container running in CF's infrastructure is not trivially exposed to tenant-controlled DNS poisoning.

The PAT is irrelevant to exploitability: it merely authenticates the triggering request. The actual attack requires an infrastructure-level adversary (BGP hijack, DNS spoofing at CF's resolver, or a compromised ghcr.io CDN node). This shifts the attack vector from Network/Low-complexity to Network/High-complexity and raises required privilege from Low to High (infra control), making CVSS 3.1 AV:N/AC:H/PR:H/UI:N/S:C/C:L/I:L/A:N ≈ 4.9 (Medium) at most, and arguably Low given the required infrastructure position.

**The cross-tenant impact is real but bounded:** If triggered (infra-level event), a poisoned tag manifest is served to all subsequent tenants until TTL expiry or manual purge. Tag manifests are small metadata objects; they do not contain binary bottle content. A poisoned manifest would cause brew clients to fail to resolve or download subsequent bottles (DoS / brew install failure) rather than silently serving malicious binaries (those would need blob-level poisoning, which IS protected).

**The recommendation is sound** regardless of severity: tag-addressed manifests are mutable by nature and should not be promoted to PUBLIC_NAMESPACE without at minimum logging the sha256 of every fetched body, or capping TTL, or storing under per-tenant namespaces. The longer-term fix (pass-through without caching, or short TTL per-tenant) correctly addresses the mutable-reference problem. The Docker-Content-Digest / ETag header approach is feasible.

**Verdict:** Confirmed as a real gap — tag paths skip all integrity verification before storage in the shared namespace — but severity is Low (not Medium) because exploitation requires infrastructure-level network position against Cloudflare's egress, not just a valid PAT.

---

### [24] LOW — Authenticated tenant can flood shared container memory via concurrent 100 MiB Turborepo PUT bodies, causing cross-tenant DoS

**surface:** routes/turbo_v8.rs

**location:** crates/corelink-container/src/routes/turbo_v8.rs:97 + 239-241

**explanation:** The Turborepo PUT route accepts bodies up to 100 MiB (`TURBO_BODY_LIMIT_BYTES = 100 * 1024 * 1024`), intentionally larger than the global 10 MiB body limit to support large build artifacts. Axum buffers the entire request body in memory before the handler runs. Because multiple tenants route to the same shared container process, an authenticated tenant can issue many concurrent 100 MiB PUTs to saturate container memory. The quota gate (ADR-0068) enforces a monthly dollar ceiling but does NOT enforce a concurrency or rate limit. A single tenant can exhaust their entire monthly budget in one burst of parallel requests, each holding 100 MiB in memory, potentially triggering OOM for all tenants sharing the container.

**attack_scenario:** 1. Attacker holds a valid CoreLink PAT at any paid tier. 2. Attacker sends N concurrent PUT /v8/artifacts/<hash>?teamId=team requests each with a 100 MiB body. 3. Each request is buffered fully in axum memory. At N=20 this is 2 GiB held simultaneously. 4. The container OOMs and the shared Durable Object (serving ALL tenants' CAS/AC/Turbo/Bazel/cargo/brew/npm/pip) crashes. 5. All tenants experience a service outage until the container restarts.

**reproduction:** 1. Obtain a CoreLink PAT with cas:rw scope. 2. Run 20 concurrent curl commands: for i in $(seq 1 20); do curl -X PUT -H 'Authorization: Bearer <PAT>' --data-binary @<100MB_FILE> 'https://corelink-api.humangr.com/v8/artifacts/aaa...?teamId=team' & done; wait. 3. Monitor container memory; observe OOM restart or service degradation for other tenants.

**impact:** Cross-tenant DoS. All tenants sharing the container experience an outage. The monthly dollar-ceiling quota gate does not prevent burst-memory attacks within a single billing period.

**evidence:** ```rust
// crates/corelink-container/src/routes/turbo_v8.rs:97
pub const TURBO_BODY_LIMIT_BYTES: usize = 100 * 1024 * 1024;

// turbo_v8.rs:239-241
.layer(axum::extract::DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES))
// Applied as innermost layer to ALL turbo routes including PUT
```

**recommendation:** Enforce a per-tenant concurrency limit at the Cloudflare Worker layer (Durable Object rate-limiter or CF rate limit rule on /v8/artifacts PUT). In the container, consider streaming the Turbo PUT body directly to R2 rather than full in-memory buffering. Add a per-tenant in-flight PUT counter that rejects new requests beyond a configurable ceiling (e.g. 4 concurrent uploads per tenant) with 429.

**confidence:** medium

**verify:** The vulnerability is real but the claimed blast radius is wrong in the most important dimension, which changes severity.

WHAT IS CONFIRMED:
- `TURBO_BODY_LIMIT_BYTES = 100 * 1024 * 1024` is real (turbo_v8.rs:97).
- The `DefaultBodyLimit::max(TURBO_BODY_LIMIT_BYTES)` is applied as the innermost axum layer to all Turbo routes including PUT (turbo_v8.rs:239-241).
- The PUT handler buffers the full body in RAM via `body: axum::body::Bytes` (turbo_v8.rs:308) — no streaming to R2 during upload.
- There is no per-tenant in-flight PUT concurrency limit anywhere in the stack: no CF rate-limit rules in wrangler.toml, no in-flight counter in the Rust container, no Worker-layer Turbo-specific concurrency gate.
- The Worker-layer request quota (worker/src/lib/quota.ts) is explicitly a no-op ("deferred until a monthly counter table is wired", quota.ts:57-60), and even when wired it counts total requests, not concurrent in-flight body bytes.
- N concurrent 100 MiB bodies do accumulate simultaneously in container RAM before axum dispatches to the handler.

WHAT IS WRONG IN THE FINDING (critical):
The claimed blast radius — "the shared Durable Object (serving ALL tenants' CAS/AC/Turbo/Bazel/cargo/brew/npm/pip) crashes" — is factually incorrect. The architecture is per-tenant, not shared. The Worker routes to `env.CORELINK_SERVER.idFromName(resolvedTenantId)` where `resolvedTenantId` is the PAT-resolved real tenant UUID (worker/src/index.ts:1758). Each tenant gets their own isolated DO instance, which manages its own container instance. This is explicitly documented in durable_object.ts line 9: "Per-tenant pinning: DO ID = idFromName(tenantId) — never cross-tenant."

An attacker flooding concurrent 100 MiB Turbo PUTs would OOM and crash THEIR OWN container only. Other tenants run in separate DO+container instances and are completely unaffected. The cross-tenant DoS claim is refuted.

REAL IMPACT:
The actual impact is a self-DoS: a tenant can OOM their own container, causing their own service to restart (losing any InMemoryKvStore state if R2 is not configured, and disrupting their own builds). This is a legitimate nuisance-level availability issue — a misconfigured or malicious client could cause their own build cache to cycle. It does not affect any other tenant. A sufficiently disruptive self-DoS is also relevant if CoreLink charges per-resource and a container OOM generates noise in operator alerting.

SEVERITY JUSTIFICATION (LOW, not MEDIUM):
CVSS reasoning: AV:N / AC:L / PR:L (valid PAT required) / UI:N / S:U (no scope change — only the attacker's own tenant/container is affected) / C:N / I:N / A:L (availability of attacker's own cache). CVSS 3.1 base score ~4.3 (Medium) but given it only affects the attacker's own resources (equivalent to a self-inflicted outage), the operational severity is LOW. No other tenant is impacted; the service restarts automatically.

---

### [25] LOW — OCI upload session buffer is process-local and unbounded — unrefinalized sessions accumulate memory with no cross-session cap

**surface:** routes/oci.rs

**location:** crates/corelink-container/src/routes/oci.rs:123-143 + 158-163

**explanation:** The `OciMoatStore` buffers all in-flight OCI blob upload sessions in a `Mutex<HashMap<String, Vec<u8>>>` held in process memory. This buffer is explicitly documented as non-durable (line 108: 'The buffer is NOT durable (process restart drops in-flight uploads)'). There is no cap on the number of open upload sessions per tenant or in total, and no inactivity timeout. A tenant can open many upload sessions via POST /v2/repo/blobs/uploads/ without finalizing them, accumulating gigabytes of memory in the shared Mutex. Combined with the Turbo 100 MiB body issue, multiple authenticated tenants can jointly exhaust container memory. On any container restart (OOM, deployment, unhandled panic), all in-flight uploads are silently dropped with no OCI-spec-compliant error to the client, requiring full re-push.

**attack_scenario:** 1. Attacker holds a valid CoreLink PAT. 2. Attacker calls POST /v2/repo/blobs/uploads/ to open a session, then PATCH /v2/repo/blobs/uploads/<uuid> repeatedly with chunk data, never calling the final PUT. 3. Each session accumulates its chunk bytes in the in-memory HashMap without bound. 4. With multiple sessions the attacker consumes gigabytes, contributing to container OOM and cross-tenant DoS.

**reproduction:** 1. Obtain a CoreLink PAT with cas:rw scope. 2. Exchange at /token for an OCI push bearer. 3. POST /v2/myrepo/blobs/uploads/ N times (N=50). 4. For each session UUID, PATCH /v2/myrepo/blobs/uploads/<uuid> with 10 MiB chunks. 5. Never call the finalizing PUT. 6. Monitor container memory; observe growth without reclaim.

**impact:** Cross-tenant DoS via memory exhaustion from accumulated unrefinalized upload sessions. All tenants on the shared container are affected. Additionally, on container restart all in-progress pushes by legitimate users fail silently with TCP reset, requiring costly full re-push.

**evidence:** ```rust
// crates/corelink-container/src/routes/oci.rs:123-143
struct OciMoatStore {
    moat: Arc<MoatCache>,
    uploads: Mutex<HashMap<String, Vec<u8>>>,  // unbounded, process-local
}
// oci.rs:158-163
async fn open_upload(&self, tenant: &TenantId) -> PortResult<String> {
    let uuid = format!("{}:{}", tenant.to_canonical_text(), Uuid::new_v4().simple());
    self.uploads.lock()...insert(uuid.clone(), Vec::new());
    // no cap on number of open sessions
    Ok(uuid)
}
```

**recommendation:** Add a per-tenant cap on open upload sessions (e.g. 4) enforced in `open_upload()` before inserting. Add an inactivity timeout (e.g. 10 minutes) implemented via a background cleanup task that cancels idle sessions. Enforce a total upload buffer size limit across all tenants. For durability, migrate to R2 multipart upload for sessions, consistent with the R2KvStore pattern used for Turbo.

**confidence:** high

**verify:** The finding is real but requires nuance on both scope and severity.

**What is confirmed:**

The `OciMoatStore.uploads` HashMap in `/crates/corelink-container/src/routes/oci.rs` (lines 123–127, 158–165) has no cap on the number of concurrent open upload sessions. `open_upload` inserts a new empty `Vec<u8>` per call with no count guard. An authenticated attacker can open arbitrarily many sessions via repeated `POST /v2/repo/blobs/uploads/`.

The PATCH handler in `/crates/corelink-adapter-host/src/oci/server/handlers.rs` line 305 collects the entire request body with `axum::body::to_bytes(body, usize::MAX)` — no HTTP-level size cap before the body is in memory. This body is then handed to `append_chunk`, and only after that is the cumulative session total compared against `blob_size_limit_bytes` (5 GiB default). So a single PATCH can buffer up to 5 GiB of attacker-supplied data in the process heap before the oversize check fires and cancels the session. With N concurrent sessions each receiving a large PATCH body, memory pressure scales as N × (chunk size), up to N × 5 GiB.

No inactivity timeout or aggregate-sessions cap exists anywhere in the upload path.

**What the finding overstates:**

The per-session memory is bounded: the oversize check in `push/upload.rs` lines 121–138 fires during each PATCH and cancels the session once cumulative bytes exceed `blob_size_limit_bytes` (5 GiB). Sessions cannot grow without bound individually. The real vector is the number of sessions, not unbounded growth of a single session.

**Severity adjustment:**

This is LOW, not lower. The attack requires a valid, authenticated CoreLink PAT. Anonymous or external-network exploitation is impossible. It is a service-availability concern (OOM on the OCI Durable Object container), not a confidentiality/integrity/money-path issue. Its practical impact is cross-tenant DoS for the duration of the OOM event — real but bounded by the container runtime's OOM killer restarting the process. Downgraded from any implicit medium to LOW is correct.

**PoC outline:**

1. Obtain a valid `cas:rw` PAT for any tenant.
2. Exchange the PAT at `GET /token?scope=repository:repo:push,pull` to obtain a bearer.
3. In a loop, issue `POST /v2/repo/blobs/uploads/` with the bearer (no body required) to open N sessions. Each call succeeds and inserts an empty `Vec<u8>` into the shared HashMap.
4. For each session UUID returned, issue `PATCH /v2/repo/blobs/uploads/<uuid>` with a large body (e.g. 1–5 GiB). The server buffers the entire body via `to_bytes(body, usize::MAX)` before running the oversize check. Never issue the finalizing PUT.
5. With enough concurrent PATCH requests, the process heap grows toward N × chunk_size, contributing to OOM and taking down the shared OCI Durable Object for all tenants.

---

### [26] INFO — OCI surface bypasses x-corelink-scope header gate by design — single-layer scope enforcement vs double-layer for all other adapters

**surface:** routes/oci.rs + worker/src/index.ts (oci_v2 route arm)

**location:** crates/corelink-container/src/routes/oci.rs:323-379

**explanation:** All other cache surfaces (cargo, brew, npm, pip, turbo, bazel, CAS/AC) receive a server-trusted `x-corelink-scope` header injected by the Cloudflare Worker from the D1 PAT record after auth, and the container enforces this header at a middleware gate before any adapter code runs. The OCI surface intentionally does NOT receive or gate on this header: the Worker forwards OCI requests raw ('pass-through'), and the container comment at line 375 states 'No oci_gate: per-op authorization is the adapter's bearer-scope enforcement plus the /token downscope'. The compensating controls are solid: full Option-B PAT re-verification (HMAC + Argon2id) at /token, OciScope::restricted_to_read() downscoping for read-only PATs, and HMAC-signed bearer verification on every data-plane operation. However, if a future bug in the OCI adapter's scope.allows() bearer-scope check were introduced, there is no backstop x-corelink-scope gate layer to catch it, unlike the other five surfaces which have both. The defense-in-depth layering is thinner for OCI.

**attack_scenario:** Not currently exploitable. Future scenario: a bug is introduced in the OCI adapter bearer scope enforcement (scope.allows() returns wrong result for some token format). On non-OCI surfaces this would be caught by the separate x-corelink-scope gate. On OCI there is no such backstop. A read-only PAT could push images if both the /token downscope and the data-plane scope.allows() were simultaneously broken.

**reproduction:** Verify by inspection: send GET /v2/alpine/blobs/sha256:0000...0000 without any Authorization header but with x-corelink-scope: cas:rw injected. Response is 401 with Www-Authenticate (OCI challenge), not 403 as a scope gate would return. This confirms the OCI data plane has no x-corelink-scope gate and relies solely on bearer scope.

**impact:** No immediate exploitable impact. The OCI surface has one fewer defense-in-depth layer than the other five cache surfaces. Future scope-escalation bugs in OCI bearer enforcement have no backstop.

**evidence:** ```rust
// crates/corelink-container/src/routes/oci.rs:375-379
// No `oci_gate`: per-op authorization is the adapter's bearer-scope
// enforcement (`scope.allows(repo, action)` on every `/v2` op) plus the
// `/token` downscope to the PAT's capability (see `router` doc). The Worker
// forwards OCI raw, so there is no server-set `x-corelink-scope` to gate on.
```

**recommendation:** Document the OCI two-leg pass-through architecture in auth_model.md as an explicit exception with compensating controls enumerated. Consider adding an OCI-specific scope gate on the /token endpoint only (where the Worker does have the Basic auth PAT and could inject scope) to restore at least one layer of defense-in-depth at the credential exchange point. Long-term: evaluate injecting x-corelink-scope on /token requests (not data-plane) to add the missing backstop without disrupting the two-leg OCI flow.

**confidence:** high

**verify:** The finding is accurate and confirmed by code evidence, but the claimed severity of "info" is correct — no downgrade or upgrade is warranted.

What the code actually shows:

1. The OCI pass-through design is real and intentional. `worker/src/index.ts:1397-1433` shows the Worker routes `oci_v2` and `oci_token` to a dedicated DO with a comment at line 1411: "Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope." The `x-corelink-scope` header that all other adapters (CAS, AC, Bazel, Turbo, npm, pip, brew, cargo) depend on as a second enforcement layer is never set for OCI requests.

2. The OCI adapter has its own single-layer enforcement chain instead. At `/token`, the PAT is fully verified via Option-B (HMAC fast-reject → D1 lookup → Argon2id → scope gate at `adapter_pat.rs:263-266`), and the resulting bearer is downscoped at `oci/server/handlers.rs:125-128`: a `cas:r` PAT is forced to `restricted_to_read()`, preventing push-bearer minting. On the data plane, every OCI operation checks `scope.allows(repo, action)` before storage access: `pull/blob.rs:20`, `pull/manifest.rs:38`, `push/upload.rs:42`, `push/manifest.rs:123`, `tags.rs:31`.

3. The architecture is documented extensively IN CODE (module-level doc in `crates/corelink-container/src/routes/oci.rs:302-322`, inline Worker comments at lines 1391-1416), but NOT in the architecture spec documents (`specs/03_architecture/auth_model.md` and `security_model.md` contain zero mentions of OCI or its two-leg exception).

4. The attack scenario requires simultaneous independent failure of two controls: (a) the `/token` downscope logic AND (b) the data-plane `scope.allows()` check. No such bug exists in the current code. The finding's framing ("future scenario", "not currently exploitable") is honest.

5. The finding's recommendation — documenting the OCI two-leg pass-through as an explicit exception in auth_model.md with its compensating controls enumerated — is valid and actionable. The absence of this documentation means a future developer adding a "missing" scope header gate for OCI would not understand why the architecture deliberately omits it, and could accidentally break the two-leg flow; conversely, a future bug in OCI scope enforcement would have no documented compensating-control checklist to audit against.

Verdict: confirmed as an info-level architectural documentation gap. The single-layer OCI scope model is correct by design given the OCI two-leg flow constraint, but the exception and its compensating controls are not recorded in the authoritative spec documents. No severity adjustment from the original "info" claim is warranted.

---

### [27] INFO — PatVerifier can_write bit discarded by non-OCI adapter resolvers — per-op write enforcement relies solely on x-corelink-scope header trust

**surface:** crates/corelink-container/src/adapter_pat.rs + routes/cargo.rs + routes/brew.rs + routes/npm.rs + routes/pip.rs

**location:** crates/corelink-container/src/adapter_pat.rs:274-278 + routes/cargo.rs:72-78

**explanation:** The shared `PatVerifier.verify_capability()` method returns `(tenant_id: String, can_write: bool)` after full HMAC + Argon2id verification against D1. The OCI adapter correctly uses the `can_write` bit to downscope the minted bearer token, providing true end-to-end scope enforcement from D1 through to the data plane. However, for cargo, brew, npm, and pip, the adapters call `PatVerifier.verify()` (not `verify_capability()`), which internally calls `verify_capability()` but discards the `can_write` bit via `.map(|(tenant, _can_write)| tenant)`. Per-operation write enforcement for these four adapters is then performed separately by the method-based gate (cargo_gate, npm gate, pip gate) which reads from the `x-corelink-scope` header injected by the Worker. This means the D1-sourced `can_write` truth from the PAT re-verify is never cross-checked against the Worker-injected header value. If the Worker ever injects `x-corelink-scope: read-write` when the PAT's D1 scope is `read-only`, the container's Option-B re-verify would compute `can_write = false` but the header-trusting gate would allow the write. The Worker is the correct and trusted source today (it reads from D1 itself), but the container does not independently validate the consistency between the re-verified scope and the header scope.

**attack_scenario:** Not currently exploitable because the Worker correctly reads scope from D1 and strips all client-supplied x-corelink-scope before injecting its own. Exploitation requires a Worker bug where x-corelink-scope is set to read-write for a read-only PAT. In that case the container would allow PUTs to cargo/brew/npm/pip for a read-only PAT.

**reproduction:** Theoretical. To verify the design gap: inspect CargoPatResolver.resolve() at cargo.rs:72-78 which calls verify() (not verify_capability()), and cargo_gate at cargo.rs:149-158 which enforces write from the x-corelink-scope header value, not from the PAT re-verify result. There is no code that cross-checks these two signals.

**impact:** No immediate exploitable impact given correct Worker behavior. Architectural gap: the defense-in-depth for write enforcement on the four non-OCI header-scoped adapters relies on Worker header correctness. The OCI surface has true end-to-end scope enforcement from D1. Aligning the other four surfaces to the OCI model would improve defense-in-depth.

**evidence:** ```rust
// adapter_pat.rs:274-278
pub async fn verify(&self, pat_plaintext: &str) -> Result<String, VerifyError> {
    self.verify_capability(pat_plaintext)
        .await
        .map(|(tenant, _can_write)| tenant)  // can_write discarded
}
// routes/cargo.rs:72-78 (CargoPatResolver)
async fn resolve(&self, pat_plaintext: &str) -> Result<String, TenantResolveError> {
    self.0.verify(pat_plaintext).await.map_err(...)
    // calls verify(), not verify_capability() -- can_write bit never surfaced
}
// routes/cargo.rs:149-158 (cargo_gate) enforces write from x-corelink-scope header
```

**recommendation:** Extend the cargo/brew/npm/pip TenantResolver port to carry the can_write bit (mirroring OCI's ResolvedPat). Thread the bit into each adapter's method gate so the write enforcement is: scope_ok_from_header AND can_write_from_pat_reverify, or even replace the header-based check with the re-verified bit entirely. This eliminates the header-trust dependency for write enforcement on these four surfaces and aligns them with OCI's stronger model.

**confidence:** high

**verify:** The finding is accurately described and the code evidence holds up at every cited location. Here is the full verification chain:

**1. `PatVerifier::verify()` discards `can_write` — confirmed.**
`adapter_pat.rs:274-278`: `verify()` is a thin wrapper over `verify_capability()` that maps `|(tenant, _can_write)| tenant`, explicitly discarding the write bit. This is documented in the inline comment at line 213: "The simpler `verify` discards the bit (per-op write enforcement for the header-scoped adapters stays at the route from `x-corelink-scope`)."

**2. Cargo/brew/npm/pip resolvers call `verify()`, not `verify_capability()` — confirmed.**
`routes/cargo.rs:72-78` (`CargoPatResolver::resolve`) calls `self.0.verify(pat_plaintext)`. The same pattern is confirmed in `routes/brew.rs`, `routes/npm.rs`, and `routes/pip.rs` (all import `PatVerifier`/`VerifyError` from `crate::adapter_pat` and follow the identical thin-shell newtype pattern).

**3. Write enforcement for these four surfaces is solely from the `x-corelink-scope` header — confirmed.**
`routes/cargo.rs:139-184` (`cargo_gate` middleware): `scope` is read from `req.headers().get(SCOPE_HEADER)` and `requires_cache_write(scope)` is the sole gate for PUT operations. If the Worker forwards a `read-write` scope, a PUT succeeds; if it forwards `read-only`, the PUT is denied. The `can_write` bit from the D1 PAT row is never consulted in this path.

**4. The Worker correctly strips client-supplied `x-corelink-scope` and sets it from D1 — confirmed.**
`worker/src/index.ts:282-313`: `x-corelink-scope` is in the `CLIENT_TRUST_HEADERS` constant list. `stripClientTrustHeaders()` is called on every forward path before the Worker sets the scope from `auth.scope` (the D1-resolved value). Concretely: line 1769 strips, line 1782 sets from `auth.scope`. This is the structural protection that makes the discarded `can_write` bit safe at present.

**5. OCI uses `verify_capability()` and surfaces the `can_write` bit — confirmed.**
`routes/oci.rs:280-299`: `OciPatResolver::resolve_pat_capability()` calls `self.0.verify_capability(pat.expose()).await` and returns `ResolvedPat { tenant, can_write }`. This makes OCI's token-minting write-gated by the PAT's actual D1 scope, independently of the Worker header, giving OCI two layers of enforcement (header + PAT re-verify). The four non-OCI adapters have only one layer (header only, at the container gate).

**Assessment: real, but not currently exploitable; severity correctly rated as info.**
The design discrepancy is real — four adapters have one container-layer write-enforcement mechanism (the Worker-set `x-corelink-scope` header), while OCI has two (header + `can_write` from PAT re-verification). The single-layer path is safe today because the Worker's `stripClientTrustHeaders` + `h.set("x-corelink-scope", auth.scope)` pipeline is correct and enforced structurally. Exploitation would require a Worker-side bug that either (a) fails to strip a client-supplied scope, or (b) sets the wrong scope value (e.g., `read-write` for a `read-only` PAT). There is no such bug in the current Worker code. The severity is info — this is a defense-in-depth gap and an architectural hardening opportunity, not a present vulnerability. The recommendation to thread `can_write` into cargo/brew/npm/pip resolvers (mirroring OCI's `ResolvedPat` pattern) is sound and would eliminate the header-trust dependency for write enforcement on those four surfaces, making the system uniformly two-layered.

**poc_or_disproof (disproof of exploitability):** A client cannot exploit this today because `stripClientTrustHeaders` in `worker/src/index.ts:309-313` deletes `x-corelink-scope` from every request before the Worker sets it from D1. There is no code path by which a client-supplied `x-corelink-scope: read-write` value can survive to the container when the PAT's D1 scope is `read-only`. The only way to reach the discarded-`can_write` path is through a Worker-layer defect, which does not currently exist.

---

### [28] MEDIUM — DSR Erasure Route Accepts Single-Character CORELINK_INTERNAL_AUTH_KEY (No Minimum-Length Enforcement)

**surface:** GDPR/DSR erasure internal API — POST /_internal/dsr/erase and /_internal/dsr/verify

**location:** crates/corelink-container/src/routes/dsr.rs:215

**explanation:** The DSR (Data Subject Request) erasure route enforces only that CORELINK_INTERNAL_AUTH_KEY is non-empty. A one-character value like 'x' passes the guard and the route mounts and accepts requests that carry that single character as the shared bearer secret. Every other route that shares this key — admin.rs, admin_pilot.rs, and internal_pat.rs — enforces a minimum of 16 characters. The DSR route is the odd one out, having no such lower bound. This creates a configuration-time trap: if an operator accidentally sets a trivially-guessable or very short key (e.g., a single word), the DSR erase and verify endpoints become reachable by any process that can guess that value, while the other routes would refuse to mount at all. Because the DSR route drives real GDPR erasure of tenant data across D1, R2, Stripe, and KV backends (Wave 1 wired in PR #254), an attacker who can reach the container's internal listener with a correct one-character key can trigger arbitrary tenant data deletion. The container's internal listener (port 50051 in dev, bound behind the DO's TCP gate in production) is not directly internet-accessible, so exploitation requires either a compromised Worker/DO or an internal-network attacker — but the misconfiguration window is real and the consequence is GDPR data destruction.

**attack_scenario:** 1. Operator accidentally sets CORELINK_INTERNAL_AUTH_KEY='x' (1 char) during a misconfigured deploy. 2. The DSR route mounts (is_empty() passes); the admin and internal_pat routes do NOT mount (< 16 check fails). 3. An attacker with internal-network access (e.g., a compromised co-tenant process on the same CF Containers host, or a malicious Worker handler) sends: POST /_internal/dsr/erase with header x-corelink-internal-auth: x and a crafted JSON body with any tenant UUID. 4. The constant-time compare succeeds (expected_bytes = [0x78], provided_padded = [0x78], len_ok = 1). 5. The erasure orchestrator runs and deletes all data for the specified tenant from D1, R2 CAS, R2 AC, Stripe subscriptions, and KV.

**reproduction:** 1. Set env var CORELINK_INTERNAL_AUTH_KEY=a (single char). 2. Boot the container (dev mode). 3. Observe that /_internal/dsr/erase IS mounted (log: 'routes: /_internal/dsr/erase route mounted'). 4. Send: POST /_internal/dsr/erase with headers x-corelink-internal-auth: a and Content-Type: application/json, body {"event": {"tenant_id": "<any-uuid>", ...}}. 5. The route processes the erasure request.

**impact:** Unauthorized full GDPR data erasure for any tenant chosen by the attacker. Blast radius: all data for the targeted tenant in D1 tables (auth, billing, telemetry), R2 CAS/AC buckets, KV namespace, and Stripe subscription cancellation. Because erasure is permanent (tombstones set), recovery is only possible from backups. This is irreversible data destruction for a customer, not merely data exfiltration.

**evidence:** ```rust
// crates/corelink-container/src/routes/dsr.rs:213-217
pub fn build_state_from_env() -> Option<DsrRouteState> {
    let internal_auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;
    if internal_auth_key.is_empty() {
        return None;
    }
    // ... route mounts with a 1-char key ...
```
Contrast with the same key check in crates/corelink-container/src/routes/admin.rs:376-385 which enforces `< 16` chars, and internal_pat.rs:399 which also enforces `< 16` chars. All three share the same env var but the DSR check only tests is_empty().

**recommendation:** Add the same 16-character minimum (or preferably 32 bytes to match the PAT_SIGNING_KEY standard) to DSR's build_state_from_env, mirroring the admin.rs guard exactly:
```rust
pub fn build_state_from_env() -> Option<DsrRouteState> {
    let internal_auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;
    if internal_auth_key.len() < 32 {
        tracing::warn!(
            "CORELINK_INTERNAL_AUTH_KEY too short (< 32 chars); \
             /_internal/dsr/* route NOT mounted"
        );
        return None;
    }
    // ...
}
```
Consider raising the floor to 32 chars across all routes for consistency with the PAT signing key requirement and the NIST 128-bit minimum for symmetric authentication secrets.

**confidence:** high

**verify:** The finding is confirmed with exact code evidence. At /Users/gustavoschneiter/Documents/HuGR/corelink-server/crates/corelink-container/src/routes/dsr.rs lines 213-217, `build_state_from_env` guards the DSR route mount with only `is_empty()` — a single-character key passes this check and causes the route to mount. In contrast, `admin.rs:378` and `internal_pat.rs:399` both enforce `key.len() < 16` on the same `CORELINK_INTERNAL_AUTH_KEY` env var. The inconsistency is real and independently verified.

The attack path is as described: an operator who sets CORELINK_INTERNAL_AUTH_KEY='x' would find the admin and internal_pat routes refusing to mount (logging a warning), but the DSR route mounting successfully. The constant-time auth check at dsr.rs:100-115 is itself correct (it does fold in length equality), so a valid 1-char key 'x' would authenticate a caller who provides the header `x-corelink-internal-auth: x`. Wave 1 real erasure adapters are wired (modules adapter_d1, adapter_r2_ac, adapter_r2_cas, adapter_stripe at lines 48-52 are real transports, not placeholders), so a successful request drives real data deletion.

Severity is medium (not high/critical) for two reasons: (1) it requires a misconfiguration precondition — an operator must set a weak key, and (2) `/_internal/dsr/erase` is an internal route reachable only from the Worker-to-container internal network, not the public internet. An attacker needs internal network access (compromised co-tenant on the CF Containers host, or a malicious Worker handler). Without both preconditions, there is no exploitable path. The severity is not escalated beyond medium, matching the original claim.

The fix is straightforward and exactly as recommended: raise the DSR `build_state_from_env` guard to `len() < 32` (matching the PAT_SIGNING_KEY standard) or at minimum `len() < 16` to be consistent with admin.rs and internal_pat.rs.

---

### [29] LOW — Doc/Code Discrepancy: internal_pat.rs Documents 32-byte Minimum for CORELINK_INTERNAL_AUTH_KEY But Enforces Only 16 Characters

**surface:** PAT mint internal route — POST /_internal/pat/mint

**location:** crates/corelink-container/src/routes/internal_pat.rs:390-399

**explanation:** The build_state_from_env function for the internal PAT mint route has a doc comment (line 390) that explicitly states CORELINK_INTERNAL_AUTH_KEY 'Must be at least 32 bytes (ASCII)'. However, the actual enforcement on line 399 only checks for 16 characters: `if auth_key.len() < 16`. This means the code accepts a 16-character secret when the documentation promises a 32-byte floor. A 16-character ASCII string derived from an operator who reads only the code (not the doc) might use a shorter, more memorable secret. Because this secret gates the PAT mint route — the route that provisions PATs for new tenants on signup — a guessable 16-character key that slips through the doc-promised 32-byte gate could allow an attacker to mint arbitrary PATs for any tenant. The route is only reachable from the signup-worker via the DO's internal TCP port, reducing exposure, but the security property promised in the doc is not enforced in the code.

**attack_scenario:** 1. Operator reads the doc comment 'Must be at least 32 bytes' and believes 16-31 character keys are rejected. 2. Operator sets CORELINK_INTERNAL_AUTH_KEY to a human-readable 20-character string (e.g. 'corelink-internal-20') thinking it would be rejected. 3. The route mounts (len() < 16 passes at 20 chars). 4. An attacker who has read the internal docs or guesses the key structure sends POST /_internal/pat/mint with x-corelink-internal-auth: corelink-internal-20 and obtains a PAT for any tenant UUID in the request body.

**reproduction:** 1. Set CORELINK_INTERNAL_AUTH_KEY=thisistwentychars (20 chars, not 32). 2. Boot container. 3. Route mounts despite doc saying it requires ≥32 bytes. 4. Verify: POST /_internal/pat/mint with header x-corelink-internal-auth: thisistwentychars and body {"tenant_id": "<uuid>", "principal_id": "<uuid>", "scopes": "admin", "ttl_seconds": 31536000} returns 200 with a minted PAT.

**impact:** Unauthorized PAT minting for any tenant. An attacker who can reach the container's internal TCP port with the correct (undersized) key can provision admin-scoped PATs for any tenant UUID, giving full cache read/write and admin control over that tenant's data. Scope is limited to operators who deploy with a key between 16 and 31 characters, believing the doc guarantee of 32-byte enforcement.

**evidence:** ```rust
// crates/corelink-container/src/routes/internal_pat.rs:389-405
/// - `CORELINK_INTERNAL_AUTH_KEY` — shared secret for the auth header gate.
///   Must be at least 32 bytes (ASCII). Missing → route returns 503 on every  // <-- doc says 32
///   request (fail-CLOSED: we never mint PATs without a secret gate).
pub fn build_state_from_env() -> Option<InternalPatRouteState> {
    let auth_key = std::env::var("CORELINK_INTERNAL_AUTH_KEY").ok()?;
    if auth_key.len() < 16 {  // <-- code enforces only 16
        tracing::warn!(
            "CORELINK_INTERNAL_AUTH_KEY too short (< 16 chars); \
             /_internal/pat/mint route NOT mounted"
        );
        return None;
    }
```

**recommendation:** Align the code with the documented promise. Change `auth_key.len() < 16` to `auth_key.len() < 32` and update the warning message:
```rust
if auth_key.len() < 32 {
    tracing::warn!(
        "CORELINK_INTERNAL_AUTH_KEY too short (< 32 chars); \
         /_internal/pat/mint route NOT mounted"
    );
    return None;
}
```
Also update admin.rs:378 from `< 16` to `< 32` to be consistent across all routes that share this key. The secrets matrix recommends `openssl rand -hex 32` (64 chars) which comfortably clears the 32-char bar.

**confidence:** high

**verify:** The finding is confirmed. The code at `crates/corelink-container/src/routes/internal_pat.rs:390-399` contains an exact discrepancy: the doc comment on lines 389-391 states "Must be at least 32 bytes (ASCII)" while the enforcement on line 399 is `if auth_key.len() < 16`. This is not a test-only artifact — `build_state_from_env()` is the production boot-time function that gates route mounting. The same function in `admin.rs:376-385` is at least internally consistent (both doc and check say 16), so the discrepancy is specific to `internal_pat.rs`.

The attack path as described is technically reachable: an operator who reads the doc comment and trusts "32 bytes minimum" but sets a 20-character key would have a weaker-than-documented secret accepted by the runtime. However, severity stays LOW for several concrete reasons: (1) The secrets-checklist at `docs/internal/secrets-checklist.md:206` recommends `openssl rand -hex 32` (producing a 64-character hex string), which clears both thresholds comfortably — operators following the actual runbook are not exposed. (2) Even with a 16-31 char ASCII key, the key must still be known or guessed; this is not an auth bypass, it is reduced entropy relative to the documented promise. (3) The `internal_auth_ok` function at lines 183-201 uses constant-time comparison (`subtle::ConstantTimeEq`) correctly, so there is no timing side-channel that would help an attacker guess a short key. (4) The route is internal-only (not exposed on the public edge Worker — it is mounted in the container runtime), reducing the attacker surface.

The fix is straightforward: change `< 16` to `< 32` in `internal_pat.rs:399` and align the warning message. The doc comment is already correct at 32. The `admin.rs` check documents 16 honestly (its own doc says 16), so whether that should be bumped to 32 is a separate consistency decision. The poc/disproof note: any key of length 16-31 chars set as CORELINK_INTERNAL_AUTH_KEY would be accepted at boot (route mounts), contradicting the documented 32-byte minimum. Example: `CORELINK_INTERNAL_AUTH_KEY=my-20-char-internal-k` (20 chars) passes the `< 16` check and the route mounts.

---

### [30] INFO — Stale Comment Claims rsplitn But Code Uses splitn — Misleading Security Rationale in Signup Token Parser

**surface:** Pilot signup route — POST /v1/signup/pilot/:token (token parsing)

**location:** crates/corelink-container/src/routes/signup.rs:261-263

**explanation:** The parse_and_verify_pilot_token function has a comment on line 261 that reads 'We use rsplitn so the <env> field cannot smuggle an underscore.' However, the actual code on line 263 uses splitn(4, '_'), not rsplitn. These two functions split from opposite ends of the string. The comment's security rationale is that rsplitn (right-to-left) would anchor on the rightmost delimiters, preventing a crafted <env> value containing underscores from consuming the timestamp and random-id fields. The code uses left-to-right splitn, which would handle a malicious env segment differently: with splitn(4, '_') and input 'pilot_evil_extra_1234567890000_abcdef1234567890', the fields would be ['pilot', 'evil', 'extra', '1234567890000_abcdef1234567890'] — 'extra' would fail the u64 timestamp parse, so the validation would correctly reject it. The actual security is preserved by the strict env allowlist validation (only 'staging' or 'prod' accepted) and the u64 timestamp validation, not by the choice of splitn direction. The comment is inaccurate but the implementation is safe. The concern is that a future developer might 'fix' the code to use rsplitn based on the comment, which would actually BREAK the parsing (rsplitn would consume the pilot prefix differently).

**attack_scenario:** Not directly exploitable as a standalone issue. If a future developer reads the comment and 'fixes' the code to use rsplitn(4, '_') matching the comment, they would parse the token body right-to-left, making the 4 segments [first_part, random16, timestamp, env_pilot_combined]. The 'pilot' prefix check would then fail (it would match the env_pilot_combined, not 'pilot' alone), breaking all legitimate token validation. This is a code-quality risk that could introduce a denial-of-service (all tokens rejected) in a future change.

**reproduction:** Inspect crates/corelink-container/src/routes/signup.rs:261-263. The comment says rsplitn but the code calls splitn. No external trigger required — this is a static code issue.

**impact:** No immediate security impact. The parsing is correct. The risk is a future regression if a developer 'corrects' the code to match the comment. If that change lands, ALL pilot signup tokens would be rejected, creating a functional outage for the pilot programme.

**evidence:** ```rust
// crates/corelink-container/src/routes/signup.rs:260-263
// 2) split body into the 4 canonical underscore-separated fields:
//    `pilot_<env>_<unix_ms>_<16-hex>`. We use rsplitn so the   <-- comment says rsplitn
//    `<env>` field cannot smuggle an underscore.
let parts: Vec<&str> = body.splitn(4, '_').collect();  // <-- code uses splitn
```

**recommendation:** Fix the comment to accurately describe the implementation. Since splitn is correct and the security is enforced by the env allowlist and u64 timestamp validation, update the comment:
```rust
// 2) split body into the 4 canonical underscore-separated fields:
//    `pilot_<env>_<unix_ms>_<16-hex>`. We use splitn(4) to limit
//    to exactly 4 segments; env is validated against the
//    {"staging", "prod"} allowlist and timestamp is validated as u64,
//    so crafted underscores in either field are rejected at validation.
let parts: Vec<&str> = body.splitn(4, '_').collect();
```

**confidence:** high

**verify:** The finding is confirmed. The code at /Users/gustavoschneiter/Documents/HuGR/corelink-server/crates/corelink-container/src/routes/signup.rs:260-263 does exactly what was cited: the comment on line 261 says "We use rsplitn so the `<env>` field cannot smuggle an underscore," but line 263 uses `body.splitn(4, '_').collect()`. The mismatch is real.

The comment's security rationale is also doubly wrong: (1) the code uses `splitn`, not `rsplitn`, and (2) even if `rsplitn` were used, it would parse segments in reverse order (`["rand16", "timestamp", "env", "pilot"]`) which would cause the `parts.first() != Some("pilot")` check to fire and reject all valid tokens — so `rsplitn` would be the broken choice, not the secure one. The real defense against a crafted underscore in `env` comes from `TokenEnv::from_str` at line 175-181, which is a strict allowlist accepting only `"staging"` or `"prod"`.

Current exploitability: none. The code as written is correct; `splitn(4)` properly produces `["pilot", "env", "timestamp", "rand16"]` for well-formed tokens. The only risk is a future developer reading the comment and "fixing" the code to use `rsplitn` to match the comment, which would invert segment order and cause all valid tokens to fail the `parts.first() != Some("pilot")` check — a denial-of-service on the pilot signup path.

Severity is appropriately `info`. No downgrade needed; the original reporter did not overstate it. The fix is to correct the comment to accurately describe `splitn(4)` and credit the allowlist (not a split direction) as the actual security mechanism.

---

### [31] INFO — Theoretical Timing Side-Channel: dummy_verify_for_constant_time Returns Early on Empty Dummy PHC Without Performing Argon2id Work

**surface:** PAT cold-path timing padding — corelink-pat crate, dummy_verify_for_constant_time

**location:** crates/corelink-pat/src/argon.rs:224-230

**explanation:** The dummy_verify_for_constant_time function is designed to ensure that a PAT lookup miss (token_id not found in DB) takes the same wall-clock time as a genuine Argon2id verify, preventing a token-enumeration timing oracle. The function calls verify_argon2id with a fixed dummy plaintext and dummy PHC hash. However, on line 224 there is a guard: if stored.as_str().is_empty() — meaning if the dummy PHC failed to initialize at startup — the function returns early WITHOUT performing any Argon2id work. This early return would make the cold path (token not found) measurably faster than the hot path (token found + Argon2id done), creating the exact timing oracle the function is designed to prevent. In practice, the dummy PHC is computed once at startup from a fixed salt ('Y29yZWxpbmtfZHVtbXkwMQ' is valid base64url) using the same OsRng as the rest of the system. The scenario where stored.as_str().is_empty() occurs requires both the primary path (OsRng success + salt decode) AND the fallback path (OsRng fallback) to fail at OnceLock initialization. If OsRng is so broken that both fail, the entire system is inoperative. Nevertheless, the code path exists and the early-return is not timing-padded.

**attack_scenario:** Exploitable only if: (1) OsRng fails during container startup in a way that returns an error without panicking; (2) The fallback SaltString::generate also fails. In that case: A network attacker who can send many PAT lookup requests can measure response time. Requests with a token_id that maps to a DB row would execute Argon2id (~250ms). Requests where dummy_verify gets an empty PHC skip Argon2id and return near-immediately. The attacker can use timing to enumerate which token_ids exist in the DB (hot vs cold path distinguishable). This would require the entropy source to be in a degraded state where getrandom succeeds for some operations but fails for others — an unusual scenario.

**reproduction:** 1. Artificially inject a fault where OsRng fails during dummy_phc() initialization (e.g., by patching dummy_phc to return PatHash::from_phc_string(String::new()) directly). 2. Issue PAT verification requests for both known and unknown token_ids. 3. Measure response latency: known token_ids invoke Argon2id (~250ms), unknown token_ids hit the early-return (<1ms). 4. Timing difference of ~250ms is easily distinguishable, leaking DB existence of token_ids.

**impact:** If triggered (degraded-entropy scenario only): token_id enumeration oracle — an attacker learns which token_ids exist in the DB. Because token_ids are non-secret (they are indexed lookup keys embedded in the plaintext), this leaks no cryptographic secret, but it could help an attacker confirm which tenants have active PATs and correlate token_ids to tenants. NOT a PAT forgery path — it does not bypass HMAC or Argon2id.

**evidence:** ```rust
// crates/corelink-pat/src/argon.rs:217-231
pub fn dummy_verify_for_constant_time(plaintext: &str) -> Result<(), PatError> {
    let dummy_pt = "dummy_constant_time_pad_v1";
    let _eq = plaintext.as_bytes().ct_eq(dummy_pt.as_bytes());
    let stored = dummy_phc();
    if stored.as_str().is_empty() {  // early return -- no Argon2id work done!
        return Err(PatError::InvalidPat);
    }
    let _ = verify_argon2id(dummy_pt, stored);
    Err(PatError::InvalidPat)
}
```

**recommendation:** Replace the early-return with a fallback Argon2id invocation using a hardcoded dummy PHC string, so the function always performs Argon2id-equivalent work even when the lazy-initialized PHC is unavailable:
```rust
pub fn dummy_verify_for_constant_time(plaintext: &str) -> Result<(), PatError> {
    let dummy_pt = "dummy_constant_time_pad_v1";
    let _eq = plaintext.as_bytes().ct_eq(dummy_pt.as_bytes());
    let stored = dummy_phc();
    let phc_to_use = if stored.as_str().is_empty() {
        // Fallback: use a hardcoded known-valid PHC string so Argon2id
        // work is always performed. The fallback hash is public; its sole
        // purpose is timing-padding, not authentication.
        &PatHash::from_phc_string(
            "$argon2id$v=19$m=65536,t=3,p=4$...<hardcoded>...".to_string())
    } else {
        stored
    };
    let _ = verify_argon2id(dummy_pt, phc_to_use);
    Err(PatError::InvalidPat)
}
```
Alternatively, use std::hint::spin_loop or tokio::time::sleep with the ARGON2_EXPECTED_DURATION constant to synthesize the delay without a real Argon2id call, eliminating the dependency on dummy PHC availability entirely.

**confidence:** medium

**verify:** The code path described in the finding is real and correctly cited. In `dummy_phc()` (argon.rs lines 178–197), both `unwrap_or_else` branches (lines 191 and 195) fall back to `PatHash::from_phc_string(String::new())`, storing an empty PHC string in the `OnceLock`. In `dummy_verify_for_constant_time` (lines 224–229), `stored.as_str().is_empty()` triggers an early return that skips `verify_argon2id`, eliminating the ~250ms Argon2id CPU work that is supposed to make the cold path timing-indistinguishable from the warm path.

The finding is real as a code-level defect: the structural gap exists. However, the severity must be significantly downgraded from "low" to "info/theoretical" for the following reasons:

1. **Trigger prerequisite is essentially impossible in production.** The empty-PHC fallback only fires if `hash_random_secret_with_salt("dummy_constant_time_pad_v1", salt)` returns `Err` (argon.rs line 195). This requires the Argon2id hasher to fail. The Argon2 params are validated as statically correct, the salt (`Salt::from_b64("Y29yZWxpbmtfZHVtbXkwMQ")`) decodes to "corelink_dummy01" (exactly 16 bytes, valid), and the hasher `OnceLock` uses `unreachable!` rather than a soft fallback. On a Cloudflare Container (Linux, kernel getrandom always available), this failure path cannot be triggered by a network attacker — it would require a fundamental runtime environment failure.

2. **OnceLock persistence is a double-edged argument.** If OsRng did somehow fail at first call, the empty PHC would be stored permanently for the process lifetime, making the timing leak persistent — this is a latent amplification risk. However, this same characteristic means the failure mode would be immediately visible (all PAT lookups would be anomalously fast), making it detectable via monitoring rather than a silent oracle.

3. **The prior reporting agent overstates the CVSS.** A timing side-channel that only reveals token_id existence (not the token secret), requires a degraded entropy source that doesn't exist on the deployment platform, and still requires hundreds of precise network measurements to exploit is at most informational in severity — not "low" in the traditional vulnerability sense.

The finding deserves to be tracked as a hardening opportunity (replace the empty-PHC fallback with a hardcoded known-valid PHC string as the recommendation suggests), but it does not constitute an exploitable vulnerability on the actual deployment platform. The code comment on line 227 ("impossible in practice") is accurate. Severity adjusted from "low" to "info".

---

### [32] INFO — OCI Config Doc Says '32 bytes (raw, not hex)' But Loading Code Passes Raw String Without Hex-Decoding — Semantic Ambiguity for Operators

**surface:** OCI registry bearer token signing key — CORELINK_OCI_TOKEN_KEY

**location:** crates/corelink-adapter-host/src/oci/config.rs:73 and crates/corelink-container/src/routes/oci.rs:500

**explanation:** The OCI adapter configuration doc at config.rs:73 states that token_signing_key 'MUST be ≥32 bytes (raw, not hex)'. The sanity_check at config.rs:184-188 enforces key_bytes.len() >= 32, where key_bytes is the raw string representation of the secret (not hex-decoded). This means if an operator follows the secrets matrix recommendation of 'openssl rand -hex 32' (which produces 64 hex characters = 32 random raw bytes encoded as 64 ASCII chars), they provide a 64-character string, the code sees 64 bytes of key material, and the check passes (64 >= 32). However, a strict reading of 'raw, not hex' in the doc would imply the 32-byte requirement refers to 32 actual random bytes, not 32 ASCII characters. An operator who provides exactly 32 characters of hex output ('openssl rand -hex 16' = 32 hex chars = 16 raw bytes) would pass the string-length check (len=32 >= 32) but actually have only 16 raw bytes of entropy — half the stated security requirement. The doc says 'not hex' which is ambiguous: it might mean 'the minimum is raw bytes, not hex-encoded' (i.e., the check is against decoded bytes), but the code checks the string length. Additionally, the comment on line 73 says 'Source env: HUGR_OCI_TOKEN_KEY' but the actual env var name is CORELINK_OCI_TOKEN_KEY (per routes/oci.rs:97), creating another documentation discrepancy.

**attack_scenario:** 1. Operator reads doc: 'MUST be ≥32 bytes (raw, not hex)'. 2. Operator runs 'openssl rand -hex 16' to get 32 hex chars = exactly 32 bytes by string length. 3. The sanity_check accepts this (32 >= 32). 4. In practice the key has only 16 bytes (128 bits) of actual random entropy, not 256 bits. 5. An attacker who can brute-force the OCI HMAC key has a smaller search space. 6. OCI bearer tokens (1-hour TTL per default) can be forged, allowing cross-tenant OCI registry access under the tenant UUID embedded in the token.

**reproduction:** 1. Set CORELINK_OCI_TOKEN_KEY to a 32-character hex string (16 random bytes, hex-encoded, e.g., 'openssl rand -hex 16' output). 2. Boot the container with OCI config. 3. The sanity_check passes (string length 32 >= MIN_TOKEN_KEY_BYTES=32). 4. The HMAC key has only 128 bits of entropy, not 256.

**impact:** Reduced cryptographic strength for OCI bearer token signing. With a 128-bit key (instead of 256-bit), the security margin is halved. Forging an OCI bearer token would still require breaking HMAC-SHA256 with a 128-bit key, which is not computationally feasible today. The practical risk is low, but the stated security guarantee is not delivered. Additionally, the wrong env var name in the doc (HUGR_OCI_TOKEN_KEY vs CORELINK_OCI_TOKEN_KEY) could cause an operator to set the wrong variable and have the OCI registry fail to mount.

**evidence:** ```rust
// crates/corelink-adapter-host/src/oci/config.rs:73 (doc says raw not hex)
/// MUST be ≥32 bytes (raw, not hex) per
/// [`Self::sanity_check`]. Source env: `HUGR_OCI_TOKEN_KEY`.  // <-- wrong env var name

// crates/corelink-adapter-host/src/oci/config.rs:184-185 (checks string bytes, not raw decoded)
let key_bytes = self.token_signing_key.as_secret_string().expose_secret();
if key_bytes.len() < defaults::MIN_TOKEN_KEY_BYTES {  // checks string length

// crates/corelink-container/src/routes/oci.rs:97 (actual env var name)
pub const OCI_TOKEN_KEY_ENV: &str = "CORELINK_OCI_TOKEN_KEY";  // different from doc

// crates/corelink-container/src/routes.rs:487-500 (loading as raw string)
crate::storage::non_empty_env(oci::OCI_TOKEN_KEY_ENV),
// ...
corelink_core::SecretWrap::new(token_key)  // no hex decode
```

**recommendation:** 1. Fix the doc comment on config.rs:73: change 'Source env: HUGR_OCI_TOKEN_KEY' to 'Source env: CORELINK_OCI_TOKEN_KEY'. 2. Clarify the doc to say 'MUST be ≥32 characters of key material (the raw string bytes are used as the HMAC key; if you generate with openssl rand -hex 32, you get 64 characters which is correct and provides 256-bit key material)'. 3. Optionally, add hex-decoding at load time so that 'openssl rand -hex 32' (64 hex chars) becomes a 32-byte decoded key and the minimum check truly verifies 32 raw bytes: the sanity_check would need to attempt hex-decode and measure decoded length. This is the safest approach for a future-proof API.

**confidence:** medium

**verify:** The finding is confirmed as real, but only as a documentation inconsistency — not a runtime security vulnerability. Two concrete doc issues are verified:

1. Stale env var name: `config.rs:73` says `Source env: HUGR_OCI_TOKEN_KEY`, but the actual env constant at `crates/corelink-container/src/routes/oci.rs:97` is `OCI_TOKEN_KEY_ENV = "CORELINK_OCI_TOKEN_KEY"`. No code reads `HUGR_OCI_TOKEN_KEY` at runtime (grep of all Rust crates confirms this). The 2026-06-02 audit doc (`specs/_audits/2026-06-02-secrets-naming-reconciliation.md:53`) itself is wrong — it claims "code reads the HUGR_ name" but does not; the container reads CORELINK_OCI_TOKEN_KEY exclusively.

2. Semantic ambiguity in entropy requirement: `config.rs:72-73` says "MUST be ≥32 bytes (raw, not hex)" but `auth.rs:165-167` passes `key_str.as_bytes()` directly to HMAC (no hex-decode). The sanity check at `config.rs:184-185` measures string length. If an operator reads "raw, not hex" and interprets it as "use 32 raw bytes generated by openssl rand -hex 16 (which gives exactly 32 hex chars = 32 string bytes)", they'd have only 128 bits of effective HMAC entropy. The secrets-checklist entry #148 at `docs/internal/secrets-checklist.md:212` contradicts itself: it says `openssl rand -hex 32 (≥32-byte raw key)` — the command is correct (produces 64 hex chars, 256 bits entropy as string bytes), but calling the output a "raw key" is misleading given the code comment says "raw, not hex."

Why severity stays info: The practical exploit path is weak. The secrets-checklist correctly instructs `openssl rand -hex 32` (64 chars, well over the 32-char minimum, 256 bits of hex-encoded entropy fed directly to HMAC-SHA256). An operator would have to actively contradict the checklist and misread the code comment to provision a weak key. No code-path vulnerability exists — the HMAC works correctly for any ≥32-byte key string; the issue is purely in how the documentation can mislead about entropy quantity. No cross-tenant token forgery is currently achievable unless an operator made the specific misconfiguration of using a 16-byte raw key as a 32-char hex string.

---

### [33] MEDIUM — Migration 0064: Tenant Triggers Silently Destroyed — primary_region Enforcement Lost After Table Rebuild

**surface:** D1 migration / schema integrity

**location:** migrations/d1/0064_tenant_tier_max.sql:78-86, 210-212

**explanation:** Migration 0064 performs the SQLite '12-step' table rebuild on the `tenant` table (DROP TABLE + RENAME) to widen the `tier` CHECK constraint. The migration's comment block at lines 78–86 explicitly claims that the three `trg_tenant_primary_region_*` triggers from migration 0028 'survive the DROP + RENAME swap' because they are 'name-bound to `tenant`, not the physical object', and explicitly decides NOT to recreate them. This claim is factually incorrect. The official SQLite documentation (https://www.sqlite.org/lang_droptable.html) states: 'Every trigger associated with a table is removed when the table is dropped.' When `DROP TABLE tenant` executes (line 210), all three triggers — `trg_tenant_primary_region_required` (BEFORE INSERT NULL guard), `trg_tenant_primary_region_immutable` (BEFORE UPDATE immutability guard), and `trg_tenant_primary_region_valid_insert` (BEFORE INSERT region validation) — are permanently destroyed. The subsequent `ALTER TABLE tenant_new RENAME TO tenant` (line 212) creates a new physical table named `tenant` with no triggers at all. A local SQLite test confirms: renaming a table moves triggers with the table (they follow the physical object), so an application RENAME of the original table without DROP would preserve triggers — but in the 12-step pattern where the ORIGINAL is DROPped and the NEW copy is renamed into its place, the triggers end up bound to the dropped (gone) table, not the new one. Post-0064, any INSERT into `tenant` with a NULL or invalid `primary_region` is silently accepted by D1, violating INV-REGION-NO-CROSS-LEAK (WI-S14-002). The `primary_region` column's NOT NULL enforcement via trigger is gone. The comparison migration 0062 (which rebuilt `tier_selections` and `stripe_checkout_sessions`) did NOT have this problem because neither of those tables ever had triggers defined on them — only `tenant` has the 0028 triggers.

**attack_scenario:** After migration 0064 is applied to production: (1) A bug in the signup flow or an internal admin tool attempts to INSERT a tenant row without a `primary_region` or with an invalid region string. (2) Pre-0064, the `trg_tenant_primary_region_required` trigger would RAISE(ABORT) and prevent the bad row. Post-0064, the trigger is gone — the INSERT succeeds, writing a NULL or invalid region into D1. (3) The tenant is now in an unconstrained state: the region-enforcer DO reads `primary_region` and the tenant could be routed to the wrong region, or the region column could remain NULL, causing runtime panics or cross-region data leak (Schrems II / LGPD Art. 33 violation). Additionally, the `trg_tenant_primary_region_immutable` trigger that prevents mutation of `primary_region` after creation is also gone, meaning an UPDATE could silently move a tenant's canonical region, bypassing the dual-approval requirement documented in migration 0028.

**reproduction:** 1. Verify the current production D1 migration level is 0064 or above (`wrangler d1 migrations list`). 2. Query `SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='tenant'` via the D1 REST API — expected result should be 3 rows (trg_tenant_primary_region_required, trg_tenant_primary_region_immutable, trg_tenant_primary_region_valid_insert); post-0064 this returns 0 rows. 3. Attempt `INSERT INTO tenant (tenant_id, primary_region, ...) VALUES ('test-id', NULL, ...)` — pre-0064 this raises ABORT; post-0064 it succeeds (constraint enforcement silently gone). Alternatively: run `sqlite3 :memory:` locally, create the tenant table + triggers, then execute the exact 0064 sequence (DROP TABLE tenant; RENAME tenant_new TO tenant), then SELECT from sqlite_master — triggers will be absent.

**impact:** All three enforcement triggers on `tenant.primary_region` are permanently destroyed after 0064 applies. (1) NULL or invalid region strings can be written to `tenant.primary_region` on any subsequent INSERT — the NOT NULL check in the original CREATE TABLE (0023) remains but the trigger adding the region-validity check is gone. (2) An UPDATE can change `primary_region` after creation without the immutability trigger blocking it, bypassing the documented dual-approval / manual-ticket process. (3) If any code path produces a NULL-region tenant post-0064, the region-enforcer DO will encounter unexpected data, potentially routing requests to the wrong region — a data residency violation under GDPR/LGPD. Severity is HIGH because the enforcement loss is silent (no runtime error, no test failure), the triggers were the documented defense-in-depth for a critical privacy invariant (INV-REGION-NO-CROSS-LEAK), and it affects ALL subsequent tenant INSERTs after the migration.

**evidence:** -- From migrations/d1/0064_tenant_tier_max.sql:78-86:
-- Trigger inventory (0028 — survive the rebuild as they reference the table
--   name, not the physical storage; D1 re-validates after RENAME TO):
--   trg_tenant_primary_region_required   (BEFORE INSERT)
--   trg_tenant_primary_region_immutable  (BEFORE UPDATE OF primary_region)
--   trg_tenant_primary_region_valid_insert (BEFORE INSERT)
--   → These are name-bound to `tenant`, not the physical object; they
--     survive the DROP + RENAME swap on the same connection. We do NOT
--     drop + recreate them — that would be a destructive trigger operation
--     and is unnecessary: D1 trigger bindings follow the table name.

-- SQLite official docs (lang_droptable.html):
-- 'Every trigger associated with a table is removed when the table is dropped.'

-- Confirmed via local SQLite test:
-- conn.execute('ALTER TABLE tenant RENAME TO tenant_old')  # move triggers to tenant_old
-- conn.execute('ALTER TABLE tenant_new RENAME TO tenant')  # new table has no triggers
-- Result: [('trg_test', 'tenant_old')]  -- triggers followed the ORIGINAL table, not the target name

**recommendation:** Add explicit trigger recreation statements to migration 0064 (or a new additive migration 0069) immediately after `ALTER TABLE tenant_new RENAME TO tenant`. The three triggers must be re-created verbatim from migration 0028:

```sql
-- Re-create the 0028 triggers (lost when DROP TABLE tenant destroyed them).
CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_required
BEFORE INSERT ON tenant
FOR EACH ROW
WHEN NEW.primary_region IS NULL
BEGIN
    SELECT RAISE(ABORT, 'primary_region is required for new tenants (WI-S14-002: INV-REGION-NO-CROSS-LEAK)');
END;

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_immutable
BEFORE UPDATE OF primary_region ON tenant
FOR EACH ROW
WHEN OLD.primary_region IS NOT NULL AND OLD.primary_region != NEW.primary_region
BEGIN
    SELECT RAISE(ABORT, 'primary_region is immutable post-INSERT (WI-S14-002: manual ticket + admin role + dual-approval required)');
END;

CREATE TRIGGER IF NOT EXISTS trg_tenant_primary_region_valid_insert
BEFORE INSERT ON tenant
FOR EACH ROW
WHEN NEW.primary_region IS NOT NULL
  AND NEW.primary_region NOT IN ('wnam','enam','weur','sam','apac','afr')
BEGIN
    SELECT RAISE(ABORT, 'primary_region must be one of: wnam, enam, weur, sam, apac, afr');
END;
```

Also update the migration comment to correctly state: 'DROP TABLE destroys all associated triggers (per SQLite lang_droptable.html); they are explicitly re-created below.' Additionally, update the gate script `scripts/check_migrations_additive.py` to include a check that any migration performing a DROP+RENAME table rebuild also includes CREATE TRIGGER statements for all triggers that existed on the original table.

**confidence:** high

**verify:** CORE TECHNICAL CLAIM CONFIRMED. Migration 0064 rebuilds the `tenant` table via DROP TABLE + RENAME TO (migrations/d1/0064_tenant_tier_max.sql:210-212), and its comment (lines 78-86) plus ADR-0064 §4 (specs/03_architecture/adrs/ADR-0064-tenant-tier-max-check-rebuild.md:121-128) both assert the three 0028 triggers "are name-bound to the table name... survive the DROP + RENAME swap... do NOT need to be dropped and recreated." This is FALSE. I empirically reproduced the exact rebuild pattern on sqlite3 3.43.2: after `DROP TABLE tenant; ALTER TABLE tenant_new RENAME TO tenant`, `SELECT count(*) FROM sqlite_master WHERE type='trigger'` returns 0, and an `INSERT ... primary_region=NULL` SUCCEEDS where the trigger would have aborted it. SQLite triggers bind to the physical table object, not the name; DROP TABLE destroys them (lang_droptable.html), and the migration recreates all 9 indexes (lines 214-256) but NO triggers. No later migration (0065-0068) recreates them, so the loss is permanent once 0064 applies. The additive gate scripts/check_migrations_additive.py only flags DROP/RENAME tokens with an allow-annotation; it has zero awareness of trigger preservation, so it would not catch this.

SEVERITY DOWNGRADED high -> medium, on two honest grounds:
(1) Two of the three lost triggers are REDUNDANT with column constraints that the rebuild PRESERVES. The rebuilt `tenant_new` keeps `primary_region TEXT NOT NULL CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr'))` inline (lines 128-129). So `trg_tenant_primary_region_required` (NULL guard) and `trg_tenant_primary_region_valid_insert` (valid-set guard) are still fully enforced at the DB column level post-0064. The finding's claims of "NULL or invalid region written into D1" and "runtime panics" are therefore REFUTED — those inserts are still blocked.
(2) Only `trg_tenant_primary_region_immutable` is genuinely lost with no equivalent surviving guard (no column constraint enforces UPDATE-immutability). This is real and matters: the privacy model treats primary_region as monotonic (crates/corelink-privacy/src/residency/migration.rs:6-10) with changes only via a dual-approval + 30-day-cooldown ticket path, and the dedicated adversarial test (crates/corelink-privacy/tests/residency_region_adversarial.rs:122-136) explicitly relies on "the D1 trigger rejects the UPDATE" — the app-layer TenantCtx immutability only protects per-request in-memory objects, not durable D1 writes. HOWEVER, exploitability is constrained: there is NO code path that issues `UPDATE tenant SET primary_region` (grep over crates/ + worker/ finds only test/comment references), so this is the loss of a defense-in-depth DB backstop reachable only by a future code bug or a privileged operator with direct D1 write access (CLOUDFLARE_API_TOKEN) — not a network-reachable, unauthenticated-tenant or cross-tenant exploit. That places it below the CRITICAL/HIGH bar reserved for real tenant-isolation/auth-bypass breaks. Mitigating further: 0064 is owner-gated and per repo memory prod D1 tail was ~0062/0063, so this may still be a pre-application catch.

NET: the bug is real, the migration+ADR comments are factually wrong, and the fix (re-create the three 0028 triggers verbatim after the RENAME, and teach the additive gate to require CREATE TRIGGER on any table rebuild) is correct and should land before 0064 is applied to prod. Severity = medium (lost residency-immutability DB backstop, compliance defense-in-depth, not a directly attacker-reachable isolation break).

---

### [34] LOW — invoice.payment_failed Missing Fallback Revocation Path — Access Gate Not Revoked When Stripe Invoice Lacks `customer` Field

**surface:** Stripe webhook handler / billing integrity

**location:** apps/signup-worker/src/webhooks/stripe.ts:1226-1233

**explanation:** The `invoice.payment_failed` webhook handler in the signup-worker revokes the canonical access gate (`tier_selections.subscription_state` → 'inactive') ONLY when the invoice payload contains a `customer` field. If the customer field is absent, the tier-selections row stays `active`, meaning a tenant whose payment has definitively failed (dunning exhausted, `next_payment_attempt === null`) would retain full paid-tier access indefinitely. Compare the analogous handlers: both `customer.subscription.updated` (lines 1116-1123) and `customer.subscription.deleted` (lines 1269-1274) implement a fallback path — when `stripeCustomerId` is absent, they call `deactivateTierSelectionBySubscription(db, { stripeSubscriptionId })`, which resolves the tenant through `tenant_billing` (which maps subscription id → tenant id) via a correlated subquery. The `invoice.payment_failed` handler (lines 1226-1233) has no equivalent fallback: it checks `if (typeof stripeCustomerId === 'string' && stripeCustomerId)` and if that is false, SKIPS the `tier_selections` deactivation entirely while still writing `tenant_billing.status = 'past_due'` via `updateBillingStatus`. This creates a state inconsistency: `tenant_billing.status = 'past_due'` (secondary mirror correctly updated) but `tier_selections.subscription_state = 'active'` (canonical gate incorrectly still active). The quota enforcement in `worker/src/lib/quota.ts::getTierForTenant` reads `tier_selections WHERE subscription_state = 'active'` — so the tenant continues to receive paid-tier quota. In practice, Stripe invoices nearly always include the `customer` field (it is part of the standard invoice object). However, certain internal Stripe invoices (e.g. dispute adjustment invoices, some test-mode scenarios) may omit it, and the `subscription_details.subscription` fallback for extracting `stripeSubscriptionId` itself demonstrates that Stripe's webhook payload structure is not guaranteed to be uniform.

**attack_scenario:** 1. Tenant T has a paid subscription that enters dunning (multiple failed payment retries). 2. Stripe exhausts dunning and fires `invoice.payment_failed` with `next_payment_attempt: null` and `subscription_id: sub_xxx` but WITHOUT a top-level `customer` field (unusual but structurally possible). 3. The handler correctly sets `tenant_billing.status = 'past_due'` (via `updateBillingStatus` keyed on `stripe_subscription_id`) but skips the tier_selections revocation because the `typeof stripeCustomerId === 'string'` guard fails. 4. `tier_selections.subscription_state` remains `'active'`. 5. `getTierForTenant` in the quota worker returns the paid tier. 6. Tenant T continues to receive paid-tier storage quota, API rate limits, and features despite a terminal payment failure — a billing integrity violation where the customer is served without payment.

**reproduction:** 1. Craft a synthetic Stripe `invoice.payment_failed` webhook payload with: `next_payment_attempt: null` (terminal), `subscription: 'sub_test123'` (present), but no top-level `customer` field (omitted). 2. Sign it with the test `STRIPE_WEBHOOK_SECRET`. 3. POST to the signup-worker webhook endpoint. 4. Verify: `tenant_billing.status` is updated to `'past_due'` (correct), but `tier_selections.subscription_state` remains `'active'` (incorrect — the bug). 5. Query `getTierForTenant` for the affected tenant — it returns the paid tier. Alternatively: review the `apps/signup-worker/tests/stripe.test.ts` test suite for the `invoice.payment_failed` arm and confirm there is no test covering the `customer`-field-absent path with a `stripeSubscriptionId` present.

**impact:** A tenant with a terminal payment failure (dunning exhausted) whose Stripe `invoice.payment_failed` payload omits the `customer` field would retain indefinite paid-tier access. The blast radius is limited by the rarity of `customer`-less invoice payloads in production, but the risk is non-zero (and the inconsistency with `subscription.updated`/`subscription.deleted` handlers indicates it was not intentional). Financially: the tenant receives paid-tier storage (up to 50 GB–2 TB depending on tier) and API rate limits without payment. The access would persist until the next `customer.subscription.updated` or `customer.subscription.deleted` event fires (which Stripe eventually does fire) — those handlers have the subscription-id fallback and would correctly deactivate.

**evidence:** // apps/signup-worker/src/webhooks/stripe.ts:1226-1233
// 2. CANONICAL gate: flip tier_selections away from 'active'.
if (typeof stripeCustomerId === 'string' && stripeCustomerId) {
    requiredWrites.push(
        deactivateTierSelectionByCustomer(db, {
            stripeCustomerId,
        }),
    );
}
// NO ELSE BRANCH — if stripeCustomerId absent, tier_selections is NOT deactivated.

// Compare: customer.subscription.updated (lines 1116-1123) HAS the fallback:
if (!grantsAccess) {
    requiredWrites.push(
        typeof stripeCustomerId === 'string' && stripeCustomerId
            ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
            : deactivateTierSelectionBySubscription(db, { stripeSubscriptionId }),  // <-- FALLBACK
    );
}

// And customer.subscription.deleted (lines 1269-1274) also has the fallback:
requiredWrites.push(
    typeof stripeCustomerId === 'string' && stripeCustomerId
        ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
        : deactivateTierSelectionBySubscription(db, { stripeSubscriptionId }),  // <-- FALLBACK
);

**recommendation:** Add the missing subscription-id fallback to the `invoice.payment_failed` handler, making its revocation pattern consistent with the `subscription.updated` and `subscription.deleted` arms:

```typescript
// 2. CANONICAL gate: flip tier_selections away from 'active'.
// FAIL-SAFE (mirrors subscription.updated/deleted fix): a terminal payment
// failure MUST revoke entitlement even if the invoice payload omits `customer`.
// Fall back to subscription-id resolution (via tenant_billing correlated subquery)
// when customer is absent — we always have stripeSubscriptionId here (guarded above).
requiredWrites.push(
    typeof stripeCustomerId === 'string' && stripeCustomerId
        ? deactivateTierSelectionByCustomer(db, { stripeCustomerId })
        : deactivateTierSelectionBySubscription(db, { stripeSubscriptionId }),
);
```

This exactly mirrors the pattern used at lines 1116-1123 and 1269-1274. Also add a unit test in `apps/signup-worker/tests/stripe.test.ts` covering `invoice.payment_failed` with `next_payment_attempt: null`, `subscription: 'sub_xxx'`, and no `customer` field, asserting that `deactivateTierSelectionBySubscription` is called.

**confidence:** high

**verify:** The code gap is real and independently verified at `/apps/signup-worker/src/webhooks/stripe.ts` lines 1226-1233. In the `invoice.payment_failed` handler, when `isTerminalFailure` is true (dunning exhausted), the tier_selections deactivation at line 1227 is conditional on `typeof stripeCustomerId === 'string' && stripeCustomerId`. If `stripeCustomerId` (sourced from `obj["customer"]` at line 1202) is absent or falsy, `deactivateTierSelectionBySubscription` is NOT called as a fallback — meaning `tier_selections.subscription_state` would remain `'active'` and the tenant would retain paid-tier access.

This is structurally inconsistent with both the `customer.subscription.updated` handler (lines 1116-1123) and the `customer.subscription.deleted` handler (lines 1269-1274), which both use the correct ternary fallback: `typeof stripeCustomerId === 'string' && stripeCustomerId ? deactivateTierSelectionByCustomer(...) : deactivateTierSelectionBySubscription(...)`.

The existing test at line 1101 only covers the happy path where `customer: 'cus_pf'` is present; there is no test for the missing-customer scenario.

Severity is downgraded from medium to low because:
1. Stripe's Invoice object specification guarantees the `customer` field is present on all subscription invoices. An attacker cannot externally control the Stripe payload content — the webhook is HMAC-signed by Stripe and verified by the handler. The `customer`-absent scenario requires Stripe itself to emit a non-standard payload.
2. Even in the `customer`-absent case, `customer.subscription.updated` (which has the proper fallback) would typically fire in the same dunning lifecycle, providing a secondary revocation path.
3. The gap is a defensive inconsistency and maintainability risk, not a directly exploitable attack vector from outside.

The fix is nonetheless correct and low-risk: replace the bare `if` guard at lines 1227-1233 with the same ternary fallback pattern used at lines 1116-1123 and 1269-1274. Adding a unit test for the no-customer case is also appropriate.

---

### [35] LOW — updateTierSelectionTierByCustomer Updates Tier on Inactive/Canceled Subscriptions — Stale Tier Data in Access Gate Table

**surface:** Stripe webhook handler / billing integrity

**location:** apps/signup-worker/src/webhooks/stripe.ts:711-723, 1090-1101

**explanation:** The `updateTierSelectionTierByCustomer` function updates `tier_selections.tier` for a Stripe customer without filtering by `subscription_state`. It is called on every `customer.subscription.updated` event when the subscription price resolves to a known tier (step (a) at lines 1090-1101). The SQL is a bare UPDATE: `UPDATE tier_selections SET tier = ?1 WHERE stripe_customer_id = ?2` — no `subscription_state` filter. On the same event, if the subscription status no longer grants access (e.g., `status: 'canceled'`), step (b) at lines 1116-1124 calls `deactivateTierSelectionByCustomer` to set `subscription_state = 'inactive'`. These two writes are added to `requiredWrites` sequentially and then executed via `Promise.all(requiredWrites)` at line 1308 — meaning they run as concurrent HTTP requests to D1's REST API and can interleave. The net result is: the `tier_selections` row ends up with `subscription_state = 'inactive'` (correct) but with a freshly-written `tier` value from the canceled subscription (stale — the tenant is now on `free` effectively). This tier value is misleading in forensics/audit queries because an operator scanning `tier_selections WHERE tier = 'pro'` would see inactive/canceled tenants appearing as if they have a Pro subscription tier. The `getTierForTenant` enforcement (quota.ts:107) only reads rows `WHERE subscription_state = 'active'`, so there is no access-control bypass — the security gate is correct. But the data quality issue means inactive rows carry incorrect tier labels, potentially causing confusion in analytics, billing reconciliation queries (`stripe_tier_drift_view`), and audit tooling.

**attack_scenario:** No direct attack vector — this is a data integrity issue, not an access control bypass. However, an operator investigating a billing dispute for a tenant whose subscription was canceled mid-plan-change could see `tier_selections.tier = 'max'` on an inactive row, falsely suggesting the customer was on Max tier at cancellation. Analytics queries counting tenants by tier (e.g., `SELECT tier, COUNT(*) FROM tier_selections`) would incorrectly count inactive/canceled tenants in paid-tier buckets if they don't also filter on `subscription_state = 'active'`.

**reproduction:** 1. A tenant has an active 'solo' subscription. 2. Stripe fires `customer.subscription.updated` with: `status: 'canceled'` (or another non-granting status), and a new price that maps to 'max' tier. 3. The handler resolves `newTier = 'max'` and `grantsAccess = false`. 4. Both writes run: (a) `UPDATE tier_selections SET tier = 'max' WHERE stripe_customer_id = ?` and (b) `UPDATE tier_selections SET subscription_state = 'inactive' WHERE stripe_customer_id = ? AND subscription_state <> 'inactive'`. 5. Result: `tier_selections` row has `tier = 'max'`, `subscription_state = 'inactive'`. Query `SELECT tier FROM tier_selections WHERE tenant_id = ?` returns 'max' even though the tenant is inactive.

**impact:** No access control bypass — `getTierForTenant` correctly gates on `subscription_state = 'active'`. Impact is limited to: stale tier data in `tier_selections` for canceled/past_due tenants, potential confusion in analytics and billing reconciliation queries that don't filter on `subscription_state`, and misleading forensic audit state. The `stripe_tier_drift_view` (0048) joins `tier_selections` on `subscription_state` but only for rows with a corresponding `active` Stripe subscription — inactive rows are not included in that view, so the drift detection is not affected.

**evidence:** // apps/signup-worker/src/webhooks/stripe.ts:711-723
async function updateTierSelectionTierByCustomer(
    db: D1DatabaseLike,
    opts: { stripeCustomerId: string; tier: PaidTier },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET tier = ?1
             WHERE stripe_customer_id = ?2`,  // NO subscription_state filter
        )
        .bind(opts.tier, opts.stripeCustomerId)
        .run();
}

// Called unconditionally (when newTier is known) at lines 1095-1101,
// BEFORE the deactivation at 1116-1124, with both running via Promise.all.

**recommendation:** Add a `subscription_state = 'active'` filter to `updateTierSelectionTierByCustomer` so it only updates the tier on rows where the subscription is still active:

```typescript
async function updateTierSelectionTierByCustomer(
    db: D1DatabaseLike,
    opts: { stripeCustomerId: string; tier: PaidTier },
): Promise<void> {
    await db
        .prepare(
            `UPDATE tier_selections
             SET tier = ?1
             WHERE stripe_customer_id = ?2
               AND subscription_state = 'active'`,  // only update active rows
        )
        .bind(opts.tier, opts.stripeCustomerId)
        .run();
}
```

This ensures that a plan-change update on a simultaneously-canceled subscription does not write a stale tier to the inactive row. The deactivation path remains unchanged.

**confidence:** high

**verify:** The finding is confirmed as a real data integrity issue, but the claimed severity (low) is accurate and arguably slightly generous.

**Code path verified at exact cited locations:**

`updateTierSelectionTierByCustomer` (line 711-723) issues `UPDATE tier_selections SET tier = ?1 WHERE stripe_customer_id = ?2` — no `subscription_state` filter.

In the `customer.subscription.updated` handler (lines 1090-1124), when a `status = "canceled"` event arrives with a recognized price (so `newTier` is non-null AND `grantsAccess = false`), BOTH operations are pushed into `requiredWrites`:
- Line 1096-1101: `updateTierSelectionTierByCustomer` (tier update, no state filter)
- Line 1117-1123: `deactivateTierSelectionByCustomer` (sets `subscription_state = 'inactive'`)

Both are flushed together at line 1308 via `await Promise.all(requiredWrites)` as two independent D1 `.run()` calls (not an atomic transaction). The execution order within D1 is not guaranteed by the application layer. In either order, the net result is the same: the inactive row ends up with `tier` set to the new (stale) value because `updateTierSelectionTierByCustomer` has no predicate filtering out already-inactive or about-to-be-inactive rows.

The code comment at lines 706-709 asserts "only touches the `tier` column so it never resurrects a canceled/past_due subscription_state" — which is technically correct (it does not touch `subscription_state`), but misses the converse problem: writing a new tier onto an inactive row creates stale tier data.

**Why severity stays low (no upward adjustment):**

1. No access-control bypass. The access gate in `quota.ts` (as confirmed by the billing-state memory doc) gates on `subscription_state = 'active'`. An inactive row with a stale `tier` value does not grant any entitlement.
2. The `deactivateTierSelectionByCustomer` function (lines 650-664) unconditionally sets `subscription_state = 'inactive'` with an idempotent guard (`AND subscription_state <> 'inactive'`), so the access gate column is always correctly written.
3. The blast radius is analytics/audit queries that count tenants by tier without also filtering on `subscription_state = 'active'`. This is an operational data-quality concern, not a security or money-path concern.
4. The scenario requires a simultaneous plan-change-and-cancellation event, which is an edge case in normal Stripe subscription lifecycle flows.

**Recommended fix (as cited):** Add `AND subscription_state = 'active'` to `updateTierSelectionTierByCustomer`'s WHERE clause so it is a no-op on already-inactive rows, preventing stale tier writes in the concurrent path. The deactivation path is correct and unchanged.

---

### [36] HIGH — Migration 0064 Comment Contains Incorrect SQLite Trigger Behavior Documentation — Maintenance Risk

**surface:** D1 migration documentation / schema integrity

**location:** migrations/d1/0064_tenant_tier_max.sql:78-86

**explanation:** Beyond the operational consequence of the trigger destruction (covered in the HIGH finding above), the migration comment at lines 78–86 embeds a technically incorrect statement about SQLite trigger semantics that will mislead future engineers working on this codebase. It claims: 'D1 trigger bindings follow the table name' — meaning triggers are said to be name-bound to the logical table name `tenant`, not to the physical storage object, and therefore survive DROP+RENAME. This is the opposite of SQLite's actual behavior. The correct mechanism is: triggers ARE bound to the table name string, but DROP TABLE destroys them along with the table. RENAME TO (ALTER TABLE ... RENAME TO) causes SQLite to update trigger references in sqlite_master to point to the new name — this is why a RENAME-only approach would preserve triggers. But the 12-step pattern used here (copy data → DROP original → RENAME copy) drops the original table first, destroying its triggers, before the rename of the copy creates a new trigger-less table. Any future migration engineer reading this comment and following the same pattern for a table that has triggers would silently lose enforcement — a recurring class of bug seeded by this incorrect documentation.

**attack_scenario:** Not a direct attack. The incorrect documentation increases the probability of the same trigger-loss bug being repeated in future migrations that rebuild tables with triggers (e.g., if `tenant` is rebuilt again in a future migration, or any other table that acquires triggers gets rebuilt).

**reproduction:** Run local SQLite test: create a table, add a trigger, RENAME the table (trigger follows), RENAME back (trigger follows again). Then create a NEW table with the original name, DROP the renamed original, and check if the original trigger now fires on the new table — it does not. The trigger is permanently gone.

**impact:** Documentation/maintenance risk only. The actual consequence of the incorrect trigger semantics is captured in the HIGH finding above. This finding records the incorrect documentation as a separate maintenance concern.

**evidence:** -- migrations/d1/0064_tenant_tier_max.sql:78-86:
-- Trigger inventory (0028 — survive the rebuild as they reference the table
--   name, not the physical storage; D1 re-validates after RENAME TO):
--   → These are name-bound to `tenant`, not the physical object; they
--     survive the DROP + RENAME swap on the same connection. We do NOT
--     drop + recreate them — that would be a destructive trigger operation
--     and is unnecessary: D1 trigger bindings follow the table name.

-- CORRECT per SQLite documentation (lang_droptable.html):
-- 'Every trigger associated with a table is removed when the table is dropped.'

**recommendation:** Update the migration comment and add a note to any migration runbook: 'In the SQLite 12-step rebuild pattern (copy → DROP original → RENAME copy), DROP TABLE destroys all triggers associated with the original table. Triggers on the NEW table (the renamed copy) must be explicitly re-created with CREATE TRIGGER after the RENAME TO. RENAME TO alone (without DROP) preserves triggers by updating sqlite_master references, but the 12-step pattern uses DROP — so re-creation is always required.' Incorporate a migration linting check: any migration file containing both `DROP TABLE <X>` and `RENAME TO <X>` patterns should be required to also contain `CREATE TRIGGER` statements for all triggers that existed on table X.

**confidence:** high

**verify:** The finding is confirmed, but the claimed severity of `info` (documentation risk only) is a significant understatement. This is a real functional regression, not merely a misleading comment.

**What the code actually does:**

Migration 0064 (`/Users/gustavoschneiter/Documents/HuGR/corelink-server/migrations/d1/0064_tenant_tier_max.sql`) performs a 12-step table rebuild: it creates `tenant_new`, copies rows with `INSERT INTO tenant_new ... SELECT ... FROM tenant`, then executes:

- Line 210: `DROP TABLE tenant;` — this destroys the physical table AND all three triggers that 0028 attached to it.
- Line 212: `ALTER TABLE tenant_new RENAME TO tenant;` — this renames the new table to `tenant`, but the old triggers are already gone.

The comment at lines 78–86 claims: "These are name-bound to `tenant`, not the physical object; they survive the DROP + RENAME swap on the same connection. We do NOT drop + recreate them — that would be a destructive trigger operation and is unnecessary: D1 trigger bindings follow the table name."

**This claim is factually wrong.** SQLite documentation (`lang_droptable.html`) is unambiguous: "Every trigger associated with a table is removed when the table is dropped." The trigger rows in `sqlite_master` are hard-linked to the table being dropped, not to the logical name. RENAME TO (without a preceding DROP) does update `sqlite_master` references and would preserve triggers — but the 12-step pattern used here uses DROP TABLE, which destroys them unconditionally.

**No subsequent migration re-creates the triggers.** Verified by grepping 0065–0068 for `TRIGGER` — all empty. The three triggers created in 0028 (`trg_tenant_primary_region_required`, `trg_tenant_primary_region_immutable`, `trg_tenant_primary_region_valid_insert`) do not exist in the D1 schema after 0064 is applied.

**What is actually lost:**

The `CREATE TABLE tenant_new` in 0064 (lines 125–162) includes `primary_region TEXT NOT NULL CHECK (primary_region IN ('wnam','enam','weur','sam','apac','afr'))`, so the NOT-NULL and valid-string-on-INSERT constraints ARE preserved by the column DDL. However:

- `trg_tenant_primary_region_immutable` (BEFORE UPDATE OF primary_region) is gone. This trigger prevented mutation of `primary_region` after row creation. Migration 0028's own comment explicitly documents this as protecting against "Schrems II + LGPD Art. 33 §1º catastrophic violation." There is no CHECK constraint substitute for an UPDATE guard — CHECK constraints run on INSERT/UPDATE but cannot prevent a field from changing to a different valid value. After 0064, any code or admin path can execute `UPDATE tenant SET primary_region = 'wnam' WHERE tenant_id = X` and silently re-route an existing tenant's data cross-region with no D1-level rejection.

- `trg_tenant_primary_region_required` and `trg_tenant_primary_region_valid_insert` (both BEFORE INSERT) are partially redundant with the NOT NULL + CHECK column constraint in the rebuilt table, so their loss has lower practical impact for new inserts.

**Severity justification — HIGH:**

The silent loss of the primary_region immutability guard has a direct GDPR/data-residency impact: it removes the D1-layer enforcement of the tenant data-locality invariant (INV-REGION-NO-CROSS-LEAK). An errant admin query, a future migration mistake, or a compromised admin path could silently move a tenant's `primary_region` pointer without any database-level rejection. The check constraint on the column does not protect against this. This is not purely a documentation risk; the production database after applying 0064 is functionally missing an invariant that the codebase explicitly treats as CRITICAL.

**Evidence:**
- 0064 line 210: `DROP TABLE tenant;` — triggers destroyed here.
- 0064 line 212: `ALTER TABLE tenant_new RENAME TO tenant;` — no triggers attached.
- 0064 lines 214–261: all indexes re-created; zero triggers re-created.
- 0065–0068: no `CREATE TRIGGER` statements for the three `trg_tenant_primary_region_*` triggers.
- 0028 lines 53–59: the immutability trigger has no DDL equivalent — only a BEFORE UPDATE trigger can enforce field immutability in SQLite.
- 0064 lines 83–86 (the erroneous comment): "they survive the DROP + RENAME swap on the same connection. We do NOT drop + recreate them."

**Recommended fix:** Migration 0064 must re-create all three triggers verbatim after the RENAME TO (same pattern 0062 uses to re-create indexes). Additionally, the erroneous comment at lines 78–86 must be corrected to document the true SQLite DROP TABLE behavior. A migration linting script should flag any migration containing `DROP TABLE X` without a subsequent `CREATE TRIGGER ... ON X` for every trigger that was known to exist on X.

---

### [37] LOW — Secret Material Exposed via Automatic Debug Derives in Vault Authentication

**surface:** corelink-byok crate - Vault KMS integration

**location:** crates/corelink-byok/src/byok_vault/auth.rs:45-91

**explanation:** The Vault authentication module contains multiple Rust structs with `#[derive(Debug)]` that hold sensitive credentials. Specifically:

1. **AuthSource enum** (lines 45-59) contains credentials in plaintext string fields:
   - `Token(String)` — the Vault token itself
   - `AppRole { role_id: String, secret_id: String }` — AppRole credentials
   - `Kubernetes { role: String, sa_jwt: String }` — Kubernetes service account JWT
   - `AwsIam { role: String }` — AWS role identifier

2. **CachedToken struct** (lines 73-77) holds a `token: String` field

3. **AuthInner struct** (lines 85-91) contains both `AuthSource` (with secrets) and `cache: Mutex<Option<CachedToken>>` (with tokens)

4. **VaultAuth struct** (lines 80-83) is public and derives `Debug`, wrapping the sensitive `AuthInner`

While the code comments state "Tokens are NEVER included in error messages or `tracing` events" (line 26), the automatic Debug trait implementation means that if any of these structures are logged via `dbg!()`, panic payloads, or debug-format logging (e.g., `format!("{:?}", vault_auth)`), the secrets would be exposed. This is a defense-in-depth gap: even though current code avoids logging these structures, future developers might inadvertently log them, or panic unwinding could include them in stack traces.

**attack_scenario:** An attacker does not need to actively compromise the system to gain secrets. Instead:
1. A future code change accidentally includes `tracing::debug!("{:?}", vault_auth)` or similar
2. A panic occurs during normal operation and the Rust backtrace includes the debug representation of a local `VaultAuth` variable
3. Logs are shipped to a third-party observability platform and a log forwarder includes backtrace context
4. An attacker with log access (internal team member, compromised log aggregator, etc.) extracts the Vault tokens, AppRole secrets, or JWT tokens from the Debug output
5. The attacker uses the exposed credentials to call Vault directly, bypassing CoreLink's access controls, and gains unauthorized access to wrapped DEKs or plaintext key material stored in Vault

**reproduction:** 1. Insert a debug statement in production code: `tracing::debug!(auth = ?vault_auth, "Vault auth created");` in `byok_vault/real.rs` or any upstream code that holds a `VaultAuth` reference
2. Trigger the code path (e.g., container startup with BYOK-Vault configured)
3. Observe logs: the full `VaultAuth` Debug output includes secrets like `Token("hvs_REAL_TOKEN")` or `AppRole { role_id: "...", secret_id: "hvs_..." }`
4. Alternatively, trigger a panic that includes the local variable: the Rust unwinding code includes Debug repr in stderr/logs

**impact:** **CRITICAL BLAST RADIUS:**
- All Vault credentials (tokens, AppRole secrets, Kubernetes JWTs) used for BYOK key management would be exposed
- These credentials allow an attacker to:
  - Read/write Vault secrets directly, bypassing CoreLink's audit trail
  - Extract plaintext Data Encryption Keys (DEKs) that wrap customer secrets
  - Mint new short-lived tokens for persistent backdoor access
  - Modify Vault policies to escalate privileges
- Affects ALL tenants using BYOK-Vault (Azure Vault, AWS KMS, GCP KMS are NOT affected if they don't share the same code path)
- Customer data encrypted with exposed DEKs could be decrypted by the attacker
- The blast radius includes not just the current token but the underlying AppRole/JWT credentials, which can be replayed until explicitly revoked in Vault

**evidence:** ```rust
// crates/corelink-byok/src/byok_vault/auth.rs:45-59
#[derive(Debug)]
enum AuthSource {
    /// Direct token (`VAULT_TOKEN`).
    Token(String),
    /// AppRole login.
    AppRole { role_id: String, secret_id: String },
    /// Kubernetes auth (in-cluster).
    Kubernetes { role: String, sa_jwt: String },
    /// AWS IAM auth.
    AwsIam { role: String },
    /// Test-only: static bearer token.
    #[doc(hidden)]
    Static(String),
}

// crates/corelink-byok/src/byok_vault/auth.rs:73-77
#[derive(Debug, Clone)]
struct CachedToken {
    token: String,
    refresh_at: Instant,
}

// crates/corelink-byok/src/byok_vault/auth.rs:80-91
#[derive(Debug, Clone)]
pub struct VaultAuth {
    inner: Arc<AuthInner>,
}

#[derive(Debug)]
struct AuthInner {
    source: AuthSource,
    http: reqwest::Client,
    vault_addr: String,
    cache: Mutex<Option<CachedToken>>,
}
```

**recommendation:** Implement custom `Debug` impls that redact secrets for all four structs:

```rust
impl std::fmt::Debug for AuthSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Token(_) => f.write_str("AuthSource::Token(<redacted>)"),
            Self::AppRole { .. } => f.write_str("AuthSource::AppRole { <redacted> }"),
            Self::Kubernetes { role, .. } => f.debug_struct("AuthSource::Kubernetes")
                .field("role", role)
                .field("sa_jwt", &"<redacted>")
                .finish(),
            Self::AwsIam { role } => f.debug_struct("AuthSource::AwsIam")
                .field("role", role)
                .finish(),
            Self::Static(_) => f.write_str("AuthSource::Static(<redacted>)"),
        }
    }
}

impl std::fmt::Debug for CachedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedToken")
            .field("token", &"<redacted>")
            .field("refresh_at", &self.refresh_at)
            .finish()
    }
}

impl std::fmt::Debug for AuthInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthInner")
            .field("source", &self.source)
            .field("vault_addr", &self.vault_addr)
            .field("cache", &"<Mutex<Option<CachedToken>>>")
            .finish()
    }
}
```

Additionally:
- Remove the blanket `#[derive(Debug)]` from VaultAuth and AuthInner
- Add a custom Debug impl for VaultAuth that redacts the inner Arc
- Consider using a secret-management crate like `secrecy::SecretString` for token fields (as done in `corelink-stripe-real` for Stripe API keys) to provide automatic zeroing on drop + redacted Debug impls

**confidence:** high

**verify:** The code quotes are accurate: AuthSource (auth.rs:46-59), CachedToken (73-77), and AuthInner (85-91) all use raw #[derive(Debug)] and hold secret material in plain String fields — Token(String), AppRole.secret_id, Kubernetes.sa_jwt, CachedToken.token. A derived Debug would render these verbatim. So the structural observation is genuine.

However, the finding is REFUTED as a "high-severity exploitable vulnerability" for several reasons:

1. NO REACHABLE LEAK EXISTS TODAY. I exhaustively grepped the crate for Debug-format usage of these types and of any variable holding them (vault_auth, auth, AuthInner, AuthSource). There is ZERO Debug-formatting of VaultAuth/AuthSource/AuthInner/CachedToken anywhere in real code. Every {:?} hit in the codebase is on unrelated types (WrappedDek, provider enums, Dek). The module doc (auth.rs:24-28) explicitly states "Tokens are NEVER included in error messages or tracing events," and login() (246-286) deliberately surfaces only status+body, never the client_token. So there is no current code path that emits these secrets.

2. THE ATTACK CHAIN IS ENTIRELY HYPOTHETICAL + CONTAINS A FACTUAL ERROR. Every step is conditioned on FUTURE events the attacker does not control ("a future code change accidentally adds tracing::debug!"). Critically, step 2 ("a panic occurs and the Rust backtrace includes the debug representation of a local VaultAuth variable") is FALSE — Rust panic backtraces print stack frames/addresses, NOT Debug reprs of local variables. This wrong premise is load-bearing for the "passive, no active compromise needed" severity framing. The remaining steps require a future bug PLUS a compromised log aggregator/insider — i.e., a chain of preconditions, not a present exploit.

3. SEVERITY DOWNGRADE. CVSS-style: there is no current attack vector (AV/AC undefined because no path exists), no present confidentiality impact. The "attacker calls Vault directly with extracted creds" outcome presupposes the leak, which does not occur. This is defense-in-depth / hardening, not an exploitable HIGH.

WHY IT IS STILL A REAL (low) ITEM, not pure noise: the project has an established, TESTED secret-redaction discipline that this file violates. byok_core/types.rs:73-77 gives Dek a custom redacting Debug ("[REDACTED]"); byok_azure/entra.rs:50-64 defines SecretString with a redacting Debug AND a test (secret_string_debug_redacts, entra.rs:400-406) asserting "super-secret" never appears and "redacted" does; byok_revocation/detector.rs:103 also custom-redacts. The Vault auth structs are an inconsistency with that in-crate standard. corelink-byok IS shipped (corelink-container/Cargo.toml:232 depends on it; vault behind feature byok-vault-real). So the recommendation (custom redacting Debug impls / SecretString newtype, matching the sibling pattern) is correct and worth doing — as a low-severity hardening fix, not a high-severity vuln.

---

