/** Authentication policy and request middleware for the edge worker. */

import type { Env } from "./index_common.js";
import { STORAGE_QUOTA_HEADER } from "./lib/quota.js";

const ONBOARDING_AUTH_KEY_MIN_LENGTH = 32;

/**
 * Resolve the authority for a money onboarding endpoint using the same
 * dedicated-first/shared-fallback contract as the container resolver. A
 * present-but-invalid dedicated value is a configuration error and fails
 * closed; only a truly absent dedicated binding may use the valid shared
 * migration fallback. Neither value is accepted below the 32-character floor.
 */
export function resolveOnboardingAuthKey(pathSuffix: string, env: Env): string | null {
  let dedicatedKey: string | undefined;
  if (pathSuffix === "/v1/onboarding/tier-select") {
    dedicatedKey = env.CORELINK_TIER_SELECT_AUTH_KEY;
  } else if (pathSuffix === "/v1/onboarding/dpa-accept") {
    dedicatedKey = env.CORELINK_DPA_ACCEPT_AUTH_KEY;
  } else {
    // Existing non-money onboarding routes retain the shared contract.
    return env.CORELINK_INTERNAL_AUTH_KEY ?? null;
  }

  if (dedicatedKey !== undefined) {
    return (
      dedicatedKey.length >= ONBOARDING_AUTH_KEY_MIN_LENGTH &&
      dedicatedKey.trim().length > 0
        ? dedicatedKey
        : null
    );
  }
  const shared = env.CORELINK_INTERNAL_AUTH_KEY;
  return (
    shared !== undefined &&
    shared.length >= ONBOARDING_AUTH_KEY_MIN_LENGTH &&
    shared.trim().length > 0
      ? shared
      : null
  );
}

/** Auth extraction result from the Authorization header. */
export type AuthResult =
  | {
      readonly ok: true;
      readonly tenantId: string;
      readonly tokenPrefix: string;
      /**
       * The PAT's D1-resolved `scope` column (security: H1). The Worker is the
       * SOLE authority for this value — it is read from the trusted D1 `pat`
       * mirror, never from the client, and forwarded to the DO/container as the
       * `x-corelink-scope` server-trust header so the container can ENFORCE it.
       * Defaults to `""` for older rows whose `scope` is NULL/absent.
       */
      readonly scope: string;
      /**
       * cf-multitenant WP5a: the D1-resolved `pat.runner_job_ac_key` marking a
       * NARROWED runner-job PAT. `null` = a normal PAT (no narrowing → no extra
       * headers forwarded). A non-null value (`"*"` = deny-DELETE only, or a
       * BLAKE3 hex = also exact-key AC restricted) causes the Worker to forward
       * `x-corelink-runner-job: 1` + `x-corelink-ac-key-allow: <value>` as
       * server-trust headers the container (WP5b) enforces. Read from the trusted
       * D1 `pat` mirror only — never the client.
       */
      readonly runnerJobAcKey: string | null;
      /**
       * Which tier served the PAT row (`l1` isolate / `kv` L2 / `d1` primary).
       * Surfaced into the `Server-Timing` `auth` desc for client latency probes;
       * NOT a trust signal and never forwarded to the container. Absent on the
       * anonymous/signup path (no PAT verify runs there).
       */
      readonly patSource?: "l1" | "kv" | "d1";
    }
  | { readonly ok: false; readonly reason: string };

// ──────────────────────────────────────────────────────────────────────────────
// Constants
// ──────────────────────────────────────────────────────────────────────────────

/**
 * P3 EDGE_DO_METER lease block `L` (tokens a shard leases from the coordinator per
 * refill). Bounds under-serve (≤ #regions·L stranded) and the coordinator-hop rate
 * (~1 hop per L requests). Capped by the tier's own cap at the call-site so a
 * small-cap tier never leases more than its ceiling.
 */
export const DO_METER_LEASE_BLOCK = 1_000;

/** Target p99 wall-clock for 404 timing-padding (ms). Covers slowest arm. */
const TIMING_PAD_TARGET_MS = 80;
/** Jitter range ±% applied to pad target. */
const TIMING_PAD_JITTER_PCT = 15;
/** Minimum pad (ms) — safety floor so sleep is never negative. */
const TIMING_PAD_MIN_MS = 5;

const ALLOWED_ORIGINS = [
  "https://humangr.com",
  "https://corelink-docs.humangr.com",
];

const CORS_HEADERS: ReadonlyArray<readonly [string, string]> = [
  ["Access-Control-Allow-Methods", "GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS"],
  ["Access-Control-Allow-Headers", "Authorization, Content-Type, X-Request-Id, Accept"],
  ["Access-Control-Expose-Headers", "X-Request-Id, X-Corelink-Tenant-Id, Server-Timing"],
  ["Access-Control-Max-Age", "86400"],
];

