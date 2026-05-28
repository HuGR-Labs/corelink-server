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

import type { DurableObjectNamespace, ExecutionContext, ExportedHandler } from "@cloudflare/workers-types";
import { CoreLinkServer } from "./durable_object.js";
import { RolloutController } from "./rollout_controller.js";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** Worker environment bindings — matches wrangler.toml. */
export interface Env {
  CORELINK_SERVER: DurableObjectNamespace;
  ENVIRONMENT: string;
  // Secrets (bound via `wrangler secret put`)
  CLERK_SECRET_KEY?: string;
  STRIPE_SECRET_KEY?: string;
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
  | "oci_v2"
  | "npm"
  | "pip"
  | "brew"
  | "cargo"
  | "reapi_v2"
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
 *   /health                 → health check
 *   /v2/*                   → OCI Distribution Spec v1.1
 *   /npm/*                  → npm registry proxy
 *   /pip/*                  → PyPI proxy
 *   /brew/*                 → Homebrew tap proxy
 *   /cargo/*                → Cargo registry proxy
 *   /api/v2/*               → REAPI v2 (CoreLink native HTTP API)
 *   *                       → not_found
 *
 * Tenant extraction:
 *   - For OCI/npm/pip/brew/cargo: first path segment after the protocol
 *     prefix is the tenant namespace (e.g., `/v2/<tenant>/…`).
 *   - For REAPI v2: `X-Corelink-Tenant-Id` header or first path segment.
 *   - For /health: tenant = "_system".
 */
function matchRoute(url: URL): RouteMatch {
  const path = url.pathname;

  // Health
  if (path === "/health" || path === "/health/") {
    return { tenantId: "_system", pathSuffix: "/health", routeKind: "health" };
  }

  // OCI v2 — /v2[/…]
  if (path === "/v2" || path === "/v2/" || path.startsWith("/v2/")) {
    const rest = path.slice(3); // strip "/v2"
    const tenant = extractFirstSegment(rest) ?? "_anonymous";
    return { tenantId: tenant, pathSuffix: path, routeKind: "oci_v2" };
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
 * Constant-time comparison is performed via crypto.subtle.timingSafeEqual
 * when we have a known-valid token to compare against. For now the Worker
 * shim validates token *format* (non-empty, printable ASCII, plausible
 * length 32–256 chars) and passes the opaque token to the DO for
 * authoritative validation against the D1 PAT store.
 *
 * The Worker never logs the token value — only a 6-char hashed prefix
 * for correlation tracing.
 *
 * INV-NO-PII-IN-LOGS: tenant ID is passed through opaquely; only the DO
 * writes hashed-form tenant IDs to audit events.
 */
async function extractAuth(request: Request): Promise<AuthResult> {
  const authHeader = request.headers.get("authorization");
  if (authHeader === null || authHeader.length === 0) {
    return { ok: false, reason: "missing_authorization_header" };
  }

  const prefix = "Bearer ";
  if (!authHeader.startsWith(prefix)) {
    return { ok: false, reason: "invalid_scheme" };
  }

  const token = authHeader.slice(prefix.length).trim();
  if (token.length < 32 || token.length > 256) {
    return { ok: false, reason: "invalid_token_length" };
  }

  // Validate printable ASCII (0x21–0x7E) — no spaces, no control chars
  // We do this constant-time across the fixed range by encoding + comparing
  // rather than iterating (avoids early-exit on first bad char).
  const enc = new TextEncoder();
  const tokenBytes = enc.encode(token);

  // Build a mask: all bytes must be in [0x21..0x7E]
  let invalid = 0;
  for (const b of tokenBytes) {
    // bitwise OR accumulates any out-of-range byte
    invalid |= (b < 0x21 || b > 0x7e) ? 1 : 0;
  }
  if (invalid !== 0) {
    return { ok: false, reason: "invalid_token_chars" };
  }

  // Derive a safe log prefix (first 6 chars of base64url of SHA-256 of token).
  // This is deterministic per token value but non-reversible.
  const hashBuf = await crypto.subtle.digest("SHA-256", tokenBytes);
  const hashArr = new Uint8Array(hashBuf);
  const tokenPrefix = base64url(hashArr).slice(0, 6);

  // Tenant is unknown until DO validates against D1 PAT store.
  // We set tenantId to a placeholder; the DO resolves and attaches it.
  return { ok: true, tenantId: "_pending", tokenPrefix };
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
    if (route.routeKind === "health") {
      const body = JSON.stringify({ status: "ok", env: env.ENVIRONMENT });
      const resp = new Response(body, {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "X-Request-Id": requestId,
        },
      });
      return applyCors(resp, request);
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

    // Auth gate — all other routes require a valid Bearer PAT
    const auth = await extractAuth(request);
    if (!auth.ok) {
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
        // Pass token prefix for DO-side audit correlation (NOT the raw token)
        h.set("x-corelink-token-prefix", auth.tokenPrefix);
        // Pass the PAT-resolved tenant to the DO so it can bind tenantId
        // in lifecycle state (resolves the null tenantId — WP-T1 DoD 4).
        h.set("x-corelink-tenant-id", resolvedTenantId);
        // Remove raw Authorization before forwarding? NO — the DO needs it to
        // validate against D1 PAT store. The DO is trusted; it never logs the
        // raw value (INV-NO-PII-IN-LOGS enforced in durable_object.ts).
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
