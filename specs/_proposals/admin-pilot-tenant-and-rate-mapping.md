---
id: "PROPOSAL-2026-06-10-ADMIN-PILOT-TENANT-RATE-MAPPING"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-06-10"
updated: "2026-06-10"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "proposal"
  - "admin"
  - "pilot-provisioning"
  - "ratelimit"
  - "tier-taxonomy"
  - "task-35"
references:
  - "crates/corelink-container/src/routes/admin.rs"
  - "crates/corelink-container/src/routes/admin_pilot.rs"
  - "crates/corelink-container/src/routes/internal_pat.rs"
  - "crates/corelink-container/src/routes/signup.rs"
  - "apps/signup-worker/src/webhooks/clerk.ts"
  - "apps/signup-worker/src/lib/d1.ts"
  - "worker/src/lib/quota.ts"
  - "crates/corelink-ratelimit/src/tier.rs"
  - "crates/corelink-ratelimit/src/config.rs"
  - "crates/corelink-ratelimit/src/key.rs"
  - "crates/corelink-billing/src/abuse/scorer.rs"
  - "migrations/d1/0010_ratelimit_buckets.sql"
  - "migrations/d1/0039_tier_selection.sql"
  - "migrations/d1/0053_pilot_signups.sql"
  - "migrations/d1/0057_tenant_tier.sql"
  - "migrations/d1/0062_expand_tier_selections_6tier.sql"
  - "apps/docs/src/lib/pricing.ts"
  - "docs/POSITIONING.md"
---

# Design: `POST /v1/admin/pilots` create-tenant + billing-tier → ratelimit-Tier mapping (task #35)

> **Status: DESIGN ONLY — no implementation in this PR.** This proposal covers
> (a) the missing admin endpoint to programmatically create a **non-Clerk tenant**
> (pilot programme, hugit-the-product, CI/build-acceleration customers —
> tenant-per-customer provisioning), and (b) the currently **undefined** mapping
> from the 6-tier billing taxonomy to the 5-tier `corelink-ratelimit` ladder.

---

## 1. Current state (verified against the tree, 2026-06-10)

### 1.1 What exists

| Surface | File | Notes |
|---|---|---|
| `GET /v1/admin/pilots`, `POST …/:tenant_id/grant-tier`, `POST …/:tenant_id/checkin` | `crates/corelink-container/src/routes/admin_pilot.rs` | Internal-auth gated (constant-time `x-corelink-internal-auth` vs `CORELINK_INTERNAL_AUTH_KEY`, fail-CLOSED), audit-before-mutate, **but `build_handlers()` binds `InMemoryPilotStore`** — no D1 binding exists. |
| `POST /v1/signup/pilot/{token}` | `crates/corelink-container/src/routes/signup.rs` | Default build binds `InMemorySignupStore` (line ~682); reserves a `pilot_signups` record, **never creates a `tenant` row**. |
| `GET /v1/admin/read/:resource`, `POST /v1/admin/mutate` (`set_tenant_tier`) | `crates/corelink-container/src/routes/admin.rs` | The grant-tier pattern to mirror: internal-auth gate → `into_request_gated` (principal from the gate, never the body) → `normalize_tier_selection` against `TIER_SELECTIONS_TIERS = ["free","solo","starter","pro","max","enterprise"]` → audit chain (`MutateAttempted` BEFORE checks, `MutateCommitted` AFTER) → D1 mirror via `D1AdminHandler`. |
| `POST /_internal/pat/mint` | `crates/corelink-container/src/routes/internal_pat.rs` | Env-gated mount (`CORELINK_INTERNAL_AUTH_KEY` ≥ 16 chars AND `PAT_SIGNING_KEY` hex ≥ 32 bytes — missing → route not mounted, fail-CLOSED). Auth checked BEFORE body parse (M3); plaintext returned once, never logged. |
| Clerk self-serve provisioning | `apps/signup-worker/src/webhooks/clerk.ts` + `lib/d1.ts` | The canonical tenant-row INSERT shape + idempotency pattern (`INSERT OR IGNORE` + read-back-winner) this design reuses. FAIL-LOUD when `CORELINK_INTERNAL_AUTH_KEY` is absent (audit B4). |
| Worker `/_internal/*` routing | `worker/src/index.ts` (~line 1138) | The Worker verifies the caller's `x-corelink-internal-auth` constant-time, strips ALL client trust headers, **re-injects the secret**, forwards to the `_system` DO → container with the path preserved. This is the only operator-reachable privileged channel from the public edge. |

