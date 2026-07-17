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
import {
  mintScopedPat,
  MintGrant,
  checkMintThrottle,
  deriveTenantCeilingThrottleKey,
} from "./session_exchange.js";
import { blake3Hex } from "./blake3.js";

/**
 * Domain-separation prefix for the exact-AC-key narrowing of a runner-job PAT
 * (cf-multitenant WP5a). When a runner mint carries an `ac_output_name`, the
 * narrowing value written to `pat.runner_job_ac_key` is
 * `blake3(RUNNER_AC_KEY_PREFIX + ac_output_name)` (hex). This MUST match the
 * key the container derives for the output workspace's AC entry (WP5b + clw).
 */
const RUNNER_AC_KEY_PREFIX = "clw/ref/runner/v1/";

/**
 * Sentinel narrowing value = "deny-DELETE only, no exact-key restriction". This
 * is what EVERY runner mint gets when no `ac_output_name` is supplied (the
 * launch path): the output-workspace name is not available at mint time today,
 * so the PAT is narrowed to at least deny-DELETE. A `"*"` in
 * `pat.runner_job_ac_key` (and forwarded as `x-corelink-ac-key-allow: *`) tells
 * the container "narrowed, but no key restriction".
 */
const RUNNER_AC_KEY_DENY_DELETE_ONLY = "*";

/**
 * Lifetime of a runner-minted PAT, in seconds (5400s = 90 minutes).
 *
 * Job hard-cap + margin: a runner job is bounded by the dispatcher's own job
 * timeout, and the dispatcher explicitly revokes the PAT on teardown — the TTL
 * is the BACKSTOP for the case where teardown never runs (crashed dispatcher,
 * lost runner). 90 minutes comfortably covers a long build while keeping the
 * blast radius of a leaked, un-revoked runner token bounded. The container
 * clamps `ttl_seconds=0` to "no expiry"; we never send 0.
 *
 * This is now the DEFAULT + HARD CAP: a caller may supply a shorter
 * `ttl_seconds` (the dispatcher sends the lease's remaining time so the PAT
 * EXPIRES WITH THE LEASE — see `handleRunnerMint`). We only ever clamp the
 * request DOWN to this cap; a caller can never extend past 90 min.
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
 * M22(b) per-tenant mint-ceiling scaling factor.
 *
 * The per-tenant runner mint ceiling is `max(max_concurrency * K, FLOOR)` over the
 * shared throttle window. Keyed off the tenant's runner ceiling so it GROWS with
 * entitlement — a bigger plan legitimately fans out more concurrent jobs, each
 * needing its own PAT — while a small K keeps a compromised runner_mint key or a
 * mint-storm bounded per tenant. This composes WITH (does not replace) the
 * per-job throttle inside {@link mintScopedPat}: a storm across many DISTINCT
 * job_ids (each under its own per-job cap) is still caught by this per-tenant gate.
 */
const RUNNER_TENANT_CEILING_K = 2;

/**
 * M22(b) per-tenant mint-ceiling FLOOR — the minimum ceiling regardless of
 * `max_concurrency`, so a tenant with a tiny (or 1) concurrency still tolerates
 * normal retry/fan-out bursts without a false 429. Deliberately small so the
 * ceiling stays a real bound.
 */
const RUNNER_TENANT_CEILING_FLOOR = 8;

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
 * Resolve the tenant of an ACQUIRING PAT by introspecting it through the
 * container (the sole PAT auth authority) — the fabricd / native-repo tenant
 * source when the mint carries no `installation_id` (frozen 2026-07-08).
 *
 * The container's `/internal/v1/auth/introspect` runs the FULL {@link
 * PatVerifier} check (HMAC fast-reject → D1 row → Argon2id), so a caller cannot
 * name a tenant: it can only prove possession of a VALID PAT whose D1 row names
 * the tenant. This is strictly stronger than the installation-map path (which
 * only needs the shared mint key + a known `installation_id`) — minting for a
 * tenant here requires a valid PAT for that tenant.
 *
 * Returns the `tenant_id` on a valid PAT, else `null` — the caller fails CLOSED
 * (uniform 403). `null` covers: introspect key unbound, upstream/DO error, a
 * non-200 (e.g. 503 store-down), a malformed body, or `valid:false` (which
 * carries NO `tenant_id` — no oracle). The PAT is NEVER logged.
 */
