/**
 * D-9 — per-job runner PAT mint + revoke (corelink-runners seam).
 *
 * corelink-runners needs to give a DISPOSABLE runner a tenant-scoped cache
 * credential, but the runner has NO Clerk session and NO bootstrap secret. The
 * trusted **dispatcher** (a backend holding the internal-auth key) mints a
 * short-TTL, tenant-scoped PAT on the runner's behalf and env-injects it into
 * the runner. On job teardown the dispatcher revokes it (the TTL is the
 * backstop). This module is the Worker-side surface for both operations.
 *
 * It is `handleTokenExchange` MINUS the Clerk-session step, PLUS a
 * runners-entitlement check — the dispatcher is the authenticated party
 * (internal-auth), not an end-user, so there is no session to verify; instead
 * the tenant must be entitled to Runners (a SEPARATE authorization axis from the
 * cache tier — `runners_entitlement`, migration 0070).
 *
 * The mint REUSES the SINGLE mint authority — {@link mintScopedPat}, which calls
 * the container's audited `/_internal/pat/mint` via the `_system` DO (one
 * signing key, one audit emit, one revocation surface for
 * INV-PAT-REVOKE-PROPAGATION). There is NO second mint path and NO new signing
 * key. The per-principal mint throttle inside `mintScopedPat` rate-limits
 * runaway runner-mint automatically.
 *
 * Revocation REUSES the same revocation surface the container's customer
 * `keys.revoke` writes: a tenant-row, idempotent `UPDATE pat SET revoked_at_ms`
 * on the shared `pat` table in CONFIG_DB. The native plane / adapters / OCI all
 * read `revoked_at_ms IS NULL` from THAT table, so this is the existing
 * propagation path (INV-PAT-REVOKE-PROPAGATION), not a new mechanism — only the
 * gate differs (internal-auth, since the dispatcher holds no customer PAT).
 *
 * Every path is fail-CLOSED: missing secret/binding → deny; bad body → 400; not
 * entitled → 403; never fail-open.
 *
 * INV-NO-PII-IN-LOGS: no token material and no raw tenant/job id is logged; only
 * a request-id-tagged error class on the failure path.
 */

import type { Env } from "../index.js";
import { requireConsumerAuth } from "./internal_auth.js";
import { mintScopedPat } from "./session_exchange.js";

/**
 * Lifetime of a runner-minted PAT, in seconds (5400s = 90 minutes).
 *
 * Job hard-cap + margin: a runner job is bounded by the dispatcher's own job
 * timeout, and the dispatcher explicitly revokes the PAT on teardown — the TTL
 * is the BACKSTOP for the case where teardown never runs (crashed dispatcher,
 * lost runner). 90 minutes comfortably covers a long build while keeping the
 * blast radius of a leaked, un-revoked runner token bounded. The container
 * clamps `ttl_seconds=0` to "no expiry"; we never send 0.
 */
const RUNNER_PAT_TTL_SECONDS = 5400;

/**
 * Scope labels the runner-mint endpoint will mint. Mirrors
 * `TOKEN_EXCHANGE_ALLOWED_SCOPES`: only the cache read/write data-plane scope is
 * allowed (`cas:rw` / its `read-write` alias). `admin`/`owner` is REFUSED — a
 * disposable runner must never carry an admin bit (least privilege). An
 * unrecognized/admin scope → 400.
 */
const RUNNER_MINT_ALLOWED_SCOPES = new Set(["cas:rw", "read-write"]);

/** Default scope minted when the caller omits `scope`. */
const RUNNER_MINT_DEFAULT_SCOPE = "cas:rw";

/**
 * REAPI error envelope builder — local mirror (avoids the index.ts ⇄ lib import
 * cycle, same as the sibling lib modules). Shape is load-bearing:
 * `{ error, message, request_id }` + `X-Request-Id` header.
 */
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