### 1.2 The gap

There is **no endpoint that creates a real (D1-durable) tenant without a Clerk
user**. Today a pilot/hugit/CI customer requires a manual D1 `INSERT INTO tenant…`
plus a manual `/_internal/pat/mint` call plus a manual `pat` row INSERT. The
admin-pilot routes operate on an in-memory store, so even `grant-tier` mutates
nothing durable.

### 1.3 Landmines this design must respect (verified)

1. **`tenant.tier` CHECK (migration 0057) = `('free','solo','starter','team','pro','org','enterprise')`
   — `max` is NOT accepted** (and neither is `pilot`). Migration 0062 widened
   `tier_selections.tier` + `stripe_checkout_sessions.tier` to the 6-tier
   taxonomy but did NOT touch `tenant.tier`. Writing `tier='max'` to the tenant
   row violates the CHECK → opaque D1 500.
2. **`pilot_state` / `cap_bytes` / `slug` columns do not exist on `tenant`** in any
   migration. `admin_pilot::PilotTenant` claims to mirror "the `pilot_state`
   column on the D1 tenants row (wave-27 schema)" — that schema was never landed.
   The durable pilot lifecycle lives in `pilot_signups` (migration 0053:
   `state IN ('RESERVED','PROVISIONED','ACTIVE','EXPIRED','CANCELLED')`).
3. **Auth migrations are additive-only** (HIGH gate; `pat.scope` precedent).
   Widening `tenant.tier`'s inline CHECK requires the 0062-style 12-step rebuild
   + an ADR with line-local `-- additive-allowed:` annotations.
4. **`/v1/admin/*` through the public Worker is PAT-routed and the Worker strips
   `x-corelink-internal-auth`** (`CLIENT_TRUST_HEADERS`), so the container's
   internal-auth-gated admin routes are *Worker-unreachable by design*. Only
   `/_internal/*` is forwarded with the secret re-established.
5. `tenant` columns added by ALTER (0037/0056) are nullable: `clerk_user_id TEXT`
   (partial UNIQUE `WHERE clerk_user_id IS NOT NULL`), `email_hash TEXT`. A
   non-Clerk tenant inserts `clerk_user_id = NULL` cleanly — no sentinel needed.
   `tenant_state` CHECK = `('active','dpa_pending','pending_billing_link','degraded_read_only')`,
   DEFAULT `'dpa_pending'`.

---

## 2. Part A — `POST /v1/admin/pilots` (create pilot tenant)

### 2.1 Route + mount + reachability

- **Canonical route:** `POST /v1/admin/pilots` — added to the existing
  `admin_pilot::router` (the path constant `PILOTS_LIST_ROUTE` already exists;
  today only `get(handle_list)` is bound — `post(handle_create)` is a pure
  addition, same `PilotAdminRouteState`).
- **Operator reachability (decision D1):** additionally mount the SAME handler at
  **`/_internal/admin/pilots`** on the top-level router (next to
  `/_internal/pat/mint` in `routes.rs::build…`). Rationale: the Worker's
  existing `/_internal/*` route-kind already does constant-time verification +
  strip-then-reinject + `_system`-DO forwarding with the path preserved — the
  endpoint becomes operator-callable from the public edge with **zero Worker
  changes**. The alternative (a Worker carve-out that forwards
  `POST /v1/admin/pilots` with internal-auth verification) adds new edge
  surface for no benefit; rejected.