// ──────────────────────────────────────────────────────────────────────────────
// Request ID middleware
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Allowed charset for a propagated `x-request-id` (F-02): RFC-style token
 * bytes only — letters, digits, `.`, `_`, `-`. The supplied id lands in JSON
 * response bodies AND forwarded request headers, so unconstrained values
 * (control chars, newlines, quotes) enable log-injection / header-shape
 * tampering. A value with any other char is rejected and replaced with a
 * freshly generated id.
 */
const REQUEST_ID_CHARSET = /^[A-Za-z0-9._-]+$/;

/** Generate or propagate a request-id. Never exposes body or PII. */
export function resolveRequestId(request: Request): string {
  const incoming = request.headers.get("x-request-id");
  if (
    incoming !== null &&
    incoming.length > 0 &&
    incoming.length <= 128 &&
    REQUEST_ID_CHARSET.test(incoming)
  ) {
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

export function applyCors(response: Response, request: Request): Response {
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

export function handlePreflight(request: Request): Response | null {
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
// Server-trust header hygiene (security: H4)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * Client-suppliable "server-trust" headers that the Worker is solely
 * responsible for establishing (or that must NEVER be client-set). A client
 * could otherwise SMUGGLE these straight through to the DO/container, which
 * trusts some of them on its admin routes and its internal-auth gate.
 *
 * On EVERY path where the Worker forwards a client request by cloning
 * `request.headers`, these MUST be deleted BEFORE the Worker sets its own
 * verified values (delete-then-set). On data-plane paths the Worker does not
 * set internal-auth, so deleting it makes the container's internal-auth-gated
 * admin routes Worker-unreachable by design (operator-only posture).
 *
 * NOTE: `x-corelink-route-kind` is not listed — the Worker unconditionally
 * `.set()`s it on every forward, so any client value is already overwritten.
 * `x-corelink-token-prefix` IS listed (F-012, overnight red-team): the prior
 * "always overwritten" assumption was FALSE on the OCI / billing-webhook / fabric
 * forward arms (which forward raw and never re-set it), so a client could smuggle
 * a forged token-prefix there. The Worker is the sole legitimate setter (from the
 * resolved PAT), so strip any client value structurally on EVERY forward.
 *
 * `x-corelink-tenant-id` IS listed (structural strip): the Worker always
 * `.set()`s it AFTER strip on PAT-backed, internal, onboarding, and fanout
 * forwards; the OCI and billing-webhook carve-outs explicitly `.delete()` it
 * instead (no tenant on those paths). The per-path explicit `.delete()` calls
 * remain as belt-and-braces but the invariant is now structural — a client can
 * never smuggle a forged tenant-id past this list onto any forward path.
 *
 * `x-corelink-scope` (security: H1) IS listed: unlike the always-overwritten
 * headers above, the Worker only `.set()`s scope on PAT-backed forwards, so it
 * MUST be deleted here too — otherwise a client could SMUGGLE a forged scope
 * (e.g. `admin`) straight through on any path. The Worker is the sole setter of
 * the scope value (read from the trusted D1 `pat.scope`), never the client.
 */
/**
 * DSR destructive-arm MFA step-up freshness window, in MINUTES.
 *
 * The `/v1/privacy/*` erasure/rectification gate in the container
 * (`routes/dsr/portal.rs`) is fail-CLOSED on the Worker-trusted
 * `x-corelink-mfa-verified: 1` marker. The Worker is its SOLE setter, and must
 * only stamp it when the Clerk session's factor-verification age is FRESH —
 * otherwise a stolen/XSS/CSRF long-lived dashboard session could trigger
 * irreversible cross-region tenant-data destruction with NO re-auth.
 *
 * Freshness = `clerkAuth.fvaMinutes` (verified `fva[0]`, minutes since the first
 * factor was last verified) `<= MFA_FVA_FRESH_MAX_MINUTES`. `undefined`
 * (absent/malformed `fva`) is treated as NOT fresh (fail-CLOSED, never `0`),
 * mirroring the session/token-exchange freshness signal (lib/session_exchange.ts).
 * The 5-minute window matches the canonical admin step-up TTL
 * (`corelink_auth::webauthn::admin_step_up_default_ttl()` = 300s) — this is the
 * SAME step-up policy, not a new one.
 */
export const MFA_FVA_FRESH_MAX_MINUTES = 5;

const CLIENT_TRUST_HEADERS: ReadonlyArray<string> = [
  "x-admin-scope",
  "x-admin-principal",
  "x-admin-tenant",
  "x-corelink-internal-auth",
  "x-corelink-fanout-from",
  "x-corelink-scope",
  // Structural tenant-id strip: the Worker is the SOLE setter of
  // x-corelink-tenant-id (from D1-resolved PAT or server constant) on every
  // forward path. Stripping here means no client can smuggle a forged
  // tenant-id regardless of which path is taken.
  "x-corelink-tenant-id",
  // Storage-quota seed strip (storage-quota fail-open fix): the Worker is the
  // SOLE setter of x-corelink-storage-quota-bytes (the tenant's resolved
  // per-tier storage cap, from QUOTAS[tier].storageBytesMax). The container
  // seeds a fresh tenant_storage_state row's bytes_quota from it, so a forged
  // value could let a client seed an arbitrarily-large (or unlimited "0") cap.
  // Strip any client value on every forward — same posture as the tenant-id.
  STORAGE_QUOTA_HEADER,
  // F1: the forgeable client-supplied XFF must be stripped on every forward —
  // the container's signup rate-limit now reads the server-trusted
  // x-corelink-client-ip (set by the Worker from cf-connecting-ip), never XFF.
  "x-forwarded-for",
  // F1: the Worker is the SOLE setter of x-corelink-client-ip (from the
  // unforgeable cf-connecting-ip). Strip any client-supplied value first so a
  // client can never smuggle a forged client IP past the rate-limiter.
  "x-corelink-client-ip",
  // backlog #29 (residency): the Worker is the SOLE setter of
  // x-corelink-primary-region (from the trusted D1 tenant.primary_region) on the
  // regional fan-out path. Strip any client value on EVERY forward so a client
  // can never smuggle a forged residency macro past the container's residency
  // guard. The local DO/container path never sets it (IAD-resident by default).
  "x-corelink-primary-region",
  // F-012 (overnight red-team): the Worker is the sole legitimate setter of
  // x-corelink-token-prefix (the resolved PAT prefix, for log/rate-limit keying).
  // It was NOT structurally stripped and the OCI/billing/fabric arms forward raw
  // without re-setting it, so a client could smuggle a forged prefix there. Strip
  // on every forward — same posture as x-corelink-tenant-id.
  "x-corelink-token-prefix",
  // cf-multitenant WP5a: the NARROWED runner-job PAT markers. The Worker is the
  // SOLE setter of both — it sets them ONLY when the D1-resolved PAT row carries
  // a non-NULL `runner_job_ac_key`, from that trusted value, never the client.
  // A client MUST NOT be able to smuggle a forged `x-corelink-runner-job` (which
  // would falsely mark its request narrowed — harmless) NOR, more importantly, a
  // forged `x-corelink-ac-key-allow` (which could try to widen/redirect the
  // container's exact-key enforcement). Strip both structurally on EVERY forward
  // so only the Worker's D1-derived values ever reach the container.
  "x-corelink-runner-job",
  "x-corelink-ac-key-allow",
  // anti AC-squat (ac-create-only): the create-only (deny-overwrite) marker the
  // Worker sets ONLY for a genuine runner-job cred (from the trusted D1 runner-job
  // narrowing), never the client. A client MUST NOT be able to smuggle a forged
  // `x-corelink-ac-create-only` (harmless if it self-narrows, but the invariant is
  // that ONLY the Worker sets it). Strip it structurally on EVERY forward so only
  // the Worker's runner-job-derived value ever reaches the container.
  "x-corelink-ac-create-only",
  // DSR portal MFA step-up freshness marker: the Worker is the SOLE setter (it
  // stamps `1` on the /v1/privacy/* plane for an edge-verified Clerk session).
  // A client MUST NOT be able to smuggle a forged `x-corelink-mfa-verified` to
  // bypass the destructive-arm (erasure/rectification) step-up gate in
  // routes/dsr/portal.rs — strip it structurally on EVERY forward.
  "x-corelink-mfa-verified",
  // Team RBAC role (migration 0074): the Worker is the SOLE setter of
  // x-corelink-role (the D1-resolved team_member role — `owner`/`admin`/`member`/
  // `viewer`), forwarded on the customer plane so the container can gate
  // role-restricted operations (e.g. OWNER-only account deletion). A client MUST
  // NOT be able to smuggle a forged role to escalate — strip it structurally on
  // EVERY forward so only the Worker's D1-derived value reaches the container.
  "x-corelink-role",
  // PAT issuance lease: only the tenant Durable Object may stamp this after
  // its serialized durable bucket decision. Never forward a client copy.
  "x-corelink-pat-issue-authorized",
];

/**
 * Strip every client-suppliable server-trust header from a forwarded request's
 * Headers. Call this BEFORE any `h.set(...)` of Worker-established trust values
 * (delete-then-set) on every DO/container forward path.
 */
export function stripClientTrustHeaders(h: Headers): void {
  for (const name of CLIENT_TRUST_HEADERS) {
    h.delete(name);
  }
}

/** Decode a pip/uv Basic credential and return only its password (the PAT).
 *
 * Basic auth is accepted exclusively by the pip route. The username is merely
 * the adapter label; the returned password still traverses the canonical PAT
 * format, HMAC, D1, expiry, and suspension checks in `extractAuth`.
 */
export function extractBasicAuthPassword(b64: string): string | null {
  let decoded: string;
  try {
    // `atob` rejects malformed base64. Its latin-1 output is intentional: the
    // canonical PAT path below rejects every non-printable/non-ASCII byte.
    decoded = atob(b64);
  } catch {
    return null;
  }
  const colon = decoded.indexOf(":");
  if (colon < 0) return null;
  const password = decoded.slice(colon + 1);
  return password.length === 0 ? null : password;
}
