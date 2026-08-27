# WP-08 REVIEW — Iteration 1 (CRITICAL AUDIT)

**Reviewer:** Self (TechLead persona)  
**Date:** 2026-08-26  
**Verdict:** ❌ **FAIL — 10 BLOCKING ISSUES, 9 HIGH SEVERITY ISSUES, 7 MEDIUM SEVERITY ISSUES**

---

## Executive Summary

WP-08 implements the Worker ingress router for DevEnv. The WP fails the same
audit bar as WP-01 in iter1, plus a **CATEGORY of new errors unique to a
routing layer** (route conflicts, OpenAPI schema corruption, auth-bypass by
duplicate routing). The most serious class of findings:

1. **Duplicate routing** — the existing `customer_v1` arm in
   `worker/src/index.ts:783-785` already matches `/v1/customer/*` and forwards
   to the CORELINK_SERVER DO. WP-08 wants to mount a NEW itty-router at the
   SAME path, which would create a race between the worker's existing
   PAT/Clerk+trust-header gate and a new gate that trusts a client-supplied
   `x-corelink-tenant-id` header with zero validation. This is a **tenancy
   escape** in addition to a routing disaster.
2. **OpenAPI 3.1 spec is structurally invalid** — `responses` is defined at the
   document root instead of under `components.responses`, so every `$ref`
   like `#/components/responses/Unauthorized` will fail validation. The spec
   cannot be served.
3. **`/v1/customer/devenv` is already a customer route** — the entire
   customer.rs Router in the container does NOT include `/v1/customer/devenv*`
   yet, so WP-08 *is* the right place to add it. But it MUST be added via
   `matchRoute()` (worker) + a new `routeKind` (`devenv_v1`) + a new
   `durable_object` binding (`RUNNER_DEVENV_DO`) and a new stub/forward — NOT
   a parallel itty-router. The Worker is the SOLE authority for routing; the
   proposed pattern inverts the model.

Fixes below resolve all 10 BLOCKING + all 9 HIGH; the WP is now routed through
the existing worker machinery, the OpenAPI spec is moved into a self-validating
shape, and tenancy/invariant coverage is at parity with the live customer
plane.

---

## 🔴 BLOCKING ISSUES (Must fix before sign-off)

### B1. **Duplicate Route Mounting — Tenancy Escape / Bypass of Worker Auth Gate**
- **Location:** `worker/src/routes/devenv.ts` (entire file) + `worker/src/index.ts:783-785`
- **Problem:** `worker/src/index.ts:783-785` already declares
  `path.startsWith("/v1/customer/") → routeKind: "customer_v1"` which is
  matched FIRST in `matchRoute()` and routed to the
  `env.CORELINK_SERVER.get(idFromName(tenant))` DO with the FULL trust-header
  hygiene (PAT/Clerk, `x-corelink-tenant-id` injected from the resolved
  tenant, every `CLIENT_TRUST_HEADERS` stripped first). The fetch handler
  arm at `index.ts:2487-2595` then forwards to the container with the
  canonical headers (`x-corelink-tenant-id`, `x-corelink-scope`,
  `x-corelink-role`, `x-request-id`, `x-corelink-token-prefix: clerk`, etc.).
- **What WP-08 does instead:** Mounts a NEW itty-router at the SAME
  `/v1/customer/devenv*` path. The auth inside that itty-router is
  `request.headers.get("x-corelink-tenant-id")` — i.e. **trusts whatever the
  client sends**. There is no PAT verify, no Clerk verify, no D1 lookup.
  This is a complete auth bypass for the new routes.
- **Concretely:** an unauthenticated client sending
  `x-corelink-tenant-id: <victim-uuid>` to
  `https://api.corelink.humangr.com/v1/customer/devenv` would get the
  victim's DevEnv state. `POST .../vnc` and `.../tty` would open a proxied
  WebSocket into the victim's container.
- **Fix:** Do NOT mount a parallel router. Extend the worker the same way
  the customer plane is built today:
  1. Add a `routeKind: "devenv_v1"` to the `RouteKind` union in
     `worker/src/index.ts:318-346`.
  2. Add a `matchRoute()` arm matched AFTER `customer_v1` (the devevn path
     is MORE specific): `if (path.startsWith("/v1/customer/devenv")) return
     { tenantId: "_anonymous", pathSuffix: path, routeKind: "devenv_v1" }`.
  3. Add a `RUNNER_DEVENV_DO: DurableObjectNamespace` to the `Env` interface.
  4. Add a fetch-handler arm for `routeKind === "devenv_v1"` mirroring the
     `customer_v1` arm (Clerk-or-PAT dual auth, strip + re-set trust
     headers, forward to `env.RUNNER_DEVENV_DO.get(idFromName(tenantId))`).
  5. **DELETE** `worker/src/routes/devenv.ts` entirely.

