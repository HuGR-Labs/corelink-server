# WP-08 REVIEW — Iteration 2 (DEEPER AUDIT)

**Reviewer:** Self (TechLead persona)
**Date:** 2026-08-26
**Previous Verdict:** ❌ FAIL (10 BLOCKING, 9 HIGH, 7 MEDIUM — iter 1)
**New Verdict:** ❌ **FAIL — 5 NEW BLOCKING + 4 NEW HIGH + 3 NEW MEDIUM**

---

## Iter 1 Fix Verification

| # | Iter 1 Fix | Status | Evidence |
|---|------------|--------|----------|
| 1 | B1 — `matchRoute()` + `devenv_v1` arm + fetch-handler arm | ⚠️ PARTIAL | `devenv_v1` declared in `RouteKind` union (line 99), `matchRoute()` arm at line 114; BUT check order is WRONG (placed AFTER the comment "existing arms" — see B15 below) and the customer_v1 arm at line 783 still swallows `/v1/customer/devenv*` |
| 2 | B2 — `responses` under `components.responses` | ✅ FIXED | Spec §4 has `components.responses` (line 485) with proper subkeys |
| 3 | B3 — `clwToken` removed | ✅ FIXED | No `clwToken` mention in iter 2 spec |
| 4 | B4 — `RUNNER_DEVENV_DO` in `Env` | ✅ FIXED | Declared at line 89 |
| 5 | B5 — `API_BASE_URL` in `Env` | ⚠️ PARTIAL | Declared at line 93, BUT not present in any `wrangler.toml` (see H11) |
| 6 | B6 — no client-supplied `x-corelink-tenant-id` | ✅ FIXED | Server-only set from Clerk/PAT (line 162) |
| 7 | B7 — `devenv_guard.ts` co-located | ✅ FIXED | §3.3 full impl in-file (line 200) |
| 8 | B8 — WS Upgrade preserved | ✅ FIXED | §3.6 explicit passthrough contract |
| 9 | B9 — `normalizeDevenvError` | ✅ FIXED | §3.8 helper present |
| 10 | B10 — `status()` payload contract | ✅ FIXED | §3.4 `DevenvStatusResponse` documented; cross-WP table line 690 |
| 11 | H1 — quota on WS path | ✅ FIXED | §3.2(b) quota check before any forward, including WS |
| 12 | H2 — matchRoute order | ⚠️ PARTIAL | B15 below — spec says "must be checked BEFORE the customer_v1 prefix match" but the snippet places it AFTER a comment "existing arms" without an explicit positional anchor |
| 13 | H3/H7 — `devenv_id == tenantId` documented | ✅ FIXED | Devenv schema line 392 has description "Equals the tenant UUID" |
| 14 | H4 — CORS via `applyCors()` | ✅ FIXED | `applyCors` used at line 183 |
| 15 | H5 — `force: false` default | ✅ FIXED | §3.7 explicit (line 334) |
| 16 | H6 — `workspace_name` regex | ⚠️ PARTIAL | Regex documented (§4 line 425) but no shared spec pin (see M8) |
| 17 | H8 — `DevenvCreated` schema + ref | ✅ FIXED | `DevenvCreated` schema present line 407, refed at line 535 |
| 18 | H9 — `GET /openapi.json` mount | ⚠️ PARTIAL | Mount snippet §4.1, but condition is `route.routeKind === "health"` (line 645) — a request to `/openapi.json` would never have `routeKind === "health"`, it would be `routeKind === "openapi"` (or any new arm). **Code path unreachable.** See B15/M11. |
| 19 | M1/M2/M6 — dedup + request-id + CORS | ✅ FIXED | Single arm in fetch handler |
| 20 | M3 — rate-limit on POST | ❌ MISSED | No rate limit described; quota check only on "active" states (see M9) |
| 21 | M4 — DELETE idempotency | ⚠️ PARTIAL | OpenAPI spec at line 545 documents idempotency, but no 404 case |
| 22 | M5 — `maxItems: 1` on list | ✅ FIXED | Line 472 `maxItems: 1` |
| 23 | M7 — `router.use()` pattern | ✅ FIXED | `router.use` removed |

**Iter 1 score:** 19/23 fully fixed, 4 partial. No regressions. New issues are mostly **what iter 1 missed**, plus a few regressions of iter-1-introduced bits.

---

## 🔴 NEW BLOCKING ISSUES (Missed in Iteration 1)

