/** Canonical URL route table for the edge worker. */

/** Route matching is isolated so auth/forwarding code cannot silently alter precedence. */

export interface RouteMatch {
  readonly tenantId: string;
  readonly pathSuffix: string;
  readonly routeKind: RouteKind;
}

export type RouteKind =
  | "health" | "health_serving" | "oci_v2" | "oci_token" | "billing_webhook"
  | "fabric_introspect" | "billing_ingest" | "npm" | "pip" | "brew" | "cargo"
  | "reapi_v2" | "customer_v1" | "devenv_v1" | "openapi" | "openapi_devenv"
  | "public_attestation" | "reapi_v1" | "bazel_v2" | "turbo_v8" | "signup"
  | "onboarding" | "session_exchange" | "tenant_lookup" | "token_exchange"
  | "runner_mint" | "runner_close_generation" | "runner_revoke"
  | "auth_rotate" | "internal"
  | "health_container" | "health_container_authed" | "not_found";

/**
 * True only for the pilot-signup endpoint (with an optional token segment).
 *
 * The base path is retained for compatibility with older callers, while the
 * canonical public endpoint is `/v1/signup/pilot/{token}`. Do not broaden
 * this to every `/v1/signup/*` path: those paths are distinct signup flows
 * and must not consume the pilot programme's IP quota.
 */
export function isPilotSignupPath(path: string): boolean {
  const base = "/v1/signup/pilot";
  if (path === base) return true;
  const token = path.slice(base.length + 1);
  return path.startsWith(`${base}/`) && token.length > 0 && !token.includes("/");
}

const INTERNAL_DO_D1_PROBE_PREFIX = "/_internal/do-d1-probe/";

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
export function matchRoute(url: URL): RouteMatch {
  const path = url.pathname;

  // Container health deep-probe — the anonymous variant forwards through the
  // _system DO but redacts storage backing below. Operators use the explicit
  // authenticated variant when they need that diagnostic signal.
  if (path === "/_health/container" || path === "/_health/container/") {
    return { tenantId: "_system", pathSuffix: "/_health", routeKind: "health_container" };
  }
  if (
    path === "/_health/container/authenticated" ||
    path === "/_health/container/authenticated/"
  ) {
    return {
      tenantId: "_system",
      pathSuffix: "/_health",
      routeKind: "health_container_authed",
    };
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

  // Stock-Bazel HTTP cache alias — /bazel/cache/{cas,ac}/<hash>
  // What vanilla `bazel --remote_cache=https://host/bazel/cache` actually sends:
  // GET/PUT on /cas/<hash> and /ac/<hash> with NO tenant segment in the URL.
  // Buck2 is NOT a client of this alias — it speaks REAPI over gRPC only.
  // Unlike /bazel/v2/<instance>/…, the tenant is NOT in the path — it is
  // resolved from the PAT and injected as
  // x-corelink-tenant-id (tenantId="_anonymous" defers to the PAT, exactly like
  // turbo_v8 / reapi_v1, and skips the URL-vs-PAT spoof check). Reuses the
  // bazel_v2 routeKind: same container router, same PAT gate + metering + DO.
  if (path.startsWith("/bazel/cache/")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "bazel_v2" };
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

  // Public OpenAPI 3.1 schema.
  //
  //   /openapi.json          → the CoreLink API contract (openapi/corelink-v1.json)
  //   /openapi/devenv.json   → the DevEnv sub-surface, and ONLY where it is wired
  //
  // These used to be one route serving one spec, and that spec was DevEnv's:
  // `/openapi.json` was introduced by the DevEnv package (#1432) and took the
  // canonical public path with it, so the only API contract CoreLink published
  // in production described eight DevEnv endpoints and none of the rest of the
  // product. Verified against prod, not inferred:
  // `GET https://corelink-api.humangr.com/openapi.json` → 200, 7707 bytes,
  // `info.title = "CoreLink DevEnv API"`, 8 paths, all `/v1/customer/devenv*`.
  if (path === "/openapi.json" || path === "/v1/openapi.json") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "openapi" };
  }
  if (path === "/openapi/devenv.json") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "openapi_devenv" };
  }

  // DevEnv cloud development environments — /v1/customer/devenv* (WP-08)
  // Checked BEFORE generic customer_v1 so it forwards to RUNNER_DEVENV_DO
  if (path.startsWith("/v1/customer/devenv") || path === "/v1/customer/devenv" || path.startsWith("/v1/devenv")) {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "devenv_v1" };
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
  if (path.startsWith("/v1/customer/") || path === "/v1/customer" || path === "/v1/pats") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "customer_v1" };
  }

  // DSR self-service portal — /v1/privacy/dsr/* (access, portability,
  // rectification, erasure, restriction, objection, status, verify-mfa). This is
  // a Clerk-session dashboard surface (the browser holds a Clerk session, not a
  // PAT), so it REUSES the customer_v1 forward: same edge Clerk-verify + tenant
  // resolution + `x-corelink-tenant-id`/`x-corelink-token-prefix: clerk` stamp.
  // The container routes it to routes/dsr/portal.rs by path. Checked BEFORE the
  // generic /v1/* arm so it never falls into the PAT-required reapi_v1 bucket.
  if (path.startsWith("/v1/privacy/") || path === "/v1/privacy") {
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
  // SECURITY NOTE (F4 — CLOSED 2026-08-19, Inc-2): /_internal/* is no longer
  // publicly reachable unauthenticated. A Cloudflare Access self-hosted app
  // (Service-Auth) gates corelink-api.humangr.com/_internal* at the NETWORK
  // layer — no `CF-Access-Client-Id`/`-Secret` service token ⇒ 403 before this
  // Worker. Same-account callers (signup pat/mint + DSR crons + erase fan-out)
  // use Service Bindings, which bypass the edge/CF Access, so are unaffected.
  // The per-consumer x-corelink-internal-auth compare below stays UNDER Access
  // (defense-in-depth). Runbook: docs/internal/inc2-cf-access-lockdown.md.
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

  // Dedicated runner lifecycle closure route. This exact match must remain
  // before generic internal and v1 routes so it reaches its runner-only
  // authority, never a shared-auth path.
  if (path === "/internal/v1/runner/credentials/close-generation") {
    return { tenantId: "_system", pathSuffix: path, routeKind: "runner_close_generation" };
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
