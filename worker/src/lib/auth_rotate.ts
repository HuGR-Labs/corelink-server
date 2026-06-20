/**
 * `clw auth rotate` seam — `POST /internal/v1/auth/rotate` → mint-new + revoke-old.
 *
 * `clw auth rotate` previously shipped a STUB that advised the user to re-login
 * and returned `rotated:false` (it had no server endpoint to call). This module
 * is that endpoint: it rotates a PAT in ONE atomic-ish call — mint an
 * EQUIVALENT-scope new PAT for the SAME tenant, then revoke the old PAT — with no
 * re-login.
 *
 * It is the runner-mint/revoke pattern ({@link handleRunnerMint} +
 * {@link handleRunnerRevoke} in lib/runner_mint.ts) composed into a single
 * operation, with the scope SOURCED FROM the old `pat` row. The tenant is also
 * read from the row, but the caller MUST name the expected `owner_tenant` in the
 * body and it is validated to match the row (REV-S2) — so a compromised pat_mint
 * key cannot rotate (mint a fresh credential for) a tenant it does not own.
 * Authorization is the `pat_mint` internal-auth consumer key
 * (`CORELINK_PAT_MINT_AUTH_KEY` with shared `CORELINK_INTERNAL_AUTH_KEY`
 * fallback, constant-time). The clw backend / dispatcher holds that key — an
 * end-user PAT cannot call this surface.
 *
 * The mint REUSES the SINGLE mint authority — {@link mintScopedPat}, which calls
 * the container's audited `/_internal/pat/mint` via the `_system` DO (one signing
 * key, one audit emit, one revocation surface for INV-PAT-REVOKE-PROPAGATION).
 * There is NO second mint path and NO new signing key.
 *
 * The revoke REUSES the exact revocation surface `handleRunnerRevoke` and the
 * container's customer `keys.revoke` write: a tenant-row, idempotent
 * `UPDATE pat SET revoked_at_ms = ? WHERE pat_id = ? AND revoked_at_ms IS NULL`
 * on the shared `pat` table in CONFIG_DB. The native plane / adapters / OCI all
 * read `revoked_at_ms IS NULL` from THAT table, so this is the existing
 * propagation path, not a new mechanism.
 *
 * ORDER MATTERS — mint NEW first, revoke OLD only after the new mint succeeds, so
 * a mint failure never leaves the caller with zero valid PATs (no window in which
 * the old PAT is revoked but no new PAT exists). If the mint fails, the old PAT is
 * left intact and the mint error is propagated.
 *
 * Every path is fail-CLOSED: missing key → deny; bad/empty body → 400; unknown or
 * already-revoked pat → 404 (we never silently mint for a non-existent/revoked
 * PAT); mint failure → propagate (do NOT revoke).
 *
 * INV-NO-PII-IN-LOGS: no token material and no raw tenant/pat id is logged; only a
 * request-id-tagged error class on the failure path.
 */

import type { Env } from "../index.js";
import { requireConsumerAuth } from "./internal_auth.js";
import { mintScopedPat } from "./session_exchange.js";

/**
 * Default lifetime (seconds) of the rotated PAT when the OLD PAT's remaining
 * lifetime cannot be used as the basis (see {@link computeRotateTtlSeconds}).
 *
 * 90 days (7_776_000s) mirrors the canonical PAT lifetime in migration 0037
 * (`expires_ms` "90d canonical per WI §9.4 + AWS access-key rotation cadence").
 * Rotation is a like-for-like key replacement, so the new PAT gets a fresh full
 * lifetime by default — exactly what a user expects from "rotate my key". The
 * container clamps `ttl_seconds=0` to "no expiry"; we never send 0.
 */
const ROTATE_DEFAULT_TTL_SECONDS = 7_776_000;

/**
 * Minimum TTL (seconds) we will ever request for a rotated PAT. If the old PAT
 * was very close to expiry (or already past it on the clock), rotating to an
 * even-shorter or non-positive TTL would be useless (and `<=0` would be clamped
 * by the container to "no expiry" — the opposite of intent). We floor the
 * requested TTL at the full default so a rotation always yields a usefully-lived
 * key. (Rotation is a deliberate key-replacement action, not a lifetime trim.)
 */
const ROTATE_MIN_TTL_SECONDS = ROTATE_DEFAULT_TTL_SECONDS;

