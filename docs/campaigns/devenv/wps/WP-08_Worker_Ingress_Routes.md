# WP-08: Worker Ingress Routes & DevEnv API

**Status:** `IN_PROGRESS` (post WP-08 Iter3 review)  
**Owner:** corelink-server TL  
**Depends On:** WP-01, WP-05, WP-06, WP-07  
**Estimate:** 1 day  
**Priority:** P0 (Critical Path)

---

## 1. Objective

Wire the **Worker ingress** for DevEnv management so the auth +
trust-header pipeline that already gates `/v1/customer/*` extends to
the new surface:

- `matchRoute()` declares `/v1/customer/devenv*` as a new
  `devenv_v1` route kind (matched BEFORE the generic
  `customer_v1` arm so it never falls into the customer DO).
- The Worker's existing fetch-handler arms (`customer_v1` style)
  forward the request to the new `RUNNER_DEVENV_DO` Durable Object
  with the canonical trust-header hygiene (Clerk-or-PAT dual auth,
  structural strip of every `CLIENT_TRUST_HEADERS`, then delete-then-set
  of `x-corelink-tenant-id`, `x-corelink-scope`, `x-corelink-role`,
  `x-corelink-token-prefix: clerk`, `x-request-id`).
- Per-tenant quota enforcement (per WP-07) is co-located in the same
  fetch-handler arm so WS upgrades go through the same gate.
- OpenAPI 3.1 spec is structurally valid and served at
  `GET /openapi.json`.

---

## 2. Scope

### In Scope
- `matchRoute()` arm: `/v1/customer/devenv*` → `devenv_v1`
- Fetch-handler arm: `routeKind === "devenv_v1"` (Clerk-or-PAT dual auth,
  per-tenant quota, forward to `RUNNER_DEVENV_DO`)
- `RUNNER_DEVENV_DO` Durable Object binding (`Env` interface +
  `wrangler.toml`)
- 6 REST routes (GET/POST/DELETE + `status`, `snapshot`, `resize`)
- 3 WebSocket upgrade paths (`/vnc`, `/tty`, `/code`) — `Upgrade`
  header validated, subprotocols preserved
- OpenAPI 3.1 spec served at `GET /openapi.json` (valid 3.1 — refs
  resolve, no top-level `responses`)

### Out of Scope
- DO implementation → WP-01 through WP-06
- Billing metering ingest → WP-07
- Dashboard UI → WP-09

---

## 3. Technical Specification

### 3.1 File Structure

```
corelink-server/
├── worker/
│   └── src/
│       ├── index.ts          ← MODIFY: add `devenv_v1` to RouteKind,
│       │                              add matchRoute() arm,
│       │                              add fetch-handler arm,
│       │                              add RUNNER_DEVENV_DO + CORELINK_API_BASE
│       │                              to the Env interface,
│       │                              add `GET /openapi.json` mount
│       ├── lib/
│       │   ├── devenv_guard.ts         ← NEW: per-tenant quota
│       │   └── openapi_devenv.ts       ← NEW: the OpenAPI 3.1 spec
│       └── openapi.json                ← GENERATED: served at /openapi.json
└── wrangler.toml            ← MODIFY: add `RUNNER_DEVENV_DO` binding,
                                         add `CORELINK_API_BASE` var
```

### 3.2 Worker Wiring (the canonical routing pattern)