/**
 * Handle `POST /internal/v1/runner/mint` → mint a per-job, short-TTL,
 * tenant-scoped PAT for a disposable runner.
 *
 * Pipeline (every step fail-CLOSED):
 *   1. Method gate (POST only → 405).
 *   2. Internal-auth gate — the `pat_mint` consumer key
 *      (`CORELINK_PAT_MINT_AUTH_KEY`) with fallback to the shared
 *      `CORELINK_INTERNAL_AUTH_KEY` (401 wrong/missing header, 403 no sized key).
 *   3. Secrets: a properly sized internal-auth key must be bound to AUTHORIZE
 *      the mint to the container (the mint is server-to-server). The runner-mint
 *      surface needs NO Clerk secret (no session is verified).
 *   4. Parse body `{ owner_tenant, job_id, scope? }`. Missing/empty
 *      owner_tenant or job_id → 400; admin/owner/unknown scope → 400.
 *   5. Runners-entitlement check: `runners_entitlement WHERE tenant_id =
 *      owner_tenant`. No row → 403 (not entitled to Runners — the runner-axis
 *      authorization, separate from cache tier).
 *   6. Mint via the SINGLE authority {@link mintScopedPat}, passing `job_id` as
 *      the principal-source string (SHA-256 → stable per-job principal UUID for
 *      audit correlation) and {@link RUNNER_PAT_TTL_SECONDS}.
 *   7. Return the standard mint envelope `{ token_plaintext, pat_id, token_id,
 *      expires_ms }`.
 */
export async function handleRunnerMint(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  // ── 1. Method gate ─────────────────────────────────────────────────────────
  if (request.method !== "POST") {
    return reapiError("METHOD_NOT_ALLOWED", "runner mint requires POST", 405, requestId);
  }

  // ── 2. Internal-auth gate (pat_mint consumer key, shared fallback) ─────────
  const authErr = requireConsumerAuth(request, env, "pat_mint", requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Fail-CLOSED on the required server secret ───────────────────────────
  // The internal-auth gate above already proved a sized consumer/shared key is
  // bound; we re-read the SHARED key here because that is the secret presented to
  // the container's /_internal/pat/mint route (the mint authority). If only a
  // dedicated pat_mint key were bound the shared key could still be absent, in
  // which case the mint to the container cannot be authorized → unavailable.
  const internalAuthKey = env.CORELINK_INTERNAL_AUTH_KEY;
  if (!internalAuthKey || internalAuthKey.length === 0) {
    return reapiError("FORBIDDEN", "runner mint unavailable", 403, requestId);
  }

  // ── 4. Parse the body (owner_tenant + job_id required; scope optional) ─────
  interface RunnerMintRequest {
    readonly owner_tenant?: unknown;
    readonly job_id?: unknown;
    readonly scope?: unknown;
  }
  let body: RunnerMintRequest;
  try {
    body = (await request.json()) as RunnerMintRequest;
  } catch {
    return reapiError("BAD_REQUEST", "invalid request body", 400, requestId);
  }
  const ownerTenant = body.owner_tenant;
  if (typeof ownerTenant !== "string" || ownerTenant.length === 0) {
    return reapiError("BAD_REQUEST", "owner_tenant required", 400, requestId);
  }
  const jobId = body.job_id;
  if (typeof jobId !== "string" || jobId.length === 0) {
    return reapiError("BAD_REQUEST", "job_id required", 400, requestId);
  }
  let scope = RUNNER_MINT_DEFAULT_SCOPE;
  if (typeof body.scope === "string" && body.scope.length > 0) {
    if (!RUNNER_MINT_ALLOWED_SCOPES.has(body.scope)) {
      // Refuse unknown / admin / owner scope (least privilege).
      return reapiError("BAD_REQUEST", "unsupported scope", 400, requestId);
    }
    scope = body.scope;
  }

  // ── 5. Runners-entitlement check (the runner-axis authorization) ───────────
  // A single keyed lookup on the dedicated entitlement table (migration 0070).
  // NO row ⇒ the tenant is not entitled to Runners ⇒ 403 (fail-CLOSED). This is
  // SEPARATE from the cache tier — a cache-only tenant gets no runner credential.
  interface EntitlementRow {
    tenant_id: string;
  }
  let entRow: EntitlementRow | null;
  try {
    entRow = await env.CONFIG_DB.prepare(
      "SELECT tenant_id FROM runners_entitlement WHERE tenant_id = ?1",
    )
      .bind(ownerTenant)
      .first<EntitlementRow>();
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] runner mint entitlement lookup failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "runner mint unavailable", 500, requestId);
  }
  if (entRow === null) {
    return reapiError("FORBIDDEN", "tenant not entitled to runners", 403, requestId);
  }

  // ── 6+7. Mint via the SINGLE authority (job_id → per-job principal UUID) ────
  return mintScopedPat(
    env,
    requestId,
    ownerTenant,
    jobId,
    RUNNER_PAT_TTL_SECONDS,
    scope,
    internalAuthKey,
  );
}

