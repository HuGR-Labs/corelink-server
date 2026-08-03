/**
 * Typed D1 helpers for the signup-worker.
 *
 * All queries are parameterized (no SQL injection surface).
 * Column ordering matches the D1 migrations:
 *   - `tenant`: 0023_residency_check_constraints.sql (base) + 0037_signup_orchestration.sql
 *     (signup columns) + 0052_tenant_config_region.sql + 0055_tenant_clerk_user_id.sql
 *   - `pat`: 0037_signup_orchestration.sql + 0054_pat_token_id.sql
 */

// Minimal local D1 types — we don't import @cloudflare/workers-types here
// to keep the unit-test surface independent of the runtime types package.
interface D1PreparedStatement {
  bind(...values: unknown[]): D1PreparedStatement;
  run(): Promise<{ success: boolean; error?: string }>;
  first<T = unknown>(): Promise<T | null>;
}
export interface D1Database {
  prepare(query: string): D1PreparedStatement;
}

/** Result of a tenant lookup by Clerk user id. */
export interface TenantRow {
  tenant_id: string;
}

/** Parameters for inserting a new tenant row. */
export interface InsertTenantParams {
  tenantId: string;
  primaryRegion: string;
  tenantSlug: string;
  emailHash: string;
  clerkUserId: string;
  nowMs: number;
}

/** Parameters for inserting a new PAT row. */
export interface InsertPatParams {
  patId: string;
  tenantId: string;
  tokenId: string;
  patHash: string;
  /** D1 `scope` CHECK constraint allows: 'read-write' | 'read-only' | 'admin' */
  scope: "read-write" | "read-only" | "admin";
  /** Expiry epoch milliseconds (> 0). */
  expiresMs: number;
  shownOnceToken: string;
  nowMs: number;
}

/**
 * Look up a tenant by Clerk user id. Returns the existing tenant row
 * or null if not found (idempotency check at webhook entry).
 */
export async function lookupTenantByClerkUser(
  db: D1Database,
  clerkUserId: string,
): Promise<TenantRow | null> {
  return db
    .prepare(
      "SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1",
    )
    .bind(clerkUserId)
    .first<TenantRow>();
}

/**
 * Insert a new tenant row. The `tenant_state` defaults to 'active' for
 * self-serve signups (the signup-worker provisions the tenant directly,
 * bypassing the DPA-pending flow used by the pilot programme).
 *
 * We INSERT OR IGNORE so a race between two concurrent webhook deliveries
 * of the same event is harmless — the first writer wins and the second
 * is silently dropped.
 */
export async function insertTenant(
  db: D1Database,
  params: InsertTenantParams,
): Promise<void> {
  await db
    .prepare(
      "INSERT OR IGNORE INTO tenant " +
        "(tenant_id, primary_region, tenant_state, email_hash, clerk_user_id, " +
        " created_at_ms, updated_at_ms, created_ms, updated_ms) " +
        "VALUES (?1, ?2, 'active', ?3, ?4, ?5, ?5, ?5, ?5)",
    )
    .bind(
      params.tenantId,
      params.primaryRegion,
      params.emailHash,
      params.clerkUserId,
      params.nowMs,
    )
    .run();
}

/**
 * Insert a new PAT row. INSERT OR IGNORE for the same race-safety
 * reasoning as `insertTenant` — if the pat_id already exists (webhook
 * retry after partial success), we skip the duplicate.
 *
 * `shown_once_consumed = 0` — the reveal endpoint (WI-S19-006) flips
 * this to 1 on first GET. For the PLG flow the plaintext is surfaced via
 * the Clerk session claim rather than the reveal endpoint, but we keep
 * `shown_once_consumed = 1` to prevent stale reveals (the user already
 * has the plaintext in their /welcome session).
 */
export async function insertPat(
  db: D1Database,
  params: InsertPatParams,
): Promise<void> {
  await db
    .prepare(
      "INSERT OR IGNORE INTO pat " +
        "(pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, " +
        " shown_once_token, shown_once_consumed, created_ms) " +
        "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
    )
    .bind(
      params.patId,
      params.tenantId,
      params.patHash,
      params.scope,
      params.expiresMs,
      params.tokenId,
      params.shownOnceToken,
      params.nowMs,
    )
    .run();
}

/**
 * Write the `tenant_org_map` identity-map row (A1 auto-provision, migration
 * 0083) that `POST /internal/v1/auth/resolve-tenant` reads. WITHOUT this row,
 * `resolve-tenant` 404s `org_not_mapped` for every real new user and locks them
 * out — so this write is LOAD-BEARING, not best-effort.
 *
 * `clerkOrgId` is the Clerk principal identifier githugr scopes a token to: the
 * Clerk `org_id` when the provisioning event carries one, else the user
 * `sub`/`id` (individual pilot users have no org). Either way, EVERY principal
 * maps to its isolated tenant.
 *
 * `INSERT OR IGNORE` for idempotency: a Svix redelivery (or an idempotent
 * re-run of provisioning for an existing tenant) is a harmless no-op, and the
 * `clerk_org_id` PRIMARY KEY means the first mapping wins.
 *
 * Deliberately NOT wrapped in try/catch — the caller MUST let a throw propagate
 * so the webhook returns non-2xx and Svix retries (a tenant without its map row
 * is the exact lockout A1 fixes).
 */