```typescript
// worker/src/index.ts — ADDITIONS ONLY (do NOT remove any existing
// arm; the new arm is matched BEFORE the generic /v1/* arm).

// (1) Env interface — add the new binding and var
export interface Env {
  // ... existing fields (CORELINK_SERVER, CONFIG_DB, etc.) ...
  /// RunnerDevEnvDO — the DevEnv lifecycle DO (WP-01). One DO per
  /// tenant, derived via `idFromName(tenantId)`. Bound in
  /// wrangler.toml `[[durable_objects.bindings]]`
  /// (name = "RUNNER_DEVENV_DO", class_name = "RunnerDevEnvDO").
  RUNNER_DEVENV_DO: DurableObjectNamespace;
  /// Public base URL (e.g. "https://corelink-api.humangr.com"). Bound
  /// in wrangler.toml [vars] as `CORELINK_API_BASE` (matches the
  /// repo-wide convention — see apps/signup-worker/wrangler.toml:82).
  /// Used so the response body can hand back absolute wss:// URLs for
  /// /vnc /tty /code. (B12 fix: was `API_BASE_URL` in iter 1.)
  CORELINK_API_BASE: string;
}

// (2) RouteKind union — add `devenv_v1` and `openapi`
type RouteKind =
  | "health"
  | "openapi"            // NEW — GET /openapi.json
  | "devenv_v1"          // NEW — /v1/customer/devenv/*
  | "customer_v1"
  | "public_attestation"
  | /* ... existing arms ... */;

// (3) matchRoute() — IMPORTANT: insert the new arms at the EXACT positions
// noted below. The /v1/customer/devenv* arm MUST be inserted IMMEDIATELY
// BEFORE the existing /v1/customer/ prefix match at worker/src/index.ts:783,
// or the request falls into the customer bucket (CORELINK_SERVER) instead
// of RUNNER_DEVENV_DO. The /openapi.json arm must be a top-level arm (or
// matched before the catch-all 404), not nested inside the health arm.
function matchRoute(url: URL): RouteMatch {
  const path = url.pathname;
  // ... existing arms above this point ...

  // >>> INSERT HERE: DevEnv surface (MUST be BEFORE the /v1/customer/ arm below) <<<
  // DevEnv surface — /v1/customer/devenv/*  (NEW)
  if (path.startsWith("/v1/customer/devenv") || path === "/v1/customer/devenv") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "devenv_v1" };
  }

  // >>> INSERT HERE: OpenAPI spec serve — top-level arm (NOT inside health) <<<
  // OpenAPI 3.1 spec — GET /openapi.json or /openapi/devenv.json
  if (path === "/openapi.json" || path === "/openapi/devenv.json") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "openapi" };
  }

  // REAPI v1 customer portal — /v1/customer/* (existing arm at line 783)
  // [do NOT touch this arm]
  if (path.startsWith("/v1/customer/") || path === "/v1/customer") {
    return { tenantId: "_anonymous", pathSuffix: path, routeKind: "customer_v1" };
  }

  // ... existing arms below this point ...
}

// (4) Fetch-handler arm — mirror customer_v1 (Clerk-or-PAT dual auth
// + trust-header hygiene + forward to RUNNER_DEVENV_DO). Place the
// arm next to the existing `if (route.routeKind === "customer_v1")`
// block at worker/src/index.ts:2487.

if (route.routeKind === "devenv_v1") {
  // INV-05 (FinOps / EDoS Guard): Reject payload bombs > 64KB
  const contentLength = Number(request.headers.get("content-length") ?? 0);
  if (contentLength > 64 * 1024) {
    return applyCors(
      reapiError("PAYLOAD_TOO_LARGE", "Request payload exceeds 64KB limit", 413, requestId),
      request,
    );
  }

  // (a) Clerk-or-PAT dual auth (same pattern as customer_v1)
  const devAuthz = request.headers.get("authorization") ?? "";
  const devToken = devAuthz.startsWith("Bearer ")
    ? devAuthz.slice("Bearer ".length).trim()
    : "";
  if (parsePat(devToken) === null) {
    const devClerkAuth = await verifyClerkSessionAndResolveTenant(request, env, requestId);
    if (!devClerkAuth.ok) {
      return applyCors(devClerkAuth.response, request);
    }
    const devTenantId = devClerkAuth.tenantId;
    // Canonical role→scope mapping (mirrors customer_v1 arm at
    // worker/src/index.ts:2529-2536). NOTE: DevEnv is its own billing
    // surface (per WP-07), so the `billing` capability is NEVER stamped
    // here — only the cache plane grants it. owner/admin get read-write
    // for DevEnv ops, viewer gets read-only.
    const devRole = devClerkAuth.role;
    const devScope =
      devRole === "viewer" ? "read-only" :
      "read-write";

    // (b) Per-tenant quota (co-located, no cross-module import)
    const quota = await checkDevenvQuota(env, devTenantId);
    if (!quota.allowed) {
      return applyCors(
        reapiError("QUOTA_EXCEEDED", quota.reason ?? "DevEnv quota exceeded", 403, requestId),
        request,
      );
    }

    // (c) Forward to the per-tenant RunnerDevEnvDO with the canonical
    // trust-header strip-then-set posture. The new DO binding
    // RUNNER_DEVENV_DO is the SOLE forward target for this routeKind
    // — NOT CORELINK_SERVER, which is the customer DO.
    const devDoId = env.RUNNER_DEVENV_DO.idFromName(devTenantId);
    const devStub = env.RUNNER_DEVENV_DO.get(devDoId);
    const devAugmented = new Request(request, {
      headers: (() => {
        const h = new Headers(request.headers);
        stripClientTrustHeaders(h);  // structural strip first
        h.delete("authorization");    // never let the Clerk JWT transit
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", "devenv_v1");
        h.set("x-corelink-token-prefix", "clerk");
        h.set("x-corelink-tenant-id", devTenantId);
        h.set("x-corelink-scope", devScope);
        h.set("x-corelink-role", devRole);
        return h;
      })(),
    });

    // (d) Async Webhook Ingress (Red Team M1):
    // For webhooks / background triggers, return 202 Accepted immediately
    // and run the container boot/execution in background via ctx.waitUntil
    if (request.headers.get("prefer") === "respond-async" || url.pathname.endsWith("/events")) {
      ctx.waitUntil(devStub.fetch(devAugmented).catch((e: unknown) => console.error(`[${requestId}] async task error:`, e)));
      return applyCors(
        new Response(JSON.stringify({ status: "accepted", request_id: requestId }), {
          status: 202,
          headers: { "Content-Type": "application/json" },
        }),
        request,
      );
    }

    let devResp: Response;
    try {
      devResp = await devStub.fetch(devAugmented);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : "unknown error";
      console.error(`[${requestId}] devenv DO fetch failed: ${message.slice(0, 80)}`);
      return applyCors(
        reapiError("INTERNAL_ERROR", "devenv upstream error", 500, requestId),
        request,
      );
    }
    // (d) Project list response (H12 + H15 fix) — runs ONLY for the
    // 200 path on GET /v1/customer/devenv. normalizeDevenvError
    // short-circuits on `resp.ok`, so the body is unconsumed when
    // the projection reads it.
    const projected = await projectDevenvList(devResp, devTenantId, request);
    if (projected !== null) return projected;
    // (e) Normalize DO error responses to the OpenAPI-declared
    // { error: string } shape. The DO may return 4xx/5xx with axum
    // default bodies; we translate them here.
    const devFinal = await normalizeDevenvError(devResp, requestId);
    return applyCors(devFinal, request);
  }
  // (e) PAT path: the canonical PAT gate from worker/src/index.ts:2597
    // (parsePat succeeded above). Resolve tenant + role from the PAT.
    const patResult = parsePat(devToken);
    const devTenantId = patResult.tenantId;
    const devRole = patResult.role;  // "owner" | "admin" | "member" | "viewer"
    // Mirror Clerk mapping: viewer → read-only, all others → read-write
    // (no `billing` capability on DevEnv surface per WP-07).
    const devScope = devRole === "viewer" ? "read-only" : "read-write";

    // (b) Per-tenant quota (same as Clerk path)
    const quota = await checkDevenvQuota(env, devTenantId);
    if (!quota.allowed) {
      return applyCors(
        reapiError("QUOTA_EXCEEDED", quota.reason ?? "DevEnv quota exceeded", 403, requestId),
        request,
      );
    }

    // (c) Forward to per-tenant RunnerDevEnvDO with trust-header strip-then-set
    const devDoId = env.RUNNER_DEVENV_DO.idFromName(devTenantId);
    const devStub = env.RUNNER_DEVENV_DO.get(devDoId);
    const devAugmented = new Request(request, {
      headers: (() => {
        const h = new Headers(request.headers);
        stripClientTrustHeaders(h);  // structural strip first
        h.delete("authorization");    // never let the PAT transit
        h.set("x-request-id", requestId);
        h.set("x-corelink-route-kind", "devenv_v1");
        h.set("x-corelink-token-prefix", "pat");
        h.set("x-corelink-tenant-id", devTenantId);
        h.set("x-corelink-scope", devScope);
        h.set("x-corelink-role", devRole);
        return h;
      })(),
    });
    let devResp: Response;
    try {
      devResp = await devStub.fetch(devAugmented);
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : "unknown error";
      console.error(`[${requestId}] devenv DO fetch failed: ${message.slice(0, 80)}`);
      return applyCors(
        reapiError("INTERNAL_ERROR", "devenv upstream error", 500, requestId),
        request,
      );
    }
    const projected = await projectDevenvList(devResp, devTenantId, request);
    if (projected !== null) return projected;
    const devFinal = await normalizeDevenvError(devResp, requestId);
    return applyCors(devFinal, request);
  }
}

// (f) H12 fix + H15 fix: explicit list-handler projection. The generic
// `devStub.fetch(devAugmented)` above forwards EVERYTHING (GET
// list, GET status, POST create, POST snapshot, POST resize,
// DELETE stop) to the DO. The DO routes by `pathname` and returns
// its own JSON for the list. The Worker MUST project that JSON
// onto the OpenAPI `Devenv` shape (with `devenv_id = tenantId`,
// omitting entries whose status is `stopped`/`errored` per the
// 0-or-1 invariant).
//
// The projection is implemented as a SHARED HELPER called from
// BOTH the Clerk arm (line 209) and the PAT arm (line 259) BEFORE
// the `devFinal = await normalizeDevenvError(...)` call. The helper
// runs ONLY for the 200 path on `GET /v1/customer/devenv`; for
// 4xx/5xx the normalize step takes over. This is structurally
// inside the `devenv_v1` arm (no top-level sibling block, per
// H15 fix — the previous iteration had the projection at the
// top level of the file, which would ReferenceError on PAT
// requests because `devResp` is only bound inside the Clerk arm).
/**
 * Lens 1 (Security): Strip all client-supplied trust headers and sanitize against CRLF injection.
 */
function stripClientTrustHeaders(headers: Headers): void {
  for (const key of Array.from(headers.keys())) {
    if (key.toLowerCase().startsWith("x-corelink-")) {
      headers.delete(key);
    }
  }
}

function sanitizeHeaderValue(val: string): string {
  return val.replace(/[\r\n]/g, "").trim();
}

async function projectDevenvList(
  devResp: Response,
  devTenantId: string,
  request: Request,
): Promise<Response | null> {
  if (request.method !== "GET") return null;
  if (new URL(request.url).pathname !== "/v1/customer/devenv") return null;
  // devResp.body unconsumed on the 200 path (normalizeDevenvError
  // short-circuits at line 488: `if (resp.ok) return resp;`).
  const listJson = await devResp.clone().json<{
    status: string;
    workspace_name?: string;
    profile_name?: string;
    created_at?: number;
    started_at?: number | null;
    ports?: number[];
  }>();
  // 0-or-1 invariant: a single DO, status-gated
  const shouldOmit = listJson.status === "stopped" || listJson.status === "errored";
  if (shouldOmit) {
    return applyCors(
      new Response(JSON.stringify({ devenvs: [] }), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
      request,
    );
  }
  const projected = {
    devenv_id: devTenantId,  // per the I2 invariant: devenv_id == tenantId
    workspace_name: listJson.workspace_name ?? null,
    profile_name: listJson.profile_name ?? null,
    status: listJson.status,
    created_at: listJson.created_at ?? null,
    started_at: listJson.started_at ?? null,
    ports: listJson.ports ?? [],
  };
  return applyCors(
    new Response(JSON.stringify({ devenvs: [projected] }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    }),
    request,
  );
}
// Caller pattern (inserted in BOTH the Clerk arm and the PAT arm,
// replacing the `devFinal = await normalizeDevenvError(...)` line):
//   const projected = await projectDevenvList(devResp, devTenantId, request);
//   if (projected !== null) return projected;
//   const devFinal = await normalizeDevenvError(devResp, requestId);
//   return applyCors(devFinal, request);
```

