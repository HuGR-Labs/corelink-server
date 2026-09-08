/** B-073: tenant-bound, verified-email team invitation redemption. */

import type { D1Database, D1PreparedStatement } from "@cloudflare/workers-types";

export const TEAM_INVITATION_TTL_MS = 14 * 24 * 60 * 60 * 1000;

export interface InvitationAcceptance {
  tenantId: string;
  role: string;
}

interface ClerkEmailAddress {
  id?: unknown;
  email_address?: unknown;
  verification?: { status?: unknown } | null;
}

/** Fetch and fail closed unless Clerk marks the primary address verified. */
export async function verifiedPrimaryEmail(
  userId: string,
  secretKey: string | undefined,
  fetcher: typeof fetch = fetch,
): Promise<string | null> {
  if (!secretKey || !userId) return null;
  const response = await fetcher(`https://api.clerk.com/v1/users/${encodeURIComponent(userId)}`, {
    headers: { Authorization: `Bearer ${secretKey}` },
  });
  if (!response.ok) return null;
  const payload: unknown = await response.json();
  if (!payload || typeof payload !== "object") return null;
  const record = payload as { primary_email_address_id?: unknown; email_addresses?: unknown };
  if (typeof record.primary_email_address_id !== "string" || !Array.isArray(record.email_addresses)) return null;
  const primary = record.email_addresses.find((entry: unknown): entry is ClerkEmailAddress => {
    if (!entry || typeof entry !== "object") return false;
    return (entry as ClerkEmailAddress).id === record.primary_email_address_id;
  });
  if (!primary || typeof primary.email_address !== "string" || primary.verification?.status !== "verified") {
    return null;
  }
  return primary.email_address;
}

interface InvitationRow {
  tenant_id: string;
  user_id: string;
  role: string;
}

function isToken(token: string): boolean {
  // The container emits exactly 32 random bytes as lower-case hex. Do not
  // accept alternate encodings or oversized input (bounded fail-closed).
  return /^[0-9a-f]{64}$/.test(token);
}

async function sha256Hex(value: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/** Cross-runtime twin of the container's normalized email pseudonym. */
export async function emailHashCandidates(email: string, salt?: string): Promise<string[]> {
  const normalized = new TextEncoder().encode(email.trim().toLowerCase());
  const legacy = await crypto.subtle.digest("SHA-256", normalized);
  let current: ArrayBuffer = legacy;
  if (salt) {
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(salt),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    current = await crypto.subtle.sign("HMAC", key, normalized);
  }
  const toHex = (bytes: ArrayBuffer): string =>
    Array.from(new Uint8Array(bytes), (byte) => byte.toString(16).padStart(2, "0")).join("");
  const first = toHex(current);
  const second = toHex(legacy);
  return first === second ? [first] : [first, second];
}

/**
 * Redeem one invitation. The caller must have already authenticated a Clerk
 * session and verified its primary email address. No tenant supplied by the
 * client participates in selection: the tenant is read from the token-bound
 * row, preventing cross-tenant substitution.
 *
 * Audit insertion and the conditional status transition are one D1 batch. A
 * second redemption therefore changes zero rows and creates no second audit
 * event. Legacy rows without a token digest are deliberately ineligible.
 */
export async function redeemTeamInvitation(
  db: D1Database,
  params: {
    clerkUserId: string;
    invitationToken: string;
    emailHashCandidates: string[];
    nowMs: number;
  },
): Promise<InvitationAcceptance | null> {
  if (!params.clerkUserId || !isToken(params.invitationToken) || !Number.isFinite(params.nowMs)) {
    return null;
  }
  const candidates = [...new Set(params.emailHashCandidates.filter((value) => /^[0-9a-f]{64}$/.test(value)))];
  if (candidates.length === 0 || candidates.length > 2) return null;
  const c0 = candidates[0];
  const c1 = candidates[1] ?? c0;
  const tokenHash = await sha256Hex(params.invitationToken);
  const cutoff = params.nowMs - TEAM_INVITATION_TTL_MS;
  const row = await db.prepare(
    "SELECT tenant_id, user_id, role FROM team_member " +
      "WHERE invitation_token_hash = ?1 AND email_hash IN (?2, ?3) " +
      "AND status = 'invited' AND invited_at_ms > ?4 AND invited_at_ms <= ?5 LIMIT 1",
  ).bind(tokenHash, c0, c1, cutoff, params.nowMs).first<InvitationRow>();
  if (!row) return null;

  const audit: D1PreparedStatement = db.prepare(
    "INSERT INTO customer_audit_events " +
      "(tenant_id, event_type, actor, target, ts_ms, detail) " +
      "SELECT tenant_id, 'team.accepted', ?1, user_id, ?2, ?3 FROM team_member " +
      "WHERE tenant_id = ?4 AND user_id = ?5 AND invitation_token_hash = ?6 " +
      "AND status = 'invited' AND invited_at_ms > ?7 AND invited_at_ms <= ?8",
  ).bind(
    params.clerkUserId,
    params.nowMs,
    `Accepted team invitation with role ${row.role}`,
    row.tenant_id,
    row.user_id,
    tokenHash,
    cutoff,
    params.nowMs,
  );
  const update: D1PreparedStatement = db.prepare(
    "UPDATE team_member SET status = 'active', joined_at_ms = ?, user_id = ? " +
      "WHERE tenant_id = ? AND user_id = ? AND invitation_token_hash = ? " +
      "AND status = 'invited' AND invited_at_ms > ? AND invited_at_ms <= ?",
  ).bind(
    params.nowMs,
    params.clerkUserId,
    row.tenant_id,
    row.user_id,
    tokenHash,
    cutoff,
    params.nowMs,
  );
  const results = await db.batch([audit, update]);
  const auditChanges = results[0]?.meta?.changes ?? 0;
  const updateChanges = results[1]?.meta?.changes ?? 0;
  if (auditChanges !== 1 || updateChanges !== 1) return null;
  return { tenantId: row.tenant_id, role: row.role };
}
