/**
 * Seam C — session-token → PAT exchange (hugit-P2 WP-C).
 *
 * `POST /v1/session/exchange` exchanges a short-lived HuGR/Clerk identity
 * SESSION (presented as a Bearer JWT, server-side only — never client-exposed
 * per ADR-0002) for a tenant-scoped, short-lived CoreLink **PAT**. The forge
 * derives `author_kind` from the authenticated principal returned here rather
 * than trusting an unauthenticated `--author-kind` CLI flag (the SOTA-audit S3
 * gap). CoreLink does the AUTHENTICATION; hugit does the author_kind matrix.
 *
 * This module is a thin, fail-CLOSED orchestrator. It does NOT mint PATs
 * itself: it REUSES the container's audited `/_internal/pat/mint` route
 * (`crates/corelink-container/src/routes/internal_pat.rs`) via the `_system`
 * Durable Object, exactly as the `internal` route arm in index.ts does. The
 * single mint authority (one audit emit, one signing key, one revocation
 * surface for INV-PAT-REVOKE-PROPAGATION) is preserved — there is no second
 * mint path.
 *
 * Pipeline (every step fail-CLOSED — a missing binding or bad token denies,
 * never opens a gate):
 *   1. Require both server secrets (CLERK_SECRET_KEY to verify the session,
 *      CORELINK_INTERNAL_AUTH_KEY to authorize to the container) → else 403.
 *   2. Verify the Clerk session JWT at the edge + resolve the CoreLink tenant
 *      via the SHARED pipeline (lib/clerk_auth.ts: M1 azp re-assert + M2 issuer
 *      pin + tenant lookup). Bad/expired/no token → 401; no tenant → 403.
 *   3. Mint a short-lived `cas:rw` PAT for the verified principal by calling
 *      `/_internal/pat/mint` on the `_system` DO with the SERVER-trusted
 *      internal-auth header (the client can never supply it — it never leaves
 *      the backend).
 *   4. Return `{ token_plaintext, pat_id, token_id, principal, tenant,
 *      expires_ms }`. The Argon2id `hash` from the mint response is NEVER
 *      forwarded to the caller (it is a server-side D1 artifact).
 *
 * INV-NO-PII-IN-LOGS: no token material, no raw tenant/principal is logged
 * here; only a request-id-tagged error class on the upstream-failure path.
 */

import type { DurableObjectNamespace } from "@cloudflare/workers-types";
import type { Env } from "../index.js";
import { verifyClerkSessionAndResolveTenant } from "./clerk_auth.js";

/**
 * Default lifetime of the exchanged PAT, in seconds. Short-lived by design:
 * the session-exchange capability is a per-forge-operation credential, not a
 * long-lived API key. 1 hour balances forge-operation duration against the
 * blast radius of a leaked token (revocation still propagates via the D1 pat
 * row per INV-PAT-REVOKE-PROPAGATION). The container clamps `ttl_seconds=0`
 * to "no expiry"; we never send 0.
 */
const EXCHANGE_PAT_TTL_SECONDS = 3600;

/**
 * Scope label minted for the exchanged PAT. `cas:rw` grants cache read/write
 * (the forge's data-plane surface) WITHOUT any admin bit — least privilege.
 * Maps to `SCOPE_CACHE_RW` in the container's internal_pat scope table.
 */
const EXCHANGE_PAT_SCOPE = "cas:rw";

/**
 * REAPI error envelope builder — local mirror of index.ts `reapiError`
 * (module-private there) and lib/clerk_auth.ts's copy. Shape is load-bearing:
 * `{ error, message, request_id }` + X-Request-Id header. Kept local to avoid
 * a runtime import cycle (index.ts ⇄ lib/session_exchange.ts).
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
 * Shape of the container `/_internal/pat/mint` 200 response
 * (`MintResponse` in internal_pat.rs). `hash` is present on the wire but is a
 * server-side D1 artifact and is deliberately NOT re-exported to the caller.
 */