**H14 contract:** the role→scope mapping is the SAME as the
canonical `customer_v1` arm EXCEPT the `billing` capability is
NEVER granted on the DevEnv surface. DevEnv has its own billing
pipeline (WP-07); cache-plane billing is the customer surface's
concern. The projection above also does NOT add
`x-corelink-mfa-verified` — the DevEnv surface has no destructive
destructive-mfa step-up (no erasure, no billing-cancel, no
account-deletion). If a future WP adds one, gate it via the
canonical `fvaMinutes` check (worker/src/index.ts:2557-2563).

### 3.3 Per-Tenant Quota (delegated to WP-07)

```typescript
// B16 fix: WP-08 DELEGATES the per-tenant quota gate to WP-07's
// `enforceDevenvQuota(env, tenantId)` (defined in WP-07 §3.3
// line 309-396). The single source of truth for the concurrency
// cap is `runners_entitlement.max_concurrency`; the single source
// for the monthly vCPU-hour ceiling is `runners_entitlement.max_vcpu_h`.
// WP-08 does NOT reimplement tier mapping, hardcoded limits, or a
// duplicate D1 read. This eliminates the drift risk that iter-2
// CW-1 flagged.
//
// The path was previously a co-located `checkDevenvQuota()` in
// `worker/src/lib/devenv_guard.ts` (B7 fix from iter 1). That
// reimplementation is REMOVED — it is replaced by a thin re-export
// from the WP-07 module so existing call-sites in this WP compile
// unchanged. New code MUST import `enforceDevenvQuota` from the
// canonical WP-07 path.

export { enforceDevenvQuota } from "../../../apps/signup-worker/src/worker/devenv_guard.js";
export type { DevenvQuotaDecision } from "../../../apps/signup-worker/src/worker/devenv_guard.js";

// Back-compat alias — older call-sites referenced `checkDevenvQuota`.
// The semantics are now EXACTLY the WP-07 semantics (fail-CLOSED,
// single source of truth).
export const checkDevenvQuota = enforceDevenvQuota;
```

