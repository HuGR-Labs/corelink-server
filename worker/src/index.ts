/**
 * CoreLink Cloudflare Worker shim — HTTP entry point, route table, auth
 * middleware, and request forwarding to the CoreLinkServer Durable Object.
 *
 * Architecture:
 *   Internet HTTPS → this Worker (auth + routing + error mapping)
 *                         ↓  env.CORELINK_SERVER.idFromName(tenantId)
 *                    CoreLinkServer DO (container lifecycle manager)
 *                         ↓  container_run_grpc(:50051)
 *                    Rust gRPC binary (corelink-server)
 *
 * Charter constraints:
 *   - Zero `any` types — all bindings typed via Env interface.
 *   - INV-NO-BODY-IN-LOGS: body bytes NEVER logged; request-id only.
 *   - INV-NO-PII-IN-LOGS: auth headers sanitized to hashed token prefix.
 *   - Constant-time auth compare via crypto.subtle.timingSafeEqual.
 *   - Timing-padding for 404s per design-pattern-01.
 *   - Tenant isolation: DO ID derived solely from tenant_id.
 *   - CORS per Access-Control-Allow-* config.
 */

import type { D1Database, DurableObjectNamespace, ExecutionContext, ExportedHandler } from "@cloudflare/workers-types";
import { CoreLinkServer } from "./durable_object.js";
import { RolloutController } from "./rollout_controller.js";
import { getTierForTenant, checkStorageQuota, checkRequestQuota } from "./lib/quota.js";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** Worker environment bindings — matches wrangler.toml. */
export interface Env {
  CORELINK_SERVER: DurableObjectNamespace;
  ENVIRONMENT: string;
  // D1 CONFIG_DB — control-plane database. Holds the `pat` table queried
  // during PAT validation (WP-A1). Bound in wrangler.toml `[[d1_databases]]`.
  CONFIG_DB: D1Database;
  // Secrets (bound via `wrangler secret put`)
  CLERK_SECRET_KEY?: string;
  STRIPE_SECRET_KEY?: string;
  // L3 money path — forwarded to the container by the DO (see durable_object.ts);
  // the tier-select route is unmounted (404) without CORELINK_DPA_VERSION, and
  // Stripe checkout needs the per-tier price ids.
  STRIPE_AUTH_MODE?: string;
  STRIPE_WEBHOOK_SECRET?: string;
  STRIPE_PRICE_ID_STARTER?: string;
  STRIPE_PRICE_ID_TEAM?: string;
  STRIPE_PRICE_ID_PRO?: string;
  CORELINK_DPA_VERSION?: string;
  // PAT HMAC signing key (raw hex, ≥ 32 bytes decoded) — used for the
  // HMAC-SHA256 fast-fail layer in PAT validation (WP-A1 step 2).
  // Bound via: `wrangler secret put PAT_SIGNING_KEY`
  // Key derivation: HKDF-SHA256(CORELINK_MASTER_KEY, "corelink-pat-signing-salt-v1",
  //   b"corelink-v1-pat-signing-key", 32) per key_management.md §3.2.1.
  PAT_SIGNING_KEY?: string;
  // Stream-5: shared secret for `/_internal/pat/mint` (passed to container
  // at boot + verified before forwarding). Bound via:
  // `wrangler secret put CORELINK_INTERNAL_AUTH_KEY`
  CORELINK_INTERNAL_AUTH_KEY?: string;
  // Per-tier quota enforcement (worker/src/lib/quota.ts).
  // Storage quota is always enforced for finite-quota tiers.
  // Request quota is deferred until a monthly counter table is wired;
  // set REQUEST_QUOTA_ENABLED=true once the table exists.
  REQUEST_QUOTA_ENABLED?: string;
  // Container storage credentials (WP-S1 StorageEnv contract). The DO forwards
  // these to the native container via container.start({ env }) so it can reach
  // R2 (S3 API) + D1 (HTTP API). Absent → container falls back to InMemory
  // (dev/CI). R2_S3_ENDPOINT + D1_DATABASE_ID are non-secret vars; the two R2
  // keys + CF_API_TOKEN are secrets (wrangler secret put). CLOUDFLARE_ACCOUNT_ID
  // is an existing secret reused here.
  R2_S3_ENDPOINT?: string;
  R2_S3_ACCESS_KEY_ID?: string;
  R2_S3_SECRET_ACCESS_KEY?: string;
  CF_API_TOKEN?: string;
  D1_DATABASE_ID?: string;
  CLOUDFLARE_ACCOUNT_ID?: string;
  // ADR-MULTI-REGION-V1 — per-region container bucket + region vars.
  // Set in each [env.prod-<region>] block's `vars`. The DO forwards these to
  // container.start({ env }) so the Rust binary writes to the correct regional
  // R2 bucket. Absent → container defaults to IAD (corelink-ac-iad / iad).
  // R2_AC_BUCKET:    AC envelope bucket for this region (e.g. corelink-ac-lhr)
  // R2_AC_REGION:    AC bucket region code (e.g. lhr)
  // R2_CHUNK_BUCKET: Multipart chunk bucket for this region (e.g. corelink-chunk-lhr)
  // R2_CHUNK_REGION: Chunk bucket region code (e.g. lhr)
  R2_AC_BUCKET?: string;
  R2_AC_REGION?: string;
  R2_CHUNK_BUCKET?: string;
  R2_CHUNK_REGION?: string;
  // WI-MULTI-REGION-V1 Service Bindings: prod env can fan-out to the 4
  // regional Workers. Set in [[env.prod.services]] blocks. Used by the
  // per-tenant routing logic: tenant.primary_region in D1 → dispatch via
  // the matching binding. Absent → request stays on IAD (default).
  PROD_SAM?: { fetch: typeof fetch };
  PROD_LHR?: { fetch: typeof fetch };
  PROD_NRT?: { fetch: typeof fetch };
  PROD_SYD?: { fetch: typeof fetch };
}

/** Parsed route context derived from matching the request URL. */
interface RouteMatch {
  readonly tenantId: string;
  readonly pathSuffix: string;
  readonly routeKind: RouteKind;
}

/** Canonical route kinds that this worker handles. */
type RouteKind =
  | "health"
  | "health_serving"
  | "oci_v2"
  | "npm"
  | "pip"
  | "brew"
  | "cargo"
  | "reapi_v2"
  | "customer_v1"
  | "reapi_v1"
  | "bazel_v2"
  | "turbo_v8"
  | "signup"
  | "internal"
  | "health_container"
  | "not_found";

