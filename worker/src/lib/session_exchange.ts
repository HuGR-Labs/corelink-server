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

import type { D1Database, DurableObjectNamespace } from "@cloudflare/workers-types";
import type { Env } from "../index.js";
import { verifyClerkSessionAndResolveTenant } from "./clerk_auth.js";
import { requireInternalAuth } from "./internal_auth.js";

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
 * Lifetime of the PAT minted by `POST /internal/v1/auth/token-exchange` (#1,
 * githugr authz). ~300s by design (RFC 8693 short-TTL token exchange): the
 * githugr window caches the org-scoped PAT per session until this TTL, so the
 * blast radius of a leaked exchange token is bounded to a few minutes. The
 * container clamps `ttl_seconds=0` to "no expiry"; we never send 0.
 */
const TOKEN_EXCHANGE_PAT_TTL_SECONDS = 300;

/**
 * Scope label minted for the exchanged PAT. `cas:rw` grants cache read/write
 * (the forge's data-plane surface) WITHOUT any admin bit — least privilege.
 * Maps to `SCOPE_CACHE_RW` in the container's internal_pat scope table.
 */
const EXCHANGE_PAT_SCOPE = "cas:rw";

/**
 * Scope labels the token-exchange endpoint will mint. The container's
 * internal_pat scope table accepts `"cas:rw"` / `"read-write"` (→ SCOPE_CACHE_RW)
 * and `"admin"` — token-exchange deliberately refuses `"admin"` (least
 * privilege: a session-derived, short-TTL token must never carry admin bits).
 * An unrecognized scope → 400.
 */
const TOKEN_EXCHANGE_ALLOWED_SCOPES = new Set(["cas:rw", "read-write"]);

/**
 * Mint-throttle window length, in ms (60s). The per-principal cap is enforced
 * over this fixed window (see {@link checkMintThrottle}).
 */
const MINT_THROTTLE_WINDOW_MS = 60_000;

/**
 * Max PAT mints allowed per principal per {@link MINT_THROTTLE_WINDOW_MS}
 * window. Legitimate forge use exchanges a session for ONE short-lived
 * (1-hour) PAT and reuses it; a handful of mints/min covers retries + multiple
 * devices while bounding loop-mint abuse. Over this ⇒ 429 (fail-CLOSED).
 */
const MINT_THROTTLE_MAX_PER_WINDOW = 10;

/**
 * Module-level in-memory burst backstop for the mint throttle (F20 fix).
 *
 * During a D1 outage the durable throttle is unavailable. This in-process
 * counter provides a LOCAL fallback: it caps the number of mints per principal
 * within a single Worker isolate cold-start lifetime. It does NOT replace the
 * durable D1 throttle — it is an additional defense-in-depth layer that
 * prevents CPU exhaustion from a loop-mint during a D1 outage.
 *
 * Invariant: this map is module-scoped (per isolate), so it naturally resets
 * on isolate recycle, which is why the durable D1 counter is the primary gate.
 * The local cap is intentionally tighter than the D1 window cap
 * (MAX_IN_MEMORY_BURST < MINT_THROTTLE_MAX_PER_WINDOW) to bound Argon2id
 * CPU cost per isolate instance during an outage.
 *
 * INV-NO-PII-IN-LOGS: the key is the opaque principalId (SHA-256-derived
 * UUID), NOT the raw Clerk user id.
 */
const _inMemoryMintCounts = new Map<string, number>();

/**
 * Max mints per principal allowed in the in-memory backstop before a 429 is
 * returned (F20 fix). This is a per-isolate, per-cold-start ceiling — not a
 * persistent window — so it is deliberately conservative.
 */
const MAX_IN_MEMORY_BURST = 5;