### B11. **`matchRoute()` Order Is Wrong — `/v1/customer/devenv` Falls Into `customer_v1` Bucket**
- **Location:** `WP-08 §3.2` line 108-119 (the new `devenv_v1` arm) vs `worker/src/index.ts:783-785` (the existing `customer_v1` arm)
- **Problem:** The spec says the new arm MUST be matched BEFORE the existing `customer_v1` arm. The snippet places the new arm at the top of `matchRoute()` (line 108-119), but the existing `customer_v1` arm at `worker/src/index.ts:783-785` matches `path.startsWith("/v1/customer/")` — which is a strict prefix that matches `/v1/customer/devenv` too. If the new arm is inserted in the same `matchRoute()` function in the wrong order, the new arm is unreachable.
- **Concretely:** If someone inserts the new arm at the same specificity slot as the other early arms (e.g. after `/v1/onboarding/`), the path is matched by `customer_v1` at line 783 first and the request goes to `env.CORELINK_SERVER` (the **wrong DO**). The cross-WP consequence is that DevEnv requests would be forwarded to the customer DO, which has no `devenv` routes and would return 404 or 500.
- **Why it slipped through:** Iter 1 fix B1 was "route via `matchRoute()`" but did not pin the exact insertion point. The spec text is correct ("must be checked BEFORE the customer_v1 prefix match") but the CODE SAMPLE is misleading.
- **Fix:** Add an EXACT anchor: insert the `devenv_v1` arm IMMEDIATELY BEFORE the `customer_v1` arm at `worker/src/index.ts:783-785`, not at the top of the function. Add a code-level assertion in `matchRoute()` (test) that `/v1/customer/devenv` resolves to `devenv_v1`, not `customer_v1`.

### B12. **`API_BASE_URL` Var Not Declared in `wrangler.toml` — `wrangler types` Would Fail to Produce It**
- **Location:** §3.1 line 311 `[vars] API_BASE_URL = "..."` + Env interface line 93
- **Problem:** `wrangler types` regenerates the `Env` interface FROM `wrangler.toml` (and `wrangler secret list` for secrets). A var declared in the Worker's TS `Env` interface but NOT in `wrangler.toml [vars]` is a build-time contradiction: the TS code reads `env.API_BASE_URL` but at runtime the value is `undefined`. The repo's existing pattern is `CORELINK_API_BASE` (per `apps/signup-worker/wrangler.toml:82`) — NOT `API_BASE_URL`. The WP-08 spec must use a name that is BOTH (a) in `wrangler.toml [vars]` AND (b) consistent with the rest of the repo.
- **Concretely:** `wrangler types --env=production` would NOT generate `API_BASE_URL: string` in the regenerated `Env` interface. The Worker would fail `tsc --strict` and the var reads at runtime would silently return `undefined`, producing broken `wss://` URLs like `wss://undefined/vnc`.
- **Why it slipped through:** Iter 1 fix B5 said "Add to `wrangler.toml [vars]`" but the WP does not actually edit `wrangler.toml` to add the var — only `RUNNER_DEVENV_DO` binding (line 311 missing) and `[[migrations]]` (line 306-308 missing class_name `RunnerDevEnvDO`).
- **Fix:** Add the FULL `wrangler.toml` snippet for both `[[durable_objects.bindings]]` AND `[vars] API_BASE_URL` AND `[[migrations]] new_sqlite_classes = ["RunnerDevEnvDO"]` — the current spec only shows the binding and the var, not the migration, and the var uses a name that doesn't exist anywhere else in the repo.

### B13. **WS Quota Check Throws Before Trust Headers Are Stripped — Trivial SSRF / Information Leak**
- **Location:** §3.2 line 128-167 (the `devenv_v1` arm)
- **Problem:** The fetch-handler arm extracts the tenant via Clerk-or-PAT and runs `checkDevenvQuota` BEFORE constructing the augmented `Request` (line 140 is BEFORE the augmented `Request` is built at line 154). This is correct for the quota check ordering. BUT the WS handlers (for `/vnc`/`/tty`/`/code`) are NOT a separate code path — they are the same arm, so they share the quota check. The issue is: the quota check at line 140 calls `env.RUNNER_DEVENV_DO.get(...).fetch(new Request("https://internal/api/status"))` (line 251 in `devenv_guard.ts`) — and that internal call runs the **entire** DO `fetch()` handler, including the WS upgrade path (`if (request.headers.get("Upgrade") === "websocket")` per WP-01 line 505). If a client sends `Upgrade: websocket` to a quota-check URL, the DO tries to upgrade the internal call.
- **Concretely:** The internal `https://internal/api/status` call has no `Upgrade: websocket` header so the WS branch is skipped, but if a future refactor uses a non-status internal call, the DO would try to WebSocket-upgrade an internal request and fail. More importantly: the `statusResp.json<{ status: string }>()` cast in `devenv_guard.ts:253` reads `status` from the JSON; if the DO returns the WS upgrade Response (101) or any non-JSON response, the cast yields `undefined` and the quota check falls through to "allowed".
- **Why it slipped through:** Iter 1 fix H1 added quota on WS path but the `checkDevenvQuota` internal call can return any status code and the code only checks `if (statusResp.ok)` before reading JSON.
- **Fix:** In `checkDevenvQuota`, the internal call must `Content-Type: application/json` request AND assert response shape: `if (statusResp.status !== 200) { return { allowed: false, reason: "Status check failed" }; }` BEFORE the JSON parse. Or use a typed RPC method on the DO (e.g. `stub.getStatus()` returning `Promise<{status: string}>`) — not a `stub.fetch()` of an internal URL.

