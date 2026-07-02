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
import * as Sentry from "@sentry/cloudflare";
import { scrubSentryEvent } from "./sentry-scrub.js";
import { CoreLinkServer } from "./durable_object.js";
import { RolloutController } from "./rollout_controller.js";
import { EventLogDO } from "./event_log_do.js";
import {
  getTierForTenant,
  checkStorageQuota,
  incrementMonthlyRequestCount,
  requestCapResultForCount,
  storageQuotaHeaderValue,
  FREE_REQUEST_CAP,
  STORAGE_QUOTA_HEADER,
} from "./lib/quota.js";
import { verifyClerkSessionAndResolveTenant } from "./lib/clerk_auth.js";
import { handleSessionExchange, handleTokenExchange } from "./lib/session_exchange.js";
import { handleRunnerMint, handleRunnerRevoke } from "./lib/runner_mint.js";
import { handleAuthRotate } from "./lib/auth_rotate.js";
import { handleTenantLookup } from "./lib/tenant_lookup.js";
import {
  resolveConsumerKey,
  constantTimeSecretEqual,
  type InternalConsumer,
} from "./lib/internal_auth.js";
import { coloForMacro } from "./region-map.js";

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/** Worker environment bindings — matches wrangler.toml. */
export interface Env {
  CORELINK_SERVER: DurableObjectNamespace;
  // ADR-0065 — per-tenant append-only event-log DO (hugit-P2 seam D).
  // One DO instance per tenant: idFromName(tenant_id). Bound in wrangler.toml
  // `[[durable_objects.bindings]]` (name = "EVENT_LOG_DO"). Optional in the
  // type so existing test envs that omit it still typecheck.
  EVENT_LOG_DO?: DurableObjectNamespace;
  ENVIRONMENT: string;
  // D1 CONFIG_DB — control-plane database. Holds the `pat` table queried
  // during PAT validation (WP-A1). Bound in wrangler.toml `[[d1_databases]]`.
  CONFIG_DB: D1Database;
  // Secrets (bound via `wrangler secret put`)
  CLERK_SECRET_KEY?: string;
  // Activate exact-issuer pin: `wrangler secret put CLERK_ISSUER_URL --env prod`
  // Value = Clerk Frontend API / issuer URL, e.g. https://<slug>.clerk.accounts.dev
  // or the prod issuer shown in the Clerk dashboard → API Keys → Frontend API URL.
  // Without this secret the worker falls back to the shape-check (https + "clerk").
  CLERK_ISSUER_URL?: string;
  // ── githugr multi-issuer (exchange/token-exchange paths only) ────────────────
  // The SEPARATE githugr Clerk instance accepted on the exchange seams IN ADDITION
  // to CoreLink's, WITHOUT touching CLERK_ISSUER_URL (so CoreLink's own dashboard +
  // onboarding keep validating CoreLink's instance). Both must be set to arm the
  // githugr path; absent ⇒ githugr sessions simply 401 (no behavior change).
  //   GITHUGR_CLERK_ISSUER_URL — `https://clerk.githugr.com` (the iss to accept).
  //   GITHUGR_CLERK_JWT_KEY    — githugr's PUBLIC instance key (PEM) for networkless
  //                              verify; public, no secret crosses products.
  // PILOT ISOLATION FIX: the former `GITHUGR_TENANT_ID` (a single shared tenant for
  // every githugr user) is REMOVED — a githugr session now provisions-or-looks-up
  // a per-user isolated tenant keyed on the Clerk `sub` (see lib/clerk_auth.ts →
  // verifyGithugrSession / lib/githugr_provision.ts).
  GITHUGR_CLERK_ISSUER_URL?: string;
  GITHUGR_CLERK_JWT_KEY?: string;
  STRIPE_SECRET_KEY?: string;
  // L3 money path — forwarded to the container by the DO (see durable_object.ts);
  // the tier-select route is unmounted (404) without CORELINK_DPA_VERSION, and
  // Stripe checkout needs the per-tier price ids.
  STRIPE_AUTH_MODE?: string;
  STRIPE_WEBHOOK_SECRET?: string;
  STRIPE_PRICE_ID_SOLO?: string;
  STRIPE_PRICE_ID_STARTER?: string;
  STRIPE_PRICE_ID_TEAM?: string;
  STRIPE_PRICE_ID_PRO?: string;
  STRIPE_PRICE_ID_MAX?: string;
  // Runners-tier prices (forwarded to the container's seed handler).
  STRIPE_PRICE_ID_RUNNER_STARTER?: string;
  STRIPE_PRICE_ID_RUNNER_PRO?: string;
  STRIPE_PRICE_ID_RUNNER_TEAM?: string;
  STRIPE_PRICE_ID_RUNNER_SCALE?: string;
  STRIPE_PRICE_ID_RUNNER_MAX?: string;
  CORELINK_DPA_VERSION?: string;
  // PAT HMAC signing key (raw hex, ≥ 32 bytes decoded) — used for the
  // HMAC-SHA256 fast-fail layer in PAT validation (WP-A1 step 2).
  // Bound via: `wrangler secret put PAT_SIGNING_KEY`
  // Key derivation: HKDF-SHA256(CORELINK_MASTER_KEY, "corelink-pat-signing-salt-v1",
  //   b"corelink-v1-pat-signing-key", 32) per key_management.md §3.2.1.
  // REQUIRED: extractAuth() fails closed (503) when this secret is absent
  // or decodes to fewer than 32 bytes. Never optional in any deployed env.
  PAT_SIGNING_KEY: string;
  // OPTIONAL rotation overlap keys (key_management.md §3.2.1, 24h overlap).
  // During a PAT_SIGNING_KEY rotation, bind the OUTGOING key as
  // PAT_SIGNING_KEY_PREV (and/or stage the INCOMING key as
  // PAT_SIGNING_KEY_NEW) so a PAT minted under either sibling still
  // HMAC-verifies through the overlap window — rotation (incl. rotate-on-
  // compromise) is then NOT an instant fleet-wide auth outage. Each is a
  // hex string (≥ 32 bytes decoded); a malformed sibling is ignored (the
  // current key remains the load-bearing gate). Bound via:
  // `wrangler secret put PAT_SIGNING_KEY_PREV` / `..._NEW`.
  PAT_SIGNING_KEY_PREV?: string;
  PAT_SIGNING_KEY_NEW?: string;
  // Stream-5: shared secret for `/_internal/*` (passed to container at boot +
  // verified before forwarding). Bound via:
  // `wrangler secret put CORELINK_INTERNAL_AUTH_KEY`
  //
  // This is the SHARED fallback. The per-consumer key split below mirrors the
  // container's just-merged Rust split (red-team #3): each internal consumer
  // gets its OWN key so a single leak does not unlock every internal surface.
  // Resolution per consumer (see lib/internal_auth.ts `resolveConsumerKey`):
  // use the consumer-specific key iff set AND >= 32 chars; else the shared key
  // iff >= 32; else fail-CLOSED. FROZEN names (identical to the Rust side):
  CORELINK_INTERNAL_AUTH_KEY?: string;
  // Per-consumer internal-auth keys (red-team #3 split). Each falls back to
  // CORELINK_INTERNAL_AUTH_KEY when unset/short. Provisioned by the operator
  // (`wrangler secret put …`) — see LEAD FLAGS in the PR. The container reads
  // the same names on its side.
  CORELINK_PAT_MINT_AUTH_KEY?: string; // gate for `/_internal/pat/mint`
  CORELINK_ADMIN_AUTH_KEY?: string;    // gate for admin `/_internal/*` routes
  CORELINK_ERASE_AUTH_KEY?: string;    // gate for erase `/_internal/*` routes
  CORELINK_RUNNER_MINT_AUTH_KEY?: string; // gate for `/internal/v1/runner/{mint,revoke}` (runner dispatcher; scoped away from signup's pat_mint)
  // Per-tier quota enforcement (worker/src/lib/quota.ts).
  // Storage quota is always enforced for finite-quota tiers.
  // Monthly request-count quota is backed by the monthly_request_counts table
  // (migration 0071) and is ENFORCED BY DEFAULT (fail-CLOSED). It is an explicit
  // opt-OUT kill-switch: enforcement runs (atomic increment-and-check) UNLESS
  // this flag === "true". Set REQUEST_QUOTA_DISABLED="true" ONLY in dev/test to
  // skip the counter; an unset var in prod keeps the contracted cap LIVE.
  REQUEST_QUOTA_DISABLED?: string;
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
  // CAS residency (F7/F8 — 2026-06-13 audit): per-region CAS storage, forwarded
  // to the container via container.start({env}). Region-correct keying; the
  // physical per-region CAS buckets are an infra follow-up.
  R2_CAS_REGION?: string;
  R2_CAS_BUCKET?: string;
  R2_AC_BUCKET_PREFIX?: string;
  R2_TURBO_BUCKET?: string;
  // Container env-contract (forwarded via durable_object.ts container.start):
  // secrets + provider vars the native container reads from its own process env.
  R2_TDK_HEX?: string;
  ERASURE_SALT_KEY?: string;
  ERASURE_ATTESTATION_SEED_HEX?: string;
  ERASURE_ATTESTATION_KEY_ID?: string;
  ERASURE_ATTESTATION_REGION?: string;
  ERASURE_ATTESTATION_SINGLE_REGION?: string;
  AUDIT_CHAIN_SIGNING_SEED_HEX?: string;
  AUDIT_CHAIN_SIGNING_KEY_ID?: string;
  EMAIL_HASH_SALT?: string;
  FABRIC_INTROSPECT_AUTH_KEY?: string;
  FABRIC_INTROSPECT_AUTH_KEY_HUGR?: string; // HuGR toolkits introspect consumer (#398) — forwarded to the container
  BILLING_INGEST_AUTH_KEY?: string; // ASK-2 runner billing usage-push ingest — gate for `/internal/v1/billing/usage`; forwarded to the container
  SIGNUP_TOKEN_KEY?: string;
  CORELINK_OCI_TOKEN_KEY?: string;
  // Legacy alias of CORELINK_OCI_TOKEN_KEY (CAA-360 #8 name drift) — the prod
  // Worker secret. Forwarded to the container, which reads it via the routes.rs
  // `.or_else(...)` fallback. See durable_object.ts forward block.
  HUGR_OCI_TOKEN_KEY?: string;
  CORELINK_PORTAL_RETURN_URL?: string;
  AWS_REGION?: string;
  GCP_REGION?: string;
  CORELINK_BYOK_AZURE_REGION?: string;
  CORELINK_BYOK_AZURE_VAULT_URL?: string;
  CORELINK_BYOK_VAULT_REGION?: string;
  // WI-MULTI-REGION-V1 Service Bindings: prod env can fan-out to the 4
  // regional Workers. Set in [[env.prod.services]] blocks. Used by the
  // per-tenant routing logic: tenant.primary_region in D1 → dispatch via
  // the matching binding. Absent → request stays on IAD (default).
  PROD_SAM?: { fetch: typeof fetch };
  PROD_LHR?: { fetch: typeof fetch };
  PROD_NRT?: { fetch: typeof fetch };
  PROD_SYD?: { fetch: typeof fetch };
  // ── Observability (Sentry error tracking) ───────────────────────────────────
  // OPTIONAL. The Sentry hook (see `export default` at the bottom of this file)
  // is a COMPLETE no-op until the operator sets SENTRY_DSN via
  // `wrangler secret put SENTRY_DSN --env prod` (and per regional env). When
  // unset the SDK init receives an empty DSN and never sends — so tests + the
  // pre-launch posture stay inert. Mirrors apps/analytics-worker/src/index.ts.
  SENTRY_DSN?: string;
  // OPTIONAL release tag surfaced on Sentry events (deploy SHA / version).
  SENTRY_RELEASE?: string;
}