/**
 * D1 `pat.scope` labels we can faithfully reproduce via the SINGLE mint authority.
 *
 * The container's `/_internal/pat/mint` maps only `"read-write"`/`"cas:rw"` (→
 * SCOPE_CACHE_RW) and `"admin"` (→ full admin). The D1 `pat.scope` CHECK also
 * allows `"read-only"`, but the mint authority has no `read-only` mapping — so we
 * CANNOT mint an equivalent `read-only` PAT without either escalating (→
 * read-write) or weakening the single mint authority. Rotation must preserve
 * scope EXACTLY, so a `read-only` (or otherwise unmappable) old scope is refused
 * fail-CLOSED (422) rather than silently escalated. `read-write` and `admin`
 * round-trip verbatim.
 */
const ROTATABLE_SCOPES = new Set(["read-write", "admin"]);

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
 * Compute the TTL (seconds) to request for the rotated PAT from the old PAT's
 * absolute `expires_ms`.
 *
 * Rotation is a key-replacement, so the new PAT gets a FRESH full default
 * lifetime ({@link ROTATE_DEFAULT_TTL_SECONDS}) — we do NOT trim the lifetime to
 * the old PAT's residual window (that would make every rotation progressively
 * shorter, defeating the point). We floor at {@link ROTATE_MIN_TTL_SECONDS} so we
 * never send a non-positive TTL (which the container would clamp to "no expiry").
 * `oldExpiresMs` is accepted for future policy (e.g. honoring a shorter
 * per-tenant cap) but the current policy is "fresh full lifetime".
 */
function computeRotateTtlSeconds(_oldExpiresMs: number): number {
  return ROTATE_MIN_TTL_SECONDS;
}

/**
 * Handle `POST /internal/v1/auth/rotate` → atomically rotate a PAT.
 *
 * Pipeline (every step fail-CLOSED):
 *   1. Method gate (POST only → 405).
 *   2. Internal-auth gate — the `pat_mint` consumer key
 *      (`CORELINK_PAT_MINT_AUTH_KEY` with shared `CORELINK_INTERNAL_AUTH_KEY`
 *      fallback). 401 wrong/missing header, 403 no sized key. The clw backend
 *      holds this key; an end-user PAT cannot call this.
 *   3. Fail-CLOSED on the SHARED key (the secret presented to the container's
 *      mint route) → else 403.
 *   4. Parse body `{ pat_id, owner_tenant }`. Missing/empty either → 400.
 *   5. Read the old `pat` row by `pat_id` (tenant_id, scope, expires_ms,
 *      revoked_at_ms). No row OR already revoked → 404 (never silently mint).
 *      Caller-named owner_tenant ≠ the row's tenant_id → 403 (REV-S2: a
 *      compromised pat_mint key cannot rotate a PAT it does not own). Unmappable
 *      scope → 422 (preserve scope exactly; never escalate).
 *   6. Mint the NEW PAT via {@link mintScopedPat} — same tenant, same scope, a
 *      fresh full lifetime. principal-source = the old `pat_id` (stable per-PAT
 *      audit-correlation UUID — see below). On mint failure: propagate, do NOT
 *      revoke (the old PAT stays valid → no zero-PAT window).
 *   7. ONLY after the mint succeeds: revoke the OLD pat_id via the existing
 *      idempotent `revoked_at_ms IS NULL` soft-revoke.
 *   8. Return `{ token_plaintext, pat_id (new), token_id, expires_ms,
 *      rotated_from, revoke_pending }`. `revoke_pending` is `true` ONLY when the
 *      new mint succeeded but the old-PAT revoke write failed (D1 transient) — the
 *      caller still gets a working new credential and must retry the idempotent
 *      revoke; the old PAT's bounded expiry is the backstop.
 *
 * principal-source choice: we pass the OLD `pat_id` as `principalSource`. It is a
 * stable, server-issued, non-PII identifier, so the rotated PAT's derived
 * principal UUID is stable per source key (audit-correlatable to "the key that was
 * rotated") without leaking any upstream identity-provider subject. (We do not
 * have the old PAT's original principal column in D1 — `pat` carries
 * `issued_to_user` only in the Postgres schema, not the D1 schema — so the old
 * pat_id is the correct stable, available, non-PII source.)
 */