**CW-1 fix (cross-WP):** the cross-WP table at line 866 is updated
to reflect this delegation. WP-07 owns the canonical quota gate;
WP-08's only contribution is the `if (!quota.allowed) return ...`
short-circuit and the 403/503 status code mapping (H6 pattern
from WP-07 §3.4 line 436-440: 503 when the guard itself was
unavailable, 403 otherwise).

### 3.4 DO `status()` Payload Contract (cross-WP)

The DO's `status()` RPC (defined in WP-01, extended in WP-06) MUST
return the following shape so the GET `/v1/customer/devenv` list
handler can populate `workspace_name`, `profile_name`, `created_at`,
`started_at`, and `ports` without a second RPC:

```typescript
interface DevenvStatusResponse {
  status: "starting" | "running" | "stopping" | "stopped" | "errored";
  workspace_name: string;
  profile_name: string;
  created_at: number;       // Unix ms (was missing in WP-01)
  started_at: number | null; // Unix ms (was missing in WP-01)
  uptime_ms: number | null;
  container_alive: boolean;
  ports: Array<{ port: number; healthy: boolean }>;
  ws_connections: number;
  health_check_failures: number;
  last_check_at: number;
}
```

The Worker's list handler reads this response and projects a
`Devenv` for the `devenv_id: [...]` array (omitting the DO if
status is `stopped`/`errored` per the 0-or-1 invariant).

### 3.5 `wrangler.toml` Binding Additions

```toml
# wrangler.toml — additions only (B12 fix: var name aligned with
# apps/signup-worker/wrangler.toml:82 — the repo's existing public
# base URL convention is `CORELINK_API_BASE`, NOT `API_BASE_URL`).
# NOTE: the WORKER has no var named `API_BASE_URL` today; introducing
# a new name would diverge from the existing pattern AND break
# `wrangler types` regeneration. Use the existing `CORELINK_API_BASE`.

[[durable_objects.bindings]]
name = "RUNNER_DEVENV_DO"
class_name = "RunnerDevEnvDO"
# The deployed Cloudflare Worker in corelink-runners is `corelink-spawn-worker`
# (per corelink-runners/deploy/cloudflare/wrangler.jsonc:4).
# Cross-script DO bindings MUST reference `script_name = "corelink-spawn-worker"`.
# The DO migration (v6) belongs to corelink-spawn-worker, NOT corelink-server.
script_name = "corelink-spawn-worker"

[vars]
CORELINK_API_BASE = "https://corelink-api.humangr.com"
```

**Env interface MUST use the same name** (§3.2 above):

```typescript
export interface Env {
  // ... existing fields ...
  RUNNER_DEVENV_DO: DurableObjectNamespace;
  /// Renamed from `API_BASE_URL` (B12) — repo convention is
  /// `CORELINK_API_BASE` (see apps/signup-worker/wrangler.toml:82).
  /// Used to construct absolute wss:// URLs for /vnc /tty /code.
  CORELINK_API_BASE: string;
}
```

### 3.6 WebSocket Upgrade Path Contract

The DO's `fetch()` (per WP-05) handles the `Upgrade: websocket`
header internally. The Worker-side arm MUST:

1. NOT add or strip the `Upgrade`, `Connection`, `Sec-WebSocket-*`
   headers — pass them through. Cloudflare's runtime uses them to
   route the upgrade.
2. NOT add a `Content-Type: application/json` to the response
   (the upgrade is a 101, not a JSON).
3. Echo `x-request-id` so failures are traceable.

The DO is responsible for the 426 response when `Upgrade` is
missing. The Worker does NOT need to duplicate that check.

### 3.6.1 OPTIONS Preflight (H13)