interface ContainerMintResponse {
  readonly token_plaintext: string;
  readonly pat_id: string;
  readonly token_id: string;
  readonly expires_ms: number;
  readonly hash?: string;
}

/**
 * Public response of `POST /v1/session/exchange`. Carries the minted PAT plus
 * the authenticated principal + tenant + expiry. `author_kind_claims` is NOT
 * computed here — per the WP-C contract, CoreLink authenticates the principal
 * and hugit's forge derives the author_kind matrix. (`hash` is never leaked.)
 */
interface SessionExchangeResponse {
  readonly token_plaintext: string;
  readonly pat_id: string;
  readonly token_id: string;
  readonly principal: string;
  readonly tenant: string;
  readonly expires_ms: number;
}

/**
 * Map an opaque Clerk user id (`user_...`, NOT a UUID) to a deterministic
 * RFC-4122 UUID so the container's `MintRequest.principal_id` (serde `Uuid`)
 * accepts it AND the same Clerk user always yields the same principal UUID
 * (per-principal PAT correlation in the audit trail).
 *
 * Derivation: SHA-256(clerk_user_id) → first 16 bytes → stamp the version
 * nibble (v8, custom) + the RFC-4122 variant bits → 8-4-4-4-12 hex. This is a
 * one-way, stable mapping (NOT reversible to the Clerk id), consistent with the
 * INV-NO-PII-IN-LOGS posture.
 */
async function clerkUserIdToPrincipalUuid(clerkUserId: string): Promise<string> {
  const enc = new TextEncoder();
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", enc.encode(clerkUserId)));
  // Take the first 16 bytes and stamp version (v8) + variant per RFC-4122.
  const b = digest.subarray(0, 16);
  const byte6 = b[6] ?? 0;
  const byte8 = b[8] ?? 0;
  b[6] = (byte6 & 0x0f) | 0x80; // version nibble → 0x8 (custom / v8)
  b[8] = (byte8 & 0x3f) | 0x80; // variant → 0b10xx_xxxx
  const hex = Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
  return (
    hex.slice(0, 8) +
    "-" +
    hex.slice(8, 12) +
    "-" +
    hex.slice(12, 16) +
    "-" +
    hex.slice(16, 20) +
    "-" +
    hex.slice(20, 32)
  );
}

/**
 * Handle `POST /v1/session/exchange`.
 *
 * Returns the public {@link SessionExchangeResponse} on success, or a
 * fail-CLOSED `reapiError` Response on any auth / binding / upstream failure.
 * The caller (index.ts) applies CORS to the returned Response.
 *
 * @param request   the inbound request (carries the Clerk session Bearer JWT)
 * @param env       Worker env bindings (secrets + CORELINK_SERVER DO namespace)
 * @param requestId the per-request id (propagated to the container + responses)
 */