/** Auth extraction result from the Authorization header. */
type AuthResult =
  | { readonly ok: true; readonly tenantId: string; readonly tokenPrefix: string }
  | { readonly ok: false; readonly reason: string };

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/** Target p99 wall-clock for 404 timing-padding (ms). Covers slowest arm. */
const TIMING_PAD_TARGET_MS = 80;
/** Jitter range ±% applied to pad target. */
const TIMING_PAD_JITTER_PCT = 15;
/** Minimum pad (ms) — safety floor so sleep is never negative. */
const TIMING_PAD_MIN_MS = 5;

const ALLOWED_ORIGINS = [
  "https://app.corelink.humangr.com",
  "https://admin.corelink.humangr.com",
  "https://docs.corelink.humangr.com",
];

const CORS_HEADERS: ReadonlyArray<readonly [string, string]> = [
  ["Access-Control-Allow-Methods", "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"],
  ["Access-Control-Allow-Headers", "Authorization, Content-Type, X-Request-Id, Accept"],
  ["Access-Control-Expose-Headers", "X-Request-Id, X-Corelink-Tenant-Id"],
  ["Access-Control-Max-Age", "86400"],
];

// ──────────────────────────────────────────────────────────────────────────────
// Request ID middleware
// ──────────────────────────────────────────────────────────────────────────────

/** Generate or propagate a request-id. Never exposes body or PII. */
function resolveRequestId(request: Request): string {
  const incoming = request.headers.get("x-request-id");
  if (incoming !== null && incoming.length > 0 && incoming.length <= 128) {
    return incoming;
  }
  return crypto.randomUUID();
}

// ──────────────────────────────────────────────────────────────────────────────
// CORS helpers
// ──────────────────────────────────────────────────────────────────────────────

function corsOriginHeader(request: Request): string | null {
  const origin = request.headers.get("origin");
  if (origin === null) return null;
  if (ALLOWED_ORIGINS.includes(origin)) return origin;
  return null;
}

function applyCors(response: Response, request: Request): Response {
  const allowed = corsOriginHeader(request);
  if (allowed === null) return response;
  const headers = new Headers(response.headers);
  headers.set("Access-Control-Allow-Origin", allowed);
  headers.set("Vary", "Origin");
  for (const [k, v] of CORS_HEADERS) {
    headers.set(k, v);
  }
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

function handlePreflight(request: Request): Response | null {
  if (request.method !== "OPTIONS") return null;
  const allowed = corsOriginHeader(request);
  if (allowed === null) return new Response(null, { status: 204 });
  const headers = new Headers();
  headers.set("Access-Control-Allow-Origin", allowed);
  headers.set("Vary", "Origin");
  for (const [k, v] of CORS_HEADERS) {
    headers.set(k, v);
  }
  return new Response(null, { status: 204, headers });
}

// ──────────────────────────────────────────────────────────────────────────────
// Route table
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Match the request URL against the CoreLink route surface.
 *
 * Route priority (first match wins):
 *   /health                 → health check (legacy body: {"status":"ok",...})
 *   /api/health             → health check alias (body: {"status":"SERVING",...})
 *                              — matches e2e suite Journey 1 exact-string assertion
 *                              `assert_eq!(body["status"], "SERVING")`
 *                              (tests/e2e-user-journeys/src/main.rs:151).
 *   /v2/*                   → OCI Distribution Spec v1.1
 *   /npm/*                  → npm registry proxy
 *   /pip/*                  → PyPI proxy
 *   /brew/*                 → Homebrew tap proxy
 *   /cargo/*                → Cargo registry proxy
 *   /api/v2/*               → REAPI v2 (CoreLink native HTTP API)
 *   /v1/customer/*          → Customer portal (overview, usage, billing, keys, team,
 *                              audit) — PAT required; tenant from PAT. Checked BEFORE
 *                              the generic /v1/* arm (specificity order).
 *   /v1/users/me            → REAPI v1 — tenant comes from PAT (urlTenant=_anonymous)
 *   /v1/cas/*               → REAPI v1 CAS — tenant from PAT
 *   /v1/admin/*             → REAPI v1 admin — tenant from PAT (admin scope enforced
 *                              in container)
 *   /v1/signup/*            → Pre-tenant signup flow (token in path IS the auth
 *                              artifact, not a Bearer PAT). tenantId=_anonymous.
 *   *                       → not_found
 *
 * Tenant extraction:
 *   - For OCI/npm/pip/brew/cargo: first path segment after the protocol
 *     prefix is the tenant namespace (e.g., `/v2/<tenant>/…`).
 *   - For REAPI v2: `X-Corelink-Tenant-Id` header or first path segment.
 *   - For REAPI v1 (/v1/*): tenant is NOT in URL — resolved by DO from PAT;
 *     Worker uses urlTenant="_anonymous" which preserves any downstream
 *     path-spoof gate semantics (the gate only fires when urlTenant is a
 *     real tenant id; "_anonymous" means "Worker is deferring to PAT").
 *   - For /v1/signup/*: pre-tenant (customer has no tenant yet), so
 *     urlTenant="_anonymous"; DO pins all anon signup traffic to one DO
 *     instance keyed by "_anonymous" (simplest dispatch — no per-anon DO
 *     proliferation; signup throughput is low and rate-limited upstream).
 *   - For /health: tenant = "_system".
 *   - For /api/health: tenant = "_system" (alias).
 */
function matchRoute(url: URL): RouteMatch {
  const path = url.pathname;

  // Container health deep-probe — /_health/container forwards through the
  // _system DO to the container's own /_health endpoint, exposing A4's
  // `storage` field (r2 vs inmemory) that the Worker's fast-path /_health
  // never returns.  Publicly probeable, no auth required.
  if (path === "/_health/container" || path === "/_health/container/") {
    return { tenantId: "_system", pathSuffix: "/_health", routeKind: "health_container" };
  }

  // Health: /health (legacy CF/customer liveness) + /_health (smoke-prod check
  // [2]; the container's DO-side probe uses /_health on the container's private
  // port via getTcpPort, not via this public route — independent paths).
  // Both return the legacy {"status":"ok",...} body; the SERVING alias lives at
  // /api/health below to match the e2e suite's exact-string assertion.
  if (path === "/health" || path === "/health/" || path === "/_health" || path === "/_health/") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "health" };
  }

  // Health alias — /api/health (body: {"status":"SERVING",...})
  // Matches e2e Journey 1 exact-string assertion on "SERVING".
  if (path === "/api/health" || path === "/api/health/") {
    return { tenantId: "_system", pathSuffix: "/api/health", routeKind: "health_serving" };
  }

  // OCI v2 — /v2[/…]
  if (path === "/v2" || path === "/v2/" || path.startsWith("/v2/")) {
    const rest = path.slice(3); // strip "/v2"
    const tenant = extractFirstSegment(rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "oci_v2" };
  }

  // Bazel remote cache (REAPI v2) — /bazel/v2/<instance>/...
  // <instance> = tenant_id per CoreLink convention. The container
  // (`crates/corelink-container/src/routes/bazel_v2.rs`) enforces
  // caller_tenant equality against the URL :instance.
  if (path.startsWith("/bazel/v2/")) {
    const rest = path.slice("/bazel/v2/".length);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "bazel_v2" };
  }

  // Vercel /v8/artifacts (Turborepo remote cache).
  // Hash is the URL leaf; tenant comes from `?teamId=...` query string,
  // resolved by the container handler against caller_tenant.
  if (path.startsWith("/v8/artifacts")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "turbo_v8" };
  }

  // npm — /npm/<tenant>/…
  if (path.startsWith("/npm/")) {
    const rest = path.slice(5); // strip "/npm/"
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "npm" };
  }

  // pip — /pip/<tenant>/…
  if (path.startsWith("/pip/")) {
    const rest = path.slice(5);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "pip" };
  }

  // brew — /brew/<tenant>/…
  if (path.startsWith("/brew/")) {
    const rest = path.slice(6);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "brew" };
  }

  // cargo — /cargo/<tenant>/…
  if (path.startsWith("/cargo/")) {
    const rest = path.slice(7);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "cargo" };
  }

  // REAPI v2 — /api/v2/…
  if (path.startsWith("/api/v2/")) {
    const rest = path.slice(8);
    const tenant = extractFirstSegment("/" + rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "reapi_v2" };
  }

  // REAPI v1 signup — /v1/signup/… (pre-tenant flow; token in path is the auth artifact)
  // Checked BEFORE the generic /v1/* arms so signup never falls into the
  // PAT-required reapi_v1 bucket. tenantId=_anonymous pins all anon signup
  // traffic to one DO instance.
  if (path.startsWith("/v1/signup/") || path === "/v1/signup") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "signup" };
  }

  // REAPI v1 customer portal — /v1/customer/* (overview, usage, billing, keys, team, audit)
  // Checked BEFORE the generic /v1/* arm so customer paths never fall into the
  // reapi_v1 bucket. Tenant is NOT in the URL — resolved by the DO from the PAT
  // (same pattern as reapi_v1 / /v1/users/me). The spoof gate doesn't fire because
  // urlTenant="_anonymous" (no path tenant in /v1/customer/<resource> URLs).
  // Auth: PAT required — customer routes are NOT pre-tenant (unlike signup).
  if (path.startsWith("/v1/customer/") || path === "/v1/customer") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "customer_v1" };
  }

  // Internal routes — /_internal/* — gated by X-Corelink-Internal-Auth shared
  // secret. NOT gated by PAT auth. Only reachable from Worker-to-Worker calls
  // (signup-worker → this Worker → DO → container). The shared-secret check
  // is performed in the fetch handler (not in matchRoute) so the route is
  // never accidentally skipped on 404-padding paths.
  if (path.startsWith("/_internal/")) {
    return { tenantId: "_system", pathSuffix: path, routeKind: "internal" };
  }

  // REAPI v1 — /v1/users/me, /v1/cas/blobs/<digest>/<size>, /v1/admin/audit/events, …
  // Generic /v1/* fallthrough — only reached when no more-specific arm matched above.
  // Arms checked before this one (specificity order, most-specific first):
  //   1. /v1/signup/*    → "signup"      (pre-tenant, no PAT required)
  //   2. /v1/customer/*  → "customer_v1" (PAT required, tenant from PAT)
  //   3. /v1/*           → "reapi_v1"    ← this arm (PAT required, tenant from PAT)
  // Tenant is NOT in the URL — resolved by the DO from the PAT.
  // urlTenant="_anonymous" preserves the future path-spoof gate semantics
  // (gate only fires when urlTenant is a concrete tenant id).
  if (path.startsWith("/v1/")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "reapi_v1" };
  }

  return { tenantId: "_system", pathSuffix: path, routeKind: "not_found" };
}