export async function insertTenantOrgMap(
  db: D1Database,
  params: { clerkOrgId: string; tenantId: string; nowMs: number },
): Promise<void> {
  await db
    .prepare(
      "INSERT OR IGNORE INTO tenant_org_map " +
        "(clerk_org_id, tenant_id, created_at_ms) VALUES (?1, ?2, ?3)",
    )
    .bind(params.clerkOrgId, params.tenantId, params.nowMs)
    .run();
}

/**
 * Free-tier entitlement DEFAULTS — kept in LOCK-STEP with the parallel
 * login-time provisioner `worker/src/lib/githugr_provision.ts` (the CANONICAL
 * shape; see its provision batch). Both provisioners MUST converge on the SAME
 * row-family so a Clerk-signup tenant and a githugr-login tenant are seeded
 * identically — a Clerk tenant missing these rows reports billing `inactive` and
 * reads an empty quota gate → degraded dashboard.
 *
 * Convergence cuts BOTH ways: when the runner grant was removed here (2026-08-02)
 * it was removed there in the same change. If you add a row-family to one, add it
 * to the other — a divergence means two classes of tenant with different rights.
 *
 * The two workers are SEPARATE deploy units (no cross-import between `worker/`
 * and `apps/signup-worker/`), so these statements + constants are replicated by
 * hand. Keep them faithful to githugr_provision.ts.
 */
/** Free-tier monthly $-ceiling BACKSTOP, micro-dollars ($1,000,000/mo, ADR-0068). */
const FREE_MONTHLY_BUDGET_USD_MICROS = 1_000_000_000_000;

// NOTE: there is deliberately NO `FREE_RUNNER_MAX_CONCURRENCY` here any more.
// Runners is a SEPARATE PAID axis (owner-ratified Option B) and a signup grants
// NO runner capacity — see the `seedTenantEntitlements` doc-comment below for the
// full reasoning. Re-introducing a free runner row re-opens the leak.

/** Parameters for seeding the free-tier entitlement row-family. */
export interface SeedEntitlementsParams {
  tenantId: string;
  nowMs: number;
}

/**
 * Seed the TWO entitlement rows a fully-provisioned tenant needs beyond
 * `tenant` / `tenant_org_map` / `pat`:
 *   - `tier_selections('free','active')` — so billing reads `active`, not `inactive`
 *   - `tenant_quota` — non-zero monthly ceiling so the container quota gate is
 *     not a flat 402 / fail-open None on an empty read
 *
 * ## Why NO `runners_entitlement` row (2026-08-02)
 *
 * This used to seed a third row, `runners_entitlement('free', max_concurrency=1)`,
 * "so the runner cap gate reads a real entitlement". That comment described the
 * intent correctly and the effect wrongly: `runners_entitlement` is not a cap the
 * gate merely READS, it is the ENTITLEMENT the gate CHECKS. Writing the row is
 * granting the capacity.
 *
 * It silently reverted an owner-ratified decision. Runners is a SEPARATE PAID axis
 * from the cache tier (Option B, 2026-06-13) — chosen precisely because the
 * alternative "leaked compute a cache-only tenant never bought". Migration 0070
 * states the contract in its own header: "NO row … the tenant is cache-only and
 * gets no runner cap … Empty table = no cap = reject." A free cache signup is
 * exactly the cache-only tenant that clause is about.
 *
 * The row made all four `handleRunnerMint` gates passable for a tenant that never
 * bought Runners: 5d (entitlement) came from here, and 5a (installation map) + 5c
 * (repo allowlist) are both seeded by `webhooks/github_install_callback.ts`, which
 * checks no entitlement of any kind. So: free signup → install the public GitHub
 * App → boxes spawn on our Cloudflare account. Slots are first-come-first-served
 * against a GLOBAL fleet cap of 20 with no reservation for paying tenants, so the
 * grant also competes directly with revenue.
 *
 * ⚠️ The trap that produced the bug is still in the schema: `max_concurrency
 * INTEGER NOT NULL CHECK(max_concurrency > 0)` makes "entitled to ZERO"
 * INEXPRESSIBLE. Anyone wanting a placeholder row is forced by the CHECK to grant
 * real capacity. Do not add one. Absence IS the zero — that is the whole design.
 *
 * CANONICAL SHAPE: mirrors `worker/src/lib/githugr_provision.ts` (~lines 154-200)
 * statement-for-statement (column lists, literal 'free'/'active', schema_version
 * 1, the `subscription_started_when_active` CHECK — 0039 — satisfied by the
 * non-NULL `subscription_started_at_ms`, and the same free-tier constants). Only
 * the `correlation_id` provenance string differs (`clerk-signup:` vs
 * `githugr-provision:`), since it is a per-path audit label.
 *
 * Every statement is `INSERT OR IGNORE` for the same race-/retry-safety as
 * `insertTenant` / `insertTenantOrgMap`: a Svix redelivery of `user.created` is a
 * harmless no-op (tenant_id PRIMARY KEY → first-writer-wins).
 *
 * Deliberately NOT wrapped in try/catch — a throw MUST propagate so the webhook
 * returns non-2xx and Svix retries. These rows are LOAD-BEARING for a working
 * dashboard, exactly like the `tenant_org_map` row. Sequential (not a D1
 * `batch`) to match this file's other insert helpers + the prepare-only
 * `D1Database` surface; the signup path's atomicity guarantee comes from the
 * handler's tenant+live-PAT idempotency re-entry, not a transaction.
 */