/**
 * Map a `/_internal/*` path to its auth CONSUMER (red-team #3 key split).
 *
 * The container exposes three internal surfaces with distinct blast radii:
 *   - `/_internal/pat/mint`  → `pat_mint` (CORELINK_PAT_MINT_AUTH_KEY)
 *   - `/_internal/admin/*`   → `admin`    (CORELINK_ADMIN_AUTH_KEY)
 *   - `/_internal/dsr/*`     → `erase`    (CORELINK_ERASE_AUTH_KEY)
 *
 * Anything else under `/_internal/*` (e.g. `/_internal/cas/*`) defaults to the
 * most-privileged data-plane consumer, `erase` — its key (and, via fallback,
 * the shared key) gates the CAS delete surface used by the DSR/erasure path.
 * Every consumer falls back to the shared CORELINK_INTERNAL_AUTH_KEY when its
 * dedicated key is unset (see resolveConsumerKey), so this never widens access.
 *
 * `pathSuffix` is the server-derived route path (NOT client-suppliable beyond
 * the URL itself, which already selected the `internal` routeKind).
 */
function internalConsumerForPath(pathSuffix: string): InternalConsumer {
  if (pathSuffix === "/_internal/pat/mint") {
    return "pat_mint";
  }
  if (pathSuffix.startsWith("/_internal/admin/")) {
    return "admin";
  }
  // DSR/erase surface (`/_internal/dsr/*`) and any other internal data-plane
  // route (`/_internal/cas/*`, …) gate on the erase consumer key.
  return "erase";
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
  | "oci_token"
  | "billing_webhook"
  | "fabric_introspect"
  | "billing_ingest"
  | "npm"
  | "pip"
  | "brew"
  | "cargo"
  | "reapi_v2"
  | "customer_v1"
  | "public_attestation"
  | "reapi_v1"
  | "bazel_v2"
  | "turbo_v8"
  | "signup"
  | "onboarding"
  | "session_exchange"
  | "tenant_lookup"
  | "token_exchange"
  | "runner_mint"
  | "runner_revoke"
  | "auth_rotate"
  | "internal"
  | "health_container"
  | "not_found";

/** Auth extraction result from the Authorization header. */
type AuthResult =
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
    }
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
  "https://corelink-admin.humangr.com",
  "https://corelink-app.humangr.com",
  "https://corelink-docs.humangr.com",
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
function resolveRequestId(request: Request): string {
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
];

/**
 * Strip every client-suppliable server-trust header from a forwarded request's
 * Headers. Call this BEFORE any `h.set(...)` of Worker-established trust values
 * (delete-then-set) on every DO/container forward path.
 */
