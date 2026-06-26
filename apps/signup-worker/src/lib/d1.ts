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
 */
export async function acceptTeamInvitation(
  db: D1Database,
  clerkUserId: string,
  emailHash: string,
): Promise<boolean> {
  const invited = await db
    .prepare(
      "SELECT tenant_id, user_id FROM team_member " +
        "WHERE email_hash = ?1 AND status = 'invited' LIMIT 1",
    )
    .bind(emailHash)
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