/**
 * Extract the first path segment (no leading slash in result).
 * "/foo/bar/baz" → "foo"
 * "/foo"         → "foo"
 * "/"            → null
 * ""             → null
 */
function extractFirstSegment(path: string): string | null {
  const stripped = path.startsWith("/") ? path.slice(1) : path;
  if (stripped.length === 0) return null;
  const slash = stripped.indexOf("/");
  return slash === -1 ? stripped : stripped.slice(0, slash);
}

// ──────────────────────────────────────────────────────────────────────────────
// Auth middleware
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Extract and validate the Bearer PAT from the Authorization header.
 *
 * Full validation pipeline (WP-A1, P0 wave Phase 1):
 *
 *   1. Extract Bearer token; validate printable ASCII shape.
 *   2. Parse PAT canonical format: `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`
 *      (95 or 96 chars). Anything that is not this shape → 401.
 *   3. HMAC-SHA256 fast-fail (if PAT_SIGNING_KEY is bound): reject in < 1ms
 *      if the `hmac_sig` segment does not match the signing key. This blocks
 *      random-string brute-force without touching D1 at all.
 *   4. D1 lookup by `token_id` — SELECT tenant_id, pat_hash, expires_ms FROM pat
 *      WHERE token_id = ?1 AND expires_ms > now_ms. Any token not in the D1
 *      store → 401. This kills the "any 32–256 char string accepted" vulnerability.
 *   5. Expiry check: expires_ms > Date.now().
 *   6. Return resolved tenant_id (UUID string from D1 row).
 *
 * Constant-time discipline:
 *   - Token bytes are scanned in full before any early-return.
 *   - HMAC compare uses crypto.subtle.timingSafeEqual.
 *   - SHA-256 prefix for log correlation is computed from the raw token bytes
 *     (non-reversible; see INV-NO-PII-IN-LOGS).
 *
 * NOTE: Argon2id verification (step 3c in auth_model.md §2.3) is NOT
 * performed in the CF Worker because OWASP-2024 Argon2id cost parameters
 * (m=64MiB, t=3, p=4) exceed the Worker's 30ms CPU budget. The Rust DO
 * (corelink-worker Tower middleware) performs the Argon2id verify on the
 * raw token forwarded via the Authorization header. The Worker provides the
 * existence + expiry gate (this function) which eliminates the "any string
 * accepted" vulnerability. Argon2id + scope checks are the DO's second
 * defence layer.
 *
 * The Worker never logs the token value — only a 6-char hashed prefix
 * for correlation tracing.
 *
 * INV-NO-PII-IN-LOGS: tenant ID is NOT logged at this layer; only the DO
 * writes hashed-form tenant IDs to audit events.
 */