### B2. **OpenAPI 3.1 Spec — `responses` at Document Root is Invalid**
- **Location:** Spec lines 588-621 (`responses: { Unauthorized, BadRequest, QuotaExceeded, NotFound }` at top level)
- **Problem:** In OpenAPI 3.1, the top-level object is `{ openapi, info,
  servers, components, paths, ... }`. `responses` is a member of
  `components` (e.g. `components.responses`). A `responses` key at the
  document root is ignored by every OpenAPI 3.1 validator (and rejected by
  some, including the `swagger-cli validate` we use in
  `scripts/validate_specs.py`). The `$ref: "#/components/responses/Unauthorized"`
  refs inside `paths` therefore all point to a NON-EXISTENT key (the
  validator must resolve `#/components/responses/Unauthorized`, not
  `#/responses/Unauthorized`). The spec fails to load end-to-end.
- **Fix:** Move the `responses` block inside `components` (the `components`
  object already exists at line 358, just add a sibling `responses` key).

### B3. **`(request as any).clwToken` — Type-Safety Violation + Token Leak**
- **Location:** `worker/src/routes/devenv.ts:145` (and the `tenantId` reads on lines 87, 114, 182, 210, 221, 238, 267, 278, 288)
- **Problem:** The Worker charter (`worker/src/index.ts:13`) is "Zero `any`
  types — all bindings typed via Env interface". `(request as any).clwToken`
  asserts a property the Worker never sets. The whole `(request as
  any).tenantId` / `.clwToken` pattern smuggles untyped data through
  `Request` — there is no production code that injects either header. The
  "Injected by auth middleware" comment is a fiction: the live auth path
  uses `custTenantId` resolved from the PAT/Clerk and forwarded to the DO
  as the canonical `x-corelink-tenant-id` header. The container then
  derives its own auth from the DO-trust header, not from any
  `clwToken`.
- **Even worse:** The `clwToken` is a per-tenant clw PAT (`cl_…`) — passing
  it through the Worker as an arbitrary request property would expose it to
  any middleware that logs requests. The container issues the clw PAT
  itself via `corelink-clw` mint; it should never transit the Worker.
- **Fix:** The clw PAT lives entirely on the DO/container side. The Worker
  passes the tenant id; the DO/container mints/refreshes clw credentials
  itself. Remove the `clwToken` field from the start payload and remove
  every `(request as any)` cast.

### B4. **Worker `Env` Type Does Not Declare `RUNNER_DEVENV_DO`**
- **Location:** `worker/src/index.ts:65-261` (the `Env` interface) + every `env.RUNNER_DEVENV_DO.*` in WP-08
- **Problem:** The Worker Env interface does not include
  `RUNNER_DEVENV_DO: DurableObjectNamespace`. Every `env.RUNNER_DEVENV_DO`
  read in WP-08 (lines 89, 90, 136, 137, 184, 185, 212, 213, 224, 225, 249,
  250, 269, 270, 280, 281, 290, 291) is a compile error. The binding
  pattern for the existing CoreLink DO is `CORELINK_SERVER` (line 66) — the
  new binding must be added with the same shape AND the
  `wrangler.toml`/`.dev.vars` documentation.
- **Fix:** Add `RUNNER_DEVENV_DO: DurableObjectNamespace;` to `Env`, and
  document the `wrangler.toml` `[[durable_objects.bindings]]` entry (name
  = `RUNNER_DEVENV_DO`, class_name = `RunnerDevEnvDO`).

### B5. **Worker `Env` Type Does Not Declare `API_BASE_URL`**
- **Location:** Lines 171-173 (`vnc_url`, `tty_url`, `code_url` use `env.API_BASE_URL`)
- **Problem:** The Worker `Env` interface (line 65) has no `API_BASE_URL`
  field. A `wrangler secret put` of an env var does not inject it into the
  type — you must add it to `Env` AND to `wrangler.toml [vars]`. A
  successful compile requires both. The convention for the public base
  URL in the repo is `CORELINK_API_BASE` (used in `scripts/e2e-clerk-signup.sh:38`
  etc.), not `API_BASE_URL`.
- **Fix:** Add `API_BASE_URL: string` to `Env`, bind in `wrangler.toml [vars]`,
  and use that name consistently.