/**
 * Hard ceiling on the number of DISTINCT principals tracked in
 * {@link _inMemoryMintCounts} at once (CAA-360 #16 fix).
 *
 * The map is module-scoped (per isolate) and previously grew one entry per
 * distinct principal for the isolate's entire lifetime — an unbounded-growth /
 * slow-leak hazard on a long-lived isolate under principal churn. We now bound
 * it with an LRU policy: each access moves the principal to the most-recent end
 * (delete-then-set on a JS Map, which preserves insertion order), and once the
 * map exceeds this ceiling the least-recently-used entry is evicted. The
 * durable D1 counter remains the primary throttle, so an evicted principal only
 * loses its tighter in-isolate burst memory (worst case: its per-isolate burst
 * counter resets) — never a correctness hole in the persistent gate.
 */
const MAX_IN_MEMORY_MINT_ENTRIES = 50_000;

/**
 * Canonicalize a mint-request scope label to a D1 `pat.scope` value.
 *
 * The D1 `pat` table has `CHECK (scope IN ('read-write','read-only','admin'))`
 * (migration 0037). Callers of {@link mintScopedPat} pass the container's mint
 * scope LABELS (`'cas:rw'` for the data plane) OR an already-canonical D1 value
 * (auth_rotate replays the OLD row's `pat.scope`, which is already canonical).
 * Both must be normalised to the CHECK-legal value before the INSERT, or the row
 * silently fails the CHECK and the token never authenticates.
 *
 * Mapping (the ONLY accepted inputs — least privilege, no wildcard pass-through):
 *   - `'cas:rw'` / `'read-write'` → `'read-write'` (SCOPE_CACHE_RW data plane)
 *   - `'admin'`                   → `'admin'`
 *   - `'read-only'`               → `'read-only'`
 *
 * An unmappable scope returns `null` → the caller fails CLOSED with a 500 rather
 * than writing a CHECK-violating (or privilege-escalating) row.
 */
function canonicalizePatScope(scope: string): "read-write" | "read-only" | "admin" | null {
  switch (scope) {
    case "cas:rw":
    case "read-write":
      return "read-write";
    case "admin":
      return "admin";
    case "read-only":
      return "read-only";
    default:
      return null;
  }
}

/**
 * Per-principal fixed-window mint throttle for `/v1/session/exchange`.
 *
 * A still-valid Clerk session could otherwise loop-mint unbounded PATs (each
 * a durable D1 row + an Argon2id hash on the shared container). This caps mints
 * per derived principal UUID (`principalId` — the opaque value, NOT the raw
 * Clerk id) to {@link MINT_THROTTLE_MAX_PER_WINDOW} per
 * {@link MINT_THROTTLE_WINDOW_MS}, mirroring the per-IP cap the pilot-signup
 * path enforces.
 *
 * The window roll + increment is ONE atomic D1 statement
 * (`INSERT … ON CONFLICT … DO UPDATE … RETURNING count`), so concurrent mints
 * cannot race past the cap. Returns a 429 `Response` when the cap is exceeded
 * (fail-CLOSED on the limit); returns `null` to proceed.
 *
 * On a D1 transport error the durable throttle is bypassed. To prevent
 * Argon2id CPU exhaustion via loop-minting during an outage, a module-level
 * in-memory backstop ({@link _inMemoryMintCounts}) caps mints per principal
 * per isolate lifetime to {@link MAX_IN_MEMORY_BURST} (F20 fix — fail-LOUD:
 * the D1 error is logged AND the in-memory backstop fires a 429 if the burst
 * ceiling is reached within this isolate).
 */