export async function handleSessionExchange(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  // ── 1. Method gate ─────────────────────────────────────────────────────────
  if (request.method !== "POST") {
    return reapiError("METHOD_NOT_ALLOWED", "session exchange requires POST", 405, requestId);
  }

  // ── 2. Fail-CLOSED on required server secrets ──────────────────────────────
  // CLERK_SECRET_KEY is needed to VERIFY the session; CORELINK_INTERNAL_AUTH_KEY
  // is needed to AUTHORIZE the mint to the container. A missing binding denies
  // (never an open gate) — same posture as the onboarding arm.
  const internalAuthKey = env.CORELINK_INTERNAL_AUTH_KEY;
  const clerkSecretKey = env.CLERK_SECRET_KEY;
  if (
    !internalAuthKey ||
    internalAuthKey.length === 0 ||
    !clerkSecretKey ||
    clerkSecretKey.length === 0
  ) {
    return reapiError("FORBIDDEN", "session exchange unavailable", 403, requestId);
  }

  // ── 3. Verify the Clerk session + resolve the tenant (shared pipeline) ──────
  // lib/clerk_auth.ts carries the full hardened flow: bearer extraction (401),
  // verifyToken with the shared azp allowlist (M1 post-verify re-assert),
  // issuer exact-pin / shape-check (M2), claims.sub required, and the tenant
  // lookup by clerk_user_id (no row → 403; D1 error → 500). The secret guard
  // above already asserted CLERK_SECRET_KEY presence, so the helper's own
  // fail-closed branch never fires here.
  const clerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
  if (!clerkAuth.ok) {
    return clerkAuth.response;
  }
  const { tenantId, clerkUserId } = clerkAuth;

  // ── 4. Mint a short-lived cas:rw PAT via the container (REUSE the one mint) ─
  // Route to the _system DO which fronts the container, then call the audited
  // /_internal/pat/mint route with the SERVER-trusted internal-auth header.
  // The browser/client can never supply that header — it never leaves the
  // backend (least-privilege: identical to the `internal` route arm).
  const principalId = await clerkUserIdToPrincipalUuid(clerkUserId);
  const mintBody = JSON.stringify({
    tenant_id: tenantId,
    principal_id: principalId,
    scopes: EXCHANGE_PAT_SCOPE,
    ttl_seconds: EXCHANGE_PAT_TTL_SECONDS,
  });

  const namespace: DurableObjectNamespace = env.CORELINK_SERVER;
  const systemDoId = namespace.idFromName("_system");
  const systemStub = namespace.get(systemDoId);

  // Build a FRESH request to the container's internal mint route. We do NOT
  // forward the inbound (session-bearing) request — the container authenticates
  // via internal-auth, and the Clerk JWT must never travel past the edge.
  const mintRequest = new Request("https://do/_internal/pat/mint", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-request-id": requestId,
      "x-corelink-route-kind": "internal",
      "x-corelink-tenant-id": "_system",
      "x-corelink-token-prefix": "internal",
      "x-corelink-internal-auth": internalAuthKey,
    },
    body: mintBody,
  });

  let mintResp: Response;
  try {
    mintResp = await systemStub.fetch(mintRequest);
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] session exchange mint fetch failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "session exchange upstream error", 500, requestId);
  }

  if (mintResp.status !== 200) {
    // The mint route denied (e.g. 401 auth, 503 no signing key, 400 bad body).
    // Do NOT surface the container's status/body verbatim — collapse to a
    // single fail-CLOSED 500 so an unauthenticated caller learns nothing about
    // the internal mint surface (the session was already verified, so a non-200
    // here is a server-side fault, not a client error).
    console.error(`[${requestId}] session exchange mint returned ${mintResp.status}`);
    return reapiError("INTERNAL_ERROR", "session exchange mint failed", 500, requestId);
  }

  let minted: ContainerMintResponse;
  try {
    minted = (await mintResp.json()) as ContainerMintResponse;
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] session exchange mint body parse failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "session exchange mint malformed", 500, requestId);
  }

  if (
    typeof minted.token_plaintext !== "string" ||
    minted.token_plaintext.length === 0 ||
    typeof minted.pat_id !== "string" ||
    typeof minted.token_id !== "string" ||
    typeof minted.expires_ms !== "number"
  ) {
    console.error(`[${requestId}] session exchange mint body missing fields`);
    return reapiError("INTERNAL_ERROR", "session exchange mint malformed", 500, requestId);
  }

  // ── 5. Return the public exchange response (NEVER the Argon2id hash) ────────
  const out: SessionExchangeResponse = {
    token_plaintext: minted.token_plaintext,
    pat_id: minted.pat_id,
    token_id: minted.token_id,
    principal: clerkUserId,
    tenant: tenantId,
    expires_ms: minted.expires_ms,
  };
  return new Response(JSON.stringify(out), {
    status: 200,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}
