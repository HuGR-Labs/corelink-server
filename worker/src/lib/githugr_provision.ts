/**
 * githugr per-user tenant PROVISION-OR-LOOKUP (pilot isolation fix).
 *
 * WHY THIS EXISTS — the isolation break it closes:
 * The githugr Clerk instance (`clerk.githugr.com`) is a SEPARATE IdP, so the
 * CoreLink signup-worker's Clerk `user.created` auto-provision NEVER fires for a
 * githugr user. The old `verifyGithugrSession` therefore resolved EVERY githugr
 * session to the single fixed `env.GITHUGR_TENANT_ID` — i.e. every githugr user
 * shared ONE CoreLink tenant, so there was NO tenant isolation between them
 * (one user's cache/quota/runner state was every user's).
 *
 * This module makes the exchange itself the provisioning authority: it derives a
 * DETERMINISTIC per-user tenant_id from the verified Clerk `sub` and
 * PROVISIONS-OR-LOOKS-UP that tenant (with its full row-family) on CONFIG_DB,
 * idempotently. Two distinct subs → two distinct tenants (ISOLATION); the same
 * sub twice → the same tenant, no dup rows (IDEMPOTENT).
 *
 * Row-set + column shapes are mirrored from the authoritative fixtures:
 *   - `scripts/family-e2e-tier-seed.sql` (the 5 row-families the container gates
 *     read: tenant / tier_selections / runners_entitlement / tenant_quota, and
 *     the identity map)
 *   - `apps/signup-worker/src/lib/d1.ts` (`insertTenantOrgMap` — `tenant_org_map`
 *     keyed on the Clerk principal id).
 *
 * FK ORDER: the `tenant` row is written FIRST (tier_selections / the others
 * reference `tenant.tenant_id`), then the child rows, then the identity map.
 * Every write is `INSERT OR IGNORE` so a concurrent second login of the same sub
 * (or a re-login) is a harmless no-op.
 *
 * HARDENING (audit H3 lookup-first + H5 atomic batch):
 *   - H3 (write-amplification): the COMMON case is a repeat login, whose row-set
 *     already exists. So we do the read-back `SELECT` FIRST; on a hit we return
 *     immediately with ZERO writes. The 5 provisioning writes run ONLY on the
 *     first-ever login for a `sub`. Deterministic id + INSERT OR IGNORE keeps
 *     this idempotency-safe and returns the identical tenant_id.
 *   - H5 (partial-provision): the 5 first-login writes are issued as a SINGLE
 *     D1 `batch([...])` (Cloudflare D1 batches are transactional — all-or-nothing
 *     on the same connection), so a mid-provision D1 fault can never leave a
 *     partial row-family lingering. FK order (tenant first) is preserved INSIDE
 *     the batch; every statement stays INSERT OR IGNORE.
 */

/**
 * Minimal local D1 surface — mirrors `apps/signup-worker/src/lib/d1.ts` so the
 * unit tests can pass a hand-rolled mock without pulling @cloudflare/workers-types.
 * The real `env.CONFIG_DB` (a full `D1Database`) is structurally compatible.
 *
 * `bind(...)` returns a prepared STATEMENT handle: it carries `run()` / `first()`
 * (the read path) AND is the value handed to `batch([...])` (the transactional
 * provision path). The real `D1PreparedStatement` satisfies this exactly.
 */
export interface GithugrPreparedStatement {
  run(): Promise<unknown>;
  first<T = unknown>(): Promise<T | null>;
}

export interface GithugrProvisionDb {
  prepare(query: string): {
    bind(...values: unknown[]): GithugrPreparedStatement;
  };
  /** Transactional multi-statement apply — all-or-nothing (D1 semantics). */
  batch(statements: GithugrPreparedStatement[]): Promise<unknown[]>;
}

/** Namespace prefix so the derivation can never collide with the DSR uuid space. */
const GITHUGR_TENANT_NS = "corelink-githugr-tenant-v1:";

/** The default global CAS region (matches `family-e2e-tier-seed.sql`). */
const DEFAULT_REGION = "wnam";
/** Free-tier monthly $-ceiling in micro-dollars ($5 tripwire — 0066 default). */
const FREE_MONTHLY_BUDGET_USD_MICROS = 5000000;
/** Free-tier runner concurrency (family-e2e free row). */
const FREE_RUNNER_MAX_CONCURRENCY = 1;

function bytesToHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Deterministic name-based (v5-shaped) UUID for a githugr Clerk subject.
 *
 * `uuidv5-shaped from SHA-256("corelink-githugr-tenant-v1:" + sub)` — the SAME
 * derivation shape as the signup-worker's `deterministicDsrId`
 * (`apps/signup-worker/src/webhooks/clerk.ts`), only the namespace prefix differs.
 * DETERMINISTIC ⇒ the same `sub` always yields the same tenant_id, so re-login is
 * idempotent (provision-or-lookup lands on the same row). Distinct subs yield
 * distinct digests ⇒ distinct tenant_ids (the isolation guarantee).
 */