async function checkMintThrottle(
  db: D1Database,
  principalId: string,
  requestId: string,
): Promise<Response | null> {
  const now = Date.now();
  interface CountRow {
    count: number;
  }
  let row: CountRow | null = null;
  let d1Failed = false;
  try {
    row = await db
      .prepare(
        "INSERT INTO session_exchange_throttle (clerk_sub, window_start_ms, count) \
         VALUES (?1, ?2, 1) \
         ON CONFLICT(clerk_sub) DO UPDATE SET \
           count = CASE \
             WHEN session_exchange_throttle.window_start_ms + ?3 <= ?2 THEN 1 \
             ELSE session_exchange_throttle.count + 1 END, \
           window_start_ms = CASE \
             WHEN session_exchange_throttle.window_start_ms + ?3 <= ?2 THEN ?2 \
             ELSE session_exchange_throttle.window_start_ms END \
         RETURNING count",
      )
      .bind(principalId, now, MINT_THROTTLE_WINDOW_MS)
      .first<CountRow>();
  } catch {
    // D1 unavailable — log the outage (fail-LOUD) and fall through to the
    // in-memory backstop (F20 fix). The durable D1 throttle is the primary
    // gate; the in-memory backstop provides a defense-in-depth ceiling for
    // this isolate's lifetime to bound Argon2id CPU cost during an outage.
    console.error(
      `[${requestId}] session exchange throttle store error; applying in-memory backstop`,
    );
    d1Failed = true;
  }

  // ── Primary gate: durable D1 counter ────────────────────────────────────────
  if (!d1Failed && row !== null && row.count > MINT_THROTTLE_MAX_PER_WINDOW) {
    return reapiError(
      "TOO_MANY_REQUESTS",
      "session exchange mint rate exceeded; retry shortly",
      429,
      requestId,
    );
  }

  // ── Backstop gate (F20): in-memory per-isolate burst ceiling ────────────────
  // Applied on EVERY request (both D1-healthy and D1-outage paths) so the
  // in-process ceiling is always current. It is intentionally a tighter cap
  // than the durable window to bound within-isolate Argon2id CPU cost.
  // LRU touch: delete-then-set moves this principal to the most-recently-used
  // end of the insertion-ordered Map, so eviction below removes the oldest
  // (least-recently-used) principal rather than an actively-minting one.
  const inMemCount = (_inMemoryMintCounts.get(principalId) ?? 0) + 1;
  _inMemoryMintCounts.delete(principalId);
  _inMemoryMintCounts.set(principalId, inMemCount);
  // Bound the map (CAA-360 #16): we add at most one entry per call, so evicting
  // a single LRU entry whenever we exceed the ceiling keeps size <= the cap.
  if (_inMemoryMintCounts.size > MAX_IN_MEMORY_MINT_ENTRIES) {
    const lru = _inMemoryMintCounts.keys().next().value;
    if (lru !== undefined) {
      _inMemoryMintCounts.delete(lru);
    }
  }
  if (inMemCount > MAX_IN_MEMORY_BURST) {
    console.error(
      `[${requestId}] session exchange in-memory backstop fired (d1_failed=${String(d1Failed)})`,
    );
    return reapiError(
      "TOO_MANY_REQUESTS",
      "session exchange mint rate exceeded; retry shortly",
      429,
      requestId,
    );
  }

  return null;
}

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
  // CLERK_SECRET_KEY is needed to VERIFY the session; the mint auth key is needed
  // to AUTHORIZE the mint to the container. A missing binding denies (never an
  // open gate) — same posture as the onboarding arm. Prefer the DEDICATED
  // CORELINK_PAT_MINT_AUTH_KEY — the container's /_internal/pat/mint gate REQUIRES
  // it with NO shared fallback (DD-HIGH, WP1) — falling back to the shared key
  // only when the dedicated is unset (additive; once provisioned the shared no
  // longer authorizes mint).
  const internalAuthKey =
    env.CORELINK_PAT_MINT_AUTH_KEY ?? env.CORELINK_INTERNAL_AUTH_KEY;
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
  const clerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId, {
    allowGithugrIssuer: true,
  });
  if (!clerkAuth.ok) {
    return clerkAuth.response;
  }
  const { tenantId, clerkUserId } = clerkAuth;

  // RBAC: a viewer seat cannot mint a write-capable PAT (least privilege).
  const mintScope = clerkAuth.role === "viewer" ? "read-only" : EXCHANGE_PAT_SCOPE;

  // ── 4. Mint a short-lived PAT (REUSE the one container mint), scoped by role ─
  return mintScopedPat(
    env,
    requestId,
    tenantId,
    clerkUserId,
    EXCHANGE_PAT_TTL_SECONDS,
    mintScope,
    internalAuthKey,
  );
}