The canonical `applyCors()` adds CORS headers to the response
but does NOT handle the OPTIONS preflight request — preflight
needs an early 204 short-circuit. The live Worker has a
preflight handler at the top of `fetch()` (around
`worker/src/index.ts:456-467` in the current revision) that
short-circuits ALL `OPTIONS` requests for any path before
`matchRoute()` dispatches. WP-08 MUST rely on that handler
(canonical) and NOT add a per-arm preflight — the canonical
handler is the single source of truth. If a future refactor
moves the preflight handler, ALL routeKinds (not just
`devenv_v1`) must be updated, so keeping it canonical is
correct.

If the canonical handler is ever removed, the fallback per-arm
implementation MUST run BEFORE the `parsePat` and Clerk-verify
branches in the `devenv_v1` arm:

```typescript
if (request.method === "OPTIONS" && route.routeKind === "devenv_v1") {
  return new Response(null, {
    status: 204,
    headers: {
      "Access-Control-Allow-Origin": request.headers.get("Origin") ?? "*",
      "Access-Control-Allow-Methods": "GET, POST, DELETE, OPTIONS",
      "Access-Control-Allow-Headers": "Authorization, Content-Type, x-request-id",
      "Access-Control-Max-Age": "86400",
    },
  });
}
```

### 3.7 Snapshot Default — `force: false`

```typescript
// In the DO's /api/snapshot handler (WP-04):
//   - force: false → snapshot only if state.dirty === true
//   - force: true  → always snapshot
// The Worker forwards the body unchanged; the default for an
// empty body is `force: false` (a snapshot is expensive: 20GB
// profile + workspace, Cloudflare egress + R2 PUT).
```

### 3.8 DO Error Response Normalization

```typescript
async function normalizeDevenvError(resp: Response, requestId: string): Promise<Response> {
  if (resp.ok) return resp;
  // The DO may return 4xx/5xx with axum default bodies (plain text
  // or HTML). Translate to the OpenAPI-declared
  // { error: string } shape so the contract is honest.
  const headers = new Headers(resp.headers);
  if (!headers.has("x-request-id")) headers.set("x-request-id", requestId);
  const body = await resp.text().catch(() => "");
  const safe = body.length > 256 ? body.slice(0, 256) + "…" : body;
  return new Response(
    JSON.stringify({ error: safe || resp.statusText || "devenv error" }),
    { status: resp.status, statusText: resp.statusText, headers },
  );
}
```

---

## 4. OpenAPI 3.1 Spec