export async function deriveGithugrTenantId(sub: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`${GITHUGR_TENANT_NS}${sub}`),
  );
  const b = new Uint8Array(digest).slice(0, 16);
  b[6] = ((b[6] ?? 0) & 0x0f) | 0x50; // version 5 (name-based)
  b[8] = ((b[8] ?? 0) & 0x3f) | 0x80; // RFC 4122 variant
  const h = bytesToHex(b.buffer);
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/**
 * PROVISION-OR-LOOKUP the isolated CoreLink tenant for a verified githugr subject.
 *
 * LOOKUP-FIRST (audit H3): the common case is a REPEAT login whose row-family
 * already exists, so we SELECT the `tenant_org_map` identity row FIRST; on a hit
 * we return that tenant_id immediately with ZERO writes. Only on the first-ever
 * login for this `sub` (no row) do we run the provisioning writes and then
 * read the mapping back.
 *
 * ATOMIC PROVISION (audit H5): the 5 first-login writes are issued as a SINGLE
 * transactional D1 `batch([...])` (all-or-nothing), FK order preserved (tenant
 * first), every statement INSERT OR IGNORE — so a mid-provision D1 fault cannot
 * leave a partial row-family behind. The authoritative read-back runs AFTER the
 * batch (a concurrent NEW-sub login — deterministic id + first-writer-wins on the
 * clerk_org_id PK — still converges to the same tenant_id).
 *
 * Returns the resolved tenant_id. THROWS on any D1 error (lookup, batch, OR
 * read-back) — the caller MUST treat a throw as fail-CLOSED (500) and NEVER fall
 * back to a shared tenant (falling back is the exact isolation break this
 * replaces).
 */
export async function provisionOrLookupGithugrTenant(
  db: GithugrProvisionDb,
  sub: string,
  nowMs: number = Date.now(),
): Promise<string> {
  const tenantId = await deriveGithugrTenantId(sub);

  // LOOKUP-FIRST: on a repeat login the identity row already exists — return it
  // with ZERO writes (removes the 5-write amplification on every session). A D1
  // fault here PROPAGATES (fail-CLOSED — never a silent shared-tenant fallback).
  const existing = await db
    .prepare("SELECT tenant_id FROM tenant_org_map WHERE clerk_org_id = ?1 LIMIT 1")
    .bind(sub)
    .first<{ tenant_id: string }>();
  if (existing && existing.tenant_id) {
    return existing.tenant_id;
  }

  // FIRST-EVER login for this sub: provision the full 5-family row-set as ONE
  // transactional batch (all-or-nothing) so a mid-provision D1 fault can't leave
  // a partial row-set. FK order is preserved by statement order (tenant first).
  await db.batch([
    // (1) tenant — FK target, MUST be written before any child row. Shape mirrors
    // family-e2e-tier-seed.sql (tenant_id, primary_region, created_at_ms, updated_at_ms).
    db
      .prepare(
        "INSERT OR IGNORE INTO tenant " +
          "(tenant_id, primary_region, created_at_ms, updated_at_ms) " +
          "VALUES (?1, ?2, ?3, ?3)",
      )
      .bind(tenantId, DEFAULT_REGION, nowMs),

    // (2) tier_selections — free/active. subscription_started_at_ms MUST be non-NULL
    // when state='active' (0039 CHECK subscription_started_when_active).
    db
      .prepare(
        "INSERT OR IGNORE INTO tier_selections " +
          "(tenant_id, tier, subscription_state, schema_version, correlation_id, subscription_started_at_ms) " +
          "VALUES (?1, 'free', 'active', 1, ?2, ?3)",
      )
      .bind(tenantId, `githugr-provision:${tenantId}`, nowMs),

    // (3) runners_entitlement — free plan, min concurrency (>0 CHECK, 0070).
    db
      .prepare(
        "INSERT OR IGNORE INTO runners_entitlement " +
          "(tenant_id, max_concurrency, plan, created_at_ms) VALUES (?1, ?2, 'free', ?3)",
      )
      .bind(tenantId, FREE_RUNNER_MAX_CONCURRENCY, nowMs),

    // (4) tenant_quota — non-zero monthly ceiling so the container quota gate isn't
    // a flat 402 / fail-open None (family-e2e note).
    db
      .prepare(
        "INSERT OR IGNORE INTO tenant_quota " +
          "(tenant_id, monthly_budget_usd_micros) VALUES (?1, ?2)",
      )
      .bind(tenantId, FREE_MONTHLY_BUDGET_USD_MICROS),

    // (5) tenant_org_map — the identity row keyed on the Clerk principal (`sub`);
    // clerk_org_id PRIMARY KEY ⇒ first writer wins (mirrors insertTenantOrgMap).
    db
      .prepare(
        "INSERT OR IGNORE INTO tenant_org_map " +
          "(clerk_org_id, tenant_id, created_at_ms) VALUES (?1, ?2, ?3)",
      )
      .bind(sub, tenantId, nowMs),
  ]);

  // Read back the AUTHORITATIVE mapping. This covers a row written by a CONCURRENT
  // first login of the same sub (deterministic derivation ⇒ same tenant_id, but we
  // still trust D1's row over our local derivation). A missing row here is
  // impossible after the batch above unless the write silently failed, so we treat
  // an absent read-back as an error (fail-CLOSED at the caller).
  const row = await db
    .prepare("SELECT tenant_id FROM tenant_org_map WHERE clerk_org_id = ?1 LIMIT 1")
    .bind(sub)
    .first<{ tenant_id: string }>();
  if (!row || !row.tenant_id) {
    throw new Error("githugr tenant_org_map read-back returned no row after provision");
  }
  return row.tenant_id;
}