/**
 * Mint a tenant-scoped, short-lived PAT for an ALREADY-VERIFIED principal, by
 * REUSING the container's audited `/_internal/pat/mint` via the `_system` DO.
 *
 * Shared by {@link handleSessionExchange} (hugit Seam C, 3600s),
 * {@link handleTokenExchange} (githugr #1, 300s), and the D-9 runner-mint
 * (`handleRunnerMint` in lib/runner_mint.ts, job-bounded TTL). The single mint
 * authority (one signing key, one audit emit, one revocation surface) is
 * preserved — there is no second mint path. Every failure path is fail-CLOSED.
 *
 * The caller MUST have authorized the request (verified the session + resolved
 * `tenantId` + matched the audience; or, for runner-mint, internal-auth +
 * runners-entitlement) BEFORE calling this — it performs no authentication of
 * its own beyond the per-principal mint throttle.
 *
 * `extraFields` (optional) is a small, JSON-serialisable bag merged verbatim
 * into the 200 response body AFTER the standard mint envelope — the D-9
 * runner-mint caller uses it to thread its `max_concurrency` (the derived
 * tenant's runner ceiling) through this single mint authority without a second
 * response shape. It NEVER carries token material (that stays in the standard
 * fields) and is absent for the session/token-exchange callers.
 *
 * `runnerJobAcKey` (optional, cf-multitenant WP5a) marks the minted PAT as a
 * NARROWED runner-job PAT: when provided, the persisted `pat` row also writes
 * `runner_job_ac_key = <value>` (`"*"` = deny-DELETE only; a BLAKE3 hex = also
 * exact-key AC restricted). The container (WP5b) reads it back via the Worker's
 * forwarded server-trust headers and ENFORCES the narrowing. When OMITTED (the
 * session/token-exchange/rotate callers) the column stays NULL and those PATs
 * are UNCHANGED — a normal PAT with no narrowing.
 *
 * @returns the public {@link SessionExchangeResponse} (200) or a fail-CLOSED
 *   `reapiError` Response (429 throttle, 500 upstream/malformed).
 */