/**
 * Handle `POST /internal/v1/runner/revoke` → revoke a runner-minted PAT by id.
 *
 * Used by the dispatcher on job teardown (the PAT TTL is the backstop). Reuses
 * the existing revocation surface — the same idempotent, tenant-row
 * `UPDATE pat SET revoked_at_ms` write the container's customer `keys.revoke`
 * performs on the shared `pat` table in CONFIG_DB — so the native plane,
 * adapters and OCI (which all filter `revoked_at_ms IS NULL`) immediately stop
 * honoring the token (INV-PAT-REVOKE-PROPAGATION). No new revocation mechanism.
 *
 * Authorized identically to mint: the `pat_mint` consumer key with shared
 * fallback. The dispatcher holds no customer PAT, so the customer revoke route
 * (which is PAT-gated) is not usable here — internal-auth is the gate.
 *
 * Pipeline (fail-CLOSED): method (405), internal-auth (401/403), bad body (400),
 * missing pat_id (400), D1 error (500). Revoking an already-revoked or unknown
 * pat_id is idempotent (200) — the `revoked_at_ms IS NULL` guard makes a
 * re-revoke a no-op, and the dispatcher needs teardown to be safely retryable.
 */
export async function handleRunnerRevoke(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  // ── 1. Method gate ─────────────────────────────────────────────────────────
  if (request.method !== "POST") {
    return reapiError("METHOD_NOT_ALLOWED", "runner revoke requires POST", 405, requestId);
  }

  // ── 2. Internal-auth gate (pat_mint consumer key, shared fallback) ─────────
  const authErr = requireConsumerAuth(request, env, "pat_mint", requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Parse the body (pat_id required) ────────────────────────────────────
  interface RunnerRevokeRequest {
    readonly pat_id?: unknown;
  }
  let body: RunnerRevokeRequest;
  try {
    body = (await request.json()) as RunnerRevokeRequest;
  } catch {
    return reapiError("BAD_REQUEST", "invalid request body", 400, requestId);
  }
  const patId = body.pat_id;
  if (typeof patId !== "string" || patId.length === 0) {
    return reapiError("BAD_REQUEST", "pat_id required", 400, requestId);
  }

  // ── 4. Revoke via the EXISTING surface (idempotent tenant-agnostic by id) ──
  // The container's customer revoke is tenant-scoped (the customer can only
  // revoke their own PATs). The dispatcher minted this PAT for a tenant it is
  // already entitled to (mint required a runners_entitlement row), and revoke is
  // keyed on the opaque, server-issued pat_id, so we revoke by pat_id with the
  // `revoked_at_ms IS NULL` idempotency guard — exactly the container's UPDATE
  // minus the tenant predicate (which the customer route adds for its PAT-scoped
  // caller; the internal dispatcher is trusted to name a pat_id).
  try {
    await env.CONFIG_DB.prepare(
      "UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND revoked_at_ms IS NULL",
    )
      .bind(Date.now(), patId)
      .run();
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] runner revoke update failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "runner revoke failed", 500, requestId);
  }

  return new Response(JSON.stringify({ pat_id: patId, revoked: true }), {
    status: 200,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}