- Mount is **env-gated like `internal_pat`**: requires `CORELINK_INTERNAL_AUTH_KEY`
  (≥ 16 chars) AND `PAT_SIGNING_KEY` (valid hex, ≥ 32 bytes decoded), because the
  endpoint mints a PAT in-process (§2.6). Missing either → route not mounted
  (fail-CLOSED, mirrors `internal_pat::build_state_from_env`).

### 2.2 Auth gating (mirrors grant-tier exactly)

Reuse `admin_pilot::require_admin_scope` unchanged — the 5-layer defense already
shipped there:

1. **PRIMARY boundary:** constant-time, length-padded `internal_auth_ok` against
   `CORELINK_INTERNAL_AUTH_KEY`; key unset → fail CLOSED 403. Gate runs BEFORE
   the body is parsed (take the body as raw `Bytes`, M3 pattern from
   `internal_pat.rs`).
2. **Secondary labels:** `x-admin-principal` (audit principal) +
   `x-admin-scope: corelink:admin:pilots` (defense-in-depth re-check; NOT the
   sole gate). The Worker strips client-supplied copies on every public path.
3. **No dual-approval** (decision D2): consistent with the rest of the pilot-admin
   family (`grant-tier` has none) and the solo-operator reality. The
   dual-approval machinery stays where it is (`/v1/admin/mutate`). Escalate to
   dual-approval only if/when a second operator exists.
4. Unauthorized probe → emit `corelink.security.admin_pilot_unauthorized.v1`
   BEFORE the 403 (`emit_or_503`), exactly as the sibling routes do.

### 2.3 Request / response shape

```jsonc
POST /v1/admin/pilots            // or /_internal/admin/pilots
x-corelink-internal-auth: <CORELINK_INTERNAL_AUTH_KEY>
x-admin-principal: gustavo@operator
x-admin-scope: corelink:admin:pilots
Content-Type: application/json
{
  "email": "cto@acme.dev",            // REQUIRED — idempotency anchor (pilot_signups UNIQUE)
  "slug": "acme-builds",              // REQUIRED — [a-z0-9-]{3,64}; ops display + audit
  "company_name": "Acme Inc",         // optional, ≤ 256 chars
  "tier": "free",                     // optional, DEFAULT "free" — see validation below
  "primary_region": "enam",           // optional, DEFAULT "enam"; CHECK enum (0023)
  "tenant_state": "dpa_pending",      // optional, DEFAULT "dpa_pending" (INV-ONBOARD-DPA-FIRST);
                                      //   operator passes "active" only with a recorded DPA
  "mint_pat": true,                   // optional, DEFAULT true
  "pat_ttl_seconds": 31536000,        // optional, DEFAULT 365d; 0 forbidden here (no no-expiry PATs)
  "seed_tier_selection": false        // optional, DEFAULT false — see Q1
}
```