export async function mintScopedPat(
  env: Env,
  requestId: string,
  tenantId: string,
  principalSource: string,
  ttlSeconds: number,
  scope: string,
  internalAuthKey: string,
  extraFields?: Readonly<Record<string, string | number | boolean>>,
  runnerJobAcKey?: string,
): Promise<Response> {
  // Route to the _system DO which fronts the container, then call the audited
  // /_internal/pat/mint route with the SERVER-trusted internal-auth header.
  // The browser/client can never supply that header — it never leaves the
  // backend (least-privilege: identical to the `internal` route arm).
  //
  // `principalSource` is the stable string from which the per-principal UUID is
  // SHA-256-derived (Clerk user id for the session/token-exchange callers; the
  // runner `job_id` for the D-9 runner-mint caller — giving per-job audit
  // correlation). The hashing is identical regardless of source.
  const principalId = await clerkUserIdToPrincipalUuid(principalSource);

  // ── Canonicalize the scope to a D1-legal value BEFORE any expensive work ────
  // The minted PAT only authenticates if its `pat` row is persisted, and the
  // `pat.scope` CHECK accepts only ('read-write','read-only','admin'). Reject an
  // unmappable scope NOW (fail-CLOSED 500) rather than after the container mint —
  // a token we could never persist must never be returned, and we avoid burning
  // an Argon2id mint on a request we will reject anyway.
  const canonicalScope = canonicalizePatScope(scope);
  if (canonicalScope === null) {
    console.error(`[${requestId}] mint scoped pat: unmappable scope (cannot persist)`);
    return reapiError("INTERNAL_ERROR", "session exchange mint failed", 500, requestId);
  }

  // ── Per-principal mint throttle (fail-CLOSED 429) ──────────────────────────
  // The session is verified, but a still-valid session must not loop-mint
  // unbounded PATs. Cap mints per derived principal UUID per fixed window
  // BEFORE the (expensive) container Argon2id mint + D1 insert.
  const throttled = await checkMintThrottle(env.CONFIG_DB, principalId, requestId);
  if (throttled !== null) {
    return throttled;
  }

  const mintBody = JSON.stringify({
    tenant_id: tenantId,
    principal_id: principalId,
    scopes: scope,
    ttl_seconds: ttlSeconds,
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

  // ── Persist the `pat` row (fail-CLOSED) — the load-bearing fix ──────────────
  // The container's /_internal/pat/mint COMPUTES the token + its Argon2id hash
  // but does NOT write the D1 `pat` row — by contract the CALLER persists it (the
  // signup-worker + customer plane do; this shared mint chokepoint previously did
  // NOT, so every minted exchange/runner token 401'd at extractAuth because no row
  // existed). We INSERT the row HERE, using the returned `hash`, BEFORE handing the
  // token back. On ANY persistence failure we fail CLOSED (500, no token): a token
  // that cannot authenticate is strictly worse than an honest error.
  //
  // Schema (migration 0037 + 0054): pat(pat_id, tenant_id, pat_hash, scope,
  // expires_ms, token_id, shown_once_token UNIQUE, shown_once_consumed, created_ms).
  //   - pat_hash      = the container's Argon2id `hash` (the auth digest extractAuth
  //                     does NOT re-derive here, but the row's existence + token_id
  //                     lookup is what extractAuth needs; hash is the canonical
  //                     stored secret artifact). Missing/empty hash on a 200 →
  //                     fail-CLOSED (never write a hash-less row).
  //   - scope         = the canonical D1 value (CHECK-legal; computed above).
  //   - shown_once_token = the per-pat-unique `token_id` (UNIQUE column). The token
  //                     is handed to the caller directly (not via the one-time
  //                     dashboard reveal), so shown_once_consumed=1 and we use the
  //                     non-secret, naturally-unique token_id rather than the raw
  //                     plaintext (which must never be stored).
  //   - tenant_id     = FK to tenant(tenant_id); an absent tenant → INSERT throws →
  //                     fail-CLOSED (no working token handed back).
  if (typeof minted.hash !== "string" || minted.hash.length === 0) {
    // A 200 with no Argon2id hash means we cannot persist an authenticatable row.
    console.error(`[${requestId}] session exchange mint 200 missing hash; cannot persist`);
    return reapiError("INTERNAL_ERROR", "session exchange mint malformed", 500, requestId);
  }
  try {
    // WP5a: when the caller marks this a NARROWED runner-job PAT, ALSO write
    // `runner_job_ac_key`. When omitted the column is not written at all → it
    // stays NULL (a normal PAT), so the session/token-exchange/rotate callers'
    // persisted row is UNCHANGED. The two INSERT variants differ only by that
    // one column+bind; everything else is byte-identical.
    const stmt =
      runnerJobAcKey === undefined
        ? env.CONFIG_DB.prepare(
            "INSERT INTO pat " +
              "(pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, " +
              " shown_once_token, shown_once_consumed, created_ms) " +
              "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
          ).bind(
            minted.pat_id,
            tenantId,
            minted.hash,
            canonicalScope,
            minted.expires_ms,
            minted.token_id,
            // shown_once_token: the unique, non-secret token_id (NEVER the plaintext).
            minted.token_id,
            Date.now(),
          )
        : env.CONFIG_DB.prepare(
            "INSERT INTO pat " +
              "(pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, " +
              " shown_once_token, shown_once_consumed, created_ms, runner_job_ac_key) " +
              "VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9)",
          ).bind(
            minted.pat_id,
            tenantId,
            minted.hash,
            canonicalScope,
            minted.expires_ms,
            minted.token_id,
            // shown_once_token: the unique, non-secret token_id (NEVER the plaintext).
            minted.token_id,
            Date.now(),
            runnerJobAcKey,
          );
    const insertResult = await stmt.run();
    // A plain INSERT (not OR IGNORE) surfaces FK / UNIQUE / CHECK violations as a
    // throw (the primary signal, caught below). As a belt-and-braces guard against
    // a silent no-op, fail CLOSED if D1 explicitly reports zero rows changed — a
    // token whose row was not written cannot authenticate. `meta.changes` is
    // present on the real D1 driver; when absent (e.g. a partial test double) we
    // trust the no-throw success path.
    const changes = (insertResult as { meta?: { changes?: number } } | undefined)?.meta?.changes;
    if (typeof changes === "number" && changes < 1) {
      console.error(`[${requestId}] session exchange pat persist wrote no row`);
      return reapiError("INTERNAL_ERROR", "session exchange mint failed", 500, requestId);
    }
  } catch (err: unknown) {
    // FK (tenant absent) / UNIQUE (token_id|shown_once_token|pat_hash collision) /
    // CHECK (scope) / transport — all fail CLOSED; do NOT return the token.
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] session exchange pat persist failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "session exchange mint failed", 500, requestId);
  }

  // ── Return the public exchange response (NEVER the Argon2id hash) ───────────
  const out: SessionExchangeResponse = {
    token_plaintext: minted.token_plaintext,
    pat_id: minted.pat_id,
    token_id: minted.token_id,
    // F-01: return the opaque, derived principal UUID — NEVER the raw Clerk
    // user id (`user_xxx`). `principalId` is the SHA-256-derived UUID already
    // sent to the container as `principal_id`; surfacing the raw Clerk id would
    // leak the upstream identity-provider subject to the client.
    principal: principalId,
    tenant: tenantId,
    expires_ms: minted.expires_ms,
  };
  // Merge any caller-supplied extra fields (e.g. runner-mint's max_concurrency)
  // AFTER the standard envelope. The spread is over a small, non-secret bag.
  const outBody = extraFields === undefined ? out : { ...out, ...extraFields };
  return new Response(JSON.stringify(outBody), {
    status: 200,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

/**
 * #1 — RFC 8693 token exchange: `POST /internal/v1/auth/token-exchange`.
 *
 * githugr's window holds a per-request Bearer = the user's Clerk session JWT and
 * needs a SHORT-TTL CoreLink PAT bound to a SPECIFIC tenant (the repo's
 * `owner_tenant`). This endpoint exchanges (session JWT + `audience`) for that
 * PAT and — critically — **403s when the session's tenant ≠ audience**. That
 * 403 IS githugr's cross-tenant-WRITE rejection (their audit CRITICAL #1): the
 * engine then validates the returned PAT via `/internal/v1/auth/introspect` and
 * a forged/borrowed session for tenant A can never obtain a PAT for tenant B.
 *
 * Contract (ratified — githugr GREENLIGHT 2026-06-15):
 *   - Bearer = Clerk `__session` JWT (verified by the same shared pipeline as
 *     every other Clerk path: azp re-assert + issuer pin + tenant resolve).
 *   - Body `{ audience: "<owner_tenant uuid>", scope?: "cas:rw" }`.
 *   - **403 on `session.tenant ≠ audience`.**
 *   - ~300s tenant-scoped PAT (least privilege; admin scope refused).
 *
 * Defense-in-depth: ALSO internal-auth gated — only githugr's trusted backend
 * (holding `CORELINK_INTERNAL_AUTH_KEY`) may call it, AND it must present a
 * valid user session AND the audience must match. Three independent checks.
 *
 * Fail-CLOSED throughout: internal-auth (401/403), method (405), secrets (403),
 * session (401/403), audience required+match (400/403), bad scope (400),
 * throttle (429), upstream (500).
 */
export async function handleTokenExchange(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  // ── 1. Method gate ─────────────────────────────────────────────────────────
  if (request.method !== "POST") {
    return reapiError("METHOD_NOT_ALLOWED", "token exchange requires POST", 405, requestId);
  }

  // ── 2. Internal-auth gate (only githugr's backend may call) ────────────────
  const authErr = requireInternalAuth(request, env, requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Fail-CLOSED on required server secrets ──────────────────────────────
  // Prefer the DEDICATED mint key (container /_internal/pat/mint requires it, no
  // shared fallback — DD-HIGH); fall back to shared only when unset (additive).
  const internalAuthKey =
    env.CORELINK_PAT_MINT_AUTH_KEY ?? env.CORELINK_INTERNAL_AUTH_KEY;
  const clerkSecretKey = env.CLERK_SECRET_KEY;
  if (
    !internalAuthKey ||
    internalAuthKey.length === 0 ||
    !clerkSecretKey ||
    clerkSecretKey.length === 0
  ) {
    return reapiError("FORBIDDEN", "token exchange unavailable", 403, requestId);
  }

  // ── 4. Parse the body (audience required; scope optional) ──────────────────
  interface TokenExchangeRequest {
    readonly audience?: unknown;
    readonly scope?: unknown;
  }
  let body: TokenExchangeRequest;
  try {
    body = (await request.json()) as TokenExchangeRequest;
  } catch {
    return reapiError("BAD_REQUEST", "invalid request body", 400, requestId);
  }
  const audience = body.audience;
  if (typeof audience !== "string" || audience.length === 0) {
    // audience is the WHOLE point of this endpoint (the cross-tenant defense);
    // a token exchange with no target tenant is rejected.
    return reapiError("BAD_REQUEST", "audience (target tenant) required", 400, requestId);
  }
  let scope = EXCHANGE_PAT_SCOPE;
  if (typeof body.scope === "string" && body.scope.length > 0) {
    if (!TOKEN_EXCHANGE_ALLOWED_SCOPES.has(body.scope)) {
      // Refuse unknown / admin scope (least privilege).
      return reapiError("BAD_REQUEST", "unsupported scope", 400, requestId);
    }
    scope = body.scope;
  }

  // ── 5. Verify the Clerk session + resolve the tenant (shared pipeline) ─────
  const clerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId, {
    allowGithugrIssuer: true,
  });
  if (!clerkAuth.ok) {
    return clerkAuth.response;
  }
  const { tenantId, clerkUserId } = clerkAuth;

  // RBAC: a viewer seat cannot mint a write-capable PAT — cap DOWN to read-only
  // regardless of the requested scope (least privilege; owner/admin/member keep
  // the requested/default read-write).
  if (clerkAuth.role === "viewer") {
    scope = "read-only";
  }

  // ── 6. CROSS-TENANT REJECTION — the githugr CRITICAL ───────────────────────
  // The session resolves to `tenantId`; the caller asked for `audience`. If they
  // differ, the session does NOT own the target tenant → 403. This is the exact
  // rejection githugr's engine relies on to block cross-tenant writes.
  if (audience !== tenantId) {
    return reapiError("FORBIDDEN", "session tenant does not match audience", 403, requestId);
  }

  // ── 7. Mint the short-TTL tenant-scoped PAT (REUSE the one container mint) ──
  // Track-B: propagate the Clerk step-up freshness signal so the engine's erase
  // step-up gate can require a recent reauth. Pass `fva_minutes` = the verified
  // `fva[0]` from the session JWT (minutes since first-factor); the engine derives
  // `fresh_auth = fva_minutes <= threshold`. OMITTED when the session carries no
  // well-formed `fva` (fail-CLOSED — the engine treats an absent field as NOT
  // fresh, i.e. today's behaviour, so the engine side is safe to ship first).
  const freshness =
    clerkAuth.fvaMinutes === undefined ? undefined : { fva_minutes: clerkAuth.fvaMinutes };
  return mintScopedPat(
    env,
    requestId,
    tenantId,
    clerkUserId,
    TOKEN_EXCHANGE_PAT_TTL_SECONDS,
    scope,
    internalAuthKey,
    freshness,
  );
}
