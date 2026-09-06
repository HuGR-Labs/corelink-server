/** Authentication, request policy, and timing-pad domain for the edge worker. */

import type { Env } from "./index_common.js";
import { isValidPatSigningKeyHex } from "./lib/pat_signing_key.js";
import { isPatExpiryLive } from "./lib/pat_expiry.js";
import { verifyPatRowCached, type KvReader } from "./lib/pat_verify_cache.js";
import { isTenantSuspended } from "./lib/tenant_suspend_gate.js";
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

export async function extractAuth(
  request: Request,
  env: Env,
  // pip/uv natively emit ONLY URL-embedded HTTP Basic (`https://hugr:<PAT>@host/…`
  // → `Authorization: Basic base64("hugr:<PAT>")`) and NEVER `Authorization:
  // Bearer` — so the pip adapter surface is unusable unless the Worker accepts
  // Basic. When `allowBasicAuth` is true (set ONLY for the `pip` adapter route by
  // the sole caller), a Basic credential is decoded and its PASSWORD is taken as
  // the PAT, then verified through the EXACT same HMAC + D1 path as a Bearer PAT
  // (username is an ignored label — `hugr`). Defaults to false so every other
  // route keeps rejecting non-Bearer schemes with `invalid_scheme` (no bypass:
  // native CAS/AC, browser, and all other adapters are unchanged).
  allowBasicAuth = false,
  // `ctx.waitUntil`, forwarded to the PAT-verify KV write-behind so it survives
  // the response (a bare fire-and-forget kv.put is cancelled → KV never warms).
  waitUntil?: (p: Promise<unknown>) => void,
): Promise<AuthResult> {
  // F18: PAT_SIGNING_KEY is the SOLE possession gate for the native plane (F3/F17).
  // Fail CLOSED and LOUD when it is absent or too short — never silently skip the
  // HMAC check. The 32-byte minimum mirrors the NIST 128-bit floor for symmetric
  // auth secrets. The caller maps this reason to HTTP 503 so operators are alerted.
  const signingKeyRaw = env.PAT_SIGNING_KEY;
  if (!signingKeyRaw || signingKeyRaw.length === 0) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_absent",
        severity: "CRITICAL",
        message: "PAT_SIGNING_KEY is unset — extractAuth failing closed (503). Provision the secret and redeploy.",
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }
  // WP-F2 (MED-1): the PRIMARY key gets the SAME full predicate as its
  // rotation siblings (shared helper — they cannot diverge again). The old
  // length-only gate let a 64+-char NON-hex key through: hexDecode() returned
  // null on every request, HMAC failed silently and ALL legitimate clients
  // took 401s with zero operational signal. Now: fail CLOSED + LOUD (503).
  if (!isValidPatSigningKeyHex(signingKeyRaw)) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_malformed",
        severity: "CRITICAL",
        message: `PAT_SIGNING_KEY is PRESENT but is not a valid signing key (need an even-length all-hex string of >= 64 chars / >= 32 bytes; got hex length ${signingKeyRaw.length}) — failing closed (503). Fix or unset the key and redeploy.`,
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }

  const authHeader = request.headers.get("authorization");
  if (authHeader === null || authHeader.length === 0) {
    return { ok: false, reason: "missing_authorization_header" };
  }

  const bearerPrefix = "Bearer ";
  const basicPrefix = "Basic ";
  let token: string;
  if (authHeader.startsWith(bearerPrefix)) {
    token = authHeader.slice(bearerPrefix.length).trim();
  } else if (allowBasicAuth && authHeader.startsWith(basicPrefix)) {
    // pip-only Basic path (see `allowBasicAuth` doc above). Decode the credential
    // and take the PASSWORD as the PAT. Malformed Basic (not base64, non-UTF8, no
    // `:`, or empty password) is rejected with the SAME 401 shape as an
    // unsupported scheme — never a bypass. From here the token flows through the
    // identical length/char/format/HMAC/D1 checks as a Bearer PAT.
    const basicPat = extractBasicAuthPassword(authHeader.slice(basicPrefix.length).trim());
    if (basicPat === null) {
      return { ok: false, reason: "invalid_scheme" };
    }
    token = basicPat;
  } else {
    return { ok: false, reason: "invalid_scheme" };
  }
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

  // ── Step 3: HMAC-SHA256 fast-fail (PAT_SIGNING_KEY — always required) ───
  // Verify the hmac_sig segment before touching D1. The key is guaranteed
  // non-empty and ≥ 32 decoded bytes by the guard at the top of extractAuth.
  // Pre-image: `<token_id>.<random_secret>` (the same preimage used by the
  // Rust verify crate).
  //
  // ── Possession model (security: H2 — explicit engineering DECISION) ───────
  // This HMAC-SHA256 check IS the sole cryptographic possession gate for the
  // native plane (F3/F17): a caller cannot present a token whose hmac_sig
  // verifies without holding the server signing key — forging a PAT requires
  // that key, not merely a stolen/guessed token_id. This is what makes the
  // scope (H1) and tenant_id we read from D1 trustworthy to forward.
  //
  // We deliberately do NOT additionally Argon2id-verify `random_secret` against
  // the stored `pat_hash` on EVERY request: Argon2id is intentionally expensive
  // (~tens-of-ms) and the Worker runs under a tight cpu_ms budget on the hot
  // cache path — per-request Argon2id would dominate latency for every CAS/AC
  // hit. The HMAC gate already binds possession to the signing key. Per-request
  // Argon2id is acceptable defence-in-depth to add LATER (amortised/cached per
  // token_id) ONLY if signing-key compromise becomes a credible concern — at
  // which point key rotation is the primary response. This is the decided
  // posture, not a TODO. Wiring adapter_pat::PatVerifier onto the native plane
  // as a container-side backstop is tracked as a TODO (F3 fix item 1).
  // Overlap key set: the current key plus any rotation siblings
  // (PAT_SIGNING_KEY_PREV / _NEW). A PAT minted under any of them
  // HMAC-verifies during the rotation overlap window so rotation is not
  // a fleet-wide auth outage.
  //
  // Finding #7 (HIGH) — symmetry with the container's fail-CLOSED stance:
  // a sibling that is ABSENT (unset / empty) is benign and simply skipped,
  // but a sibling that is PRESENT (env var set, non-empty) yet FAILS the
  // validity check (decodes to < 32 bytes / wrong length) is a LOUD FATAL
  // config error — never silently dropped. Silently dropping it would let a
  // one-character typo in a rotation sibling at deploy silently shrink the
  // overlap set (the container's `from_env` already refuses to mount in that
  // case), so we fail CLOSED (503) and alert the operator instead.
  const signingKeySet: string[] = [signingKeyRaw];
  for (const [name, sibling] of [
    ["PAT_SIGNING_KEY_PREV", env.PAT_SIGNING_KEY_PREV],
    ["PAT_SIGNING_KEY_NEW", env.PAT_SIGNING_KEY_NEW],
  ] as const) {
    // ABSENT (undefined / null / empty) → benign skip.
    if (typeof sibling !== "string" || sibling.length === 0) {
      continue;
    }
    // PRESENT but INVALID → LOUD fatal config error, fail CLOSED. Symmetric
    // with the container's `Some(None) => return None` refusal in
    // `adapter_pat::from_env`, which mirrors `hex::decode` + `>= 32 bytes`:
    // a valid signing key is an EVEN-length, all-hex string of >= 64 chars
    // (>= 32 bytes). This rejects ALL of finding #7's named malformations —
    // a short value, an odd-length hex string, and a non-hex typo — none of
    // which must be silently dropped (which would shrink the overlap set).
    // WP-F2: same SHARED predicate as the primary key (was inline here).
    const isValidHexKey = isValidPatSigningKeyHex(sibling);
    if (!isValidHexKey) {
      console.error(
        JSON.stringify({
          event: "pat_signing_key_sibling_malformed",
          severity: "CRITICAL",
          sibling: name,
          message: `${name} is PRESENT but is not a valid signing key (need an even-length all-hex string of >= 64 chars / >= 32 bytes; got hex length ${sibling.length}) — failing closed (503). Fix or unset the rotation sibling and redeploy.`,
        }),
      );
      return { ok: false, reason: "signing_key_not_configured" };
    }
    signingKeySet.push(sibling);
  }
  const hmacOk = await verifyPatHmacMulti(
    signingKeySet,
    parsed.hmacPreimage,
    parsed.hmacSigBytes,
  );
  if (!hmacOk) {
    return { ok: false, reason: "invalid_pat_hmac" };
  }

  // ── Step 4: PAT-row lookup by token_id (replica-read, cached) ────────────
  // Any token whose token_id is not in the D1 store → 401. This kills the
  // "any 32–256 char string accepted" vulnerability (P0-2). The D1 read
  // (`SELECT … FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL`) is the
  // per-request auth cost; trans-continental to the D1 primary it was ~0.5–0.7 s
  // (perf #99). ROOT FIX: route it through a D1 read-replication session
  // (`first-unconstrained` → nearest replica, ~tens of ms globally) with a
  // PRIMARY fallback for read-after-write freshness (a just-minted PAT that has
  // not yet replicated is re-checked on the primary — see readPatRow), so a new
  // token still authenticates immediately while a token revoked on the primary
  // is honored only for the ≤replication-lag window (sub-second; well within
  // ADR-0030's 60 s p99 revocation SLA). Still fronted by the per-isolate 5 s
  // single-flight cache (herd protection). Consulted ONLY here, AFTER the HMAC
  // possession proof; positive results only; D1 stays the source-of-truth.
  // See lib/pat_verify_cache.ts.
  // Feature-detect the Sessions API: on a runtime (or a test double) without
  // read replication, degrade gracefully to the primary handle rather than
  // crash. When present, `first-unconstrained` reads the nearest replica.
  const readSession =
    typeof env.CONFIG_DB.withSession === "function"
      ? env.CONFIG_DB.withSession("first-unconstrained")
      : env.CONFIG_DB;
  // L2: globally-replicated, per-colo KV cache — the latency fix for callers far
  // from the ENAM D1 primary (SAM has no D1 replica region). Feature-detected so
  // a build without the binding just skips L2. Reuses METADATA_KV (`patrow:` prefix).
  const kvBinding = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
  const verify = await verifyPatRowCached(readSession, parsed.tokenId, {
    primaryDb: env.CONFIG_DB,
    ...(kvBinding ? { kv: kvBinding } : {}),
    ...(waitUntil ? { waitUntil } : {}),
  });
  if (verify.kind === "error") {
    // D1 errors (network partition, DB unavailable) must not fail-open. Return a
    // distinct reason; the caller maps d1_lookup_error to 503 (transient,
    // retryable) — still fail-closed (access denied), NOT 401 "bad credentials" (H1).
    return { ok: false, reason: "d1_lookup_error" };
  }
  if (verify.kind === "not_found") {
    // token_id not in D1 (unknown or revoked) — reject.
    return { ok: false, reason: "pat_not_found" };
  }
  const row = verify.row;

  // ── Step 5: Expiry check ──────────────────────────────────────────────────
  // `expires_ms === 0` is the canonical "never expires" sentinel (mint:
  // internal_pat.rs:400) — the container SQL honors it (adapter_pat.rs:115:
  // `expires_ms = 0 OR expires_ms > now`). The edge MUST match, else a no-TTL
  // PAT works in the container but is dead-on-arrival here (split-brain, looks
  // like a forged token). Guard the sentinel.
  if (!isPatExpiryLive(row.expires_ms)) {
    return { ok: false, reason: "pat_expired" };
  }

  // ── Step 6: Tenant fast-suspend gate (go-live GAP G4) ─────────────────────
  // A valid, unexpired PAT is NOT sufficient if its tenant has been suspended
  // or erased (tenant_offboarding_state.state ∈ {suspended, erased}, migration
  // 0046). Without this gate a suspended/abusive tenant keeps full CAS/AC read
  // + write access until every one of its PATs is individually revoked. The
  // check is a three-tier read — L1 isolate → L2 KV (`tsusp:` on METADATA_KV) →
  // L3 D1 (the replica session), mirroring the pat read's L2 (ADR-0070) — so it
  // adds no uncached per-request far-D1 round-trip on the SAM hot path. It reads
  // through the same `first-unconstrained` replica session as the pat lookup
  // (perf #99) and, on a cold L1+L2, serves the rest of the colo's traffic from
  // the edge-local KV. A newly-suspended tenant is honored only for the bounded
  // ≤KV-TTL enforcement window (ADR-0070's ratified trade-off; PAT revoke stays
  // the immediate lever). Fail-OPEN on a D1 fault (availability), but a
  // KNOWN-suspended cached value still denies. The caller maps `tenant_suspended`
  // to 403 (fail-closed, distinct from the 401 bad-credential and 503 infra arms).
  if (
    await isTenantSuspended(readSession, row.tenant_id, {
      ...(kvBinding ? { kv: kvBinding } : {}),
      ...(waitUntil ? { waitUntil } : {}),
    })
  ) {
    return { ok: false, reason: "tenant_suspended" };
  }

  // Resolved tenant_id + scope from D1. The Worker forwards `scope` to the
  // container as the server-trusted `x-corelink-scope` header (H1) so scopes
  // written to D1 are actually ENFORCED downstream (previously they were stored
  // but never read here, leaving every PAT effectively unscoped). NULL scope on
  // older rows is normalised to "" so the container sees an explicit value.
  return {
    ok: true,
    tenantId: row.tenant_id,
    tokenPrefix,
    // ADR-0071: a FIND-ONLY PAT (`pat.find_only = 1`, migration 0093) stores the
    // CHECK-safe base `read-only` but is NARROWED here to the find-missing
    // capability ONLY — the Worker (sole `x-corelink-scope` authority) forwards
    // the literal `find-missing`, so the container grants `can_find_missing()`
    // and 403s CAS read/write. NULL/0 = normal PAT → forward the base scope.
    scope: row.find_only === 1 ? "find-missing" : (row.scope ?? ""),
    // WP5a: carry the narrowed runner-job marker (NULL on normal PATs). The
    // forward sites set the runner-job headers only when this is non-NULL.
    runnerJobAcKey: row.runner_job_ac_key ?? null,
    // Observability only (Server-Timing `auth` desc) — which tier served the row.
    patSource: verify.source,
  };
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
export function parsePat(token: string): PatParsed | null {
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
/**
 * Verify the PAT HMAC against an OVERLAP KEY SET (key_management.md §3.2.1).
 *
 * Mirrors the Rust `verify_hmac_sig_multi`: a PAT minted under ANY key in the
 * set verifies, so an operator can rotate PAT_SIGNING_KEY (incl. rotate-on-
 * compromise) while old-key tokens stay valid through the overlap window —
 * instead of an instant fleet-wide auth outage.
 *
 * Constant-time discipline: every key is evaluated (no early-return on the
 * first match) and the per-key results are folded with a bitwise OR, so the
 * observable latency does not leak WHICH key matched (which would reveal
 * whether a token is on the old vs. new key during rotation). Each per-key
 * compare itself uses crypto.subtle.timingSafeEqual.
 *
 * Fail-closed: an empty key set returns false (nothing verifies). The caller
 * guarantees the current key is present and well-formed before calling.
 */
export async function verifyPatHmacMulti(
  signingKeysHex: string[],
  hmacPreimage: Uint8Array,
  expectedSigBytes: Uint8Array,
): Promise<boolean> {
  // Fail-closed: no key set bound ⇒ nothing verifies.
  if (signingKeysHex.length === 0) {
    return false;
  }
  let matched = 0;
  for (const keyHex of signingKeysHex) {
    // Do NOT early-return on a match: fold every key so the matching-key
    // identity does not leak via timing.
    const ok = await verifyPatHmac(keyHex, hmacPreimage, expectedSigBytes);
    matched |= ok ? 1 : 0;
  }
  return matched !== 0;
}

export async function verifyPatHmac(
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
export function hexDecode(hex: string): Uint8Array | null {
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
export function base64url(bytes: Uint8Array): string {
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
export async function applyTimingPad(
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