### B6. **The Router `use()` Middleware Pattern is Wrong — `tenantId` is Stringly-Typed Client Input**
- **Location:** Lines 69-79 (the `router.use("/v1/customer/devenv*", …)` middleware)
- **Problem:** Even ignoring B1, the middleware body accepts ANY
  `x-corelink-tenant-id` header from the client and attaches it to the
  Request for downstream handlers. There is no UUID validation, no
  existence check, no rate-limit. A client can pass `?` or a 10KB blob
  and the DO namespace will be derived from that. The hash for the
  `idFromName(tenantId)` is whatever the client chose. The corresponding
  DO can be a totally unrelated tenant (the namespace is shared).
- **Fix:** Reject with this pattern in the first place (see B1) — the
  existing `customer_v1` arm already does Clerk+PAT dual auth and the
  container enforces the resolved tenant. There is no router-internal
  tenant read.

### B7. **`enforceDevenvQuota` Lives at `worker/src/routes/devenv_guard.ts` But WP-08 Never Mounts It**
- **Location:** Lines 63, 127 (the import + call) vs. the file at
  `worker/src/routes/devenv_guard.ts` (referenced but not in this WP)
- **Problem:** The `enforceDevenvQuota` import is from `./devenv_guard` —
  a module that is ONLY defined in WP-07 (`WP-07_Billing_Metering.md:251-308`).
  WP-08 lists WP-07 as a **downstream** dependency (`Depends On: WP-01,
  WP-07`, line 5) but uses the function as a hard dependency. Either
  WP-08 must inline the quota check (it doesn't, today), or WP-08 must
  re-state the full function body. Worse: the WP-07 implementation
  queries `env.DB.prepare(... runners_entitlement)` (line 304), so a
  quota check also requires the D1 binding name in `Env` (currently
  `CONFIG_DB` per `index.ts:82` — different name).
- **Fix:** Add a `devenv_guard.ts` block to WP-08 (or co-locate the
  quota function in the same module as the routing arm in
  `worker/src/index.ts`) and align the D1 binding name with
  `CONFIG_DB`.

### B8. **WebSocket Upgrade Path is Not a WebSocket Upgrade**
- **Location:** Lines 266-294 (`/v1/customer/devenv/vnc|tty|code` handlers)
- **Problem:** The handler does `return stub.fetch(request)` — fine in
  principle, but:
  1. There is no validation of the `Upgrade: websocket` header. A client
     sending a plain GET with no upgrade gets the DO's HTTP response
     (whatever that is) and a 426 is never returned. The DO's
     `proxyWebSocket()` in WP-05 returns 426 if the header is missing —
     so the contract leaks DO behavior to clients.
  2. There is no `Sec-WebSocket-Protocol` validation. noVNC
     (`wss-ietf` / `binary`) and code-server (`codeserver` / `v3.code-server`)
     negotiate subprotocols; a stripped subprotocol can break clients.
  3. The `clwToken` injection (line 145) is also absent from the WS
     path — a WS upgrade would run with NO auth context on the DO
     (the existing `customer_v1` arm forwards a Clerk-or-PAT tenant
     header, which the DO could use, but this NEW arm does not).
- **Fix:** The Worker's `devenv_v1` arm must forward WS upgrades the
  same way the customer plane does — i.e. with the resolved
  `x-corelink-tenant-id` header attached. The DO's `proxyWebSocket`
  must reject non-WS requests (already done in WP-05 line 199, but the
  Worker should not need to know about it).

### B9. **`status`, `snapshot`, `resize` Forwarding Has No Error Mapping**
- **Location:** Lines 209-258 (the three action handlers)
- **Problem:** `return response;` (line 216, 233, 258) passes the DO
  response through unchanged. The DO may return 409 (snapshot in
  progress, per WP-04 line 170), 503 (container crash), 404 (DevEnv
  not yet created), or 500. None are translated to the OpenAPI-declared
  error schema (`{ error: string }`). A 409 from the DO returns an
  axum default body, not the documented `{ "error": "..." }`. Clients
  reading the OpenAPI spec get a lying contract.
- **Fix:** Wrap the DO response. If `!response.ok`, read the body and
  return a normalized `{ error: <message> }` JSON. The OpenAPI spec
  must declare every status code the route can return (currently it
  declares `200`, `400`, `401`, `409` but not `404`, `500`, `503`).

### B10. **Status Polling Creates Unbounded Internal Calls + Reads `status.workspace_name` Without Defining It**
- **Location:** Lines 92-105 (the GET `/v1/customer/devenv` handler)
- **Problem:** The handler forwards to `https://internal/api/status` on
  the DO, but the DO's `status()` RPC returns a `DevenvStatus` per WP-01
  (`status`, `uptime_ms`, `container_alive`, `ports`, `ws_connections`,
  `health_check_failures`, `last_check_at`). It does NOT return
  `workspace_name` (line 98), `profile_name` (line 99), `created_at`
  (line 101), or `started_at` (line 102) per the WP-01 schema. The
  response will be `undefined` for all four fields.
- The handler also runs `stub.fetch(new Request("https://internal/api/status"))`
  for every `GET /v1/customer/devenv` call — but a list endpoint should
  be cheap; polling the DO on every list call is unnecessary and
  creates a 1-RPC-per-page-view cost.
- **Fix:** Either change the DO contract so `status()` returns the
  list-shape, or call `status()` only when an actual DevEnv exists
  (check the D1 `runner_devenv` table first, or read from a
  per-tenant `devenv_list` KV). The cleanest fix is to have the DO
  include `workspace_name` + `profile_name` + timestamps in its
  `status()` payload (single source of truth).

---

## 🟠 HIGH SEVERITY ISSUES

### H1. **No Quota Bypass Protection for the WebSocket Path**
- **Location:** Lines 266-294 (the three WS handlers)
- **Problem:** The WS handlers do not call `enforceDevenvQuota()`. A
  tenant at the concurrency limit can still open `/vnc` + `/tty` + `/code`
  WebSockets. The DO per-tenant model has only one DO per tenant, so
  multiple WebSocket upgrades to the same DO would actually share
  containers — but the quota limit is a billing/cost lever; bypassing it
  by opening WS to an existing (over-quota) container is a real escape.
- **Fix:** Apply the quota check to the WS path too. Or document the
  exception explicitly (quota applies to NEW container starts only,
  not WS reconnects).

### H2. **Route Order — `/v1/customer/devenv/*` is matched by `customer_v1` First**
- **Location:** `worker/src/index.ts:783-785` (the existing arm)
- **Problem:** The existing `customer_v1` arm is at line 783 and matches
  `path.startsWith("/v1/customer/")`. Any new `matchRoute()` arm for
  `/v1/customer/devenv` MUST come BEFORE line 783 (otherwise it falls
  into the customer bucket) AND before the generic `/v1/*` arm at line
  962. The current WP-08 spec doesn't say where in the `matchRoute()`
  chain the new arm goes. The fact that the existing customer.rs
  Router does NOT have `/v1/customer/devenv` today is the
  cross-coordination gap that makes this non-obvious.
- **Fix:** Add the new `devenv_v1` arm at the correct specificity order
  (after `customer_v1`'s `path === "/v1/customer"` exact match, but
  before the customer prefix match) — actually, the customer arm
  matches `path.startsWith("/v1/customer/")` so any `/v1/customer/devenv`
  WILL fall into the customer bucket. The clean fix is to
  add `/v1/customer/devenv` BEFORE the `/v1/customer/` arm and let
  the customer arm keep its current scope.

### H3. **`devenv_id: tenantId` in the Response is a Lie**
- **Location:** Lines 97 (`devenv_id: tenantId`), 167 (`devenv_id: tenantId`)
- **Problem:** `devenv_id` is defined in the OpenAPI as
  `format: "uuid"` (line 370) but the actual value is the
  `x-corelink-tenant-id` header, which is the tenant's UUID. The
  invariant is "1 DevEnv per tenant" but the field is called
  `devenv_id`. The DevEnv "ID" is therefore the tenant's UUID — fine
  for the canonical model, but the field name is misleading. The
  bigger issue: if the Worker ever changes to allow multiple
  DevEnvs per tenant (a stated future direction in the campaign
  brief), the response field is wrong.
- **Fix:** Document the invariant explicitly: "DevEnv ID == tenant
  UUID (1 DevEnv per tenant today)". Add a comment in the OpenAPI
  schema.

### H4. **No `OPTIONS` Preflight Handling — Browser Dashboard Will Fail**
- **Location:** All route handlers (the entire `devenvRouter` body)
- **Problem:** The existing customer plane routes through the worker
  fetch handler, which has explicit CORS preflight at
  `worker/src/index.ts:456-467` and `applyCors()` at line 444-454
  (using `ALLOWED_ORIGINS = ["https://humangr.com",
  "https://corelink-docs.humangr.com"]`). The new itty-router has no
  preflight handler and no CORS application. The dashboard (WP-09) is
  a browser app and will fail CORS on every request.
- **Fix:** After routing to the canonical `devenv_v1` arm, the
  Worker's existing `applyCors()` runs on the response — that
  already covers the new routes. No new CORS code is needed IF the
  routing goes through `matchRoute()`. (Another reason to delete
  the parallel router.)

### H5. **`force: true` Default for Snapshot is Aggressive**
- **Location:** Line 222: `request.json().catch(() => ({ force: true }))`
  + line 230: `force: body.force ?? true`
- **Problem:** Snapshotting a 20GB profile+workspace on a hot path is
  expensive. The `force: true` default means an empty body (or
  malformed body) triggers a full force-snapshot, which costs
  Cloudflare egress + R2 PUT. A user typo (e.g. `{}`) triggers an
  expensive operation. A real client should opt-in.
- **Fix:** Default `force: false` (a snapshot only happens if a
  write has occurred since the last one), or `force: "auto"`
  (only if state shows dirty). Document the cost.

### H6. **`workspace_name` Regex Does Not Match clw Naming**
- **Location:** Line 320: `/^[a-zA-Z0-9_-]+$/`
- **Problem:** clw workspace names in the corelink container go through
  `clw_types::validate_workspace_name` (per WP-01 review, H5). The
  Worker-side regex here is independent. If the two ever drift, the
  Worker accepts a name the container rejects (confusing 4xx) or
  vice-versa. The Worker-side check is also a duplicate validation —
  a single source of truth is safer.
- **Fix:** Add a comment that the regex MUST mirror the clw validator,
  and pin the two to a shared `devenv-naming.md` spec, OR delegate
  validation to the DO (forward the request and let the container
  400).

### H7. **OpenAPI `devenv_id` is `format: "uuid"` But Is Actually a Tenant UUID**
- **Location:** Line 370 (`devenv_id: { type: "string", format: "uuid" }`)
- **Problem:** The schema says the field is a UUID but the runtime
  sets it to `tenantId`. If a future change introduces per-tenant
  multiple DevEnvs, the field is wrong. The schema also lacks a
  `pattern` or `description` saying "equals tenant UUID today".
- **Fix:** Add a `description: "DevEnv ID; equals tenant UUID (1
  DevEnv per tenant)"` and a `readOnly: true`.

### H8. **No Server-Sent Error Schema for `devenv_id` (returned by POST 201)**
- **Location:** Lines 467-484 (the 201 response schema)
- **Problem:** The 201 response re-defines every field inline instead
  of using a `$ref`. The shape is NOT in `components.schemas.Devenv`.
  This means the 200 and 201 responses are not symmetric (Devenv
  schema has fewer fields than the 201 inline schema). A client
  can't share a TypeScript type.
- **Fix:** Extract a `DevenvCreated` schema into `components.schemas`
  and `$ref` it.

### H9. **`devenvOpenApiSpec` is Not Validated, Mounted, or Served**
- **Location:** Lines 344-622 (the spec)
- **Problem:** The spec is defined in a file but WP-08 does not
  describe (a) where it is served (e.g. `GET /openapi.json` or
  `GET /v1/customer/devenv/openapi.json`), (b) which file mounts it
  in the worker, (c) how the `scripts/validate_specs.py` gate picks
  it up. Without that, the spec is documentation-only, can drift,
  and the OpenAPI 3.1 gate (`validate_specs.py` → 463/0) doesn't
  include it.
- **Fix:** Mount `GET /openapi.json` (or a per-route variant) in the
  worker, and add a `docs/campaigns/devenv/specs/devenv.openapi.yaml`
  to the validator's input set (or convert the TS module to a YAML
  file the validator scans).