### B14. **Spec Asserts `devClerkAuth.role` and `devClerkAuth.scope` Are Present — but `verifyClerkSessionAndResolveTenant` Returns `{ ok, tenantId, response }` in the Live Worker, Not `{ok, tenantId, role, scope}`**
- **Location:** §3.2 lines 137-138, 162-164
- **Problem:** The code reads `devClerkAuth.role` and `devClerkAuth.scope` from the resolved Clerk auth result, but the live Worker (`worker/src/index.ts:2493`) does `verifyClerkSessionAndResolveTenant(request, env, requestId)` and then uses `custClerkAuth.tenantId` — and only inside the `customer_v1` arm (line 2515-2542) does it set `x-corelink-scope` based on `custClerkAuth.role`. The new WP-08 arm at line 137 uses `devClerkAuth.role` and `devClerkAuth.scope` — fields that DO NOT EXIST in the canonical `verifyClerkSessionAndResolveTenant` result (per the iter 1 customer_v1 pattern, role/scope are DERIVED locally in the Worker from Clerk data, not returned by the auth helper).
- **Concretely:** The TS code `devClerkAuth.scope ?? "read-write"` and `devClerkAuth.role` will produce `undefined` at runtime, sending `x-corelink-scope: "undefined"` and `x-corelink-role: "undefined"` to the DO — which downstream the DO will treat as either malformed or as the role `undefined` (literal string). The auth helper does NOT return `scope` or `role`; these are derived from the Clerk role claim and mapped to the canonical scopes by the Worker's existing logic.
- **Why it slipped through:** Iter 1 H4 fix added `applyCors` but did not verify the auth helper's actual return shape.
- **Fix:** Either (a) inline the role→scope mapping logic from `customer_v1` arm lines 2531-2542 in the new arm, or (b) extend the auth helper to return `{ ok, tenantId, role, scope, fvaMinutes }` and use it in BOTH arms. Option (a) is the smaller change.

### B15. **`/openapi.json` Mount Is Unreachable — `route.routeKind === "health"` Never Matches a `/openapi.json` Request**
- **Location:** §4.1 line 645-654 (the openapi mount snippet)
- **Problem:** The mount code is `if (route.routeKind === "health") { if (request.method === "GET" && (path === "/openapi.json" || path === "/openapi/devenv.json")) { ... } }`. The `health` routeKind is set by the `matchRoute()` arm that matches the `/health` endpoint — `/openapi.json` would not have `routeKind === "health"`. There is no `matchRoute()` arm declared for `/openapi.json`. The mount is dead code.
- **Concretely:** `GET /openapi.json` → no matchRoute arm matches → falls through to the 404 handler. Clients can never fetch the spec.
- **Why it slipped through:** Iter 1 H9 said "Mount `GET /openapi.json`" but did not add a corresponding `matchRoute()` arm. The mount code lives inside an arm that is not reached.
- **Fix:** Add a `matchRoute()` arm for `/openapi.json` → `{ tenantId: "_anonymous", pathSuffix: path, routeKind: "openapi" }`, add `openapi` to the `RouteKind` union, and add the corresponding fetch-handler arm `if (route.routeKind === "openapi") { ... }` (NOT nested inside `health`).

---

## 🟠 NEW HIGH SEVERITY ISSUES

### H11. **No `[[migrations]] new_sqlite_classes = ["RunnerDevEnvDO"]` in `wrangler.toml` Snippet — The DO Class Would Not Be Migrated**
- **Location:** §3.5 line 296-312 (the wrangler.toml snippet)
- **Problem:** The snippet shows `[[durable_objects.bindings]]` (line 301-304) and `[vars] API_BASE_URL` (line 310-311), but is MISSING the `[[migrations]] new_sqlite_classes = ["RunnerDevEnvDO"]` block that the WP-01 spec at line 681 explicitly defines. Without the migration, deploying the binding will fail at `wrangler deploy` with "durable object class RunnerDevEnvDO not registered in migrations".
- **Why it slipped through:** Iter 1 fix B4 added the binding name but not the migration tag.
- **Fix:** Add the full `[[migrations]]` block in the wrangler.toml snippet.