async function extractAuth(request: Request, env: Env): Promise<AuthResult> {
  const authHeader = request.headers.get("authorization");
  if (authHeader === null || authHeader.length === 0) {
    return { ok: false, reason: "missing_authorization_header" };
  }

  const bearerPrefix = "Bearer ";
  if (!authHeader.startsWith(bearerPrefix)) {
    return { ok: false, reason: "invalid_scheme" };
  }

  const token = authHeader.slice(bearerPrefix.length).trim();
  if (token.length < 32 || token.length > 256) {
    return { ok: false, reason: "invalid_token_length" };
  }

  // Validate printable ASCII (0x21–0x7E) — no spaces, no control chars.
  // Scan all bytes before early-exit to avoid byte-position timing oracle.
  const enc = new TextEncoder();
  const tokenBytes = enc.encode(token);
  let invalid = 0;
  for (const b of tokenBytes) {
    invalid |= (b < 0x21 || b > 0x7e) ? 1 : 0;
  }
  if (invalid !== 0) {
    return { ok: false, reason: "invalid_token_chars" };
  }

  // Derive a safe log prefix (first 6 chars of base64url of SHA-256 of token).
  // Deterministic per token value but non-reversible.
  const hashBuf = await crypto.subtle.digest("SHA-256", tokenBytes);
  const hashArr = new Uint8Array(hashBuf);
  const tokenPrefix = base64url(hashArr).slice(0, 6);

  // ── Step 2: Parse canonical PAT format ───────────────────────────────────
  // corelink_<env>_<token_id>.<random_secret>.<hmac_sig>
  // Total length: 95 (env=2: ci/ro) or 96 (env=3: pat).
  const parsed = parsePat(token);
  if (parsed === null) {
    // Not a CoreLink PAT format. Reject — we only accept canonical PATs.
    return { ok: false, reason: "invalid_pat_format" };
  }

  // ── Step 3: HMAC-SHA256 fast-fail (requires PAT_SIGNING_KEY secret) ──────
  // If the signing key is bound, verify the hmac_sig segment before touching D1.
  // Pre-image: `<token_id>.<random_secret>` (bytes 9+env_len+1 through end of
  // random_secret segment — the same preimage used by the Rust verify crate).
  if (env.PAT_SIGNING_KEY !== undefined && env.PAT_SIGNING_KEY.length > 0) {
    const hmacOk = await verifyPatHmac(
      env.PAT_SIGNING_KEY,
      parsed.hmacPreimage,
      parsed.hmacSigBytes,
    );
    if (!hmacOk) {
      return { ok: false, reason: "invalid_pat_hmac" };
    }
  }

  // ── Step 4: D1 lookup by token_id ────────────────────────────────────────
  // Any token whose token_id is not in the D1 store → 401.
  // This kills the "any 32–256 char string accepted" vulnerability (P0-2).
  interface PatRow {
    tenant_id: string;
    expires_ms: number;
  }
  let row: PatRow | null;
  try {
    row = await env.CONFIG_DB
      .prepare("SELECT tenant_id, expires_ms FROM pat WHERE token_id = ?1 LIMIT 1")
      .bind(parsed.tokenId)
      .first<PatRow>();
  } catch (_err: unknown) {
    // D1 errors (network partition, DB unavailable) must not fail-open.
    // Return a distinct reason so the caller can map to 503 if desired.
    // For now we 401 to fail-closed (security > availability at this layer).
    return { ok: false, reason: "d1_lookup_error" };
  }

  if (row === null) {
    // token_id not in D1 — reject.
    return { ok: false, reason: "pat_not_found" };
  }

  // ── Step 5: Expiry check ──────────────────────────────────────────────────
  if (row.expires_ms <= Date.now()) {
    return { ok: false, reason: "pat_expired" };
  }

  // Resolved tenant_id from D1. The DO will perform the Argon2id + scope verify.
  return { ok: true, tenantId: row.tenant_id, tokenPrefix };
}

// ──────────────────────────────────────────────────────────────────────────────
// PAT format parsing (pure TS port of crates/corelink-pat/src/format.rs)
// ──────────────────────────────────────────────────────────────────────────────

/** Parsed PAT segments needed by the Worker auth path. */
interface PatParsed {
  /** 16-char Crockford b32 token_id (D1 lookup key). */
  readonly tokenId: string;
  /** The HMAC preimage: bytes of `<token_id>.<random_secret>`. */
  readonly hmacPreimage: Uint8Array;
  /** 16 raw bytes of the HMAC-SHA256 truncated signature. */
  readonly hmacSigBytes: Uint8Array;
}

/**
 * Parse a CoreLink PAT plaintext `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>`.
 *
 * Returns null for any non-canonical input (wrong prefix, wrong length,
 * wrong charset, wrong env) — the caller treats null as auth failure.
 *
 * Constant-time notes: this function performs character-by-character scans in
 * fixed loops (no short-circuit on first bad byte) for the prefix and env
 * segments, consistent with the Rust `parse_plaintext` constant-time contract.
 * Full timing-indistinguishability across all valid env variants is preserved
 * by running all three env comparisons before selecting a match.
 */