PAT scope is **hard-coded `read-write`** (`cas:rw` bitset) — this endpoint never
mints `admin` PATs; admin credentials stay on the `/_internal/pat/mint` +
operator-manual path. (Mirrors the signup-worker's "self-serve PATs are
read-write, never admin" rule.)

**Validation (L4, all BEFORE any write):**

- `email`: RFC-trivial shape check, lower-cased; ≤ 254 chars.
- `slug`: `^[a-z0-9][a-z0-9-]{1,62}[a-z0-9]$` (matches the tenant validator
  bounds used by `tenantSlugFor`).
- `tier`: normalized via the `admin.rs::normalize_tier_selection` discipline,
  then restricted to the **intersection** of `TIER_SELECTIONS_TIERS` and the
  0057 `tenant.tier` CHECK = `{free, solo, starter, pro, enterprise}`.
  `max` → `400 invalid_tier_for_create` with a pointer to the grant-tier flow +
  the 0063 follow-up (§2.8). Default `free` keeps the common path trivially safe.
- `primary_region` ∈ `('wnam','enam','weur','sam','apac','afr')` (0023 CHECK).
- `tenant_state` ∈ `('dpa_pending','active')` only (the other two CHECK values
  are lifecycle states this endpoint must not create).

**Response `201 Created` (fresh) / `200 OK` (idempotent replay):**

```jsonc
{
  "tenant_id": "0190…uuid",
  "slug": "acme-builds",
  "email": "cto@acme.dev",
  "tier": "free",
  "primary_region": "enam",
  "tenant_state": "dpa_pending",
  "pilot_state": "PROVISIONED",
  "created": true,                    // false on idempotent replay
  "tier_selection_seeded": false,
  "pat": {                            // null when mint_pat=false OR on replay (§2.4)
    "pat_id": "uuid",
    "token_id": "16-char-b32",
    "token_plaintext": "corelink_pat_…",   // returned ONCE, NEVER logged (CTRL-CRED-001)
    "expires_ms": 1780000000000
  }
}
```

### 2.4 Idempotency (decision D3)

`tenant_id` is generated fresh (UUIDv7) per attempt, so `INSERT OR IGNORE` on
`tenant` alone cannot deduplicate. The anchor is **`pilot_signups.email`**
(UNIQUE index, migration 0053), reusing the clerk.ts *insert-or-ignore +
read-back-the-winner* pattern:

1. `INSERT OR IGNORE INTO pilot_signups (…)` with the fresh `tenant_id`.
2. `SELECT tenant_id, state FROM pilot_signups WHERE email = ?1` — adopt the
   winning row. If the winner's `tenant_id` ≠ ours, this is a replay/duplicate:
   skip the tenant INSERT, set `created=false`.
3. All subsequent writes key off the winner's `tenant_id` and are themselves
   `INSERT OR IGNORE` / `ON CONFLICT` shaped, so a crashed half-provisioned
   attempt **converges on retry** (same email → same tenant).
4. **PAT on replay:** if a `pat` row already exists for the winner tenant
   (`SELECT 1 FROM pat WHERE tenant_id = ?1 LIMIT 1`), return `pat: null` —
   the plaintext was already shown once and can never be re-shown
   (CTRL-CRED-001). If NO pat row exists (prior attempt died between the tenant
   write and the mint), the retry mints normally — this is the self-healing
   path for the partial-failure mode in §2.7.

### 2.5 D1 writes (order matters; all parameterized)

Executed via the container's `D1HttpClient` (`StorageEnv` path, same as
`D1AdminHandler`). D1 HTTP has no multi-statement transaction here — ordering +
idempotent shapes are the consistency mechanism (same posture as the
signup-worker flow).

```sql
-- (1) Pilot lifecycle record — idempotency anchor.
INSERT OR IGNORE INTO pilot_signups
  (id, tenant_id, email, company_name, tier_hint, expected_use_case,
   signed_up_at, state, token_id)
VALUES (?uuid, ?tenant_id, ?email, ?company, ?tier, 'admin-provisioned',
        ?now_ms, 'PROVISIONED', ?admin_token_id);
-- token_id = "admin:" || uuid  → satisfies the UNIQUE token replay index without
-- colliding with HMAC pilot-token redemptions. state='PROVISIONED' (a real
-- tenant row exists immediately; 'RESERVED' is the token-redemption state).

-- (2) Read back the winner (idempotency pivot — see §2.4).
SELECT tenant_id, state FROM pilot_signups WHERE email = ?email LIMIT 1;

-- (3) Tenant row — mirrors apps/signup-worker/src/lib/d1.ts::insertTenant,
--     with clerk_user_id = NULL (non-Clerk) and the validated tier.
INSERT OR IGNORE INTO tenant
  (tenant_id, primary_region, tenant_state, email_hash, clerk_user_id,
   tier, created_at_ms, updated_at_ms, created_ms, updated_ms)
VALUES (?winner_tenant_id, ?region, ?tenant_state, ?sha256_email_hex, NULL,
        ?tier, ?now_ms, ?now_ms, ?now_ms, ?now_ms);
-- email_hash = hex(SHA-256(lowercased email)) — the tenant row follows the
-- email-hash discipline even though pilot_signups stores plaintext (DEBT-028).

-- (4) OPTIONAL tier_selections seed (only when seed_tier_selection=true; Q1).
--     Mirrors tier_select_store.rs free-activation UPSERT byte-for-byte:
INSERT INTO tier_selections
  (tenant_id, tier, subscription_state, subscription_started_at_ms, correlation_id)
VALUES (?winner_tenant_id, ?tier, 'active', ?now_ms, ?correlation_id)
ON CONFLICT(tenant_id) DO UPDATE SET
  tier = excluded.tier, subscription_state = 'active',
  subscription_started_at_ms = excluded.subscription_started_at_ms,
  correlation_id = excluded.correlation_id;

-- (5) PAT row — only after a successful in-process mint (§2.6); mirrors
--     d1.ts::insertPat (scope 'read-write', shown_once_consumed=1).
INSERT OR IGNORE INTO pat
  (pat_id, tenant_id, pat_hash, scope, expires_ms, token_id,
   shown_once_token, shown_once_consumed, created_ms)
VALUES (?pat_id, ?winner_tenant_id, ?argon2id_phc, 'read-write', ?expires_ms,
        ?token_id, ?uuid, 1, ?now_ms);
```

