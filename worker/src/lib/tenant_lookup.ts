/**
 * #3 — server-side tenant lookup: `POST /internal/v1/auth/tenant/lookup`.
 *
 * githugr (the public "window" over the same shared Clerk instance) authenticates
 * a user via the shared Clerk JWT and already holds the verified `sub`
 * (clerk_user_id). To resolve WHICH CoreLink tenant that user owns — without a
 * second authentication or a GitHub round-trip — its www server calls this
 * endpoint server-to-server with the `sub`.
 *
 * Contract (ratified — githugr GREENLIGHT 2026-06-15):
 *   - **Key on `sub`** (clerk_user_id). No `github_id` column — `sub` is the
 *     cleaner key and both sides trust the same Clerk JWT.
 *   - **Fail-CLOSED 404** when no tenant maps to the subject.
 *   - Returns `{ tenant_id, role, tier, tenant_state }`. `role` is `"owner"`:
 *     the clerk_user_id is the tenant's owner (1:1 at provision, migration 0056
 *     UNIQUE INDEX idx_tenant_clerk_user_id). githugr persists `tenant_id` as
 *     the repo's `owner_tenant` (its #2 visibility gate).
 *
 * Email fallback (specced, but NOT implementable — documented N/A): the D1
 * `tenant.email_hash` column stores `SHA-256(clerk_user_id)` — a privacy
 * surrogate, NOT a hash of the raw email (no raw email is ever stored;
 * GDPR-pseudonymized at signup). So there is no real-email → tenant mapping to
 * resolve against. This is moot in practice: githugr always holds `sub` from
 * the shared JWT, so the primary key is always available.
 *
 * Security:
 *   - Internal-auth gated ({@link requireInternalAuth}) — only a trusted backend
 *     (githugr www) holding `CORELINK_INTERNAL_AUTH_KEY` may call it.
 *   - Parameterized D1 query (no injection surface).
 *   - INV-NO-PII-IN-LOGS: the raw `sub` is never logged; only a request-id-tagged
 *     error class on the D1-fault path.
 */

import type { Env } from "../index.js";
import { requireInternalAuth } from "./internal_auth.js";

/** REAPI error envelope builder — local mirror (avoids the index.ts ⇄ lib cycle). */
function reapiError(error: string, message: string, status: number, requestId: string): Response {
  const body = { error, message, request_id: requestId };
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

/** Inbound request body. Only `sub` is honored (see module doc re: email). */
interface TenantLookupRequest {
  readonly sub?: unknown;
  readonly email?: unknown;
}

/** D1 row shape for the lookup query. */
interface TenantRow {
  readonly tenant_id: string;
  readonly tier: string | null;
  readonly tenant_state: string | null;
}

/**
 * Public response of `POST /internal/v1/auth/tenant/lookup`.
 * `role` is always `"owner"` — the clerk_user_id is the tenant owner.
 */
interface TenantLookupResponse {
  readonly tenant_id: string;
  readonly role: "owner";
  readonly tier: string;
  readonly tenant_state: string | null;
}

/**
 * Handle `POST /internal/v1/auth/tenant/lookup`.
 *
 * Fail-CLOSED on every error path: method (405), auth (401/403), bad body (400),
 * D1 fault (500), no tenant (404). The caller (index.ts) applies CORS.
 */
export async function handleTenantLookup(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  // ── 1. Method gate ─────────────────────────────────────────────────────────
  if (request.method !== "POST") {
    return reapiError("METHOD_NOT_ALLOWED", "tenant lookup requires POST", 405, requestId);
  }

  // ── 2. Internal-auth gate (server-to-server only) ──────────────────────────
  const authErr = requireInternalAuth(request, env, requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Parse the body ──────────────────────────────────────────────────────
  let body: TenantLookupRequest;
  try {
    body = (await request.json()) as TenantLookupRequest;
  } catch {
    return reapiError("BAD_REQUEST", "invalid request body", 400, requestId);
  }
  const sub = body.sub;
  if (typeof sub !== "string" || sub.length === 0) {
    // `sub` (clerk_user_id) is the ratified key. The `email` fallback is N/A
    // (see module doc — email_hash is a clerk-id surrogate, not a raw-email
    // hash), so an email-only request cannot resolve and is a 400.
    return reapiError("BAD_REQUEST", "sub (clerk_user_id) required", 400, requestId);
  }

  // ── 4. Resolve the tenant by clerk_user_id (parameterized) ─────────────────
  let row: TenantRow | null;
  try {
    row = await env.CONFIG_DB.prepare(
      "SELECT tenant_id, tier, tenant_state FROM tenant WHERE clerk_user_id = ?1 LIMIT 1",
    )
      .bind(sub)
      .first<TenantRow>();
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] tenant lookup D1 error: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "tenant lookup error", 500, requestId);
  }

  // ── 5. Fail-CLOSED 404 when no tenant maps to the subject ──────────────────
  if (!row || !row.tenant_id) {
    return reapiError("NOT_FOUND", "no tenant for subject", 404, requestId);
  }

  // ── 6. Success ─────────────────────────────────────────────────────────────
  const out: TenantLookupResponse = {
    tenant_id: row.tenant_id,
    role: "owner",
    // `tier` defaults to the most-restrictive plan if the column is unset,
    // mirroring getTierForTenant's hard default.
    tier: row.tier && row.tier.length > 0 ? row.tier : "free",
    tenant_state: row.tenant_state,
  };
  return new Response(JSON.stringify(out), {
    status: 200,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}