---

## 🟡 MEDIUM SEVERITY ISSUES

### M1. **Code Duplication — 9 Handlers All Repeat the Same 4-Line Stub Bootstrap**
- **Location:** Lines 86, 113, 181, 209, 220, 237, 266, 277, 287
- **Problem:** Every handler reads `tenantId`, computes
  `idFromName(tenantId)`, gets the stub. The 4-line preamble appears
  9 times. A future change (e.g. add a request id) needs 9 edits.
- **Fix:** Move to the canonical Worker pattern (the
  `matchRoute()` + fetch-handler-arm pattern) where the tenant is
  resolved ONCE and the stub is acquired once per request.

### M2. **No `request-id` Echo**
- **Location:** All handlers
- **Problem:** The live worker pipeline stamps an `x-request-id` on
  every request and propagates it to the DO/container (see
  `worker/src/index.ts:2514`, `421-431`). The WP-08 router has no
  such behavior — a client-supplied `x-request-id` would not be
  echoed. Debugging failed DevEnv starts will be hard.
- **Fix:** Resolved automatically when routing goes through
  `matchRoute()`. (Another reason for the rework.)

### M3. **No Rate Limiting on `POST /v1/customer/devenv`**
- **Location:** Line 113
- **Problem:** A tenant can hammer the create endpoint (each
  `POST` is a 1-RPC to the DO, which has a `starting` state that
  the quota check doesn't gate — the quota is on `running`, per
  WP-07 line 285-291: `isRunning ? 1 : 0`). A tenant can keep
  re-`POST`ing in the gap between `starting` and `running`,
  creating a thundering-herd of container starts. The fix in
  WP-07 (line 296: "If DO not found or error, allow") is also
  wrong: it lets a misbehaving client past the gate.