### 2.6 PAT minting (decision D4 — in-process, not HTTP)

The handler calls **`corelink_pat::mint::mint(...)` directly** (the same
function `internal_pat::handle_mint` wraps), using the `PatSigningKey` from the
route state — no loopback HTTP call to `/_internal/pat/mint`. Benefits:

- Removes a network failure mode and keeps the Argon2id `hash` **inside the
  container** — this endpoint writes the `pat` row itself, which is exactly the
  M7 follow-up direction documented in `internal_pat.rs` ("removing [hash from
  the response] requires moving the D1 pat-row write into the container").
- Same hard rules: `token_plaintext` never logged, returned exactly once;
  `PatMinted`-equivalent audit (the §2.7 payload carries `pat_id`/`token_id`,
  never plaintext, tenant only as the row key — logs use `hash_for_log`).

### 2.7 Audit emit + failure modes

Audit (fail-CLOSED, `emit_or_503` family; new constant
`EVENT_TYPE_PILOT_CREATED: corelink.admin.pilot_created.v1`):

| Order | Row | When |
|---|---|---|
| 1 | `pilot_created` / `exit_status="attempt"`, payload `{email_hash, slug, tier, region, mint_pat}` | AFTER the gate + validation, BEFORE write (1). Sink `Err` → 503, nothing written. |
| 2 | `pilot_created` / `exit_status="ok"`, payload `{tenant_id, created, pat_id?, tier_selection_seeded}` | AFTER all writes. Sink `Err` → 503 (writes landed; the `attempt` row is the SEC signal — identical posture to `handle_grant_tier`'s post-emit). |
| — | `admin_pilot_unauthorized` / `cross_tenant` | unchanged, from `require_admin_scope`. |

Failure modes (exhaustive):

| Mode | Response | State afterwards | Recovery |
|---|---|---|---|
| Key unset at boot | route not mounted (404) | none | configure secrets, redeploy |
| Bad/absent internal auth | 403 (+ unauthorized audit row) | none | — |
| Validation failure (slug/tier/region/email/state/ttl) | 400 with stable `error` code | none | fix request |
| Audit sink down (pre-emit) | 503 `audit pipeline closed` | none | retry after sink recovery |
| D1 down at write (1) | 503 `store unavailable` | none | retry (idempotent) |
| Crash between (1) and (3) | 5xx | pilot row only | retry converges (read-back winner → tenant INSERT) |
| Crash between (3) and (5) / mint failure | 502 `pat_mint_failed` | tenant exists, no PAT | retry: replay path sees no `pat` row → re-mints (§2.4.4) |
| Duplicate email, params match | 200 `created=false`, `pat=null` | unchanged | — |
| Duplicate email, conflicting `slug`/`tier` | 409 `email_already_provisioned` (no silent param drift) | unchanged | operator resolves manually |
| `tier='max'` requested | 400 `invalid_tier_for_create` | none | use `free` + grant-tier, or wait for 0063 |

### 2.8 Schema follow-ups (separate PRs, additive-only)

1. **0063 (required for full 6-tier parity):** widen `tenant.tier` CHECK to add
   `'max'` (and decide on `'pilot'`) via the 0062-style 12-step rebuild + ADR +
   `-- additive-allowed:` annotations. Until then this endpoint refuses `max`
   at create time (§2.3).
2. **Optional:** `ALTER TABLE pilot_signups ADD COLUMN slug TEXT` (pure additive)
   so the list endpoint's D1 binding can serve `PilotTenant.slug` without a
   join; until then `slug` lives in the audit payload + response only.
3. **Noted, not blocking:** `admin_pilot::PilotState` has 6 states
   (NEW/RESERVED/PROVISIONED/ACTIVE/GRADUATED/TERMINATED) vs `pilot_signups`
   CHECK's 5 (RESERVED/PROVISIONED/ACTIVE/EXPIRED/CANCELLED) — the eventual D1
   `PilotStore` binding must reconcile (additive CHECK widening or enum mapping).

---

## 3. Part B — billing-tier → `ratelimit::Tier` mapping

### 3.1 The two taxonomies (verified)

- **Billing (6, FROZEN):** `free | solo | starter | pro | max | enterprise`
  (`apps/docs/src/lib/pricing.ts::CANONICAL_TIERS`, `tier_selections` post-0062;
  plus D1 legacy values `team`, `org` retained for back-compat).
- **Rate ladder (5, FROZEN at the data-model layer):** `corelink_eviction::Tier
  = Free | Solo | Team | Business | Enterprise`, refill/burst in
  `crates/corelink-ratelimit/src/tier.rs`. **Nothing in the tree currently maps
  one to the other** — `refill_rate_for_tier`'s only production caller is the
  abuse scorer, which receives a `Tier` it never resolves from billing data, and
  bucket materialization falls back to `RateLimitConfig::canonical()` (= Team
  200 rps / 1000 burst) "pre plan resolution". The plan resolution is this
  mapping.

### 3.2 The math (rate-card monthly caps vs per-second refill)

Monthly request caps (signed rate card, `TIER_RATE_CARD` / POSITIONING.md):
free 500 K · solo 2 M · starter 6 M · pro 20 M · max 80 M · enterprise uncapped.
Using a 30-day month (2 592 000 s):

| Billing tier | Cap/mo | Cap-implied avg RPS | Candidate class | Sustained refill | Headroom (refill ÷ avg) | Refill-implied max/mo |
|---|---|---|---|---|---|---|
| free | 500 K | 0.19 | **Free** 10 rps / 50 | 10 | 52× | 25.9 M |
| solo | 2 M | 0.77 | **Solo** 50 rps / 200 | 50 | 65× | 129.6 M |
| starter | 6 M | 2.31 | Solo 50 → 22× / **Team** 200 rps / 1000 → 87× | 200 | 87× | 518 M |
| pro | 20 M | 7.72 | Team 200 → 26× / **Business** 1000 rps / 5000 → 130× | 1000 | 130× | 2.59 B |
| max | 80 M | 30.9 | **Business** 1000 → 32× / Enterprise 10 000 → 324× | 1000 | 32× | 2.59 B |
| enterprise | ∞ | — | **Enterprise** 10 000 rps / 50 000 | 10 000 | — | 25.9 B |

Design rule: the token bucket is the **spike governor**, the monthly quota is
the **volume governor**. Pick the smallest class whose burst envelope absorbs
the tier's worst-case *legitimate* parallel-CI spike (a single Bazel/Turbo run
fans out hundreds of concurrent CAS reads), while keeping refill far enough
above the cap-implied average that the limiter never becomes a de-facto monthly
cap — but low enough that a runaway client cannot burn an absurd multiple of
the monthly cap before the (still-TODO, see §3.5) request-quota counter reacts.