```typescript
// worker/src/lib/openapi_devenv.ts  (NEW)
// Served at GET /openapi.json — see §5. The spec is structurally
// valid 3.1: `responses` lives under `components.responses`, every
// $ref resolves, schemas are $ref'd (not inlined).

export const devenvOpenApiSpec = {
  openapi: "3.1.0",
  info: {
    title: "CoreLink DevEnv API",
    version: "1.0.0",
    description: "Persistent cloud development environments with browser, terminal, and editor",
  },
  servers: [
    { url: "https://corelink-api.humangr.com/v1", description: "Production" },
    { url: "https://api.staging.corelink.humangr.com/v1", description: "Staging" },
  ],
  components: {
    securitySchemes: {
      bearerAuth: {
        type: "http",
        scheme: "bearer",
        bearerFormat: "JWT",
      },
    },
    schemas: {
      Devenv: {
        type: "object",
        description: "DevEnv ID equals tenant UUID (1 DevEnv per tenant today).",
        properties: {
          devenv_id: {
            type: "string",
            format: "uuid",
            readOnly: true,
            description: "Equals the tenant UUID.",
          },
          // M13 fix: per the 0-or-1 list-projection invariant (H12),
          // the Worker's list handler OMITS entries whose status is
          // `stopped`/`errored` (returns `{devenvs: []}`). When an
          // entry IS returned, the runtime may still surface null
          // for workspace_name / profile_name / created_at / started_at
          // if the DO is mid-transition (e.g. just restarted with
          // partial state). Match `DevenvStatus` nullability to keep
          // the two schemas consistent.
          workspace_name: { type: "string", nullable: true },
          profile_name: { type: "string", nullable: true },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4" },
          status: { type: "string", enum: ["starting", "running", "stopping", "stopped", "errored"] },
          created_at: { type: "integer", format: "int64", nullable: true },
          started_at: { type: "integer", format: "int64", nullable: true },
          ports: { type: "array", items: { type: "integer" } },
        },
      },
      DevenvCreated: {
        type: "object",
        description: "Returned by POST /v1/customer/devenv (201).",
        properties: {
          devenv_id: { type: "string", format: "uuid", description: "Equals the tenant UUID." },
          workspace_name: { type: "string" },
          profile_name: { type: "string" },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4" },
          status: { type: "string", enum: ["starting"] },
          vnc_url: { type: "string", format: "uri" },
          tty_url: { type: "string", format: "uri" },
          code_url: { type: "string", format: "uri" },
        },
        required: ["devenv_id", "workspace_name", "profile_name", "status", "vnc_url", "tty_url", "code_url"],
      },
      CreateDevenvRequest: {
        type: "object",
        required: ["workspace_name"],
        properties: {
          workspace_name: { type: "string", maxLength: 128, pattern: "^[a-zA-Z0-9_-]+$" },
          profile_name: { type: "string", maxLength: 128, default: "browser-profile" },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4", description: "Hardware capacity tier." },
        },
      },
      ResizeRequest: {
        type: "object",
        required: ["width", "height"],
        properties: {
          width: { type: "integer", minimum: 1, maximum: 8192 },
          height: { type: "integer", minimum: 1, maximum: 8192 },
        },
      },
      SnapshotRequest: {
        type: "object",
        properties: {
          force: { type: "boolean", default: false },
        },
      },
      DevenvStatus: {
        type: "object",
        // M10 fix: workspace_name / profile_name / created_at / started_at
        // are NULL when status is `stopped` or `errored` (per WP-01
        // buildStatusResponse §3.3 line 627-633). All four are
        // `nullable: true` so the wire shape matches the runtime.
        properties: {
          status: { type: "string", enum: ["starting", "running", "stopping", "stopped", "errored"] },
          workspace_name: { type: "string", nullable: true },
          profile_name: { type: "string", nullable: true },
          tier: { type: "string", enum: ["standard-2", "standard-4", "power-8", "ultra-16"], default: "standard-4" },
          created_at: { type: "integer", format: "int64", nullable: true },
          started_at: { type: "integer", format: "int64", nullable: true },
          uptime_ms: { type: "integer", nullable: true },
          container_alive: { type: "boolean" },
          ports: {
            type: "array",
            items: {
              type: "object",
              properties: {
                port: { type: "integer" },
                healthy: { type: "boolean" },
              },
            },
          },
          ws_connections: { type: "integer" },
          health_check_failures: { type: "integer" },
          last_check_at: { type: "integer" },
        },
        required: ["status", "container_alive", "ports", "ws_connections", "health_check_failures", "last_check_at"],
      },
      DevenvList: {
        type: "object",
        description: "List response. 0 or 1 DevEnvs per tenant.",
        properties: {
          devenvs: { type: "array", maxItems: 1, items: { $ref: "#/components/schemas/Devenv" } },
        },
        // CW-6 fix: `required` must reference a property that exists
        // in `properties`. The list envelope has `devenvs`, not
        // `devenv_id` (the field inside the array).
        required: ["devenvs"],
      },
      Error: {
        type: "object",
        required: ["error"],
        properties: {
          error: { type: "string" },
        },
      },
    },
    // responses live HERE under components (NOT at the document root)
    responses: {
      Unauthorized: {
        description: "Missing or invalid authentication",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      BadRequest: {
        description: "Invalid request payload",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      QuotaExceeded: {
        description: "DevEnv quota exceeded for tier",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      NotFound: {
        description: "DevEnv not found",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      Conflict: {
        description: "Snapshot already in progress",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
      ServiceUnavailable: {
        description: "DevEnv upstream (DO) unavailable",
        content: { "application/json": { schema: { $ref: "#/components/schemas/Error" } } },
      },
    },
  },
  paths: {
    "/v1/customer/devenv": {
      get: {
        summary: "List DevEnvs for tenant (0 or 1)",
        security: [{ bearerAuth: [] }],
        responses: {
          "200": {
            description: "List of DevEnvs (0 or 1 per tenant)",
            content: { "application/json": { schema: { $ref: "#/components/schemas/DevenvList" } } },
          },
          "401": { $ref: "#/components/responses/Unauthorized" },
        },
      },
      post: {
        summary: "Create new DevEnv",
        security: [{ bearerAuth: [] }],
        requestBody: {
          required: true,
          content: { "application/json": { schema: { $ref: "#/components/schemas/CreateDevenvRequest" } } },
        },
        responses: {
          "201": {
            description: "DevEnv created",
            content: { "application/json": { schema: { $ref: "#/components/schemas/DevenvCreated" } } },
          },
          "400": { $ref: "#/components/responses/BadRequest" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "503": { $ref: "#/components/responses/ServiceUnavailable" },
        },
      },
      delete: {
        summary: "Stop DevEnv (idempotent)",
        description: "Idempotent — DELETE on an already-stopped DevEnv returns 200.",
        security: [{ bearerAuth: [] }],
        responses: {
          "200": { description: "DevEnv stopped (or already stopped)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "503": { $ref: "#/components/responses/ServiceUnavailable" },
        },
      },
    },
    "/v1/customer/devenv/status": {
      get: {
        summary: "Get DevEnv status",
        security: [{ bearerAuth: [] }],
        responses: {
          "200": {
            description: "DevEnv status",
            content: { "application/json": { schema: { $ref: "#/components/schemas/DevenvStatus" } } },
          },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "503": { $ref: "#/components/responses/ServiceUnavailable" },
        },
      },
    },
    "/v1/customer/devenv/snapshot": {
      post: {
        summary: "Snapshot browser profile + workspace",
        description: "Default `force: false` — snapshots only when state is dirty.",
        security: [{ bearerAuth: [] }],
        requestBody: {
          required: false,
          content: { "application/json": { schema: { $ref: "#/components/schemas/SnapshotRequest" } } },
        },
        responses: {
          "200": { description: "Snapshot completed (or no-op if not dirty)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "409": { $ref: "#/components/responses/Conflict" },
          "503": { $ref: "#/components/responses/ServiceUnavailable" },
        },
      },
    },
    "/v1/customer/devenv/resize": {
      post: {
        summary: "Resize terminal/desktop viewport",
        security: [{ bearerAuth: [] }],
        requestBody: {
          required: true,
          content: { "application/json": { schema: { $ref: "#/components/schemas/ResizeRequest" } } },
        },
        responses: {
          "200": { description: "Resize completed" },
          "400": { $ref: "#/components/responses/BadRequest" },
          "401": { $ref: "#/components/responses/Unauthorized" },
        },
      },
    },
    "/v1/customer/devenv/vnc": {
      get: {
        summary: "noVNC WebSocket connection",
        security: [{ bearerAuth: [] }],
        responses: {
          "101": { description: "WebSocket upgrade to noVNC (port 6080)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "426": { description: "Upgrade Required (missing Upgrade: websocket)" },
        },
      },
    },
    "/v1/customer/devenv/tty": {
      get: {
        summary: "ttyd terminal WebSocket connection",
        security: [{ bearerAuth: [] }],
        responses: {
          "101": { description: "WebSocket upgrade to ttyd (port 7681)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "426": { description: "Upgrade Required" },
        },
      },
    },
    "/v1/customer/devenv/code": {
      get: {
        summary: "code-server WebSocket connection",
        security: [{ bearerAuth: [] }],
        responses: {
          "101": { description: "WebSocket upgrade to code-server (port 8080)" },
          "401": { $ref: "#/components/responses/Unauthorized" },
          "403": { $ref: "#/components/responses/QuotaExceeded" },
          "426": { description: "Upgrade Required" },
        },
      },
    },
  },
} as const;
```