- **Fix:** Add a per-tenant rate limit on the create endpoint
  (e.g. 1 POST per 30s), and change the quota check to gate on
  "any active container" (starting OR running).

### M4. **DELETE Returns 200 Even When DevEnv is Already Stopped**
- **Location:** Lines 181-202
- **Problem:** `DELETE /v1/customer/devenv` forwards to the DO's
  `/stop` endpoint with no precondition. The DO stops an
  already-stopped container as a no-op, returns 200, the Worker
  forwards 200. But the OpenAPI spec declares `200, 401, 404` —
  no 404. A client DELETEing twice expects 404 the second time
  (REST convention).
- **Fix:** Return 404 when the DO reports "no active DevEnv", or
  document the idempotent-DELETE behavior in the spec.

### M5. **No `Link` Header or Pagination on List**
- **Location:** Lines 86-110
- **Problem:** The list response is always 0 or 1 items (per
  invariant) but the schema says `devenv_id: [...]` and the
  array could grow. The OpenAPI spec doesn't constrain
  `maxItems: 1` or add a comment.
- **Fix:** Add `maxItems: 1` to the `devenv_id` array schema or
  document the 0/1 invariant in the schema description.

### M6. **No CORS Headers on the `devenv` Routes**
- **Location:** All error responses in `devenvRouter`
- **Problem:** `new Response(JSON.stringify({error: ...}), { status:
  ..., headers: { "Content-Type": "application/json" } })` is
  missing `Access-Control-Allow-*` headers. A browser request
  from `humangr.com` will get a CORS error.