### 3.3 Proposed mapping (the deliverable)

| Billing tier (wire) | → `ratelimit::Tier` | Rationale |
|---|---|---|
| `free` | `Free` (10 rps / 50) | Name-aligned; 52× headroom over the cap average; smallest spike envelope fits "personal projects / evaluation". |
| `solo` | `Solo` (50 rps / 200) | Name-aligned; 65× headroom; a single developer's parallel build fits the 200-token burst. |
| `starter` | **`Team`** (200 rps / 1000) | NOT Solo: (a) Starter is sold as "a small team getting onto a shared cache" — multi-developer concurrent CI; one 32-way Bazel run exceeds Solo's 200-token burst in the first second, and 50 rps refill would 429 a second concurrent pipeline; (b) `RateLimitConfig::canonical()` ALREADY gives every unresolved tenant Team — mapping a *paid* tier below today's implicit default is a day-one regression; (c) the 3× cap step over solo (6 M vs 2 M) tracks the 4× refill step. 87× headroom. |
| `pro` | **`Business`** (1000 rps / 5000) | The anchor SKU, "teams shipping production builds": 2–3 concurrent heavy pipelines exhaust Team's 1000-token burst; Business's 5000 burst covers ~5 concurrent runs. 130× headroom. |
| `max` | **`Business`** (1000 rps / 5000) | NOT Enterprise: (a) Enterprise's 10 k rps is the contract-negotiated class with S-13 admin override AND the unknown-tier fallback — granting it to self-serve erases the Enterprise upsell surface; (b) at 10 k rps a runaway Max client could burn the full 80 M cap in ~2.2 h and 324× the cap in a month, with request-quota enforcement still a no-op (§3.5) — Business bounds the worst case at 32×; (c) max vs pro differentiation already lives in quota + storage (4× both), not per-second spikes. Headroom 32× is the lowest in the table but still ample (sustained 1000 rps for 22 h = the whole monthly cap — any client doing that is the abuse scorer's job, not the happy path). |
| `enterprise` | `Enterprise` (10 000 rps / 50 000) | Contractual; per-tenant admin override supersedes the default (tier.rs doc). |
| `team` (D1 legacy, kept by 0062) | `Team` | Name-identical; legacy rows keep their semantics. |
| `org` (D1 legacy = pre-S-19 "pro") | `Business` | `worker/src/lib/quota.ts` already treats org as pro's quota class; follow it. |
| `pilot` (grant-tier label) / unknown | `Team` | Matches today's pre-resolution default → zero behavior change for pilots; unknown stays on tier.rs's documented most-permissive-fallback debate (see Q5). |