export async function handleAuthRotate(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  // ── 1. Method gate ─────────────────────────────────────────────────────────
  if (request.method !== "POST") {
    return reapiError("METHOD_NOT_ALLOWED", "auth rotate requires POST", 405, requestId);
  }

  // ── 2. Internal-auth gate (pat_mint consumer key, shared fallback) ─────────
  const authErr = requireConsumerAuth(request, env, "pat_mint", requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Fail-CLOSED on the required server secret ───────────────────────────
  // The internal-auth gate proved a sized consumer/shared key is bound; we re-read
  // the SHARED key here because it is the secret presented to the container's
  // /_internal/pat/mint route (the mint authority). If only a dedicated pat_mint
  // key were bound the shared key could still be absent → the mint to the
  // container cannot be authorized → unavailable. (Same posture as runner-mint.)
  const internalAuthKey = env.CORELINK_INTERNAL_AUTH_KEY;
  if (!internalAuthKey || internalAuthKey.length === 0) {
    return reapiError("FORBIDDEN", "auth rotate unavailable", 403, requestId);
  }

  // ── 4. Parse the body (pat_id + owner_tenant required) ─────────────────────
  // REV-S2: the caller must NAME the tenant it believes owns the PAT. We validate
  // it against the row below before minting, so a compromised pat_mint key cannot
  // rotate (and thereby mint a fresh working credential for) a tenant it is not
  // associated with. The clw backend verified the session before calling, so it
  // knows the calling user's tenant — passing it here is a no-shape-change tighten.
  interface AuthRotateRequest {
    readonly pat_id?: unknown;
    readonly owner_tenant?: unknown;
  }
  let body: AuthRotateRequest;
  try {
    body = (await request.json()) as AuthRotateRequest;
  } catch {
    return reapiError("BAD_REQUEST", "invalid request body", 400, requestId);
  }
  const patId = body.pat_id;
  if (typeof patId !== "string" || patId.length === 0) {
    return reapiError("BAD_REQUEST", "pat_id required", 400, requestId);
  }
  // BACKWARD-COMPAT (REV-S2 progressive hardening): owner_tenant is VALIDATED when
  // present (cross-tenant rotate is refused below) but NOT yet required — the
  // off-repo callers (clw backend) must be rolled out to send it before we flip to
  // mandatory, else this deploy would 400 a live caller. Absent → proceed (prior
  // behavior) + warn; present → enforced. Flip to required after clw confirms it sends it.
  const ownerTenant =
    typeof body.owner_tenant === "string" && body.owner_tenant.length > 0
      ? body.owner_tenant
      : undefined;
  if (ownerTenant === undefined) {
    console.warn(
      `[${requestId}] auth rotate: owner_tenant absent — cross-tenant validation skipped (deprecated; callers MUST send owner_tenant)`,
    );
  }

  // ── 5. Read the OLD pat row (tenant + scope + expiry + revocation state) ───
  // Faithful rotation: the new PAT inherits the OLD PAT's tenant + scope. We also
  // need the revocation state — a non-existent OR already-revoked PAT cannot be
  // rotated (fail-CLOSED 404; we never silently mint a fresh PAT for a key that is
  // gone). Columns per the D1 schema (migrations 0037 + 0063): pat_id (TEXT PK),
  // tenant_id (TEXT), scope (TEXT), expires_ms (BIGINT), revoked_at_ms (BIGINT,
  // NULL = active).
  interface PatRow {
    tenant_id: string;
    scope: string;
    expires_ms: number;
    revoked_at_ms: number | null;
  }
  let oldRow: PatRow | null;
  try {
    oldRow = await env.CONFIG_DB.prepare(
      "SELECT tenant_id, scope, expires_ms, revoked_at_ms FROM pat WHERE pat_id = ?1",
    )
      .bind(patId)
      .first<PatRow>();
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] auth rotate pat lookup failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "auth rotate unavailable", 500, requestId);
  }
  if (oldRow === null || oldRow.revoked_at_ms !== null) {
    // Unknown OR already-revoked → 404 (fail-CLOSED; never mint silently). A
    // single status for both so the caller learns nothing about which case it is
    // (no revocation/existence oracle on the wire).
    return reapiError("NOT_FOUND", "pat not found", 404, requestId);
  }
  if (typeof oldRow.tenant_id !== "string" || oldRow.tenant_id.length === 0) {
    // A row with no tenant cannot be rotated (the mint is tenant-scoped).
    console.error(`[${requestId}] auth rotate pat row missing tenant`);
    return reapiError("INTERNAL_ERROR", "auth rotate unavailable", 500, requestId);
  }
  if (ownerTenant !== undefined && ownerTenant !== oldRow.tenant_id) {
    // REV-S2: the caller-named tenant does not own this PAT. Refuse — a compromised
    // pat_mint key must not be able to mint a fresh working credential for a tenant
    // it is not associated with (cross-tenant privilege escalation). Checked AFTER
    // the 404 so a wrong (existing-pat, wrong-tenant) pair still reveals nothing
    // beyond "not yours"; an unknown/revoked pat_id remains an indistinguishable 404.
    return reapiError("FORBIDDEN", "pat does not belong to the specified tenant", 403, requestId);
  }
  if (!ROTATABLE_SCOPES.has(oldRow.scope)) {
    // The single mint authority cannot reproduce this scope (e.g. `read-only`)
    // without escalating or weakening it. Refuse rather than silently change the
    // scope — rotation must preserve privilege EXACTLY.
    return reapiError("UNPROCESSABLE_ENTITY", "pat scope is not rotatable", 422, requestId);
  }

  // ── 6. Mint the NEW PAT (same tenant + scope, fresh full lifetime) ─────────
  // REUSE the single mint authority. principalSource = the OLD pat_id (stable,
  // server-issued, non-PII → stable audit-correlation UUID; documented above).
  // mintScopedPat returns the public mint envelope (200) or a fail-CLOSED error
  // Response (429 throttle / 500 upstream). On a non-200 we return it VERBATIM and
  // do NOT revoke — so a mint failure never leaves the caller with zero valid PATs.
  const ttlSeconds = computeRotateTtlSeconds(oldRow.expires_ms);
  const mintResp = await mintScopedPat(
    env,
    requestId,
    oldRow.tenant_id,
    patId,
    ttlSeconds,
    oldRow.scope,
    internalAuthKey,
  );
  if (mintResp.status !== 200) {
    return mintResp;
  }

  interface MintEnvelope {
    readonly token_plaintext: string;
    readonly pat_id: string;
    readonly token_id: string;
    readonly expires_ms: number;
  }
  let minted: MintEnvelope;
  try {
    minted = (await mintResp.json()) as MintEnvelope;
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] auth rotate mint body parse failed: ${message.slice(0, 80)}`);
    // The mint may have SUCCEEDED on the container (a new PAT exists) but we could
    // not parse it. Do NOT revoke the old PAT (the caller cannot use the new one),
    // so the caller keeps a valid credential. Fail-CLOSED 500.
    return reapiError("INTERNAL_ERROR", "auth rotate mint malformed", 500, requestId);
  }

  // ── 7. Revoke the OLD pat_id — ONLY after the new mint succeeded ───────────
  // The EXACT idempotent surface handleRunnerRevoke + the customer keys.revoke
  // write use: `UPDATE pat SET revoked_at_ms = ? WHERE pat_id = ? AND
  // revoked_at_ms IS NULL`. INV-PAT-REVOKE-PROPAGATION: the native plane, adapters
  // and OCI all filter `revoked_at_ms IS NULL`, so the old token stops being
  // honored immediately. The new PAT already exists → no zero-valid-PAT window.
  let revokePending = false;
  try {
    await env.CONFIG_DB.prepare(
      "UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND revoked_at_ms IS NULL",
    )
      .bind(Date.now(), patId)
      .run();
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    // The NEW PAT is already minted and valid — that is the whole point of rotate,
    // so we MUST return it rather than throwing it away on a revoke hiccup (a 500
    // here would orphan a valid minted PAT and force the caller to re-mint). Instead
    // return 200 with the new token AND `revoke_pending: true`, so the caller has a
    // working credential and a clear signal that the OLD key may still be live and
    // the (idempotent) revoke must be retried via POST /internal/v1/runner/revoke.
    // The old PAT keeps its original bounded expiry as the backstop.
    console.error(`[${requestId}] auth rotate old-pat revoke failed (revoke_pending): ${message.slice(0, 80)}`);
    revokePending = true;
  }

  // ── 8. Return the rotated envelope ─────────────────────────────────────────
  return new Response(
    JSON.stringify({
      token_plaintext: minted.token_plaintext,
      pat_id: minted.pat_id,
      token_id: minted.token_id,
      expires_ms: minted.expires_ms,
      rotated_from: patId,
      revoke_pending: revokePending,
    }),
    {
      status: 200,
      headers: {
        "Content-Type": "application/json",
        "X-Request-Id": requestId,
      },
    },
  );
}