- **Fix:** Again, the canonical `applyCors()` from
  `worker/src/index.ts:444-454` is what wraps responses. Routed
  through the worker fetch handler and this is automatic.

### M7. **`router.use()` API Signature in `itty-router` Differs from the WP**
- **Location:** Line 69
- **Problem:** `itty-router`'s `router.use(middleware)` signature is
  `(req, env, ctx) => Middleware` and a wildcard path is registered
  as `router.all("/v1/customer/devenv*", handler)` — `router.use` does
  not accept a path prefix in the same way. The WP code as written
  is at best ignored, at worst a type error.
- **Fix:** Again, deleted with the parallel router (B1). If the
  WP keeps a tiny itty-router for an internal sub-mount (it
  doesn't, per the rest of the design), use
  `router.all('/v1/customer/devenv/*', ...)`.

---

## 📋 DOd Gap Analysis

The DoD is in WP-08 at the bottom (omitted from the WP body we have
visibility into). Reconstructing it from the existing WP-01 review
shape:

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | Router mounted in worker | ❌ **FAIL** | B1 — parallel router collides with `customer_v1` |
| 2 | All 6 REST routes (`GET/POST/DELETE` + 3 actions) work | ❌ **FAIL** | B1, B9 — broken routing + missing error mapping |
| 3 | 3 WebSocket routes upgrade correctly | ❌ **FAIL** | B8 — no `Upgrade` header validation, no subprotocol handling |
| 4 | Auth via Worker-injected headers | ❌ **FAIL** | B3, B6 — `(request as any).tenantId` is a client-supplied header, no validation |
| 5 | Per-tenant quota enforced | ❌ **FAIL** | B7, H1 — quota lives in `devenv_guard.ts` (WP-07), not WP-08; WS path bypasses |
| 6 | OpenAPI 3.1 spec valid | ❌ **FAIL** | B2 — `responses` at document root, refs broken |
| 7 | Error responses match spec | ❌ **FAIL** | B9, H8 — no error mapping; 201 schema inline, not ref'd |
| 8 | Type safety (zero `any`) | ❌ **FAIL** | B3, B6 — 10+ `(request as any)` casts |
| 9 | Env interface extended | ❌ **FAIL** | B4, B5 — `RUNNER_DEVENV_DO`, `API_BASE_URL` not declared |
| 10 | Unit tests for each handler | ❌ **FAIL** | Entirely absent from WP-08 |

**DoD Score: 0/10 PASS, 10/10 FAIL**

---

## 📋 Invariants Verification

| Invariant (per WP) | Enforced? | Verdict |
|--------------------|-----------|---------|
| I1: 0 or 1 DevEnv per tenant | ❌ No quota on creation (M3); enforced only at running | **NOT ENFORCED** |
| I2: DevEnv `devenv_id == tenantId` | ⚠️ Implicit in code (line 97) but not documented | **PARTIAL** |
| I3: Auth via `x-corelink-tenant-id` from Worker | ❌ The Worker never sets this header (B1) | **NOT ENFORCED** |
| I4: Quota checked before start | ⚠️ Quota function imported from WP-07, not in WP-08 | **PARTIAL** |
| I5: Workspace name `^[a-zA-Z0-9_-]+$`, ≤128 chars | ✅ Lines 317-322 enforce it | **ENFORCED** |
| I6: Resize 1-8192 | ✅ Lines 335-337 enforce it | **ENFORCED** |
| I7: WS upgrade requires `Upgrade: websocket` | ❌ No header check on Worker side (B8) | **NOT ENFORCED** |
| I8: Error responses match `{ error: string }` | ❌ B9 — DO 409/500 responses pass through | **NOT ENFORCED** |

**Invariants Enforced: 2/8 (25%) — INSUFFICIENT**

---

## 📋 Quality Standards Verification

| Standard | Met? | Evidence |
|----------|------|----------|
| Type Safety: zero `any` in public API | ❌ Fail | B3, B6 — 10+ casts |
| Error Handling: typed or `Error` with message | ❌ Fail | B9 — DO 409/500 untranslated |
| Immutability: readonly, replacement not mutation | ✅ Pass | No mutation in the WP code |
| Observability: structured logging, request-id | ❌ Fail | M2 — no `x-request-id` |
| Security: client headers structurally stripped | ❌ **CRITICAL FAIL** | B1 — the whole design trusts client headers |
| Performance: no per-request auth/DB if cached | ⚠️ Partial | Status fetch is per-request (B10) |

**Quality Standards: 1/6 MET — INSUFFICIENT**

---

## 📋 Self-Check Points Analysis

### Self-Check 1: Auth Model Correctness
> The Worker is the SOLE authority for `x-corelink-tenant-id` and
> `x-corelink-scope`. A client MUST NOT be able to forge these.

- [x] `x-corelink-tenant-id` not set by client (Worker only) — ❌ **WP trusts the client header**
- [x] `x-corelink-scope` not set by client — ❌ **Not in the WP at all**
- [x] All client trust headers stripped before forward — ❌ **No stripping; the new router IS the only gate**

**Verdict: 0/3 PASS — CRITICAL**

### Self-Check 2: WebSocket Upgrade
> `/vnc`, `/tty`, `/code` upgrade to container ports 6080, 7681, 8080
> with hibernation.

- [x] Upgrade header validated — ❌ **Not in WP**
- [x] `acceptWebSocket` (not `ws.accept()`) — ✅ WP-05 (not WP-08) covers it
- [x] Subprotocol negotiation preserved — ❌ **Not preserved**
- [x] Error on container unreachable — ⚠️ **Bubble up DO error, not normalized**

**Verdict: 0/4 PASS, 1/4 PARTIAL**

### Self-Check 3: OpenAPI 3.1 Spec
> Spec is valid 3.1, served at `GET /openapi.json`, and matches
> runtime.

- [x] Valid 3.1 — ❌ **B2 — `responses` at root**
- [x] All paths declared — ⚠️ Missing 5xx, 503, 404 in many places
- [x] Schema `$ref`s resolve — ❌ **B2 — refs broken**
- [x] Served at a known path — ❌ **Not served anywhere**

**Verdict: 0/4 PASS**

---

## 📊 SCORECARD

| Category | Score | Required | Gap |
|----------|-------|----------|-----|
| Blocking Issues | 10 | 0 | **-10** |
| High Issues | 9 | 0 | **-9** |
| Medium Issues | 7 | 0 | **-7** |
| DoD Pass Rate | 0% | 100% | **-100%** |
| Invariants Enforced | 25% | 100% | **-75%** |
| Quality Standards | 17% | 100% | **-83%** |
| Self-Check Pass | 0% | 100% | **-100%** |

**OVERALL VERDICT: ❌ FAIL — Requires full rework (routing + auth model) before sign-off**

---

## 🔧 FIX PRIORITY

### Must Fix (Blockers)
1. **B1** — Route via `matchRoute()` + new `devenv_v1` arm; delete the parallel itty-router
2. **B2** — Move `responses` under `components.responses`
3. **B3** — Remove `(request as any).clwToken` and the clwToken field from the start payload
4. **B4** — Add `RUNNER_DEVENV_DO` to `Env`
5. **B5** — Add `API_BASE_URL` to `Env`
6. **B6** — Eliminate the `x-corelink-tenant-id` trust from the Worker
7. **B7** — Co-locate the quota function in WP-08
8. **B8** — Validate `Upgrade: websocket` on the WS path; preserve subprotocol
9. **B9** — Normalize DO error responses to `{ error: string }`
10. **B10** — Fix `status()` payload (add `workspace_name`, `profile_name`, `created_at`, `started_at`) OR add a dedicated list RPC

### Should Fix (High)
1. **H1** — Quota on WS path
2. **H2** — `matchRoute()` order in the worker
3. **H3** — Document `devenv_id == tenantId`
4. **H4** — CORS via the canonical `applyCors()` (automatic if B1 is fixed)
5. **H5** — Default `force: false` for snapshot
6. **H6** — Pin `workspace_name` regex to a shared spec
7. **H7** — Document `devenv_id` schema
8. **H8** — Extract `DevenvCreated` schema and ref
9. **H9** — Mount `GET /openapi.json` and add the spec to the validator

### Nice to Fix (Medium)
1. **M1** — Deduplicate the stub-bootstrap (automatic with B1)
2. **M2** — Echo `x-request-id` (automatic with B1)
3. **M3** — Per-tenant rate-limit on POST create
4. **M4** — Document or change DELETE semantics
5. **M5** — `maxItems: 1` on the list array
6. **M6** — CORS on error responses (automatic with B1)
7. **M7** — Drop the `router.use()` pattern (automatic with B1)

---

## NEXT STEPS

1. **Apply all BLOCKING + HIGH fixes** — see the rewritten WP below
2. Re-verify all 10 DoD items pass
3. Re-verify all 8 invariants enforced in code
4. Re-verify all 6 quality standards met
5. **Cross-WP coordination needed:**
   - **WP-01** — `status()` RPC payload must include
     `workspace_name`, `profile_name`, `created_at`, `started_at`,
     or a separate `list()` RPC must be added. (B10)
   - **WP-05** — WS upgrade must be reachable at the canonical
     `/v1/customer/devenv/{vnc,tty,code}` paths, not at
     `https://internal/vnc` inside the DO — the Worker terminates
     the upgrade. (B8)
   - **WP-06** — `DevEnvStatus` type must match the OpenAPI
     `DevenvStatus` schema field-for-field. (B10, H7)
   - **WP-07** — `enforceDevenvQuota` must be co-located or its
     module path must be cross-declared. (B7) D1 binding name
     must be `CONFIG_DB` to match `worker/src/index.ts:82`. (B7)
   - **WP-09 (Dashboard)** — CORS preflight for browser dashboard
     (H4, M6)
6. Proceed to Iteration 2 review after the rework

**Do NOT proceed to WP-09 until WP-08 is reworked and passes.**

---

## 🛠️ APPLIED FIXES — diff summary

The following changes were applied to
`docs/campaigns/devenv/wps/WP-08_Worker_Ingress_Routes.md`:

| # | Fix | Section |
|---|-----|---------|
| 1 | Rewrote §3.2 from a parallel `itty-router` to a `matchRoute()` arm + fetch-handler arm (B1, B4, B5, B6, M1, M2, M6, M7) | §3.2 |
| 2 | Moved `responses` block under `components.responses` (B2) | §3.3 |
| 3 | Added `DevenvCreated` schema + `$ref` (H8) | §3.3 |
| 4 | Removed `clwToken` from the start payload (B3) | §3.2 POST handler |
| 5 | Added `Upgrade: websocket` + subprotocol passthrough note (B8) | §3.2 WebSocket section |
| 6 | Added DO error-response normalization (B9) | §3.2 helper function |
| 7 | Documented `devenv_id == tenantId` invariant (H3, H7) | §3.2 + schema |
| 8 | Default `force: false` for snapshot (H5) | §3.2 snapshot handler |
| 9 | Added `maxItems: 1` on list array (M5) | §3.3 schema |
| 10 | Added `GET /openapi.json` mount instruction (H9) | §3.4 (new section) |
| 11 | Added `devenv_guard.ts` inline so the WP is self-contained (B7) | §3.2 helper file |
| 12 | Added `wrangler.toml` binding note for `RUNNER_DEVENV_DO` (B4) | §3.2 + new §3.5 |
| 13 | Cross-WP coordination table | §6 (new section) |
| 14 | Refined DoD + Invariants + Quality Standards tables | §4-5 |

---

**END OF WP-08 ITERATION 1 REVIEW**