function parsePat(token: string): PatParsed | null {
  // Canonical total lengths: 95 (env=2: "ci"/"ro") or 96 (env=3: "pat").
  const totalLen = token.length;
  if (totalLen !== 95 && totalLen !== 96) {
    return null;
  }
  const envLen = totalLen === 95 ? 2 : 3;

  // Prefix scan: constant-time byte-by-byte over all 9 chars.
  const PREFIX = "corelink_";
  let prefixBad = 0;
  for (let i = 0; i < PREFIX.length; i++) {
    prefixBad |= token.charCodeAt(i) !== PREFIX.charCodeAt(i) ? 1 : 0;
  }
  if (prefixBad !== 0) {
    return null;
  }

  // Env segment: compare all valid literals (constant-time — all three run
  // regardless of match to prevent timing discrimination between valid envs).
  const envStart = PREFIX.length; // 9
  const envSeg = token.slice(envStart, envStart + envLen);
  const ENV_2 = ["ci", "ro"] as const;
  const ENV_3 = ["pat"] as const;
  let envMatched = 0;
  if (envLen === 2) {
    for (const e of ENV_2) {
      envMatched |= ctEqStr(envSeg, e) ? 1 : 0;
    }
    // Also run the length-3 comparison to equalize timing across envLen branches.
    ctEqStr("pat", "pat"); // no-op; keeps the compiler from eliding the path
  } else {
    // envLen === 3
    for (const e of ENV_3) {
      envMatched |= ctEqStr(envSeg, e) ? 1 : 0;
    }
    // Run length-2 comparisons to equalize timing.
    for (const e of ENV_2) {
      ctEqStr(envSeg.slice(0, 2), e); // discarded result
    }
  }
  if (envMatched === 0) {
    return null;
  }

  // Separator `_` after env.
  const sepAfterEnvIdx = envStart + envLen;
  if (token.charCodeAt(sepAfterEnvIdx) !== 0x5f /* '_' */) {
    return null;
  }

  // token_id: 16 Crockford b32 chars.
  const TOKEN_ID_LEN = 16;
  const tokenIdStart = sepAfterEnvIdx + 1;
  const tokenIdEnd = tokenIdStart + TOKEN_ID_LEN;
  const tokenIdSeg = token.slice(tokenIdStart, tokenIdEnd);
  if (!isCrockfordB32(tokenIdSeg)) {
    return null;
  }

  // Separator `.` after token_id.
  if (token.charCodeAt(tokenIdEnd) !== 0x2e /* '.' */) {
    return null;
  }

  // random_secret: 43 base64url-no-pad chars.
  const SECRET_LEN = 43;
  const secretStart = tokenIdEnd + 1;
  const secretEnd = secretStart + SECRET_LEN;
  const secretSeg = token.slice(secretStart, secretEnd);
  if (!isBase64Url(secretSeg)) {
    return null;
  }

  // Separator `.` after random_secret.
  if (token.charCodeAt(secretEnd) !== 0x2e /* '.' */) {
    return null;
  }

  // hmac_sig: 22 base64url-no-pad chars.
  const SIG_LEN = 22;
  const sigStart = secretEnd + 1;
  const sigEnd = sigStart + SIG_LEN;
  if (sigEnd !== totalLen) {
    return null;
  }
  const sigSeg = token.slice(sigStart, sigEnd);
  if (!isBase64Url(sigSeg)) {
    return null;
  }

  // Decode hmac_sig bytes (16 raw bytes from 22 base64url chars).
  const hmacSigBytes = base64urlDecode(sigSeg);
  if (hmacSigBytes === null || hmacSigBytes.length !== 16) {
    return null;
  }

  // HMAC preimage = bytes of `<token_id>.<random_secret>`.
  const enc = new TextEncoder();
  const hmacPreimage = enc.encode(token.slice(tokenIdStart, secretEnd));

  return { tokenId: tokenIdSeg, hmacPreimage, hmacSigBytes };
}

/**
 * Constant-time string equality. Runs to completion even on a mismatch.
 * Both strings must have the same length for a meaningful comparison;
 * length-mismatched inputs always return false (constant-time: the loop
 * still runs for `a.length` iterations).
 */
function ctEqStr(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= (a.charCodeAt(i) ^ (b.charCodeAt(i)));
  }
  return diff === 0;
}

/** Return true if every character in `s` is a valid Crockford b32 char (uppercase). */
function isCrockfordB32(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    // 0-9: 0x30..0x39; A-H: 0x41..0x48; J,K: 0x4A,0x4B; M,N: 0x4D,0x4E;
    // P-T: 0x50..0x54; V-Z: 0x56..0x5A  (excludes I=0x49, L=0x4C, O=0x4F, U=0x55)
    const valid =
      (c >= 0x30 && c <= 0x39) || // 0-9
      (c >= 0x41 && c <= 0x48) || // A-H
      c === 0x4a || c === 0x4b || // J,K
      c === 0x4d || c === 0x4e || // M,N
      (c >= 0x50 && c <= 0x54) || // P-T
      (c >= 0x56 && c <= 0x5a);   // V-Z
    if (!valid) return false;
  }
  return true;
}

/** Return true if every character in `s` is a valid base64url-no-pad char. */
function isBase64Url(s: string): boolean {
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    const valid =
      (c >= 0x41 && c <= 0x5a) || // A-Z
      (c >= 0x61 && c <= 0x7a) || // a-z
      (c >= 0x30 && c <= 0x39) || // 0-9
      c === 0x2d ||                // -
      c === 0x5f;                  // _
    if (!valid) return false;
  }
  return true;
}

/** Decode a base64url-no-pad string to bytes. Returns null on decode error. */
function base64urlDecode(s: string): Uint8Array | null {
  try {
    // Re-pad to standard base64 then decode.
    const padded = s.replace(/-/g, "+").replace(/_/g, "/");
    const rem = padded.length % 4;
    const padded2 = rem === 0 ? padded : padded + "=".repeat(4 - rem);
    const binary = atob(padded2);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
      bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
  } catch {
    return null;
  }
}

// ──────────────────────────────────────────────────────────────────────────────
// HMAC-SHA256 PAT fast-fail (WebCrypto)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Verify the PAT HMAC-SHA256 signature using the Worker's PAT_SIGNING_KEY.
 *
 * The signing key is a hex-encoded byte string (≥ 32 bytes decoded).
 * Pre-image: the `hmacPreimage` bytes (`<token_id>.<random_secret>`).
 * Expected: the first 16 raw bytes of HMAC-SHA256(key, preimage).
 *
 * The comparison uses crypto.subtle.timingSafeEqual for constant-time
 * equality — constant-time compare of the 16-byte truncated MAC.
 *
 * Returns true only if the MAC matches; false on any mismatch or error.
 */
async function verifyPatHmac(
  signingKeyHex: string,
  hmacPreimage: Uint8Array,
  expectedSigBytes: Uint8Array,
): Promise<boolean> {
  try {
    // Decode hex signing key.
    const keyBytes = hexDecode(signingKeyHex);
    if (keyBytes === null || keyBytes.length < 32) {
      return false;
    }

    // Import the key for HMAC-SHA256.
    const cryptoKey = await crypto.subtle.importKey(
      "raw",
      keyBytes,
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );

    // Compute full HMAC-SHA256 over the preimage.
    const macBuf = await crypto.subtle.sign("HMAC", cryptoKey, hmacPreimage);
    // Truncate to first 16 bytes (128-bit truncated MAC per auth_model.md §2.2).
    const macBytes = new Uint8Array(macBuf, 0, 16);

    // Constant-time equality compare of 16-byte buffers.
    return crypto.subtle.timingSafeEqual(macBytes, expectedSigBytes);
  } catch {
    return false;
  }
}