function stripClientTrustHeaders(h: Headers): void {
  for (const name of CLIENT_TRUST_HEADERS) {
    h.delete(name);
  }
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
 *                              audit) — dual-auth (WP-1): CoreLink PAT (tenant from
 *                              PAT) OR Clerk session JWT (tenant from clerk_user_id).
 *                              Checked BEFORE the generic /v1/* arm (specificity order).
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

  // OCI token endpoint — EXACT /token (the second leg of OCI two-leg auth).
  // The OCI client GETs /token with `Authorization: Basic base64(user:<PAT>)`;
  // the container verifies the PAT (Option-B) and mints a short-lived HMAC
  // Bearer. There is NO tenant path segment, so tenantId is the shared "_oci"
  // sentinel (same dedicated DO as /v2/*); the container derives the real
  // tenant from the PAT. The Worker is a pure forwarder here (no PAT gate).
  if (path === "/token") {
    return { tenantId: "_oci", pathSuffix: path, routeKind: "oci_token" };
  }

  // OCI v2 — /v2[/…]
  // OCI has NO tenant path segment: the first /v2/ segment is the repository
  // NAME (e.g. /v2/alpine/blobs/...), NOT a tenant. tenantId is the shared
  // "_oci" sentinel so the dedicated OCI DO is used; the container does its own
  // two-leg auth (Option-B PAT verify at /token, HMAC Bearer on /v2) and
  // derives + namespaces the tenant from the OCI token. pathSuffix is forwarded
  // UNCHANGED.
  if (path === "/v2" || path === "/v2/" || path.startsWith("/v2/")) {
    return { tenantId: "_oci", pathSuffix: path, routeKind: "oci_v2" };
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
  // Auth: dual (WP-1) — a canonical CoreLink PAT takes the generic PAT gate
  // (unchanged), anything else takes the Clerk-session bridge (edge-verified
  // JWT, tenant from clerk_user_id). Customer routes are NOT pre-tenant
  // (unlike signup).
  if (path.startsWith("/v1/customer/") || path === "/v1/customer") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "customer_v1" };
  }

  // Onboarding — /v1/onboarding/* — Clerk-authenticated self-serve flow (tier
  // select checkout, etc.). The browser presents a Clerk SESSION JWT, not a
  // CoreLink PAT, so this arm is verified at the edge (Clerk JWKS) in the fetch
  // handler — NOT via the PAT path. Tenant is resolved there from the verified
  // Clerk user id (not the URL), so urlTenant stays "_anonymous". Checked BEFORE
  // the generic /v1/* arm so onboarding never falls into the PAT-only bucket.
  if (path.startsWith("/v1/onboarding/") || path === "/v1/onboarding") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "onboarding" };
  }

  // Session→token exchange — EXACT /v1/session/exchange (hugit-P2 WP-C, seam C).
  // The caller presents a Clerk SESSION JWT (server-side only, never client-
  // exposed per ADR-0002); the Worker verifies it at the EDGE (same shared
  // pipeline as onboarding/customer) and mints a short-lived tenant-scoped
  // CoreLink PAT by REUSING the container's audited /_internal/pat/mint route.
  // Tenant is NOT in the URL — resolved from the verified Clerk user id — so
  // urlTenant stays "_anonymous". Checked BEFORE the generic /v1/* arm so it is
  // never swallowed into the PAT-required reapi_v1 bucket (the caller holds a
  // session, not a PAT).
  if (path === "/v1/session/exchange") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "session_exchange" };
  }

  // Internal routes — /_internal/* — gated by X-Corelink-Internal-Auth shared
  // secret. NOT gated by PAT auth. Intended for Worker-to-Worker calls
  // (signup-worker → this Worker → DO → container). The shared-secret check
  // is performed in the fetch handler (not in matchRoute) so the route is
  // never accidentally skipped on 404-padding paths.
  //
  // SECURITY NOTE (F4): /_internal/* is currently reachable from the public
  // internet (no WAF rule / CF Access / IP allowlist). The sole gate is the
  // constant-time CORELINK_INTERNAL_AUTH_KEY compare below. A leak of this
  // single shared secret enables any-tenant admin-PAT minting via
  // /_internal/pat/mint. Hardening tracked as F4 follow-up:
  //   (1) Restrict to Service Binding only (remove public route).
  //   (2) Separate per-consumer secrets (mint vs onboarding vs signup-worker).
  //   (3) Add per-tenant authorization to the mint endpoint.
  if (path.startsWith("/_internal/")) {
    return { tenantId: "_system", pathSuffix: path, routeKind: "internal" };
  }

  // corelink-runners fabric introspect — EXACT /internal/v1/auth/introspect (no
  // underscore, per the ratified runners contract — distinct from the /_internal/*
  // family above). The container mounts this route and is the SOLE auth authority,
  // gated by FABRIC_INTROSPECT_AUTH_KEY (a DEDICATED secret, NOT the shared
  // CORELINK_INTERNAL_AUTH_KEY of /_internal/*). So the Worker is a pure
  // pass-through: it forwards the caller's x-corelink-internal-auth (the FABRIC
  // secret) UNCHANGED to the _system DO and applies NO edge gate. (Wiring gap from
  // #261: the container had the route but the Worker never forwarded this path,
  // so introspect 404'd end-to-end — fixed 2026-06-13.)
  if (path === "/internal/v1/auth/introspect") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "fabric_introspect" };
  }

  // resolve-tenant — EXACT /internal/v1/auth/resolve-tenant. The container mounts
  // it on the SAME router as introspect and is the SOLE auth authority, gated by
  // the SAME FABRIC_INTROSPECT_AUTH_KEY (+ optional _HUGR). So this reuses the
  // fabric_introspect pass-through: forward the caller's x-corelink-internal-auth
  // UNCHANGED to the _system DO, NO edge gate. Lets a fabric consumer (githugr)
  // resolve clerk_org_id(=sub)→tenant_id for isolation verification + per-tenant
  // reads. (Same #261-class wiring gap as introspect: the container had the route
  // but the Worker never forwarded this path — it 404'd end-to-end until here.)
  if (path === "/internal/v1/auth/resolve-tenant") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "fabric_introspect" };
  }

  // corelink-runners billing usage-push INGEST — EXACT
  // /internal/v1/billing/usage (no underscore, mirrors the fabric introspect
  // contract). The container mounts this route and is the SOLE auth authority,
  // gated by BILLING_INGEST_AUTH_KEY (a DEDICATED secret, NOT the shared
  // CORELINK_INTERNAL_AUTH_KEY of /_internal/*, NOR the FABRIC_INTROSPECT_AUTH_KEY).
  // So the Worker is a pure pass-through: it forwards the caller's
  // x-corelink-internal-auth (the ingest secret) UNCHANGED to the _system DO and
  // applies NO edge gate (mirrors the introspect / billing-webhook carve-outs).
  if (path === "/internal/v1/billing/usage") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "billing_ingest" };
  }

  // githugr authz #3 — tenant lookup. EXACT /internal/v1/auth/tenant/lookup.
  // Handled AT the Worker (not forwarded): a parameterized D1 read of the
  // tenant by clerk_user_id, gated by the shared CORELINK_INTERNAL_AUTH_KEY (the
  // gate runs inside handleTenantLookup). Distinct from /_internal/* (underscore)
  // and from the runners FABRIC introspect secret above.
  if (path === "/internal/v1/auth/tenant/lookup") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "tenant_lookup" };
  }

  // githugr authz #1 — RFC 8693 token exchange. EXACT
  // /internal/v1/auth/token-exchange. Handled AT the Worker: verifies the Clerk
  // session, 403s on session.tenant ≠ audience (the cross-tenant-write
  // rejection), and mints a ~300s tenant-scoped PAT via the container. Internal-
  // auth gated (githugr backend) AND session-gated (user) — both inside the
  // handler. The caller holds a session + the internal secret, not a PAT, so this
  // is matched BEFORE the generic PAT-required /v1/* arms.
  if (path === "/internal/v1/auth/token-exchange") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "token_exchange" };
  }

  // corelink-runners D-9 — per-job runner PAT mint. EXACT
  // /internal/v1/runner/mint. Handled AT the Worker (like token-exchange):
  // internal-auth gated (pat_mint consumer key + shared fallback), runners-
  // entitlement checked, then a short-TTL tenant-scoped PAT is minted via the
  // container's single mint authority. The trusted DISPATCHER calls this — no
  // Clerk session, no PAT — so it is matched BEFORE the generic /v1/* arms and
  // distinct from the /_internal/* (underscore) family.
  if (path === "/internal/v1/runner/mint") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "runner_mint" };
  }

  // corelink-runners D-9 — runner PAT revoke by pat_id. EXACT
  // /internal/v1/runner/revoke. Internal-auth gated (same pat_mint key); reuses
  // the existing `UPDATE pat SET revoked_at_ms` revocation surface. Called by the
  // dispatcher on job teardown (TTL is the backstop).
  if (path === "/internal/v1/runner/revoke") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "runner_revoke" };
  }

  // clw `auth rotate` seam — atomic mint-new + revoke-old. EXACT
  // /internal/v1/auth/rotate. Internal-auth gated (same pat_mint consumer key as
  // runner-mint/revoke); reads the old `pat` row for its tenant + scope, mints an
  // equivalent new PAT via the container's single mint authority, then revokes the
  // old pat_id via the existing `UPDATE pat SET revoked_at_ms` surface. The clw
  // backend (holding the internal key) calls this — no Clerk session, no PAT — so
  // it is matched BEFORE the generic /v1/* arms and distinct from the /_internal/*
  // (underscore) family.
  if (path === "/internal/v1/auth/rotate") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "auth_rotate" };
  }

  // Stripe billing webhook — EXACT /v1/billing/stripe-webhook (mounted in the
  // container at crates/corelink-container/src/webhook.rs). Stripe authenticates
  // with a `Stripe-Signature` HMAC header, NOT a Bearer PAT, so this is a pure
  // pass-through (mirrors the OCI carve-out): the Worker forwards the RAW body +
  // Stripe-Signature to the container, which is the SOLE authority for verifying
  // the signature (constant-time, replay-windowed) and deriving the tenant from
  // the signed event metadata. There is NO URL tenant — route to the shared
  // "_system" DO. Checked BEFORE the generic /v1/* arm so it is never swallowed
  // into the PAT-required reapi_v1 bucket (which would 401 the un-PAT'd webhook).
  if (path === "/v1/billing/stripe-webhook") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "billing_webhook" };
  }

  // PUBLIC erasure-attestation verifier — /v1/public/* (Artifact 1, WP-C1).
  // UNAUTHENTICATED by design: anyone can verify a GDPR erasure offline (fetch the
  // signed bundle + the region public key, recompute the Ed25519 signature). The
  // container mounts these GET routes OUTSIDE its auth/ratelimit/residency layers
  // (same as the /_internal/* family) and is the SOLE authority. So the Worker is a
  // pure pass-through: NO PAT gate, NO internal-auth. Checked BEFORE the generic
  // /v1/* arm so it is never swallowed into the PAT-required reapi_v1 bucket.
  // tenantId="_anonymous" (no tenant in the URL; an erasure proof is public).
  if (path.startsWith("/v1/public/")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "public_attestation" };
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
 *   4. D1 lookup by `token_id` — SELECT tenant_id, expires_ms, scope FROM pat
 *      WHERE token_id = ?1 AND revoked_at_ms IS NULL LIMIT 1. Any token not in
 *      the D1 store → 401. This kills the "any 32–256 char string accepted"
 *      vulnerability. (Expiry is NOT filtered in SQL — it is checked in
 *      application code in step 5 below, so an expired token is fetched then
 *      rejected, distinguishing `pat_expired` from `pat_not_found`.)
 *   5. Expiry check (application-side): reject when expires_ms <= Date.now().
 *   6. Return resolved tenant_id (UUID string from D1 row).
 *
 * Constant-time discipline:
 *   - Token bytes are scanned in full before any early-return.
 *   - HMAC compare uses crypto.subtle.timingSafeEqual.
 *   - SHA-256 prefix for log correlation is computed from the raw token bytes
 *     (non-reversible; see INV-NO-PII-IN-LOGS).
 *
 * NOTE: Argon2id verification is NOT performed in the CF Worker (OWASP-2024
 * cost parameters m=64MiB, t=3, p=4 exceed the Worker's cpu_ms budget) AND
 * is NOT performed by the Durable Object or the native container plane either
 * (F3/F17). Argon2id is wired only for the cache ADAPTER routes (cargo/brew/
 * npm/pip/oci) via adapter_pat::PatVerifier. The native CAS/AC/Bazel/Turbo/
 * customer plane trusts the Worker-injected x-corelink-tenant-id directly.
 * Possession on the native plane therefore rests SOLELY on this HMAC gate
 * (PAT_SIGNING_KEY required). Wiring adapter_pat::PatVerifier onto the native
 * plane as a container-side second layer is tracked as a TODO (F3 fix item 1).
 *
 * The Worker never logs the token value — only a 6-char hashed prefix
 * for correlation tracing.
 *
 * INV-NO-PII-IN-LOGS: tenant ID is NOT logged at this layer; only the DO
 * writes hashed-form tenant IDs to audit events.
 */