export async function seedTenantEntitlements(
  db: D1Database,
  params: SeedEntitlementsParams,
): Promise<void> {
  const { tenantId, nowMs } = params;

  // (1) tier_selections — free/active. subscription_started_at_ms MUST be
  // non-NULL when state='active' (0039 CHECK subscription_started_when_active).
  await db
    .prepare(
      "INSERT OR IGNORE INTO tier_selections " +
        "(tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms) " +
        "VALUES (?1, 'free', 'active', 1, ?2, ?3)",
    )
    .bind(tenantId, `clerk-signup:${tenantId}`, nowMs)
    .run();

  // (2) tenant_quota — effectively-unlimited $-ceiling backstop (ADR-0068), so
  // the container quota gate isn't a flat 402 / fail-open None on an empty read.
  await db
    .prepare(
      "INSERT OR IGNORE INTO tenant_quota " +
        "(tenant_id, monthly_budget_usd_micros) VALUES (?1, ?2)",
    )
    .bind(tenantId, FREE_MONTHLY_BUDGET_USD_MICROS)
    .run();
}

/** A single outstanding `invited` team-member row (the acceptance target). */
interface InvitedMemberRow {
  tenant_id: string;
  user_id: string;
}

/**
 * Accept an outstanding team invitation (C-ACCEPT, ADR-S33-001 WP-4).
 *
 * When a Clerk `user.created` event fires for an email that was previously
 * invited to a team (a `team_member` row with `status='invited'`, keyed by the
 * SHA-256 `email_hash` per CTRL-PRIV-001 — never the raw email), flip that seat
 * to `active`: stamp `joined_at_ms` and bind the real Clerk `user_id` (the
 * invited row carried the Clerk invitation id as a placeholder, migration 0074).
 *
 * Returns `true` iff a row was flipped. A signup whose email matches no
 * outstanding invitation (the common self-serve case) returns `false` and is a
 * no-op — the caller treats this as benign.
 *
 * The UPDATE re-asserts `status='invited'` so a concurrent acceptance (double
 * webhook delivery) cannot double-flip or clobber an already-`active` seat.
 *
 * DUAL-READ (safe EMAIL_HASH_SALT activation): `emailHashCandidates` is the
 * deduped set `{salted, legacy}` — one value when the salt is unset (identical
 * to today), two once it is set. Matching `email_hash IN (...)` binds an invite
 * regardless of whether its row was written under the legacy pre-salt scheme (the
 * 5 pending invites) or the new salted scheme. Writes stay salted; lookups find
 * both.
 */
export async function acceptTeamInvitation(
  db: D1Database,
  clerkUserId: string,
  emailHashCandidates: string[],
): Promise<boolean> {
  // Deduped 1-or-2 candidates → a fixed 2-slot IN list. Padding the single-salt
  // case with a repeat of the same value keeps ONE prepared statement shape and
  // is a semantic no-op (`x IN (a, a)` ≡ `x = a`).
  const [c0, c1] = [
    emailHashCandidates[0],
    emailHashCandidates[1] ?? emailHashCandidates[0],
  ];
  const invited = await db
    .prepare(
      "SELECT tenant_id, user_id FROM team_member " +
        "WHERE email_hash IN (?1, ?2) AND status = 'invited' LIMIT 1",
    )
    .bind(c0, c1)
    .first<InvitedMemberRow>();
  if (invited === null) return false;

  await db
    .prepare(
      "UPDATE team_member " +
        "SET status = 'active', joined_at_ms = ?1, user_id = ?2 " +
        "WHERE tenant_id = ?3 AND user_id = ?4 AND status = 'invited'",
    )
    .bind(Date.now(), clerkUserId, invited.tenant_id, invited.user_id)
    .run();
  return true;
}