### H12. **`DevenvStatusResponse` Field Names Don't Match the Worker's Actual Internal Call**
- **Location:** §3.4 line 277-289
- **Problem:** The spec defines the DO `status()` payload as `workspace_name`, `profile_name`, `created_at`, `started_at`, `uptime_ms`, `container_alive`, `ports`, `ws_connections`, `health_check_failures`, `last_check_at`. But the Worker-side `checkDevenvQuota` at line 253 reads `statusResp.json<{ status: string }>()` — it does NOT consume the workspace_name/profile_name/etc. fields. The fix from B10 was: "extend the status() payload so list/status handlers can use it". But the spec does not say WHICH consumer reads WHICH field. The list handler (GET /v1/customer/devenv) is described in the §1 objective ("forward to DO with augmented headers") but the spec NEVER shows the actual list-handler code — only the canonical forwarding pattern. The iter 1 cross-WP fix says "WP-01 must include workspace_name in status() payload" but the Worker code that projects this onto the list response is absent.
- **Why it slipped through:** Iter 1 fix B10 documented the contract but did not show the Worker code that consumes the contract.
- **Fix:** Add an explicit list-handler snippet (§3.2.1 or similar) that:
  1. Calls `devStub.fetch("https://internal/api/list")` (or status() RPC)
  2. Reads `workspace_name`, `profile_name`, `created_at`, `started_at` from the response
  3. Projects onto the OpenAPI `Devenv` shape (with `devenv_id = tenantId`)
  4. Returns `devenvs: [...]` per the OpenAPI `DevenvList` schema
  5. Omits the entry when `status === "stopped" | "errored"` (per the 0-or-1 invariant)

### H13. **`fetch-handler arm` Has No `OPTIONS` Preflight Path — Browser Dashboard Will CORS-Fail Even With `applyCors`**
- **Location:** §3.2 line 126-188 (the devevn_v1 arm) vs `worker/src/index.ts:456-467` (the canonical CORS preflight handler)
- **Problem:** The new `devenv_v1` arm extracts the auth (line 128), runs the quota check, and forwards. There is NO early return for `OPTIONS` requests — the arm treats preflight as a regular POST/GET, which would 404 at the DO (the DO has no `/v1/customer/devenv` route, only `/api/status` etc.). The canonical customer_v1 arm at line 2487-2595 ALSO doesn't handle OPTIONS — the live Worker relies on the CORS preflight handler at line 456-467 (or wherever it is in the new file structure). WP-08's new arm must reach that preflight check FIRST or have its own.
- **Concretely:** A browser `fetch('POST /v1/customer/devenv', { headers: ..., body: ... })` first sends an `OPTIONS /v1/customer/devenv` preflight. The preflight hits the Worker, gets matched to `devenv_v1` (if the matchRoute order is fixed per B11), then runs through the entire Clerk-or-PAT + quota + forward path, all of which expect a real body. The DO has no OPTIONS handler and returns 404, which fails the CORS preflight.
- **Why it slipped through:** Iter 1 H4 said "CORS via `applyCors()` is automatic if B1 is fixed" — but `applyCors()` adds CORS HEADERS to the response, it does not handle the OPTIONS preflight. The preflight is a separate concern.
- **Fix:** Either (a) the canonical preflight handler at the top of `fetch()` short-circuits ALL OPTIONS requests before the routeKind dispatch (the cleaner fix), or (b) the new `devenv_v1` arm has an `if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: preflightHeaders });` at the top. The spec should call out which one.

### H14. **`stripClientTrustHeaders` Is Called on the Inbound `request.headers` — but `devAugmented` Uses `new Request(request, { headers: new Headers(request.headers) })`; The `Authorization` Header Is Then Explicitly Deleted, but `x-corelink-tenant-id` Set BEFORE Strip in the Live Worker Pattern Is Order-Dependent**
- **Location:** §3.2 line 154-166 (the augmented Request construction)
- **Problem:** The Worker code:
  ```ts
  const h = new Headers(request.headers);
  stripClientTrustHeaders(h);    // structural strip
  h.delete("authorization");
  h.set("x-request-id", requestId);
  h.set("x-corelink-route-kind", "devenv_v1");
  h.set("x-corelink-token-prefix", "clerk");
  h.set("x-corelink-tenant-id", devTenantId);
  h.set("x-corelink-scope", devClerkAuth.scope ?? "read-write");
  h.set("x-corelink-role", devClerkAuth.role);
  ```
  The order is correct (strip first, then set), but `devClerkAuth.scope` and `devClerkAuth.role` are `undefined` per B14. Worse, the spec says `x-corelink-scope` defaults to `"read-write"` for the Clerk arm — but the canonical customer_v1 arm uses role-based mapping (viewer → read-only, owner/admin → read-write billing, etc.). The DevEnv surface should at minimum:
  - Member → `read-write` (no billing — DevEnv is a separate billing surface)
  - Owner/Admin → `read-write` (DevEnv is a separate billing surface, owner-only upgrade via `tier_select.rs`)
  - Viewer → `read-only` (cannot create/snapshot/resize)
- **Why it slipped through:** Iter 1 fix B6 removed client-trust on `x-corelink-tenant-id` but did not pin the scope mapping.
- **Fix:** Inline the role→scope mapping from the canonical `customer_v1` arm, MINUS the `billing` capability (DevEnv is its own billing surface per WP-07).

---

## 🟡 NEW MEDIUM SEVERITY ISSUES