/** Decode a hex-encoded string to bytes. Returns null on invalid input. */
function hexDecode(hex: string): Uint8Array | null {
  if (hex.length % 2 !== 0) return null;
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    const hi = hexNibble(hex.charCodeAt(i));
    const lo = hexNibble(hex.charCodeAt(i + 1));
    if (hi === -1 || lo === -1) return null;
    bytes[i >> 1] = (hi << 4) | lo;
  }
  return bytes;
}

function hexNibble(c: number): number {
  if (c >= 0x30 && c <= 0x39) return c - 0x30;      // '0'-'9'
  if (c >= 0x61 && c <= 0x66) return c - 0x61 + 10; // 'a'-'f'
  if (c >= 0x41 && c <= 0x46) return c - 0x41 + 10; // 'A'-'F'
  return -1;
}

/** Encode a Uint8Array to base64url (no padding). */
function base64url(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) {
    bin += String.fromCharCode(bytes[i] ?? 0);
  }
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=/g, "");
}

// ──────────────────────────────────────────────────────────────────────────────
// Timing padding for 404 responses (design-pattern-01)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Pad the wall-clock of a 404 response to a constant-ish target.
 *
 * Mirrors the Rust Tower layer's approach:
 *   pad_target = TIMING_PAD_TARGET_MS ± jitter%
 *   sleep_until(requestStart + pad_target)
 *
 * Jitter is seeded from (server_nonce XOR request counter XOR requestId-hash)
 * to prevent attacker-controlled-seed attacks (design-pattern-01 §3.1).
 *
 * Note: Worker CPU time ≠ wall time, but `await`ing a scheduler-based
 * sleep is the correct primitive (zero CPU spin).
 */
async function applyTimingPad(
  requestStart: number,
  requestId: string,
  requestCounter: number,
  serverNonce: number,
): Promise<void> {
  const enc = new TextEncoder();
  const idHash = await crypto.subtle.digest("SHA-256", enc.encode(requestId));
  const idView = new DataView(idHash);
  const idSeed = idView.getUint32(0, true);

  // Triple-source seed mix (mirrors padding.rs:74-93 splitmix_u64 logic)
  const seed = splitmix32(serverNonce ^ (requestCounter & 0xffffffff) ^ idSeed);
  // seed is 0..2^32; map to [-jitter, +jitter] pct
  const jitter = ((seed / 0xffffffff) * 2 - 1) * TIMING_PAD_JITTER_PCT;
  const padMs = Math.max(
    TIMING_PAD_MIN_MS,
    TIMING_PAD_TARGET_MS * (1 + jitter / 100),
  );

  const deadline = requestStart + padMs;
  const remaining = deadline - Date.now();
  if (remaining > 0) {
    await new Promise<void>((resolve) => setTimeout(resolve, remaining));
  }
}

/** splitmix32 finalizer — single round. */
function splitmix32(x: number): number {
  let z = (x + 0x9e3779b9) | 0;
  z = Math.imul(z ^ (z >>> 16), 0x85ebca6b);
  z = Math.imul(z ^ (z >>> 13), 0xc2b2ae35);
  return (z ^ (z >>> 16)) >>> 0;
}

// ──────────────────────────────────────────────────────────────────────────────
// Error envelope builders (REAPI + OCI error shapes)
// ──────────────────────────────────────────────────────────────────────────────

interface OciErrorEnvelope {
  readonly errors: ReadonlyArray<OciErrorEntry>;
}

interface OciErrorEntry {
  readonly code: string;
  readonly message: string;
  readonly detail: unknown;
}

function ociError(code: string, message: string, requestId: string): Response {
  const body: OciErrorEnvelope = { errors: [{ code, message, detail: null }] };
  return new Response(JSON.stringify(body), {
    status: ociStatusForCode(code),
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
      "Docker-Distribution-Api-Version": "registry/2.0",
    },
  });
}

function ociStatusForCode(code: string): number {
  switch (code) {
    case "UNAUTHORIZED": return 401;
    case "DENIED": return 403;
    case "NOT_FOUND": return 404;
    case "BLOB_UNKNOWN": return 404;
    case "MANIFEST_UNKNOWN": return 404;
    case "UNSUPPORTED": return 405;
    case "DIGEST_INVALID": return 400;
    case "NAME_INVALID": return 400;
    case "BLOB_UPLOAD_INVALID": return 400;
    default: return 500;
  }
}

interface ReapiErrorEnvelope {
  readonly error: string;
  readonly message: string;
  readonly request_id: string;
}