Monotonicity check: free ⊂ solo ⊂ starter ⊂ pro = max ⊂ enterprise — no
paid tier ever rate-limits below a cheaper tier.

### 3.4 Where it hooks (candidate seams — found, NOT modified)

1. **Single Rust authority (recommended home):**
   `crates/corelink-ratelimit/src/tier.rs` — add
   `pub fn tier_for_billing_label(label: &str) -> Tier` next to
   `refill_rate_for_tier`. One function, total over the 8 wire strings above +
   fallback arm. Every other seam consumes this.
2. **Billing-tier resolution already exists at the edge:**
   `worker/src/lib/quota.ts::getTierForTenant(db, tenantId)` resolves
   `tier_selections.tier → tenant.tier → 'free'` per request and feeds
   `QUOTAS`. An edge-side limiter (or the DO bucket-materialization call) would
   add a sibling pure map `rateClassForTier(tier)` mirroring (1). Cross-language
   drift MUST be pinned by a test the way
   `apps/docs/src/lib/pricing.test.ts` pins `TIER_RATE_CARD`.
3. **Bucket materialization / plan update (container/DO side):**
   `ratelimit_buckets.refill_rate_per_sec` + `burst_capacity` (migration 0010)
   are per-row; `RateLimiter::update_plan(...)` is the existing write API —
   `crates/corelink-billing/src/abuse/scorer.rs` (~line 330) already calls
   `refill_rate_for_tier(tier)` to materialize a missing bucket and
   `update_plan` to downgrade. "Plan resolution" = resolve billing label
   (seam 2) → map (seam 1) → `refill_rate_for_tier` → `update_plan`, replacing
   the blanket Team default in `RateLimitConfig::canonical()`.