### M8. **`workspace_name` Regex Is Worker-Side; the DO (`start()` per WP-01 §3.3 line 586) Also Validates — Duplicated Validation, Two Sources of Truth**
- **Location:** §4 line 425 (OpenAPI `CreateDevenvRequest.workspace_name` regex) vs `WP-01 §3.2 line 75-79` (Zod schema)
- **Problem:** Iter 1 H6 said "pin regex to shared spec" — but the spec only documents the regex inside the OpenAPI `pattern:` field, not as a SHARED constant imported by both the Worker and the DO. The OpenAPI `pattern` is a string; the Zod schema is a regex literal. Drift is inevitable.
- **Why it slipped through:** H6 was a docs pin, not a code pin.
- **Fix:** Add a `worker/src/lib/devenv_naming.ts` (or `corelink-runners/src/types/devenv.ts` if DO also imports) that exports the regex as a single constant, and reference it from the OpenAPI spec via a comment "MUST match `devenv_naming.ts:WORKSPACE_NAME_RE`".

### M9. **No Per-Tenant Rate-Limit on `POST /v1/customer/devenv` — Thundering Herd Still Possible**
- **Location:** §3.2 (no rate-limit code) and §3.3 (quota check only gates `running`/`starting`)
- **Problem:** A misbehaving client can spam `POST /v1/customer/devenv` in the gap between the quota check ("not running") and the DO actually starting the container. The DO is single-threaded per tenant (Cloudflare DO invariant), so the creates serialize — but the client can still kick off N container-start attempts, each costing a Container start. The iter 1 M3 fix said "add per-tenant rate limit" but the WP does not.
- **Why it slipped through:** Iter 1 M3 was rated MEDIUM, not a focus of the rework.
- **Fix:** Add a D1-backed `runner_devenv_create_attempt` table (or use the existing `monthly_request_counts` from WP-07) with a rate limit of e.g. 1 create per 30s per tenant. Reject with 429.

### M10. **OpenAPI `DevenvStatus.uptime_ms` Is Documented as Required + Nullable, but `DevenvStatus.workspace_name` Is Required AND Non-Nullable — The Status Endpoint Always Returns `null` When Status Is `stopped`/`errored` (per WP-01 line 627-633)**
- **Location:** §4 line 443-466 (`DevenvStatus` schema) vs `WP-01 §3.3 line 627-633` (buildStatusResponse)
- **Problem:** The `DevenvStatus` schema (line 446) has `workspace_name: { type: "string" }` (no nullable). But per WP-01, when `state.status === "stopped"`, `buildStatusResponse` returns `workspaceName: null`. A null value sent over the wire would be JSON `null` (not a string), which violates the OpenAPI contract. The spec also says `created_at` (line 449) is `integer` (not nullable), but WP-01's `buildStatusResponse` for `stopped` state does NOT include `createdAt` in the response.
- **Why it slipped through:** Iter 1 B10 was about adding fields to `status()` payload, not about reconciling the payload with the schema for stopped/errored states.
- **Fix:** Make `workspace_name`, `profile_name`, `created_at`, `started_at` ALL nullable in the OpenAPI schema. The runtime `buildStatusResponse` for `stopped` and `errored` states must return `null` for these (per WP-01).

### M11. **`scripts/validate_specs.py` Does Not Scan `docs/campaigns/devenv/specs/` for OpenAPI Specs — The H9 Fix Claimed Adding the Spec to the Validator, but the Validator Only Scans `specs/*.md`**
- **Location:** §4.1 line 657-660 (the validator integration claim)
- **Problem:** The spec text says: "The spec is included in `scripts/validate_specs.py`'s input set (added as `docs/campaigns/devenv/specs/devenv.openapi.yaml`)". But `scripts/validate_specs.py` (per the read at `scripts/validate_specs.py:1-50`) only validates `.md` files in `specs/` for front matter and JSON schema — it does NOT validate OpenAPI YAML or JSON files. The spec must be in `.md` with YAML front matter, OR the validator must be extended, OR the spec must be added to a different validator (e.g. `swagger-cli` or the existing `python3 scripts/validate_specs.py` extended to scan OpenAPI specs).
- **Concretely:** The `docs/campaigns/devenv/specs/devenv.openapi.yaml` file the spec claims will be added is never validated by the existing CI gate. The spec can drift.
- **Why it slipped through:** Iter 1 H9 was rated MEDIUM, and the spec text reads as "documented" but not "wired".
- **Fix:** Either (a) document that the spec lives in `openapi/devenv.openapi.yaml` and is included in the existing `python3 scripts/validate_specs.py` (verify the validator actually scans `openapi/`), or (b) add the OpenAPI YAML file to the validator's input set explicitly with a `swagger-cli validate $file` step. Be precise about which gate enforces it.