async function resolveTenantFromAcquiringPat(
  env: Env,
  requestId: string,
  patToken: string,
): Promise<string | null> {
  const introspectKey = env.FABRIC_INTROSPECT_AUTH_KEY;
  if (typeof introspectKey !== "string" || introspectKey.length === 0) {
    console.error(
      `[${requestId}] runner mint: FABRIC_INTROSPECT_AUTH_KEY unbound — cannot resolve acquiring-PAT tenant`,
    );
    return null;
  }
  const namespace = env.CORELINK_SERVER;
  const stub = namespace.get(namespace.idFromName("_system"));
  // Fresh server-to-server introspect (never forward the inbound request). The
  // container introspect gate is the sole authority; we present the FABRIC key.
  const introspectRequest = new Request("https://do/internal/v1/auth/introspect", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-request-id": requestId,
      "x-corelink-route-kind": "fabric_introspect",
      "x-corelink-tenant-id": "_system",
      "x-corelink-internal-auth": introspectKey,
    },
    body: JSON.stringify({ token: patToken }),
  });
  let resp: Response;
  try {
    resp = await stub.fetch(introspectRequest);
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] runner mint introspect fetch failed: ${message.slice(0, 80)}`);
    return null;
  }
  if (resp.status !== 200) {
    // 503 (store/clock unavailable) or any non-200 → fail-CLOSED (never serve a
    // tenant we could not authoritatively resolve).
    return null;
  }
  let parsed: { valid?: unknown; tenant_id?: unknown };
  try {
    parsed = (await resp.json()) as { valid?: unknown; tenant_id?: unknown };
  } catch {
    return null;
  }
  if (
    parsed.valid !== true ||
    typeof parsed.tenant_id !== "string" ||
    parsed.tenant_id.length === 0
  ) {
    return null;
  }
  return parsed.tenant_id;
}

/**
 * Handle `POST /internal/v1/runner/mint` → mint a per-job, short-TTL,
 * tenant-scoped PAT for a disposable runner.
 *
 * Pipeline (every step fail-CLOSED):
 *   1. Method gate (POST only → 405).
 *   2. Internal-auth gate — the `runner_mint` consumer key
 *      (`CORELINK_RUNNER_MINT_AUTH_KEY`) with fallback to the shared
 *      `CORELINK_INTERNAL_AUTH_KEY` (401 wrong/missing header, 403 no sized key).
 *   3. Secrets: a properly sized internal-auth key must be bound to AUTHORIZE
 *      the mint to the container (the mint is server-to-server). The runner-mint
 *      surface needs NO Clerk secret (no session is verified).
 *   4. Parse body `{ job_id, repo_full_name, installation_id?, scope?,
 *      ttl_seconds? }`. Missing/empty job_id/repo_full_name → 400; `installation_id`
 *      is OPTIONAL (frozen 2026-07-08 — see step 5a) but, when present, must be a
 *      non-empty string; admin/owner/unknown scope → 400; a non-positive/non-integer
 *      ttl_seconds → 400 (a caller may only SHORTEN the TTL toward its lease
 *      deadline; it is clamped down to the 90-min cap). The tenant is NEVER taken
 *      from the body — it is DERIVED server-side (step 5a).
 *   5. Server-side tenant DERIVATION + AUTHORIZATION chokepoint (cf-multitenant
 *      WP2). Every check reads CONFIG_DB, is fail-CLOSED, and every miss returns
 *      the SAME generic 403 (no oracle distinguishing which check failed):
 *        a. Derive tenant from ONE of two unforgeable server-side sources — never
 *           a body value:
 *             • `installation_id` PRESENT → `tenant_gh_installation_map WHERE
 *               installation_id` (CF-worker / webhook path). No row → 403.
 *             • `installation_id` ABSENT → introspect the acquiring PAT presented
 *               as `Authorization: Bearer` (fabricd / native path); the container's
 *               full PatVerifier resolves the tenant. No bearer → 401; invalid PAT
 *               / introspect unavailable → 403. A native repo has no GitHub App
 *               installation, so this is the ONLY tenant source for that path.
 *           Capture `tenantId` (the DERIVED tenant — never a body value).
 *        b. Suspend: `tenant_offboarding_state WHERE tenant_id` — a row EXISTS
 *           (the tenant is offboarding/suspended) → 403 (an active tenant has NO
 *           offboarding row).
 *        c. Allowlist: `runner_repo_allowlist WHERE tenant_id AND
 *           repo_full_name`. No row → 403.
 *        d. Entitlement + ceiling: `runners_entitlement WHERE tenant_id`. No row
 *           → 403; capture `max_concurrency` (the runner ceiling).
 *      Any D1 exception in a/b/c/d → 500 "runner mint unavailable".
 *   6. Mint via the SINGLE authority {@link mintScopedPat}, using the DERIVED
 *      `tenantId` and passing `job_id` as the principal-source string (SHA-256 →
 *      stable per-job principal UUID for audit correlation) and the clamped
 *      lease-bound TTL (≤ {@link RUNNER_PAT_TTL_SECONDS}).
 *   7. Return the standard mint envelope `{ token_plaintext, pat_id, token_id,
 *      expires_ms, tenant, max_concurrency }` (tenant = the derived tenant).
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

  // ── 2. Internal-auth gate (runner_mint consumer key, shared fallback) ──────
  // Scoped to the runner dispatcher's OWN key (CORELINK_RUNNER_MINT_AUTH_KEY),
  // distinct from signup's `pat_mint` — a leaked runner key mints/revokes ONLY
  // per-job runner PATs, never the signup PAT-mint / erase / admin surfaces.
  const authErr = requireConsumerAuth(request, env, "runner_mint", requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Fail-CLOSED on the required server secret ───────────────────────────
  // The internal-auth gate above already proved a sized consumer/shared key is
  // this is the secret presented to the container's /_internal/pat/mint route
  // (the mint authority). That gate REQUIRES the DEDICATED CORELINK_PAT_MINT_AUTH_KEY
  // with NO shared fallback (DD-HIGH, WP1), so present the dedicated key when set;
  // fall back to the shared key only when the dedicated is unset (additive — once
  // the dedicated is provisioned the shared no longer authorizes the mint).
  const internalAuthKey =
    env.CORELINK_PAT_MINT_AUTH_KEY ?? env.CORELINK_INTERNAL_AUTH_KEY;
  if (!internalAuthKey || internalAuthKey.length === 0) {
    return reapiError("FORBIDDEN", "runner mint unavailable", 403, requestId);
  }

  // ── 4. Parse the body (job_id + repo_full_name + installation_id required) ──
  // The tenant is NO LONGER a body field: it is DERIVED server-side (step 5a).
  // A caller can no longer name the tenant it mints for (the single-tenant hole
  // this WP closes).
  interface RunnerMintRequest {
    readonly job_id?: unknown;
    readonly repo_full_name?: unknown;
    readonly installation_id?: unknown;
    readonly scope?: unknown;
    readonly ttl_seconds?: unknown;
    // WP5a: OPTIONAL exact-key narrowing. When present, the runner-job PAT is
    // additionally restricted to the single AC key of that output workspace
    // (blake3(RUNNER_AC_KEY_PREFIX + name)). Dormant at launch — the dispatcher
    // does not yet know the output workspace name at mint time — so today every
    // runner mint takes the sentinel ("*" = deny-DELETE only) path.
    readonly ac_output_name?: unknown;
  }
  let body: RunnerMintRequest;
  try {
    body = (await request.json()) as RunnerMintRequest;
  } catch {
    return reapiError("BAD_REQUEST", "invalid request body", 400, requestId);
  }
  const jobId = body.job_id;
  if (typeof jobId !== "string" || jobId.length === 0) {
    return reapiError("BAD_REQUEST", "job_id required", 400, requestId);
  }
  const repoFullName = body.repo_full_name;
  if (typeof repoFullName !== "string" || repoFullName.length === 0) {
    return reapiError("BAD_REQUEST", "repo_full_name required", 400, requestId);
  }
  // installation_id is OPTIONAL (frozen 2026-07-08): present ⇒ CF-worker/webhook
  // path (tenant via the installation map, step 5a); absent ⇒ fabricd/native path
  // (tenant via acquiring-PAT introspection, step 5a). When present it MUST be a
  // non-empty string; an empty string is a malformed request (400).
  const installationId = body.installation_id;
  if (
    installationId !== undefined &&
    (typeof installationId !== "string" || installationId.length === 0)
  ) {
    return reapiError(
      "BAD_REQUEST",
      "installation_id, when present, must be a non-empty string",
      400,
      requestId,
    );
  }
  let scope = RUNNER_MINT_DEFAULT_SCOPE;
  if (typeof body.scope === "string" && body.scope.length > 0) {
    if (!RUNNER_MINT_ALLOWED_SCOPES.has(body.scope)) {
      // Refuse unknown / admin / owner scope (least privilege).
      return reapiError("BAD_REQUEST", "unsupported scope", 400, requestId);
    }
    scope = body.scope;
  }

  // ── 4b. Lease-bound TTL (optional; clamp DOWN to the cap, never extend) ─────
  // The dispatcher sends `ttl_seconds` = the lease's REMAINING time so the PAT
  // EXPIRES WITH THE LEASE. Without honoring it, the 90-min default outlives any
  // shorter lease → the dispatcher's `expires_ms ≤ lease_deadline` assertion
  // trips → fail-closed provision. We clamp the request DOWN to the cap and NEVER
  // up (extending would break that assertion). `0`/negative/non-int is REFUSED
  // (the container maps `ttl_seconds=0` → "no expiry" — a non-expiring runner PAT
  // must be impossible to request). Omitted → the 90-min default (backward-compat).
  let ttlSeconds = RUNNER_PAT_TTL_SECONDS;
  if (body.ttl_seconds !== undefined) {
    const t = body.ttl_seconds;
    if (typeof t !== "number" || !Number.isInteger(t) || t <= 0) {
      return reapiError("BAD_REQUEST", "ttl_seconds must be a positive integer", 400, requestId);
    }
    ttlSeconds = Math.min(t, RUNNER_PAT_TTL_SECONDS);
  }

  // ── 4c. Narrowed runner-job PAT marker (WP5a) ──────────────────────────────
  // EVERY runner mint is narrowed: the resulting PAT is marked in D1 so the
  // container (WP5b) enforces a tighter scope than a normal PAT (deny-DELETE on
  // the native plane). The marker value:
  //   - no `ac_output_name`  → the sentinel "*" = deny-DELETE ONLY. This is the
  //     LAUNCH path (the output-workspace name is unavailable at mint today).
  //   - an `ac_output_name`  → blake3(RUNNER_AC_KEY_PREFIX + name) hex = additionally
  //     restrict the token to that exact AC key (dormant, forward-wired path).
  // `ac_output_name`, if present, MUST be a non-empty string (else 400).
  let runnerJobAcKey: string;
  if (body.ac_output_name === undefined) {
    runnerJobAcKey = RUNNER_AC_KEY_DENY_DELETE_ONLY;
  } else {
    const name = body.ac_output_name;
    if (typeof name !== "string" || name.length === 0) {
      return reapiError("BAD_REQUEST", "ac_output_name must be a non-empty string", 400, requestId);
    }
    runnerJobAcKey = await blake3Hex(RUNNER_AC_KEY_PREFIX + name);
  }

  // ── 5. Server-side tenant DERIVATION + AUTHORIZATION chokepoint (WP2) ───────
  // Four fail-CLOSED CONFIG_DB reads. EVERY miss returns the SAME generic 403 —
  // NO oracle tells the caller which check failed (an unmapped installation, a
  // suspended tenant, a non-allowlisted repo, and a non-entitled tenant are
  // byte-identical responses). Any D1 exception → 500 "runner mint unavailable".
  const forbidden = (): Response =>
    reapiError("FORBIDDEN", "runner mint unauthorized", 403, requestId);

  let tenantId: string;
  let maxConcurrency: number;
  try {
    // 5a. Derive the tenant SERVER-SIDE — NEVER a body value. Two unforgeable
    // sources (frozen 2026-07-08); the single-tenant hole stays closed on both.
    if (typeof installationId === "string") {
      // CF-worker / webhook path: the GitHub installation → tenant map. The
      // caller names installation_id, not the tenant; the map is server-side.
      const mapRow = await env.CONFIG_DB.prepare(
        "SELECT tenant_id FROM tenant_gh_installation_map WHERE installation_id = ?1",
      )
        .bind(installationId)
        .first<{ tenant_id: string }>();
      if (
        mapRow === null ||
        typeof mapRow.tenant_id !== "string" ||
        mapRow.tenant_id.length === 0
      ) {
        return forbidden();
      }
      tenantId = mapRow.tenant_id;
    } else {
      // fabricd / native path: introspect the acquiring PAT presented as
      // `Authorization: Bearer`. The container's full PatVerifier resolves the
      // tenant from the PAT's D1 row — the caller proves possession, never names
      // a tenant. A native repo has no GitHub App installation, so this is the
      // only viable source. No bearer → 401; invalid PAT / introspect down → 403.
      const authz = request.headers.get("authorization") ?? "";
      const bearer = /^Bearer\s+(.+)$/i.exec(authz)?.[1];
      if (bearer === undefined) {
        return reapiError(
          "UNAUTHORIZED",
          "acquiring PAT (Authorization: Bearer) required when installation_id is absent",
          401,
          requestId,
        );
      }
      const patTenant = await resolveTenantFromAcquiringPat(env, requestId, bearer);
      if (patTenant === null) {
        return forbidden();
      }
      tenantId = patTenant;
    }

    // 5b. Suspend gate: an ACTIVE tenant has NO offboarding row; a row EXISTS ⇒
    // the tenant is offboarding/suspended ⇒ deny.
    const offRow = await env.CONFIG_DB.prepare(
      "SELECT 1 FROM tenant_offboarding_state WHERE tenant_id = ?1 LIMIT 1",
    )
      .bind(tenantId)
      .first<{ 1: number }>();
    if (offRow !== null) {
      return forbidden();
    }

    // 5c. Allowlist gate: the (tenant, repo) pair must be explicitly allowlisted.
    const allowRow = await env.CONFIG_DB.prepare(
      "SELECT 1 FROM runner_repo_allowlist WHERE tenant_id = ?1 AND repo_full_name = ?2 LIMIT 1",
    )
      .bind(tenantId, repoFullName)
      .first<{ 1: number }>();
    if (allowRow === null) {
      return forbidden();
    }

    // 5d. Entitlement + ceiling: the tenant must be entitled to Runners; capture
    // its max_concurrency (the runner ceiling threaded into the response).
    const entRow = await env.CONFIG_DB.prepare(
      "SELECT max_concurrency FROM runners_entitlement WHERE tenant_id = ?1",
    )
      .bind(tenantId)
      .first<{ max_concurrency: number }>();
    if (entRow === null || typeof entRow.max_concurrency !== "number") {
      return forbidden();
    }
    maxConcurrency = entRow.max_concurrency;
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] runner mint authz lookup failed: ${message.slice(0, 80)}`);
    return reapiError("INTERNAL_ERROR", "runner mint unavailable", 500, requestId);
  }

  // ── 5e. M22(b) per-tenant mint ceiling (AFTER derivation, BEFORE the mint) ──
  // The per-job throttle inside mintScopedPat caps mints per job_id, but a
  // mint-storm across MANY distinct job_ids for the SAME tenant slips past it
  // (each job stays under its own cap). Apply a SECOND throttle keyed by a
  // domain-separated, hashed per-tenant key with a cap SCALED off the tenant's
  // runner ceiling — so it grows with entitlement (never squeezing legitimate
  // fan-out) yet bounds a per-tenant storm. Composes with, does not replace, the
  // per-job throttle. This is a per-WINDOW ceiling: the durable D1 counter is the
  // gate, so the in-memory backstop is DISABLED here (Infinity). The in-memory
  // backstop is a monotonic per-isolate-lifetime counter (it never window-resets),
  // so a finite value would squeeze a busy tenant's legitimate fan-out ACROSS
  // windows over the isolate's life — the opposite of "grows with entitlement".
  // Outage CPU safety is already provided by the per-JOB burst backstop inside
  // mintScopedPat, and during a D1 outage the pat-row INSERT fails-CLOSED anyway
  // (no PAT is persisted), so the tenant gate needs no in-memory arm.
  const tenantCeiling = Math.max(
    maxConcurrency * RUNNER_TENANT_CEILING_K,
    RUNNER_TENANT_CEILING_FLOOR,
  );
  const tenantThrottleKey = await deriveTenantCeilingThrottleKey(tenantId);
  const tenantThrottled = await checkMintThrottle(env.CONFIG_DB, tenantThrottleKey, requestId, {
    maxPerWindow: tenantCeiling,
    inMemoryBurstCap: Number.POSITIVE_INFINITY,
  });
  if (tenantThrottled !== null) {
    return tenantThrottled;
  }

  // ── 6+7. Mint via the SINGLE authority with the DERIVED tenant ─────────────
  // `max_concurrency` is threaded through mintScopedPat's extraFields bag so it
  // appears in the returned JSON alongside the standard envelope; `tenant` in
  // the response is the DERIVED tenantId (mintScopedPat sources it from the
  // branded grant). L12(b): a runner grant's ceiling is `read-write` — a
  // disposable runner can never mint an admin PAT.
  return mintScopedPat(
    env,
    requestId,
    // Domain-separate the per-JOB throttle principal ("runner-job:") from the
    // per-TENANT ceiling key ("runner-tenant:"): both are hashed into the same
    // throttle table, so a raw `job_id` of "runner-tenant:<victimTenant>" would
    // otherwise collide with that tenant's ceiling row and let a caller burn a
    // victim tenant's runner-mint budget (M22b review finding). Distinct prefixes
    // make a preimage collision impossible; per-job semantics are unchanged.
    MintGrant.fromRunnerDerivation(tenantId, "runner-job:" + jobId),
    ttlSeconds,
    scope,
    internalAuthKey,
    { max_concurrency: maxConcurrency },
    // WP5a: persist the narrowed runner-job marker on the `pat` row so the
    // Worker's auth-resolve can forward it to the container for enforcement.
    runnerJobAcKey,
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
 * Authorized identically to mint: the `runner_mint` consumer key with shared
 * fallback. The dispatcher holds no customer PAT, so the customer revoke route
 * (which is PAT-gated) is not usable here — internal-auth is the gate.
 *
 * Pipeline (fail-CLOSED): method (405), internal-auth (401/403), bad body (400),
 * missing pat_id / owner_tenant (400), D1 error (500). Revoking an already-revoked
 * or unknown (pat_id, owner_tenant) pair is idempotent (200) — the
 * `revoked_at_ms IS NULL` guard makes a re-revoke a no-op, and the dispatcher
 * needs teardown to be safely retryable.
 *
 * TENANT-SCOPED REVOKE (REV-S2): the UPDATE carries a `tenant_id = owner_tenant`
 * predicate (the dispatcher already supplies `owner_tenant` at mint time). This
 * bounds the blast radius of a compromised `runner_mint` key to the tenants the
 * caller actually names — without it, a leaked mint key could revoke ANY tenant's
 * PAT (including a customer's long-lived primary API key) as a targeted DoS.
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

  // ── 2. Internal-auth gate (runner_mint consumer key, shared fallback) ──────
  // Scoped to the runner dispatcher's OWN key (CORELINK_RUNNER_MINT_AUTH_KEY),
  // distinct from signup's `pat_mint` — a leaked runner key mints/revokes ONLY
  // per-job runner PATs, never the signup PAT-mint / erase / admin surfaces.
  const authErr = requireConsumerAuth(request, env, "runner_mint", requestId);
  if (authErr) {
    return authErr;
  }

  // ── 3. Parse the body (pat_id required; owner_tenant OPTIONAL, 2026-07-08) ──
  interface RunnerRevokeRequest {
    readonly pat_id?: unknown;
    readonly owner_tenant?: unknown;
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
  // owner_tenant is OPTIONAL (2026-07-08 runners contract SUPERSEDES the
  // 2026-06-21 REV-S2 mandatory requirement). The runners dispatcher's frozen
  // revoke body is `{pat_id}` only — naming the tenant client-side was itself the
  // single-tenant shape that the 2026-07-08 (owner-ratified) contract removed, and
  // the dispatcher's tests now ASSERT owner_tenant is not on the wire. When PRESENT
  // (legacy callers / opt-in defense-in-depth) it MUST be a non-empty string and
  // STILL scopes the UPDATE (below); when ABSENT, revoke by pat_id alone — the
  // pat_id IS the capability (possessing it already permits the revoke), so the
  // tenant predicate was weak scoping, not a real isolation boundary.
  const ownerTenant = body.owner_tenant;
  if (
    ownerTenant !== undefined &&
    (typeof ownerTenant !== "string" || ownerTenant.length === 0)
  ) {
    return reapiError(
      "BAD_REQUEST",
      "owner_tenant, when present, must be a non-empty string",
      400,
      requestId,
    );
  }

  // ── 4. Revoke via the EXISTING surface (idempotent) ────────────────────────
  // The `revoked_at_ms IS NULL` guard keeps a re-revoke idempotent (no-op 200).
  // owner_tenant PRESENT → keep the tenant predicate (defense-in-depth for a
  // caller that opts in; a (pat_id, owner_tenant) mismatch matches zero rows → a
  // no-op 200, no cross-tenant write). owner_tenant ABSENT (2026-07-08 contract)
  // → revoke by pat_id alone; the pat_id already uniquely identifies the row.
  try {
    if (typeof ownerTenant === "string") {
      await env.CONFIG_DB.prepare(
        "UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL",
      )
        .bind(Date.now(), patId, ownerTenant)
        .run();
    } else {
      await env.CONFIG_DB.prepare(
        "UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND revoked_at_ms IS NULL",
      )
        .bind(Date.now(), patId)
        .run();
    }
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