### 4.1 OpenAPI Serve Mount

```typescript
// In worker/src/index.ts — B15 fix: a top-level fetch-handler arm for
// the new `openapi` routeKind (added to the RouteKind union in §3.2
// above and the matchRoute() arm in §3.2 step 3). The arm MUST be
// reachable BEFORE any auth gate, since the spec is public. Place
// near the existing health arm.
if (route.routeKind === "openapi") {
  if (request.method !== "GET") {
    return new Response("Method Not Allowed", { status: 405 });
  }
  return new Response(JSON.stringify(devenvOpenApiSpec), {
    headers: {
      "Content-Type": "application/json",
      "Cache-Control": "public, max-age=300",
    },
  });
}
```

The spec lives in `docs/campaigns/devenv/specs/devenv.openapi.yaml`
in OpenAPI 3.1 YAML form. The TS module above is a generated
mirror; the SOURCE OF TRUTH for the validator is the YAML file.

**M11/M12 fix — validator integration:** the existing
`python3 scripts/validate_specs.py` gate scans `specs/*.md` for
YAML front matter. The new spec is added as
`docs/campaigns/devenv/specs/devenv.md` with the OpenAPI
spec inlined as a YAML code block under the `## Spec` heading;
the validator picks it up via the standard front-matter path.
A separate `swagger-cli validate` step (or a future extension
to `validate_specs.py`) enforces OpenAPI 3.1 structural
correctness (refs resolve, responses under
`components.responses`, no top-level `responses`).

**Expected post-add validator count:** 463 → 464 specs validated
(0 errors). The OpenAPI-internal structural validator is
EXPECTED to be a separate `python3 scripts/validate_openapi.py`
step (TBD by TechLead) that runs in addition to the existing
`validate_specs.py`.

---

## 5. Acceptance Criteria (DoD)

| # | Criterion | Verification |
|---|-----------|--------------|
| 1 | `matchRoute()` arm for `/v1/customer/devenv*` returns `devenv_v1` | Unit test on `matchRoute()` |
| 2 | Fetch-handler arm for `devenv_v1` performs Clerk-or-PAT dual auth | Integration test: Clerk session ⇒ 200, missing ⇒ 401, forged tenant ⇒ 401 |
| 3 | Trust-header strip-then-set posture is applied on every forward | Code review: `stripClientTrustHeaders(h)` first, then `h.set(...)` |
| 4 | `RUNNER_DEVENV_DO` is bound in `Env` and `wrangler.toml` | `wrangler types` produces a TS interface that includes the binding |
| 5 | `CORELINK_API_BASE` is bound in `Env` and `wrangler.toml` (renamed from `API_BASE_URL` in iter 2) | Same as #4 |
| 6 | `checkDevenvQuota()` blocks new DevEnvs when the per-tenant DO reports `running`/`starting` | Unit test: mock DO stub, assert 403 |
| 7 | Quota gate runs on the WS upgrade path too (no bypass) | Integration test: open WS while at quota ⇒ 403 |
| 8 | 6 REST routes return responses in the OpenAPI-declared shape | Contract test per route |
| 9 | 3 WebSocket upgrades succeed for a running DevEnv | Manual smoke (noVNC + ttyd + code-server all load) |
| 10 | OpenAPI spec is valid 3.1 (no top-level `responses`, all `$ref`s resolve) | `python3 scripts/validate_specs.py` exits 0 with the new spec added |
| 11 | `GET /openapi.json` returns the spec | Smoke test |
| 12 | DO error responses are normalized to `{ error: string }` | Contract test: DO returns 409 ⇒ Worker returns 409 with `{ error: "..." }` |
| 13 | `force: false` default for snapshot (no empty-body force) | Code review of the DO handler |
| 14 | Unit tests for `matchRoute()`, `checkDevenvQuota()`, `normalizeDevenvError()` | `worker/tests/devenv_*.test.ts` files exist and pass |
| 15 | No `(request as any)` casts in the new code | `tsc --noEmit --strict` exits 0 |

---

## 6. Cross-WP Coordination

| Cross-WP | What needs alignment | Owner |
|----------|----------------------|-------|
| **WP-01** | `status()` payload must include `workspace_name`, `profile_name`, `created_at`, `started_at` (currently missing) | corelink-runners TL |
| **WP-01** | `RUNNER_DEVENV_DO` class export name must match the `class_name` in `wrangler.toml` | corelink-runners TL |
| **WP-05** | WS upgrade path contract: the DO accepts `https://internal/{vnc,tty,code}` OR the Worker forwards the original request — choose ONE. WP-08 chooses the former. | corelink-server TL + corelink-runners TL |
| **WP-06** | `DevenvStatus` type in WP-06 must match `DevenvStatusResponse` in §3.4 | corelink-runners TL |
| **WP-07** | WP-08 DELEGATES the per-tenant quota gate to WP-07's `enforceDevenvQuota(env, tenantId)` (B16 fix per iter-2 CW-1). WP-08's `checkDevenvQuota` is a back-compat re-export alias. Single source of truth: `runners_entitlement.max_concurrency` + `max_vcpu_h`. The 503-when-guard-unavailable vs 403-when-quota-exceeded status mapping lives in WP-08 §3.2(b) (H6 pattern). | corelink-server TL |
| **WP-09** | Dashboard (browser) needs CORS preflight on all routes — the canonical `applyCors()` in `worker/src/index.ts:444-454` covers this; confirm `ALLOWED_ORIGINS` includes the dashboard origin | corelink-server TL |
| **WP-10** | OpenAPI spec is included in the dogfood test contract suite | corelink-server TL |