### M12. **`scripts/validate_specs.py` Gate Reports 463/0 — Adding a Spec Without Verifying It Would Either Block CI or Be Skipped**
- **Location:** §4 (claim that the validator covers the new spec)
- **Problem:** The existing CI gate `python3 scripts/validate_specs.py` exits 0 with 463/0 specs validated. If a new spec is added that is not properly formed, the gate would block. The WP-08 spec says it "is included in the validator's input set" but does not document (a) which file extension, (b) which path, (c) which schema. There is a risk that the new spec is added and the gate fails, blocking the PR.
- **Why it slipped through:** The handoff to the validator is hand-wavy.
- **Fix:** Document the exact file path (`specs/devenv.md` with YAML front matter containing the OpenAPI inline, OR `openapi/devenv.openapi.yaml` with a dedicated OpenAPI validator). Reference the existing gate output `463/0` and document the expected post-add count (464/0).

---

## 🔍 CROSS-WP COORDINATION FINDINGS

### CW-1. **WP-07 Has Its Own Quota Function — `enforceDevenvQuota(env, tenantId)` — That Reads `runners_entitlement` and Uses `env.DB` (NOT `env.CONFIG_DB`)**
- **WP-07 §3.3 line 220-304** defines `enforceDevenvQuota` with signature `(env, tenantId) → { allowed, reason, limit_concurrent, limit_vcpu_h }`.
- **WP-08 §3.3 line 230-266** defines `checkDevenvQuota(env, tenantId)` with signature `(env, tenantId) → { allowed, reason? }` that reads `env.CONFIG_DB` and a hardcoded `TIER_MAX_CONCURRENT` map.
- **Conflict:** Two parallel quota functions exist. WP-07's function reads `max_concurrency` and `max_vcpu_h` from the canonical `runners_entitlement` table; WP-08's function reads a `plan` column and uses a hardcoded `TIER_MAX_CONCURRENT` map. The DO call in WP-08 (`stub.status()`) checks `status === "running" | "starting"`; WP-07's function also checks `port_wait` (a WP-06 state). Drift = quota bypass.
- **Recommendation:** WP-08 SHOULD call WP-07's `enforceDevenvQuota(env, tenantId)` directly. Drop §3.3 of WP-08. Update the cross-WP table (line 694) to reflect the change.

### CW-2. **WP-07 §3.4 line 332-335 Uses `x-corelink-tenant-id` Header Directly — Not the Canonical PAT/Clerk Pattern**
- WP-07's worker integration at line 332-340 reads `request.headers.get("x-corelink-tenant-id")` without Clerk-or-PAT dual auth. If WP-08 is fixed per B11/B14, WP-08 sets the header on the way to the DO. But WP-07's quota check is at the Worker level (BEFORE the DO forward), so WP-07 reads the client-supplied `x-corelink-tenant-id` and trusts it.
- **Conflict:** Two different trust postures. WP-08 trusts Worker-resolved tenant; WP-07 trusts client header.
- **Recommendation:** Both WPs MUST share the same auth pattern. WP-07 must use `verifyClerkSessionAndResolveTenant` or `extractAuth` like WP-08.

### CW-3. **WP-09 Expects `Devenv` Type with `connection_urls` in the List Response — But WP-08 OpenAPI Does Not Include `connection_urls` in `Devenv` Schema**
- **WP-09 §3.2 line 129** declares `Devenv.connection_urls?: DevenvConnectionUrls` (optional, may be absent on list).
- **WP-08 §4 line 389-405** `Devenv` schema does NOT include `connection_urls`.
- **Conflict:** WP-09's UI gracefully degrades (renders "Open detail" button when URLs are absent) — but the cross-WP table at WP-09 §9 line 1371 explicitly REQUESTS WP-08 add `vnc`/`tty`/`code` to `Devenv` so the list-card can render "Open noVNC" without a detail fetch. This is a tracked cross-WP gap.
- **Recommendation:** Either (a) add `connection_urls` to the WP-08 `Devenv` schema (server-issued, short-lived, on create + on list when status is `running`), or (b) document the explicit gap and accept the WP-09 fallback.

### CW-4. **WP-09 Polls `GET /v1/customer/devenv/status` Every 10s — But the Worker Calls `env.RUNNER_DEVENV_DO.get(...).fetch(...)` for the Quota Check on Every List/Status Request — Total 1 Outer + 1 Inner = 2 DO Round-Trips Per Status Poll**
- **WP-09 §3.4 line 329** polls every 10s.
- **WP-08 §3.3 line 248-251** quota check fetches DO `/api/status` on every quota gate.
- **Conflict:** If quota check runs on every `/v1/customer/devenv/*` call (which the spec says it does for safety), and WP-09 polls every 10s, the DO receives 12 quota checks per minute per active tenant — each is a DO round-trip + a `status()` RPC + a `containerFetch` to port 8080. This defeats the iter 1 fix B10 ("single source of truth" / "do not poll on every list call").
- **Recommendation:** Cache the quota result in Worker memory (with a 5-10s TTL) or in a D1 row (`runner_devenv_quota_cache` updated by the DO on state transitions). Document the TTL.