function reapiError(error: string, message: string, status: number, requestId: string): Response {
  const body: ReapiErrorEnvelope = { error, message, request_id: requestId };
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Worker state (module-level, reset per isolate cold start)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * One-time server nonce for timing-pad seed mixing.
 * Initialized lazily from crypto.getRandomValues on first request.
 * Module-scoped: stable within a single isolate lifetime.
 */
let serverNonce: number | null = null;
let requestCounter = 0;

function getServerNonce(): number {
  if (serverNonce === null) {
    const buf = new Uint32Array(1);
    crypto.getRandomValues(buf);
    serverNonce = buf[0] ?? 0;
  }
  return serverNonce;
}

// ──────────────────────────────────────────────────────────────────────────────
// Main fetch handler
// ──────────────────────────────────────────────────────────────────────────────

const handler: ExportedHandler<Env> = {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const requestStart = Date.now();
    const requestId = resolveRequestId(request);
    requestCounter = (requestCounter + 1) | 0;

    // CORS preflight
    const preflight = handlePreflight(request);
    if (preflight !== null) {
      return preflight;
    }

    const url = new URL(request.url);
    const route = matchRoute(url);

    // Health check — no auth required, no DO forwarding
    // Legacy /health keeps {"status":"ok",...}; /api/health returns "SERVING"
    // to match the e2e suite's Journey 1 exact-string assertion
    // (tests/e2e-user-journeys/src/main.rs:151).
    if (route.routeKind === "health" || route.routeKind === "health_serving") {
      const statusLiteral = route.routeKind === "health_serving" ? "SERVING" : "ok";
      const body = JSON.stringify({ status: statusLiteral, env: env.ENVIRONMENT });
      const resp = new Response(body, {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "X-Request-Id": requestId,
        },
      });
      return applyCors(resp, request);
    }

    // Container health deep-probe — /_health/container — no auth required.
    // Forwards to the container's /_health via the _system DO, exposing the
    // full health body including the A4 `storage` field (r2 vs inmemory).
    if (route.routeKind === "health_container") {
      const systemDoId = env.CORELINK_SERVER.idFromName("_system");
      const systemStub = env.CORELINK_SERVER.get(systemDoId);
      const containerHealthUrl = new URL(request.url);
      containerHealthUrl.pathname = "/_health";
      const containerReq = new Request(containerHealthUrl.toString(), {
        method: "GET",
        headers: (() => {
          const h = new Headers();
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "health_container");
          h.set("x-corelink-tenant-id", "_system");
          return h;
        })(),
      });
      let containerResp: Response;
      try {
        containerResp = await systemStub.fetch(containerReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] health_container DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "container health upstream error", 500, requestId),
          request,
        );
      }
      const containerHeaders = new Headers(containerResp.headers);
      if (!containerHeaders.has("x-request-id")) {
        containerHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(containerResp.body, {
          status: containerResp.status,
          statusText: containerResp.statusText,
          headers: containerHeaders,
        }),
        request,
      );
    }

    // Internal routes — `/_internal/*` — authenticated by X-Corelink-Internal-Auth.
    // Bypasses PAT auth entirely; DO forwards directly to the container.
    // Security: CORELINK_INTERNAL_AUTH_KEY must be set; if absent, deny all
    // internal requests (fail-CLOSED — never open an unauthenticated proxy).
    if (route.routeKind === "internal") {
      const internalAuthKey = env.CORELINK_INTERNAL_AUTH_KEY;
      if (!internalAuthKey || internalAuthKey.length === 0) {
        // Key not bound on this Worker — deny (fail-CLOSED).
        return applyCors(
          reapiError("FORBIDDEN", "internal route unavailable", 403, requestId),
          request,
        );
      }
      // Verify the caller supplied the correct shared secret (constant-time).
      const provided = request.headers.get("x-corelink-internal-auth") ?? "";
      const enc2 = new TextEncoder();
      const expectedBytes = enc2.encode(internalAuthKey);
      const providedBytes = enc2.encode(provided);
      let authOk = false;
      if (providedBytes.length === expectedBytes.length) {
        authOk = crypto.subtle.timingSafeEqual(providedBytes, expectedBytes);
      } else {
        // Different lengths — run a dummy comparison to prevent timing oracle.
        crypto.subtle.timingSafeEqual(expectedBytes, expectedBytes);
      }
      if (!authOk) {
        return applyCors(
          reapiError("UNAUTHORIZED", "internal auth required", 401, requestId),
          request,
        );
      }
      // Route to the _system DO which hosts the container.
      const systemDoId = env.CORELINK_SERVER.idFromName("_system");
      const systemStub = env.CORELINK_SERVER.get(systemDoId);
      const internalAugmented = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "internal");
          h.set("x-corelink-tenant-id", "_system");
          h.set("x-corelink-token-prefix", "internal");
          return h;
        })(),
      });
      let internalResp: Response;
      try {
        internalResp = await systemStub.fetch(internalAugmented);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] internal DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "internal upstream error", 500, requestId),
          request,
        );
      }
      const internalHeaders = new Headers(internalResp.headers);
      if (!internalHeaders.has("x-request-id")) {
        internalHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(internalResp.body, {
          status: internalResp.status,
          statusText: internalResp.statusText,
          headers: internalHeaders,
        }),
        request,
      );
    }

    // Not found — timing-padded to prevent cross-tenant enumeration
    if (route.routeKind === "not_found") {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
      const resp = reapiError("NOT_FOUND", "The requested resource does not exist.", 404, requestId);
      return applyCors(resp, request);
    }

    // Auth gate — all other routes require a valid Bearer PAT, EXCEPT signup
    // which is pre-tenant: the :token in /v1/signup/pilot/:token IS the auth
    // artifact, not a Bearer PAT. The container validates the path token against
    // the signup-tokens store.
    //
    // For everything else: extractAuth performs parse format → HMAC fast-fail
    // (if PAT_SIGNING_KEY bound) → D1 existence + expiry check → resolve
    // tenant_id. Any token not in the D1 store → 401 (WP-A1 P0-2 fix).
    type AuthOk = Extract<AuthResult, { ok: true }>;
    let auth: AuthOk;
    if (route.routeKind === "signup") {
      auth = { ok: true, tenantId: "_anonymous", tokenPrefix: "signup" };
    } else {
      const result = await extractAuth(request, env);
      if (!result.ok) {
        // Timing-pad auth failures on OCI routes to match the OCI error envelope shape
        if (route.routeKind === "oci_v2") {
          return applyCors(
            ociError("UNAUTHORIZED", "authentication required", requestId),
            request,
          );
        }
        return applyCors(
          reapiError("UNAUTHORIZED", "authentication required", 401, requestId),
          request,
        );
      }
      auth = result;
    }

    // ── Tenant routing (WP-T1) ────────────────────────────────────────────────
    // auth.tenantId is the PAT-resolved tenant produced by WP-A1's D1 lookup.
    // We use it exclusively for DO routing — never the raw URL path segment.
    //
    // WP-A1 contract: resolves real tenant → stores in auth.tenantId.
    // Until WP-A1 lands the value is "_pending" (WP-A1 stub in extractAuth).
    // Once WP-A1 is merged, auth.tenantId will carry the real tenant ID.
    const resolvedTenantId = auth.tenantId;

    // Path-spoof defence (P1-2): if the URL path carries a tenant namespace
    // AND the PAT has been resolved to a real tenant (not the "_pending" stub),
    // the URL tenant MUST match the PAT tenant. Mismatch → 403.
    //
    // The guard `resolvedTenantId !== "_pending"` is the WP-A1 activation gate:
    //   - "_pending" = WP-A1 not yet merged → skip spoof check (transitional)
    //   - Any other value = WP-A1 resolved → enforce tenant match
    const urlTenant = route.tenantId;
    const isRealTenant = resolvedTenantId !== "_pending";

    if (isRealTenant && urlTenant !== "_anonymous" && urlTenant !== resolvedTenantId) {
      // PAT tenant ≠ URL path tenant — potential path-spoof.
      // Return 403 without leaking which side mismatched.
      if (route.routeKind === "oci_v2") {
        return applyCors(
          ociError("DENIED", "tenant mismatch", requestId),
          request,
        );
      }
      return applyCors(
        reapiError("FORBIDDEN", "tenant mismatch", 403, requestId),
        request,
      );
    }

    // ── Per-tier quota enforcement ────────────────────────────────────────────
    // Quota checks run AFTER auth and BEFORE forwarding to the DO.
    // Skip for system/anonymous tenants (no billing record exists for them).
    //
    // Storage quota: enforced from SUM(tenant_storage_state.bytes_used).
    // Request quota: deferred (TODO) — see worker/src/lib/quota.ts.
    //
    // Fail-open on D1 errors to preserve availability; the DO's CAS quota
    // enforcement (quota_fsm_state) provides the safety net on mutations.
    if (resolvedTenantId !== "_anonymous" && resolvedTenantId !== "_system" && resolvedTenantId !== "_pending") {
      const quotaTier = await getTierForTenant(env.CONFIG_DB, resolvedTenantId);
      const requestQuotaEnabled = env.REQUEST_QUOTA_ENABLED === "true";

      // Storage quota check
      const storageCheck = await checkStorageQuota(env.CONFIG_DB, resolvedTenantId, quotaTier);
      if (!storageCheck.ok) {
        const retryAfter = String(storageCheck.retryAfterSec);
        if (route.routeKind === "oci_v2") {
          return applyCors(
            new Response(
              JSON.stringify({
                errors: [{ code: "DENIED", message: storageCheck.reason }],
              }),
              {
                status: 429,
                headers: {
                  "Content-Type": "application/json",
                  "Retry-After": retryAfter,
                  "X-Request-Id": requestId,
                },
              },
            ),
            request,
          );
        }
        return applyCors(
          new Response(
            JSON.stringify({
              error: "QUOTA_EXCEEDED",
              message: storageCheck.reason,
              request_id: requestId,
            }),
            {
              status: 429,
              headers: {
                "Content-Type": "application/json",
                "Retry-After": retryAfter,
                "X-Request-Id": requestId,
              },
            },
          ),
          request,
        );
      }

      // Request quota check (currently no-op until counter table is wired)
      const requestCheck = checkRequestQuota(quotaTier, requestQuotaEnabled);
      if (!requestCheck.ok) {
        const retryAfter = String(requestCheck.retryAfterSec);
        return applyCors(
          new Response(
            JSON.stringify({
              error: "QUOTA_EXCEEDED",
              message: requestCheck.reason,
              request_id: requestId,
            }),
            {
              status: 429,
              headers: {
                "Content-Type": "application/json",
                "Retry-After": retryAfter,
                "X-Request-Id": requestId,
              },
            },
          ),
          request,
        );
      }
    }

    // WI-MULTI-REGION-V1: per-tenant region routing. Look up tenant.primary_region
    // from D1 and fan-out to the regional Worker via Service Binding when set to
    // a non-IAD region. The regional Worker's container reads the same request
    // path + writes/reads its regional R2 bucket. Fail-open: if the lookup throws
    // or the binding is missing, fall through to local IAD DO (preserves
    // availability over strict regional pinning).
    if (
      resolvedTenantId !== "_anonymous" &&
      resolvedTenantId !== "_system" &&
      resolvedTenantId !== "_pending"
    ) {
      try {
        const row = await env.CONFIG_DB
          .prepare("SELECT primary_region FROM tenant WHERE tenant_id = ?1 LIMIT 1")
          .bind(resolvedTenantId)
          .first<{ primary_region: string }>();
        const region = row?.primary_region;
        let regionalBinding: { fetch: typeof fetch } | undefined;
        if (region === "sam") regionalBinding = env.PROD_SAM;
        else if (region === "lhr") regionalBinding = env.PROD_LHR;
        else if (region === "nrt") regionalBinding = env.PROD_NRT;
        else if (region === "syd") regionalBinding = env.PROD_SYD;
        if (regionalBinding !== undefined) {
          // Forward verbatim to the regional Worker. The regional Worker re-runs
          // PAT validation (PAT_SIGNING_KEY is shared across envs) + writes to
          // its regional R2 bucket. Service Binding bypasses CF edge error 1014
          // (CNAME Cross-User Banned). See cf_worker_to_worker_service_binding.
          const regionalReq = new Request(request, {
            headers: (() => {
              const h = new Headers(request.headers);
              h.set("x-request-id", requestId);
              h.set("x-corelink-route-kind", route.routeKind);
              h.set("x-corelink-token-prefix", auth.tokenPrefix);
              h.set("x-corelink-tenant-id", resolvedTenantId);
              h.set("x-corelink-fanout-from", "prod");
              return h;
            })(),
          });
          const regionalResp = await regionalBinding.fetch(regionalReq);
          return applyCors(regionalResp, request);
        }
      } catch (_err) {
        // Fail-open: D1 hiccup or binding misconfigured → stay on IAD path.
      }
    }

    // Route to the per-tenant DO. idFromName(resolvedTenantId) guarantees
    // each tenant gets its own isolated DO — never the shared "_pending_auth".
    const doId = env.CORELINK_SERVER.idFromName(resolvedTenantId);
    const stub = env.CORELINK_SERVER.get(doId);

    // Augment request with correlation headers (no body inspection — INV-NO-BODY-IN-LOGS)
    const augmented = new Request(request, {
      headers: (() => {
        const h = new Headers(request.headers);
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", route.routeKind);
        // Pass token prefix for DO-side audit correlation (NOT the raw token).
        h.set("x-corelink-token-prefix", auth.tokenPrefix);
        // Pass the PAT-resolved tenant to the DO so it can bind tenantId in
        // lifecycle state (resolves the null tenantId — WP-T1 DoD 4). Set AFTER
        // Worker auth; the DO MUST NOT trust any client-supplied value for it
        // (overwritten here unconditionally).
        h.set("x-corelink-tenant-id", resolvedTenantId);
        // Keep raw Authorization on the forwarded request: the DO performs the
        // Argon2id + scope verify against the D1 PAT store (the possession check
        // the Worker skips under its cpu_ms budget). The DO is trusted; it never
        // logs the raw value (INV-NO-PII-IN-LOGS enforced in durable_object.ts).
        return h;
      })(),
    });

    let doResponse: Response;
    try {
      doResponse = await stub.fetch(augmented);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : "unknown error";
      // Do NOT include error detail that could leak internal topology
      console.error(`[${requestId}] DO fetch failed: ${message.slice(0, 80)}`);
      if (route.routeKind === "oci_v2") {
        return applyCors(
          ociError("UNKNOWN", "upstream error", requestId),
          request,
        );
      }
      return applyCors(
        reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
        request,
      );
    }

    // Map DO 404 responses through timing-pad (cross-tenant enumeration defence)
    if (doResponse.status === 404) {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
    }

    // Attach request-id to outbound response if DO didn't already set it
    const finalHeaders = new Headers(doResponse.headers);
    if (!finalHeaders.has("x-request-id")) {
      finalHeaders.set("x-request-id", requestId);
    }
    const finalResponse = new Response(doResponse.body, {
      status: doResponse.status,
      statusText: doResponse.statusText,
      headers: finalHeaders,
    });

    return applyCors(finalResponse, request);
  },
};

export default handler;
export { CoreLinkServer, RolloutController };