async function extractAuth(request: Request, env: Env): Promise<AuthResult> {
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
  // Validate the decoded key length: the hex string encodes raw bytes, so
  // length/2 gives decoded byte count. A key < 64 hex chars = < 32 bytes.
  if (signingKeyRaw.length < 64) {
    console.error(
      JSON.stringify({
        event: "pat_signing_key_too_short",
        severity: "CRITICAL",
        message: `PAT_SIGNING_KEY decodes to fewer than 32 bytes (hex length ${signingKeyRaw.length}) — failing closed (503).`,
      }),
    );
    return { ok: false, reason: "signing_key_not_configured" };
  }

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
    const isValidHexKey =
      sibling.length >= 64 &&
      sibling.length % 2 === 0 &&
      /^[0-9a-fA-F]+$/.test(sibling);
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

  // ── Step 4: D1 lookup by token_id ────────────────────────────────────────
  // Any token whose token_id is not in the D1 store → 401.
  // This kills the "any 32–256 char string accepted" vulnerability (P0-2).
  interface PatRow {
    tenant_id: string;
    expires_ms: number;
    // H1: the PAT's persisted scope (D1 `pat.scope`, SINGULAR TEXT column).
    // Prod values are `cas:rw` (post back-fill) / historically `admin`. May be
    // NULL on older rows — normalised to "" at the return site below.
    scope: string | null;
  }
  let row: PatRow | null;
  try {
    row = await env.CONFIG_DB
      .prepare("SELECT tenant_id, expires_ms, scope FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL LIMIT 1")
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
  // `expires_ms === 0` is the canonical "never expires" sentinel (mint:
  // internal_pat.rs:400) — the container SQL honors it (adapter_pat.rs:115:
  // `expires_ms = 0 OR expires_ms > now`). The edge MUST match, else a no-TTL
  // PAT works in the container but is dead-on-arrival here (split-brain, looks
  // like a forged token). Guard the sentinel.
  if (row.expires_ms !== 0 && row.expires_ms <= Date.now()) {
    return { ok: false, reason: "pat_expired" };
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
    scope: row.scope ?? "",
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
async function verifyPatHmacMulti(
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
// Error envelope builders (REAPI error shapes)
// ──────────────────────────────────────────────────────────────────────────────

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

const baseHandler: ExportedHandler<Env> = {
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
      // F19: omit `env` — deployment environment must not be disclosed on
      // unauthenticated endpoints. Serve env detail only on internal/authed paths.
      const body = JSON.stringify({ status: statusLiteral });
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
    // Forwards to the container's /_health via the _system DO.
    //
    // Security (L1): the container body includes a `storage` field (`r2` vs
    // `inmemory`) that leaks when prod cold-starts into the InMemory fallback.
    // Strip `storage` from the JSON before returning to unauthenticated callers:
    // parse the container's JSON response, delete `storage`, re-serialize. The
    // liveness `status` field is preserved so monitoring tools still work.
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
      // Strip the `storage` field (L1 fix): parse JSON, delete `storage`,
      // re-serialize. If the body is not valid JSON (container returned an
      // error body or non-JSON), pass it through unmodified — liveness
      // semantics are preserved by the upstream status code.
      let redactedBody: BodyInit;
      try {
        const raw = await containerResp.json() as Record<string, unknown>;
        delete raw["storage"];
        redactedBody = JSON.stringify(raw);
        containerHeaders.set("Content-Type", "application/json");
      } catch {
        // Non-JSON body (e.g. container down, returned plain-text error):
        // fall back to streaming the raw body through without redaction.
        redactedBody = containerResp.body ?? "";
      }
      return applyCors(
        new Response(redactedBody, {
          status: containerResp.status,
          statusText: containerResp.statusText,
          headers: containerHeaders,
        }),
        request,
      );
    }

    // Internal routes — `/_internal/*` — authenticated by X-Corelink-Internal-Auth.
    // Bypasses PAT auth entirely; DO forwards directly to the container.
    //
    // Security (red-team #3): the gate uses the PER-CONSUMER key split, mirroring
    // the container's Rust split — a leak of one consumer's secret must not unlock
    // every internal surface. The consumer is derived from the path prefix; each
    // consumer key falls back to the shared CORELINK_INTERNAL_AUTH_KEY when its
    // dedicated key is unset/short (resolveConsumerKey). If neither qualifies,
    // deny (fail-CLOSED — never open an unauthenticated proxy).
    if (route.routeKind === "internal") {
      const internalConsumer = internalConsumerForPath(route.pathSuffix);
      const internalAuthKey = resolveConsumerKey(env, internalConsumer);
      if (!internalAuthKey || internalAuthKey.length === 0) {
        // No properly sized key bound for this consumer — deny (fail-CLOSED).
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
      // Constant-time auth WITHOUT a secret-length oracle. The previous
      // `providedBytes.length === expectedBytes.length` branch took a timing
      // path that depended on the PROVIDED length (and the else-branch compared
      // the secret to itself, not to the provided bytes) — both distinguishable
      // → a length oracle. Instead: copy the provided bytes into a fixed buffer
      // sized to the EXPECTED length (pad with zeros / truncate the overflow),
      // run exactly ONE timingSafeEqual over equal-length buffers, then AND with
      // a constant-time length-equality bit. No early branch depends on the
      // provided length. Mirrors the padded Rust internal_pat.rs/admin.rs gates.
      const fixed = new Uint8Array(expectedBytes.length);
      const copyLen =
        providedBytes.length < expectedBytes.length
          ? providedBytes.length
          : expectedBytes.length;
      fixed.set(providedBytes.subarray(0, copyLen));
      const bytesEqual = crypto.subtle.timingSafeEqual(fixed, expectedBytes);
      // Length-equality bit — a length mismatch can never authenticate (a wrong
      // length that pads to the same prefix bytes is still rejected). This is a
      // single integer compare, not a per-character path, so it carries no
      // length oracle: timingSafeEqual already ran over equal-length buffers.
      const lenEqual = providedBytes.length === expectedBytes.length;
      const authOk = bytesEqual && lenEqual;
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
          // Strip ALL client-suppliable trust headers BEFORE re-establishing
          // them from server-trusted values (delete-then-set). A client must
          // never smuggle x-admin-* / fanout-from, nor a forged internal-auth.
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "internal");
          h.set("x-corelink-tenant-id", "_system");
          h.set("x-corelink-token-prefix", "internal");
          // Re-set internal-auth from the server secret the Worker just verified
          // the caller against — the container's internal-auth gate requires it.
          h.set("x-corelink-internal-auth", internalAuthKey);
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

    // Onboarding — /v1/onboarding/* — Clerk-authenticated self-serve (tier-select
    // checkout). The browser holds a Clerk SESSION JWT, NOT a CoreLink PAT, so the
    // EDGE is the trust boundary: verify the JWT against Clerk's JWKS (fetched +
    // cached per isolate, keyed by CLERK_SECRET_KEY), resolve the tenant from the
    // user id, then forward to the tenant's DO with the internal-auth contract the
    // container's tier_select route requires (x-corelink-internal-auth +
    // x-corelink-tenant-id). Fail-CLOSED on any missing binding or bad token. The
    // internal-auth secret NEVER leaves the backend and is NEVER accepted from the
    // client (inbound trust headers are stripped before injection).
    if (route.routeKind === "onboarding") {
      const internalAuthKey = env.CORELINK_INTERNAL_AUTH_KEY;
      const clerkSecretKey = env.CLERK_SECRET_KEY;
      if (!internalAuthKey || internalAuthKey.length === 0 || !clerkSecretKey) {
        // A required server secret is unbound — deny (fail-CLOSED).
        return applyCors(
          reapiError("FORBIDDEN", "onboarding route unavailable", 403, requestId),
          request,
        );
      }

      // Verify the Clerk session + resolve the tenant via the SHARED pipeline
      // (worker/src/lib/clerk_auth.ts — dashboard revival WP-1 extraction).
      // The helper carries the full hardened flow verbatim: bearer extraction
      // (401), verifyToken with the shared azp allowlist (M1 post-verify
      // re-assert), issuer exact-pin / shape-check (M2), claims.sub required,
      // and the tenant lookup by clerk_user_id (no row → 403; D1 error → 500).
      // `clerkSecretKey` presence was already asserted by the arm-level
      // fail-CLOSED guard above, so the helper's own secret guard never fires
      // here (onboarding behavior unchanged).
      const onbClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
      if (!onbClerkAuth.ok) {
        return applyCors(onbClerkAuth.response, request);
      }
      const onbTenantId = onbClerkAuth.tenantId;

      // Forward to the tenant's DO. CRITICAL: strip ALL client-supplied trust
      // headers FIRST (the browser must never spoof internal-auth or the tenant
      // id), then set them from server-trusted values. Drop the Clerk JWT — the
      // container authenticates via internal-auth, not the session token.
      const onbDoId = env.CORELINK_SERVER.idFromName(onbTenantId);
      const onbStub = env.CORELINK_SERVER.get(onbDoId);
      const onbAugmented = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          // Strip the FULL set of client-suppliable trust headers (x-admin-*,
          // fanout-from, scope, tenant-id, internal-auth) BEFORE re-establishing
          // them from server-trusted values (delete-then-set).
          stripClientTrustHeaders(h);
          // Belt-and-braces: x-corelink-tenant-id is now in the strip list so
          // the line above already removed any client value; kept for clarity.
          h.delete("x-corelink-tenant-id");
          h.delete("authorization");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "onboarding");
          h.set("x-corelink-token-prefix", "clerk");
          h.set("x-corelink-tenant-id", onbTenantId);
          h.set("x-corelink-internal-auth", internalAuthKey);
          return h;
        })(),
      });
      let onbResp: Response;
      try {
        onbResp = await onbStub.fetch(onbAugmented);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] onboarding DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "onboarding upstream error", 500, requestId),
          request,
        );
      }
      const onbHeaders = new Headers(onbResp.headers);
      if (!onbHeaders.has("x-request-id")) {
        onbHeaders.set("x-request-id", requestId);
      }
      return applyCors(
        new Response(onbResp.body, {
          status: onbResp.status,
          statusText: onbResp.statusText,
          headers: onbHeaders,
        }),
        request,
      );
    }

    // Session→token exchange — POST /v1/session/exchange (hugit-P2 WP-C, seam C).
    // The caller presents a Clerk SESSION JWT; the edge verifies it (shared
    // pipeline) and exchanges it for a short-lived tenant-scoped CoreLink PAT,
    // REUSING the container's audited /_internal/pat/mint via the _system DO.
    // The handler is fully fail-CLOSED (missing secret → 403, bad/expired
    // session → 401, no tenant → 403, upstream fault → 500) and never forwards
    // the session token past the edge. CORS is applied here, mirroring the
    // onboarding/customer arms.
    if (route.routeKind === "session_exchange") {
      const sessResp = await handleSessionExchange(request, env, requestId);
      return applyCors(sessResp, request);
    }

    // githugr authz #3 — tenant lookup (POST /internal/v1/auth/tenant/lookup).
    // Internal-auth gated; parameterized D1 read; fail-CLOSED 404 when no tenant
    // maps to the subject. CORS applied here, mirroring the arms above.
    if (route.routeKind === "tenant_lookup") {
      const lookupResp = await handleTenantLookup(request, env, requestId);
      return applyCors(lookupResp, request);
    }

    // githugr authz #1 — token exchange (POST /internal/v1/auth/token-exchange).
    // Internal-auth + Clerk-session gated; 403 on session.tenant ≠ audience (the
    // cross-tenant-write rejection); mints a ~300s tenant-scoped PAT via the
    // container. Fully fail-CLOSED; never forwards the session token past the edge.
    if (route.routeKind === "token_exchange") {
      const xchgResp = await handleTokenExchange(request, env, requestId);
      return applyCors(xchgResp, request);
    }

    // corelink-runners D-9 — runner PAT mint (POST /internal/v1/runner/mint).
    // Internal-auth gated (pat_mint consumer key + shared fallback) + runners-
    // entitlement checked; mints a job-bounded tenant-scoped PAT via the
    // container's single mint authority. Handled AT the Worker: the handler
    // builds a FRESH server-trusted request to the _system DO (it never forwards
    // the inbound request), so client trust headers can never reach the mint
    // route — the same posture as token-exchange. CORS applied here.
    if (route.routeKind === "runner_mint") {
      const mintResp = await handleRunnerMint(request, env, requestId);
      return applyCors(mintResp, request);
    }

    // corelink-runners D-9 — runner PAT revoke (POST /internal/v1/runner/revoke).
    // Internal-auth gated; revokes by pat_id via the existing
    // `UPDATE pat SET revoked_at_ms` surface (INV-PAT-REVOKE-PROPAGATION). The
    // handler touches CONFIG_DB directly — no inbound headers are forwarded. CORS
    // applied here.
    if (route.routeKind === "runner_revoke") {
      const revokeResp = await handleRunnerRevoke(request, env, requestId);
      return applyCors(revokeResp, request);
    }

    // clw `auth rotate` (POST /internal/v1/auth/rotate). Internal-auth gated
    // (pat_mint consumer key + shared fallback); reads the old PAT's tenant +
    // scope, mints an equivalent new PAT via the container's single mint authority
    // (FRESH server-trusted request to the _system DO — no inbound headers
    // forwarded), then revokes the old pat_id ONLY after the mint succeeds (no
    // zero-valid-PAT window). Fully fail-CLOSED. CORS applied here.
    if (route.routeKind === "auth_rotate") {
      const rotateResp = await handleAuthRotate(request, env, requestId);
      return applyCors(rotateResp, request);
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

    // OCI two-leg auth pass-through — /v2/* and /token.
    // OCI does its OWN auth in the container (Option-B PAT verify at /token,
    // HMAC Bearer on /v2). The Worker is a pure forwarder: no extractAuth, no
    // tenant/scope injection, no path-spoof/quota/region-tenant binding (there
    // is no edge-resolved tenant for OCI). Route to a dedicated shared DO; the
    // container does per-tenant CAS namespacing from the OCI-token tenant.
    if (route.routeKind === "oci_v2" || route.routeKind === "oci_token") {
      const ociDoId = env.CORELINK_SERVER.idFromName("_oci");
      const ociStub = env.CORELINK_SERVER.get(ociDoId);
      const ociReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);          // delete any client-forged x-corelink-*
          // Belt-and-braces: x-corelink-tenant-id is now in the strip list so
          // the delete above already removed any client value. The explicit
          // delete below is kept as belt-and-braces documentation that the OCI
          // pass-through deliberately never sets a tenant-id header.
          h.delete("x-corelink-tenant-id");
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          // F-016 (OCI DoS fairness): the OCI plane has no edge-resolved tenant,
          // so the container's per-request velocity gate falls back to a per-repo
          // key — but to also bound abuse per-SOURCE it needs the UNFORGEABLE
          // client IP. Forward cf-connecting-ip as the server-trusted
          // x-corelink-client-ip (stripClientTrustHeaders above already deleted
          // any client-supplied value, so a client cannot spoof it). The true
          // per-IP EDGE cap is a Cloudflare WAF rule on corelink-oci (infra); this
          // gives the in-container gate the trusted IP to key on in the meantime.
          h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
          // Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope.
          // The OCI adapter derives the tenant from the OCI Bearer/PAT and
          // enforces per-op scope from its own HMAC bearer token. The raw
          // Authorization header (OCI Basic at /token, OCI Bearer at /v2) is
          // preserved by the `new Headers(request.headers)` clone above —
          // stripClientTrustHeaders does NOT remove Authorization.
          return h;
        })(),
      });
      let ociResp: Response;
      try {
        ociResp = await ociStub.fetch(ociReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        // (parity with the PAT-path DO forward catch below).
        console.error(`[${requestId}] OCI DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(ociResp, request);
    }

    // Stripe billing webhook pass-through — POST /v1/billing/stripe-webhook.
    // Mirrors the OCI carve-out: Stripe authenticates with a `Stripe-Signature`
    // HMAC header (NOT a Bearer PAT), so the Worker is a pure forwarder here —
    // no extractAuth, no tenant/scope injection. The container is the SOLE
    // authority: it re-computes the Stripe HMAC over the EXACT raw body bytes
    // (constant-time, replay-windowed) and derives the tenant from the signed
    // event metadata. Route to the shared "_system" DO (the webhook has no URL
    // tenant). CRITICAL CORRECTNESS: forward the body UNCHANGED — `new Request(
    // request, { headers })` preserves the body stream unread, so the bytes the
    // container hashes are byte-identical to what Stripe signed. We do NOT
    // read/clone/parse the body (a re-serialized body would break the signature).
    if (route.routeKind === "billing_webhook") {
      const billingDoId = env.CORELINK_SERVER.idFromName("_system");
      const billingStub = env.CORELINK_SERVER.get(billingDoId);
      const billingReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          // Strip any client-forged x-corelink-* server-trust headers BEFORE we
          // set our own (delete-then-set). stripClientTrustHeaders does NOT
          // remove `stripe-signature` (the webhook's auth) nor `authorization`
          // — both are preserved by the `new Headers(request.headers)` clone.
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", route.routeKind);
          // Belt-and-braces: the container derives the tenant SOLELY from the
          // signed Stripe event, never from a header. x-corelink-tenant-id is
          // now in the strip list so the delete above already removed any client
          // value; the explicit delete below documents that the billing-webhook
          // path deliberately never sets a tenant-id header.
          h.delete("x-corelink-tenant-id");
          h.delete("x-corelink-scope");
          // Deliberately NOT set: x-corelink-tenant-id / x-corelink-scope.
          return h;
        })(),
      });
      let billingResp: Response;
      try {
        billingResp = await billingStub.fetch(billingReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        // Do NOT include error detail that could leak internal topology
        // (parity with the OCI / PAT-path DO forward catches).
        console.error(`[${requestId}] billing webhook DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(billingResp, request);
    }

    // PUBLIC erasure-attestation verifier — /v1/public/* (Artifact 1, WP-C1).
    // Pure pass-through to the _anonymous DO → container, which is the SOLE
    // authority. These are UNAUTHENTICATED GETs (an erasure proof is publicly
    // verifiable) — NO PAT gate, NO internal-auth (mirrors the billing-webhook
    // carve-out, minus any signature). Client-forged x-corelink-* trust headers
    // are stripped; the container routes are mounted outside its auth layers.
    if (route.routeKind === "public_attestation") {
      const pubDoId = env.CORELINK_SERVER.idFromName("_anonymous");
      const pubStub = env.CORELINK_SERVER.get(pubDoId);
      const pubReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "public_attestation");
          h.set("x-corelink-tenant-id", "_anonymous");
          return h;
        })(),
      });
      let pubResp: Response;
      try {
        pubResp = await pubStub.fetch(pubReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] public attestation DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(pubResp, request);
    }

    // corelink-runners fabric introspect — pure pass-through to the _system DO →
    // container, which is the SOLE auth authority (FABRIC_INTROSPECT_AUTH_KEY).
    // The Worker forwards the caller's x-corelink-internal-auth (the FABRIC secret)
    // UNCHANGED and applies NO edge gate (mirrors the billing-webhook carve-out,
    // where the container verifies the Stripe signature). See the matchRoute note.
    if (route.routeKind === "fabric_introspect") {
      const fbDoId = env.CORELINK_SERVER.idFromName("_system");
      const fbStub = env.CORELINK_SERVER.get(fbDoId);
      // Capture the FABRIC secret BEFORE stripping client trust headers.
      const fabricAuth = request.headers.get("x-corelink-internal-auth") ?? "";
      const fbReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "fabric_introspect");
          h.set("x-corelink-tenant-id", "_system");
          // Re-forward the caller's FABRIC secret unchanged — the container's
          // introspect gate is the sole authority; the Worker never inspects it.
          h.set("x-corelink-internal-auth", fabricAuth);
          return h;
        })(),
      });
      let fbResp: Response;
      try {
        fbResp = await fbStub.fetch(fbReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] fabric introspect DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(fbResp, request);
    }

    // corelink-runners billing usage-push ingest — pure pass-through to the
    // _system DO → container, which is the SOLE auth authority
    // (BILLING_INGEST_AUTH_KEY). The Worker forwards the caller's
    // x-corelink-internal-auth (the ingest secret) UNCHANGED and applies NO edge
    // gate (mirrors the fabric_introspect / billing-webhook carve-outs). See the
    // matchRoute note.
    if (route.routeKind === "billing_ingest") {
      const biDoId = env.CORELINK_SERVER.idFromName("_system");
      const biStub = env.CORELINK_SERVER.get(biDoId);
      // Capture the INGEST secret BEFORE stripping client trust headers.
      const ingestAuth = request.headers.get("x-corelink-internal-auth") ?? "";
      const biReq = new Request(request, {
        headers: (() => {
          const h = new Headers(request.headers);
          stripClientTrustHeaders(h);
          h.set("x-request-id", requestId);
          h.set("x-corelink-route-kind", "billing_ingest");
          h.set("x-corelink-tenant-id", "_system");
          // Re-forward the caller's INGEST secret unchanged — the container's
          // ingest gate is the sole authority; the Worker never inspects it.
          h.set("x-corelink-internal-auth", ingestAuth);
          return h;
        })(),
      });
      let biResp: Response;
      try {
        biResp = await biStub.fetch(biReq);
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : "unknown error";
        console.error(`[${requestId}] billing ingest DO fetch failed: ${message.slice(0, 80)}`);
        return applyCors(
          reapiError("INTERNAL_ERROR", "upstream error", 500, requestId),
          request,
        );
      }
      return applyCors(biResp, request);
    }

    // Customer portal dual-auth dispatch (dashboard revival WP-1) —
    // /v1/customer/* accepts EITHER a CoreLink PAT (existing path, byte-identical
    // — handled by the generic PAT gate below) OR a Clerk session JWT (the
    // browser dashboard holds a Clerk session, not a PAT).
    //
    // Dispatch guard: parsePat() accepts ONLY the canonical
    // `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` shape — a Clerk JWT
    // (or any non-PAT bearer) can NEVER parse as one, so any parseable PAT
    // (including expired/revoked ones) falls through to the PAT gate exactly as
    // before; the PAT surface is untouched. The token is derived the same way
    // extractAuth derives it (slice "Bearer " + trim) so the dispatch decision
    // and the PAT gate's parse can never disagree.
    //
    // Clerk arm: verify via the SHARED pipeline (lib/clerk_auth.ts — same M1
    // azp re-assert + M2 issuer pin + tenant lookup as onboarding), then
    // forward to the PER-TENANT DO. Mirrors the onboarding forward but
    // deliberately WITHOUT x-corelink-internal-auth — customer routes resolve
    // the tenant from the server-trust x-corelink-tenant-id header and do not
    // need the internal-auth key (least privilege: the dashboard surface must
    // not carry the operator-grade credential).
    //
    // Storage-quota gate is BYPASSED on this arm (deliberate): an over-quota
    // tenant must still see the dashboard to upgrade — same posture as
    // onboarding (which also never passes through the quota gate).
    if (route.routeKind === "customer_v1") {
      const custAuthz = request.headers.get("authorization") ?? "";
      const custToken = custAuthz.startsWith("Bearer ")
        ? custAuthz.slice("Bearer ".length).trim()
        : "";
      if (parsePat(custToken) === null) {
        const custClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
        if (!custClerkAuth.ok) {
          return applyCors(custClerkAuth.response, request);
        }
        const custTenantId = custClerkAuth.tenantId;

        // Forward to the tenant's DO. CRITICAL: strip ALL client-supplied trust
        // headers FIRST (the browser must never spoof internal-auth or the
        // tenant id), then set them from server-trusted values. Drop the Clerk
        // JWT — the container trusts the Worker-set x-corelink-tenant-id, and
        // the session token must not travel further than the edge.
        const custDoId = env.CORELINK_SERVER.idFromName(custTenantId);
        const custStub = env.CORELINK_SERVER.get(custDoId);
        const custAugmented = new Request(request, {
          headers: (() => {
            const h = new Headers(request.headers);
            // Strip the FULL set of client-suppliable trust headers (x-admin-*,
            // fanout-from, scope, tenant-id, internal-auth) BEFORE re-establishing
            // them from server-trusted values (delete-then-set).
            stripClientTrustHeaders(h);
            h.delete("authorization");
            h.set("x-request-id", requestId);
            h.set("x-corelink-route-kind", "customer_v1");
            h.set("x-corelink-token-prefix", "clerk");
            h.set("x-corelink-tenant-id", custTenantId);
            // Clerk-session callers get the dashboard read-write surface; the
            // Worker is the sole setter (stripClientTrustHeaders deleted any
            // client-supplied value above).
            h.set("x-corelink-scope", "read-write");
            // Deliberately NOT set: x-corelink-internal-auth (least privilege —
            // customer routes don't need the operator-grade credential).
            return h;
          })(),
        });
        let custResp: Response;
        try {
          custResp = await custStub.fetch(custAugmented);
        } catch (err: unknown) {
          const message = err instanceof Error ? err.message : "unknown error";
          console.error(`[${requestId}] customer clerk DO fetch failed: ${message.slice(0, 80)}`);
          return applyCors(
            reapiError("INTERNAL_ERROR", "customer upstream error", 500, requestId),
            request,
          );
        }
        const custHeaders = new Headers(custResp.headers);
        if (!custHeaders.has("x-request-id")) {
          custHeaders.set("x-request-id", requestId);
        }
        return applyCors(
          new Response(custResp.body, {
            status: custResp.status,
            statusText: custResp.statusText,
            headers: custHeaders,
          }),
          request,
        );
      }
      // else: the bearer parses as a canonical PAT — fall through to the
      // generic PAT gate below (byte-identical to the pre-WP-1 behavior).
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
      // Signup is pre-tenant: the path :token IS the auth artifact, not a PAT,
      // so there is no D1-resolved scope — forward an empty scope (H1).
      auth = { ok: true, tenantId: "_anonymous", tokenPrefix: "signup", scope: "" };
    } else {
      const result = await extractAuth(request, env);
      if (!result.ok) {
        // OCI (oci_v2 / oci_token) never reaches here — it is handled by the
        // dedicated pass-through branch ABOVE (which forwards to the container
        // for its own two-leg auth), so this PAT-gate path only sees PAT routes.
        //
        // F18: signing_key_not_configured means PAT_SIGNING_KEY is absent or
        // too short — the operator MUST be alerted via 503 (not 401, which would
        // silently look like a bad client credential). The structured error log
        // is emitted inside extractAuth; here we map to 503 Service Unavailable.
        //
        // H1: d1_lookup_error is a TRANSIENT D1 infra fault (network partition /
        // DB unavailable) raised by the PAT D1 lookup — NOT a bad credential. It
        // MUST map to 503 (retryable) too, otherwise a D1 hiccup makes every
        // client see "bad credentials" → CI failures + spurious PAT rotation +
        // on-call chasing the wrong thing. Genuine bad/unknown PATs
        // (pat_not_found / pat_expired / invalid_*) still fall through to 401.
        if (
          result.reason === "signing_key_not_configured" ||
          result.reason === "d1_lookup_error"
        ) {
          return applyCors(
            reapiError("SERVICE_UNAVAILABLE", "authentication service unavailable", 503, requestId),
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
      // Return 403 without leaking which side mismatched. (OCI never reaches
      // this PAT-gate path — see the dedicated pass-through branch above.)
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
    // Request quota: enforced BY DEFAULT via the monthly_request_counts atomic
    // counter (fail-CLOSED) — disabled only by REQUEST_QUOTA_DISABLED="true"
    // (dev/test). See worker/src/lib/quota.ts.
    //
    // D1-error posture is verb-aware (CAA-360 #25): reads fail OPEN for
    // availability, byte-adding writes (PUT/POST) fail CLOSED so an outage
    // cannot be used to write past the cap. The DO's CAS quota enforcement
    // (quota_fsm_state) provides the deeper safety net on mutations.
    //
    // The resolved per-tier storage cap is also forwarded to the container as
    // the server-trusted STORAGE_QUOTA_HEADER so the container's byte-accounting
    // reservation seeds a FRESH tenant_storage_state row with the REAL cap
    // (not the legacy uncapped `0`). It is set ONLY for a real tenant with a
    // confirmed tier; `null` (system/anon/pending, or a D1-error tier) ⇒ the
    // header is omitted and the container fails closed on an unseeded tenant.
    let storageQuotaHeader: string | null = null;
    if (resolvedTenantId !== "_anonymous" && resolvedTenantId !== "_system" && resolvedTenantId !== "_pending") {
      // Helper: emit the shared 429 quota-exceeded response shape.
      const quotaExceeded = (reason: string, retryAfterSec: number): Response =>
        applyCors(
          new Response(
            JSON.stringify({
              error: "QUOTA_EXCEEDED",
              message: reason,
              request_id: requestId,
            }),
            {
              status: 429,
              headers: {
                "Content-Type": "application/json",
                "Retry-After": String(retryAfterSec),
                "X-Request-Id": requestId,
              },
            },
          ),
          request,
        );

      // Request-count quota is ENFORCED BY DEFAULT (fail-CLOSED). The monthly
      // per-tenant request cap is a CONTRACTED ceiling, so an unset env var in
      // prod must NOT silently disable it (Cluster D). The gate is an explicit
      // opt-OUT kill-switch: enforcement is live UNLESS REQUEST_QUOTA_DISABLED
      // === "true" (set only in dev/test). Mirrors the fail-closed posture of
      // the other quota/security gates in this file.
      const requestQuotaEnabled = env.REQUEST_QUOTA_DISABLED !== "true";

      // ── #11: multi-region fan-out over-count fix ──────────────────────────
      // A regional Worker invocation that is itself an INTERNAL fan-out sub-
      // request of one logical client request must NOT re-meter that request:
      // the PRIMARY Worker already incremented the monthly counter once before
      // it fanned out. Counting again here double-charges multi-region tenants
      // (customer-unfavourable).
      //
      // FORGERY-SAFE (tech-lead review of b5ba30c1): the fan-out marker MUST be
      // a value a client cannot forge. The public edge does NOT ingress-strip
      // x-corelink-fanout-from, so a PRESENCE check on the header alone would let
      // ANY client send `x-corelink-fanout-from: anything` to SKIP metering → a
      // request-quota BYPASS (fail-OPEN — worse than the over-count it replaced).
      // So the marker carries the shared server-to-server secret
      // CORELINK_INTERNAL_AUTH_KEY (bound on [env.prod] AND every regional worker
      // env — prod-sam/lhr/nrt/syd — per ADR-MULTI-REGION-V1 §Consequences and the
      // wrangler.toml per-region secret block). The primary Worker sets the header
      // to that secret AFTER stripClientTrustHeaders on the fan-out forward, and it
      // travels ONLY over the service binding (never to a client). We treat the
      // request as a fan-out ONLY on a CONSTANT-TIME match against that secret:
      // a forged value ("prod", a random guess, "") does NOT match → metering
      // still happens. Fail-SAFE: if the secret is unbound, the match can never
      // succeed → every request meters (no bypass).
      //
      // SCOPE: this gates ONLY the metering (the increment + the request-cap
      // comparison). Tier resolution (getTierForTenant) and the server-trusted
      // STORAGE_QUOTA_HEADER forwarding stay UNCONDITIONAL below — a fan-out
      // sub-request still needs the resolved storage cap forwarded to its
      // regional container, and gating those would re-introduce the regional
      // storage-header regression.
      const fanoutHeader = request.headers.get("x-corelink-fanout-from");
      const isFanout =
        fanoutHeader !== null &&
        typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
        env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
        constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, fanoutHeader);

      // ── rt-nuclear #24: cheap monthly request-count check FIRST ───────────
      // Do the single atomic counter UPSERT (the cheapest D1 op in the quota
      // pipeline) BEFORE the costlier checkStorageQuota (1 SUM read). A
      // $-ceiling-capped / over-quota tenant already over even the LOWEST tier
      // cap (FREE = 500K/mo) is 429'd here WITHOUT re-paying the storage-SUM
      // read on every request all month long. Counting happens exactly once
      // (no double increment).
      //
      // Correctness vs. paid tenants: when the post-increment count is within
      // the FREE cap it is within EVERY tier's cap, so the request gate is
      // already cleared regardless of tier (`withinFreeCap`). Only when the
      // count EXCEEDS the free cap does the tier matter — a paid tenant has
      // real headroom and must NOT be rejected; a free tenant gets the 429.
      // getTierForTenant is resolved unconditionally because the SERVED path
      // needs the tier for both the storage check and the server-trusted
      // STORAGE_QUOTA_HEADER forwarded to the container.
      // #11: skip the metering UPSERT on an internal fan-out sub-request (the
      // primary Worker already counted this logical request). `counted:false`
      // yields `withinFreeCap === true`, so the request-cap comparison below is
      // also skipped — a fan-out sub-request is never re-metered nor 429'd on
      // the request cap.
      const inc = isFanout
        ? { counted: false, count: 0 }
        : await incrementMonthlyRequestCount(
            env.CONFIG_DB,
            resolvedTenantId,
            requestQuotaEnabled,
          );
      const withinFreeCap = !inc.counted || inc.count <= FREE_REQUEST_CAP;

      // UNCONDITIONAL (also on fan-out): the served path — including a fan-out
      // sub-request forwarding to its regional container — needs the resolved
      // tier for the server-trusted STORAGE_QUOTA_HEADER. Gating these on
      // !isFanout would re-introduce the regional storage-header regression.
      const quotaTier = await getTierForTenant(env.CONFIG_DB, resolvedTenantId);
      storageQuotaHeader = storageQuotaHeaderValue(quotaTier);

      // Monthly request-count quota (red-team #5): compare the already-counted
      // value against the tenant's RESOLVED tier cap. No re-increment. Within
      // the free cap → already within every cap, skip the comparison. Over the
      // free cap → a free tenant rejects HERE, before the storage SUM below; a
      // paid tenant under its (higher) cap passes through.
      if (!withinFreeCap) {
        const requestCheck = requestCapResultForCount(inc.count, quotaTier.tier);
        if (!requestCheck.ok) {
          return quotaExceeded(requestCheck.reason, requestCheck.retryAfterSec);
        }
      }

      // A storage-increasing op is a write verb (PUT uploads / POST). DELETE
      // reduces storage and reads (GET/HEAD) cannot grow it, so both stay
      // available during a D1 outage.
      const isStorageMutating = request.method === "PUT" || request.method === "POST";

      // Storage quota check. (OCI never reaches this PAT-gate path — see the
      // dedicated pass-through branch above; the container enforces OCI quota.)
      const storageCheck = await checkStorageQuota(
        env.CONFIG_DB,
        resolvedTenantId,
        quotaTier,
        isStorageMutating,
      );
      if (!storageCheck.ok) {
        return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
      }
    }

    // WI-MULTI-REGION-V1: per-tenant region routing. Look up tenant.primary_region
    // from D1, map the MACRO code to its serving colo via the FROZEN region map,
    // and fan-out to the regional Worker via Service Binding when the colo is
    // non-IAD. (backlog #29 — Schrems II residency leak.)
    //
    // The previous code routed by literal colo strings ("lhr"/"nrt"/"syd") that
    // the macro `primary_region` NEVER equals (D1 holds wnam/enam/weur/sam/...),
    // so EU `weur` tenants never matched "lhr" and stayed on IAD = US storage.
    // The colo map removes that vocabulary mismatch.
    //
    // FAIL-CLOSED for non-IAD residency regions: if the macro maps to a non-IAD
    // colo but the matching Service Binding is missing, OR if the D1 lookup
    // throws, we return 503 — we DO NOT fall through to the IAD path, because
    // that fall-through is precisely the cross-border leak. wnam/enam map to IAD
    // (the local path) so they legitimately fall through to the DO below.
    // F-014: recompute the forgery-safe fan-out marker in THIS scope (the metering
    // block's isFanout above is scoped to that sibling block). Same constant-time
    // secret check — a fanned-out request (already on a regional Worker) must not
    // re-route through the region fan-out below (the regional envs lack PROD_*
    // bindings → 503). Pure check, no side effects.
    const regionFanoutHeader = request.headers.get("x-corelink-fanout-from");
    const isFanout =
      regionFanoutHeader !== null &&
      typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
      env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
      constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, regionFanoutHeader);

    // F-015 (overnight red-team): primaryRegion is hoisted OUT of the block so the
    // LOCAL DO path below can also stamp x-corelink-primary-region (not just the
    // fan-out path). Without that stamp, a client hitting a public regional host
    // (e.g. lhr.corelink-api.humangr.com) directly with a non-matching tenant
    // reaches that region's container with NO residency header → the container
    // guard's absent→Allow → cross-region placement. Stamping it lets the
    // container backstop (residency.rs) reject the mismatch.
    let primaryRegion: string | undefined;
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
        primaryRegion = row?.primary_region;
      } catch (_err) {
        // D1 hiccup: we cannot establish residency. FAIL-CLOSED — refuse rather
        // than risk routing an EU tenant to US storage on a transient error.
        return applyCors(
          new Response(
            JSON.stringify({
              error: "RESIDENCY_UNAVAILABLE",
              message: "could not resolve tenant data-residency region",
              request_id: requestId,
            }),
            {
              status: 503,
              headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
            },
          ),
          request,
        );
      }

      const colo = primaryRegion !== undefined ? coloForMacro(primaryRegion) : undefined;
      // Non-IAD residency: must fan-out to the matching regional Service Binding.
      // colo === undefined here means an unknown/unprovisioned macro (e.g. afr) —
      // also fail-closed (never serve such a tenant from IAD).
      // F-014 (overnight red-team): gate the fan-out on !isFanout. A request that
      // ALREADY arrived as a fan-out (isFanout — running on a regional Worker)
      // must NOT re-route: the regional envs lack the PROD_* service bindings, so
      // re-entering this branch would hit `regionalBinding === undefined` → 503 for
      // every non-IAD tenant. A fanned-out request falls through to the local DO
      // path on the regional Worker (which serves its own region) instead.
      if (!isFanout && primaryRegion !== undefined && colo !== "iad") {
        let regionalBinding: { fetch: typeof fetch } | undefined;
        if (colo === "lhr") regionalBinding = env.PROD_LHR;
        else if (colo === "sam") regionalBinding = env.PROD_SAM;
        else if (colo === "nrt") regionalBinding = env.PROD_NRT;

        if (regionalBinding === undefined) {
          // FAIL-CLOSED: non-IAD residency but the regional binding is missing
          // (or the macro is unprovisioned/unknown). Refuse — do NOT fall
          // through to IAD, which would store the tenant's data cross-border.
          return applyCors(
            new Response(
              JSON.stringify({
                error: "RESIDENCY_UNAVAILABLE",
                message: `data-residency region '${primaryRegion}' is not currently servable`,
                request_id: requestId,
              }),
              {
                status: 503,
                headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
              },
            ),
            request,
          );
        }

        // Forward verbatim to the regional Worker. The regional Worker re-runs
        // PAT validation (PAT_SIGNING_KEY is shared across envs) + writes to
        // its regional R2 bucket. Service Binding bypasses CF edge error 1014
        // (CNAME Cross-User Banned). See cf_worker_to_worker_service_binding.
        const regionalReq = new Request(request, {
          headers: (() => {
            const h = new Headers(request.headers);
            // Strip ALL client-suppliable trust headers BEFORE the Worker
            // sets its own (delete-then-set). The Worker legitimately sets
            // x-corelink-fanout-from + x-corelink-primary-region below from
            // server-trusted values.
            stripClientTrustHeaders(h);
            h.set("x-request-id", requestId);
            h.set("x-corelink-route-kind", route.routeKind);
            h.set("x-corelink-token-prefix", auth.tokenPrefix);
            h.set("x-corelink-tenant-id", resolvedTenantId);
            // Storage-quota fail-open fix: forward the resolved per-tier storage
            // cap so the regional container seeds a fresh tenant_storage_state
            // row with the REAL cap (not uncapped `0`). Omitted when null
            // (system/anon/pending or D1-error tier) ⇒ container fails closed.
            // stripClientTrustHeaders above already deleted any client value.
            if (storageQuotaHeader !== null) {
              h.set(STORAGE_QUOTA_HEADER, storageQuotaHeader);
            }
            // H1: forward the D1-resolved PAT scope as a server-trust header.
            // stripClientTrustHeaders above already deleted any client value.
            h.set("x-corelink-scope", auth.scope);
            // F1: forward CF's unforgeable client IP as x-corelink-client-ip
            // (the client-forgeable x-forwarded-for was stripped above) so the
            // regional Worker/container rate-limits signup off a trusted IP.
            h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
            // #11 (forgery-safe): mark the internal fan-out with the shared
            // server-to-server secret CORELINK_INTERNAL_AUTH_KEY, NOT a guessable
            // literal. The regional Worker accepts the fan-out (and so SKIPS
            // re-metering) only on a constant-time match against this same secret
            // (see the isFanout gate above) — a client-forged x-corelink-fanout-from
            // can never match, so it can never bypass metering. The value flows
            // ONLY over this service binding (set AFTER stripClientTrustHeaders),
            // never back to a client. If the secret is unbound the header is
            // omitted ⇒ the regional Worker meters (fail-SAFE over-count, never a
            // bypass).
            if (env.CORELINK_INTERNAL_AUTH_KEY && env.CORELINK_INTERNAL_AUTH_KEY.length > 0) {
              h.set("x-corelink-fanout-from", env.CORELINK_INTERNAL_AUTH_KEY);
            }
            // backlog #29: the trusted residency macro the container's residency
            // guard cross-checks against its own R2_CAS_REGION (defence-in-depth
            // against a mis-bound regional Worker). Set AFTER the strip so no
            // client value survives.
            h.set("x-corelink-primary-region", primaryRegion as string);
            return h;
          })(),
        });
        const regionalResp = await regionalBinding.fetch(regionalReq);
        return applyCors(regionalResp, request);
      }
      // colo === "iad" (wnam/enam) or primaryRegion undefined (tenant with no
      // row): fall through to the local IAD DO path below.
    }

    // Route to the per-tenant DO. idFromName(resolvedTenantId) guarantees
    // each tenant gets its own isolated DO — never the shared "_pending_auth".
    const doId = env.CORELINK_SERVER.idFromName(resolvedTenantId);
    const stub = env.CORELINK_SERVER.get(doId);

    // Augment request with correlation headers (no body inspection — INV-NO-BODY-IN-LOGS)
    const augmented = new Request(request, {
      headers: (() => {
        const h = new Headers(request.headers);
        // Security (H4): strip ALL client-suppliable server-trust headers on the
        // data-plane forward. The Worker does NOT set internal-auth here, so the
        // container's internal-auth-gated admin routes become Worker-unreachable
        // by design — admin is operator-only (internal-auth path) posture.
        stripClientTrustHeaders(h);
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", route.routeKind);
        // Pass token prefix for DO-side audit correlation (NOT the raw token).
        h.set("x-corelink-token-prefix", auth.tokenPrefix);
        // Pass the PAT-resolved tenant to the DO so it can bind tenantId in
        // lifecycle state (resolves the null tenantId — WP-T1 DoD 4). Set AFTER
        // Worker auth; the DO MUST NOT trust any client-supplied value for it
        // (overwritten here unconditionally).
        h.set("x-corelink-tenant-id", resolvedTenantId);
        // Storage-quota fail-open fix: forward the resolved per-tier storage cap
        // so the container's byte-accounting reservation seeds a fresh
        // tenant_storage_state row with the REAL cap (not the legacy uncapped
        // `0`). Omitted when null (system/anon/pending or a D1-error tier) ⇒ the
        // container fails closed on an unseeded tenant (absence ≠ unlimited).
        // stripClientTrustHeaders above already deleted any client value (the
        // Worker is the sole setter — exactly like x-corelink-tenant-id).
        if (storageQuotaHeader !== null) {
          h.set(STORAGE_QUOTA_HEADER, storageQuotaHeader);
        }
        // H1: forward the D1-resolved PAT scope as a server-trust header so the
        // container can ENFORCE it. stripClientTrustHeaders above already deleted
        // any client-supplied x-corelink-scope (the Worker is the sole setter).
        h.set("x-corelink-scope", auth.scope);
        // F1: forward Cloudflare's UNFORGEABLE client IP as the server-trusted
        // x-corelink-client-ip so the container's signup rate-limit keys off it
        // (NOT the client-forgeable x-forwarded-for, which stripClientTrustHeaders
        // already deleted above). cf-connecting-ip is set by the CF edge and a
        // client cannot spoof it. The signup routeKind reaches THIS forward.
        h.set("x-corelink-client-ip", request.headers.get("cf-connecting-ip") ?? "");
        // Keep raw Authorization on the forwarded request: the DO proxies it
        // to the container. NOTE (F3/F17): the native plane (CAS/AC/Bazel/Turbo)
        // does NOT perform Argon2id re-verify — possession rests solely on the
        // Worker's HMAC gate above. The DO is trusted; it never logs the raw
        // value (INV-NO-PII-IN-LOGS enforced in durable_object.ts).
        // TODO(F3): wire adapter_pat::PatVerifier onto the native plane as a
        // container-side second possession layer (Option-B extension).
        // F-015: stamp the trusted residency macro on the LOCAL path too (not only
        // the fan-out path), so the container's residency backstop (residency.rs)
        // can cross-check it against its own R2_CAS_REGION. For an IAD-resident
        // tenant on the IAD container this maps enam/wnam→iad==iad → Allow (no
        // change); for a client that picked a regional public host with a
        // non-matching tenant it maps to a different colo → 409 residency_violation.
        // Omitted for anon/system/pending (primaryRegion undefined). stripClientTrustHeaders
        // already deleted any client-supplied value (the Worker is the sole setter).
        if (primaryRegion !== undefined) {
          h.set("x-corelink-primary-region", primaryRegion);
        }
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
      // OCI never reaches this PAT-gate DO forward — see the dedicated
      // pass-through branch above (which has its own DO forward + error path).
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

// ──────────────────────────────────────────────────────────────────────────────
// Sentry error-tracking wrapper (observability)
// ──────────────────────────────────────────────────────────────────────────────
//
// Mirrors apps/analytics-worker/src/index.ts EXACTLY: init is gated on
// `env.SENTRY_DSN` (empty DSN ⇒ the SDK treats init as a no-op), so the hook is
// COMPLETELY inert until the operator sets the secret. `withSentry` captures any
// unhandled error thrown out of the fetch handler before the runtime 500s, with
// no behavior change to the (already error-mapped) success paths.
//
// INV-NO-PII-IN-LOGS: scrub PII/secrets from every event before it leaves the
// Worker — not just header KEYS but message/exception bodies + extra/contexts
// VALUES (see ./sentry-scrub.ts) — and sendDefaultPii=false so Sentry never
// auto-attaches request bodies / IPs.

const handler = Sentry.withSentry(
  (env: Env) => ({
    // Empty string when the secret is unset → Sentry SDK init is a no-op.
    dsn: env.SENTRY_DSN ?? "",
    environment: env.ENVIRONMENT,
    release: env.SENTRY_RELEASE ?? "unknown",
    sendDefaultPii: false,
    tracesSampleRate: 0.1,
    sampleRate: 1.0,
    beforeSend(event: Sentry.ErrorEvent) {
      return scrubSentryEvent(event);
    },
    beforeSendTransaction(event) {
      return scrubSentryEvent(event);
    },
  }),
  // `@sentry/cloudflare` re-bundles `@cloudflare/workers-types`; the cast keeps
  // both type graphs happy without weakening the inner handler's types.
  baseHandler as unknown as Parameters<typeof Sentry.withSentry>[1],
) as ExportedHandler<Env>;

export default handler;
export { CoreLinkServer, RolloutController, EventLogDO };