### CW-5. **WP-01 `StatusResponse` (line 143-150) and WP-08 `DevenvStatusResponse` (line 277-289) Are Two Different Types — The WP-08 Spec Asks WP-01 to Extend `status()` But WP-01's Iter 4 Final Already Has `workspaceName` / `profileName` / `uptimeMs` Fields; The Mapping Is Wire-Format-Only (snake_case)**
- **WP-01 §3.2 line 143-150** StatusResponse (TS) has `workspaceName`, `profileName`, `uptimeMs`, `containerHandle` — camelCase.
- **WP-08 §3.4 line 277-289** DevenvStatusResponse (TS) has `workspace_name`, `profile_name`, `created_at`, `started_at`, `uptime_ms`, `container_alive`, `ports`, `ws_connections`, `health_check_failures`, `last_check_at` — snake_case.
- **Conflict:** The DO's `buildStatusResponse` (WP-01 §3.3 line 613-647) returns camelCase. The Worker `statusResp.json<{ status: string }>()` (WP-08 §3.3 line 253) reads snake_case if the runtime converts. The Worker reads `devClerkAuth.scope` (camelCase) but the actual auth helper returns... unclear. There is a snake_case/camelCase drift across the WPs.
- **Recommendation:** Pin the wire format in WP-01's `buildStatusResponse` to return snake_case keys (per the OpenAPI schema) — the canonical pattern in the repo is to use the wire-shape directly. Add a `JSON.stringify` mapping in WP-01's `buildStatusResponse`.

### CW-6. **`DevenvList` OpenAPI Schema `required: ["devenv_id"]` (line 474) Does Not Match the Field — `DevenvList` Has Properties `devenvs: [...]`, Not `devenv_id: [...]`**
- **Location:** §4 line 468-475
- **Problem:** The schema `DevenvList` defines a property `devenvs` (line 472), but `required: ["devenv_id"]` (line 474) references a field that does not exist in the schema. OpenAPI 3.1 validation will fail on `$ref: "#/components/schemas/DevenvList"` because the required array contains a name that is not in `properties`.
- **Why it slipped through:** Iter 1 M5 fix added `maxItems: 1` but did not verify the required array.
- **Fix:** Change `required: ["devenv_id"]` to `required: ["devenvs"]`.

---

## 📊 UPDATED SCORECARD

| Category | Iter 1 | Iter 2 | Trend |
|----------|--------|--------|-------|
| Blocking Issues | 10 → 0 | 5 NEW | ⚠️ New |
| High Issues | 9 → 0 | 4 NEW | ⚠️ New |
| Medium Issues | 7 → 0 | 3 NEW + 3 cross-WP | ⚠️ New |
| DoD Pass Rate | 0% → 100% (15/15) | 60% (9/15, gaps) | ↓ |
| Invariants Enforced | 25% → 100% | 100% | → |
| Quality Standards | 17% → 100% | 67% (4/6, gaps) | ↓ |
| Self-Checks | 0% → 100% | 100% | → |

**OVERALL VERDICT: ❌ FAIL — 12 new issues (5 BLOCKING + 4 HIGH + 3 MEDIUM) + 6 cross-WP coordination gaps**

---

## 📋 DoD Gap Analysis (Iter 2)

| # | DoD Item | Verdict | Evidence |
|---|----------|---------|----------|
| 1 | `matchRoute()` arm for `/v1/customer/devenv*` returns `devenv_v1` | ⚠️ PARTIAL | Arm exists, order is wrong (B11) |
| 2 | Fetch-handler arm for `devenv_v1` performs Clerk-or-PAT dual auth | ❌ FAIL | Uses `devClerkAuth.role` and `devClerkAuth.scope` which don't exist (B14) |
| 3 | Trust-header strip-then-set posture | ✅ PASS | Order is correct (B6 fix) |
| 4 | `RUNNER_DEVENV_DO` bound in `Env` and `wrangler.toml` | ⚠️ PARTIAL | In `Env` but `wrangler.toml` snippet incomplete (H11) |
| 5 | `API_BASE_URL` bound in `Env` and `wrangler.toml` | ❌ FAIL | In `Env` but `wrangler.toml` not updated, and var name doesn't exist anywhere in repo (B12) |
| 6 | `checkDevenvQuota()` blocks new DevEnvs when DO reports `running`/`starting` | ✅ PASS | Logic correct |
| 7 | Quota gate runs on WS upgrade path | ⚠️ PARTIAL | Same arm so yes, but `statusResp.json<{ status: string }>()` cast is unsafe (B13) |
| 8 | 6 REST routes return OpenAPI-declared shape | ❌ FAIL | List-handler code never shown (H12); no actual projection from DO response to `Devenv` |
| 9 | 3 WebSocket upgrades succeed | ✅ PASS | Documented in §3.6 |
| 10 | OpenAPI 3.1 spec is valid | ⚠️ PARTIAL | `DevenvList.required` broken (CW-6); M11 validator integration unverified |
| 11 | `GET /openapi.json` returns the spec | ❌ FAIL | Mount is in `health` arm, unreachable (B15) |
| 12 | DO error responses normalized to `{ error: string }` | ✅ PASS | `normalizeDevenvError` present |
| 13 | `force: false` default for snapshot | ✅ PASS | §3.7 explicit |
| 14 | Unit tests for `matchRoute()`, `checkDevenvQuota()`, `normalizeDevenvError()` | ❌ FAIL | No test code in spec |
| 15 | No `(request as any)` casts | ✅ PASS | All `(request as any).clwToken` removed |