4. **Per-endpoint dimension is orthogonal:** the ladder sets the per-`BucketKey`
   rates; `BucketKey::per_tenant_per_endpoint` (`key.rs`) isolates hot endpoints
   *within* the tenant's class. This proposal sets the per-tenant class only.

**Gaps flagged while tracing (not fixed here):** (a) the
`tenant_rate_override` table the abuse scorer's docs lean on (WI §6.1.4 CI-2)
has **no migration**; (b) the Worker performs no token-bucket rate limiting at
all today — only storage quota; (c) `checkRequestQuota` is a documented no-op.

### 3.5 Coupling warning — the enum also drives eviction TTL

`corelink_eviction::Tier` is shared: the same mapped value selects the TTL
ladder (`ttl_for_tier`: Free 7 d, Solo 30 d, Team 90 d, Business 365 d,
Enterprise 365 d/override-730 d). Under this mapping, starter inherits 90-day
cache retention and pro/max inherit 365-day — directionally sensible
(retention scales with price) but it becomes a **customer-visible retention
promise** the moment the mapping ships, so the rate card / docs must state it
(see Q5).

---

## 4. Open questions for the tech lead (max 5)

1. **Comped-pilot `tier_selections` semantics (ties to #32/#34):** when
   `seed_tier_selection=true` with a paid tier, we write
   `subscription_state='active'` with NO Stripe customer/subscription ids. Does
   the billing materializer / reconciliation tolerate an active row without
   Stripe linkage, or do pilots need a distinct state (additive CHECK widening,
   e.g. `'comped'`)? Default in this design is `seed_tier_selection=false`
   (quota.ts falls back to `tenant.tier`) until answered.
2. **`tenant.tier` CHECK widening (0063):** approve the 0062-style rebuild ADR to
   add `'max'` (and `'pilot'`?) to `tenant.tier` — or rule that create-time tier
   stays `{free,solo,starter,pro,enterprise}` forever and `max` is only ever
   reached via `tier_selections` (leaving `tenant.tier` as the quota fallback
   only)?
3. **Reachability:** is the `/_internal/admin/pilots` alias mount (zero Worker
   changes, operator-only channel) acceptable as the ONLY edge path, or do you
   want a Worker carve-out so a future admin-UI can call `POST /v1/admin/pilots`
   with internal auth injected server-side?
4. **PAT-on-replay posture:** replay returns `pat: null` and never re-mints once
   a pat row exists (CTRL-CRED-001). Is "operator deletes the pat row manually
   (or uses a future rotate op) to recover a lost pilot credential" acceptable
   for launch, or should this design include a `rotate=true` flag from day one?
5. **Mapping ratification:** confirm (a) `starter→Team`, `pro→Business`,
   `max→Business` as proposed (esp. max NOT Enterprise); (b) `pilot`/unknown →
   `Team` — note this *narrows* tier.rs's documented unknown→Enterprise
   fallback for strings, which only makes sense if we also accept (c) the
   eviction-TTL coupling in §3.5 becoming a published retention promise per
   tier. Ship order: Rust authority fn first (inert until limiter wiring), TS
   mirror + pin test with the first consumer.

---

## 5. Non-goals

- No implementation in this PR (design only).
- No change to the dual-approval admin mutate plane.
- No Worker rate-limiter implementation (the mapping defines *what class*, not
  *where enforcement runs* — enforcement wiring is the limiter epic).
- No change to the in-memory pilot-signup route (`signup.rs`); it remains the
  self-serve token-redemption surface and converges on the same
  `pilot_signups` table once its D1 binding lands.