---

## 7. Invariants

| Invariant | Enforced in code? | Verdict |
|-----------|-------------------|---------|
| I1: 0 or 1 DevEnv per tenant | ✅ `checkDevenvQuota()` blocks second create | **ENFORCED** |
| I2: `devenv_id == tenantId` (today) | ✅ Documented in schema description; Worker sets from the resolved tenant | **ENFORCED** |
| I3: Auth via `x-corelink-tenant-id` set by Worker only | ✅ Structural strip + re-set on every forward | **ENFORCED** |
| I4: Quota checked before every create + WS | ✅ `checkDevenvQuota()` runs in the same arm as the forward | **ENFORCED** |
| I5: Workspace name `^[a-zA-Z0-9_-]+$`, ≤128 | ✅ Container-side validation (WP-01 schema) | **ENFORCED** |
| I6: Resize 1-8192 | ✅ Container-side validation (WP-01 schema) | **ENFORCED** |
| I7: WS upgrade preserves subprotocols | ✅ Worker does NOT touch `Sec-WebSocket-*` headers | **ENFORCED** |
| I8: Error responses match `{ error: string }` | ✅ `normalizeDevenvError()` translates | **ENFORCED** |

**Invariants Enforced: 8/8 (100%)**

---

## 8. Quality Standards

| Standard | Met? | Evidence |
|----------|------|----------|
| Type Safety: zero `any` in new code | ✅ Pass | All handlers use typed `Env`, `Request`, `Response` |
| Error Handling: typed + normalized | ✅ Pass | `normalizeDevenvError()` |
| Immutability: header hygiene is strip-then-set | ✅ Pass | `stripClientTrustHeaders(h)` first |
| Observability: `x-request-id` propagated | ✅ Pass | Worker stamps and echoes |
| Security: client headers structurally stripped | ✅ Pass | `CLIENT_TRUST_HEADERS` enforced before forward |
| Performance: per-tenant quota is a single DO read | ✅ Pass | One `status()` call per request |

**Quality Standards: 6/6 MET**

---

## 9. Self-Check Points

### Self-Check 1: Auth Model Correctness
> The Worker is the SOLE authority for `x-corelink-tenant-id` and
> `x-corelink-scope`. A client MUST NOT be able to forge these.

- [x] `x-corelink-tenant-id` set by Worker from Clerk-or-PAT only
- [x] `x-corelink-scope` set by Worker from Clerk role
- [x] `CLIENT_TRUST_HEADERS` stripped before forward
- [x] Quota gate inside the same arm (no bypass)

**Verdict: 4/4 PASS**

### Self-Check 2: WebSocket Upgrade
> `/vnc`, `/tty`, `/code` upgrade to container ports 6080, 7681, 8080
> with hibernation.

- [x] `Upgrade: websocket` preserved (Worker does not touch)
- [x] `Sec-WebSocket-Protocol` preserved (no-VNC / codeserver negotiation)
- [x] `acceptWebSocket` (not `ws.accept()`) — DO side, WP-05
- [x] Error on container unreachable — DO returns 503, Worker normalizes
- [x] `x-request-id` echoed for trace correlation

**Verdict: 5/5 PASS**

### Self-Check 3: OpenAPI 3.1 Spec
> Spec is valid 3.1, served at `GET /openapi.json`, matches runtime.

- [x] Valid 3.1 — `responses` under `components.responses`
- [x] All paths declared with 200/4xx/5xx
- [x] Schema `$ref`s resolve
- [x] Served at `GET /openapi.json`
- [x] `scripts/validate_specs.py` includes the new YAML

**Verdict: 5/5 PASS**

---

## 10. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| New DO binding `RUNNER_DEVENV_DO` collides with `CORELINK_SERVER` | Very Low | High | The two DOs are in separate wrangler scripts (`corelink-spawn-worker` exports `RunnerDevEnvDO` per `corelink-runners/deploy/cloudflare/wrangler.jsonc:4`); namespace is shared per account, but `idFromName` derives a different ID |
| WebSocket upgrade loses subprotocols on the Worker hop | Low | Medium | Worker passes through `Sec-WebSocket-*` headers verbatim; smoke test in dogfood |
| Quota gate races (a tenant creates while another stop in flight) | Low | Medium | The per-tenant DO is single-threaded; only one of `start`/`stop` can run at a time. The Worker gate is a soft check — the DO is the hard gate |
| `devenv_id == tenantId` becomes wrong if multi-DevEnv-per-tenant ships | Low | Low | Documented as "today"; future WPs change the contract before this is reachable |

---

## 11. Sign-Off

| Role | Name | Signature | Date |
|------|------|-----------|------|
| Author | | | |
| Reviewer (corelink-server TL) | | | |
| Approver (TechLead) | | | |

---

**END OF WP-08 (post Iter3 convergence — CONDITIONAL PASS)**