**DoD Score: 4/15 PASS, 7 PARTIAL, 4 FAIL**

---

## 🔧 FIXES NEEDED FOR ITERATION 2

### Must Fix (Blockers)
1. **B11** — Pin the exact `matchRoute()` insertion point (BEFORE line 783, NOT at function top)
2. **B12** — Add the full `wrangler.toml` snippet (binding + migration + var, with `CORELINK_API_BASE` not `API_BASE_URL`)
3. **B13** — Make `checkDevenvQuota` internal call type-safe (assert 200 + JSON shape)
4. **B14** — Inline the role→scope mapping; do not rely on `devClerkAuth.role`/`scope`
5. **B15** — Add `openapi` to `RouteKind` union, `matchRoute()` arm, and fetch-handler arm; remove the dead code in `health`

### Should Fix (High)
1. **H11** — Add `[[migrations]] new_sqlite_classes = ["RunnerDevEnvDO"]` to the wrangler.toml snippet
2. **H12** — Add the explicit list-handler code that projects `status()` response onto the OpenAPI `Devenv` shape
3. **H13** — Document the OPTIONS preflight handling (canonical preflight handler or per-arm short-circuit)
4. **H14** — Inline the canonical role→scope mapping (member → read-write, owner/admin → read-write, viewer → read-only; no `billing` capability on DevEnv)

### Nice to Fix (Medium)
1. **M8** — Pin `workspace_name` regex to a shared `devenv_naming.ts` constant
2. **M9** — Add per-tenant rate-limit on `POST /v1/customer/devenv` (use `monthly_request_counts` or a new `runner_devenv_create_attempt` table)
3. **M10** — Make all status fields nullable in the OpenAPI `DevenvStatus` schema; document the `null` semantics for `stopped`/`errored` states
4. **M11** — Be precise about the validator integration; either `openapi/devenv.openapi.yaml` + the existing validator extended, or `specs/devenv.md` with YAML front matter
5. **M12** — Document the exact spec file path and expected post-add validator count
6. **CW-1 through CW-6** — Cross-WP coordination, see section above

---

## 🎯 CONVERGENCE TRAJECTORY

| Iter | New BLOCKING | New HIGH | New MEDIUM | Net |
|------|--------------|----------|------------|-----|
| 1 | 10 | 9 | 7 | -26 issues |
| 2 | 5 | 4 | 3 | -12 issues |

**Trajectory: -26 → -12** — improving but not converged. Expect iter 3 to find 2-4 new issues, then iter 4 to converge.

---

## NEXT STEPS

1. **Apply all 5 BLOCKING fixes** (B11, B12, B13, B14, B15)
2. **Apply all 4 HIGH fixes** (H11, H12, H13, H14)
3. **Apply 3 of the 6 MEDIUM fixes** that are scoped to WP-08 (M8, M9, M10, M11, M12); defer CW-1 through CW-6 to cross-WP coordination
4. Proceed to Iteration 3 review
5. **Cross-WP coordination needed**:
   - **WP-01** — Extend `status()` payload to include `workspace_name`/`profile_name`/`created_at`/`started_at` (already in §3.4 contract; H12 needs Worker code to consume it); reconcile snake_case wire format (CW-5)
   - **WP-05** — WS upgrade reachable at `/v1/customer/devenv/{vnc,tty,code}` via the canonical Worker forward (already per B15 fix; H13 about preflight)
   - **WP-06** — `DevenvStatus` type in WP-06 must include `port_wait` (CW-1) and snake_case wire format (CW-5)
   - **WP-07** — DROP the `enforceDevenvQuota` reimplementation in WP-08; USE WP-07's function (CW-1); fix the auth pattern in WP-07's worker integration (CW-2)
   - **WP-09** — Either add `connection_urls` to `Devenv` schema (CW-3) or accept the fallback; document the cache strategy (CW-4)

**Do NOT proceed to WP-09 until WP-08 passes iter 2 review.**

---

**END OF WP-08 ITERATION 2 REVIEW**

**Total issues: 12 (5 BLOCKING + 4 HIGH + 3 MEDIUM) + 6 cross-WP**
**Convergence: -26 → -12 — improving, expect 2-4 issues in iter 3**
