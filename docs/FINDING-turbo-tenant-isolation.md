# SECURITY FINDING — Turborepo cache: client `teamId` is not bound to the authenticated tenant

> **Found by:** the r2_kv SOTA test wave (2 independent agents flagged pad16
> non-injectivity) + lead verification of the full auth path · 2026-06-05
> **Surface:** `/v8/artifacts/*` (Turborepo remote cache), **live in prod**.
> **Severity: HIGH** — authenticated cross-tenant read/write. NOT unauthenticated.
> **Status: UNCOMMITTED finding — awaiting Owner's call on handling + fix scope.**

## What is verified (file:line)

1. **Auth IS required.** `worker/src/index.ts:1236-1253` — non-signup routes
   (incl. `turbo_v8`) run `extractAuth`; invalid PAT → 401. So the surface is
   not anonymous (an earlier agent claim of "unauthenticated" was wrong).
2. **Worker resolves the real tenant and injects it.** `index.ts:1263` +
   `:1433` — `x-corelink-tenant-id: <PAT-resolved tenant>` is forwarded to the
   container.
3. **Worker does NOT bind the client `teamId` to the PAT tenant.** The path-spoof
   guard `index.ts:1275` only fires when `urlTenant !== "_anonymous"`. Turbo
   routes are classified `tenantId:"_anonymous"` (`index.ts:296-298`), so the
   guard is skipped and the `?teamId=` query param is never compared to the PAT
   tenant.
4. **Container keys storage by the client `teamId`, ignoring the injected
   header.** `crates/corelink-container/src/routes/turbo_v8.rs:224-243` —
   `caller_tenant = params.team_id` (its own comment: *"for Phase 0 we use teamId
   as the caller_tenant … production wiring must thread the bearer-verified
   tenant from a tower layer here"*). The cross-tenant check
   (`corelink-turbo-bridge/src/handler.rs`) is `team_id == caller_tenant` — a
   tautology, since both come from the same client query param.
5. **Storage key derivation falls to `pad16`.** `r2_kv.rs:106-114` —
   `object_key` uses the injective HMAC `derive_prefix` only for `tdk=Some` AND
   a **UUID** tenant. A Turbo `teamId` is not a UUID ⇒ `pad16(teamId)` path,
   which truncates to 16 chars + pads short names ⇒ **non-injective** (two
   distinct teamIds sharing a 16-char prefix collide into one namespace).

## Impact

Any authenticated customer (valid PAT, any tier incl. free) can set
`?teamId=<victim>` and read or overwrite the victim's Turbo cache artifacts —
cross-tenant data disclosure (build outputs can carry proprietary code/secrets)
and cache poisoning. The `pad16` truncation is a secondary collision vector on
top of the primary "teamId not bound to tenant" defect.

## Interaction with PR #141 (durable Turbo cache) — the reason to HOLD

Today the Turbo store is `InMemoryKvStore` ⇒ any cross-tenant bleed is
**ephemeral** (per-container RAM, lost on restart). **PR #141 makes it durable
(R2-backed) and shared across all containers** ⇒ it would upgrade a latent
ephemeral gap into a **persistent, globally-shared cross-tenant data bleed**.
**Recommendation: do not merge #141 until tenant binding is fixed.**

## Recommended fix (clean, matches the route's own TODO)

1. **Bind storage isolation to the authenticated tenant, not the client teamId.**
   The container turbo route must derive the storage tenant from the injected
   `x-corelink-tenant-id` (the PAT-resolved tenant), threading it via a tower
   layer / extractor. `teamId` may remain a logical sub-namespace *within* the
   tenant, but it must never be the isolation boundary. This makes `object_key`
   receive the UUID tenant ⇒ the injective `derive_prefix` (HMAC) path ⇒ pad16
   becomes genuinely test-only.
2. **Fail closed in `object_key`.** When `tdk=Some` but the tenant is not a UUID,
   do not silently `pad16`-truncate; use a collision-resistant derivation (HMAC
   of the full string) or reject — defense in depth.
3. **Parallel review (separate lane):** `pad16` mirrors `r2_s3` raw-prefix
   behavior (`r2_kv.rs:153`). Audit CAS/AC for the same "is the storage tenant
   ever a non-UUID, client-influenced value?" question. Likely safe there
   (CAS/AC path tenant is PAT-bound + spoof-guarded), but confirm.

## Class-level root cause (escalated scope — 2026-06-05, lead verification)

The DO forwards the request to the container with **path + query unchanged and
only the tenant as a header** — `worker/src/durable_object.ts:214-225`
(`containerUrl = http://localhost:${PORT}${url.pathname}${url.search}`; no path
rewrite; headers, incl. the Worker-set `x-corelink-tenant-id`, preserved). The
Worker resolves the tenant from the PAT (`index.ts:282` DO reads
`x-corelink-tenant-id`).

⇒ **The ONLY trustworthy tenant source inside the container is the
`x-corelink-tenant-id` header.** Any route that keys isolation off a path or
query value is trusting client-controlled input, because (a) the DO never
rewrites the path, and (b) for routes the Worker classifies `"_anonymous"`
(turbo `/v8/*` AND the generic `/v1/*` arm — `index.ts:296-298`, `:381-383`) the
path-spoof guard (`index.ts:1275`) does not fire.

Per-surface posture (Worker classification → container tenant source):

| Surface | Worker class | Spoof guard | Container tenant source | Verdict |
|---|---|---|---|---|
| Bazel `/bazel/v2/<instance>` | path tenant | enforced | `x-corelink-tenant-id` header, `instance==header` | **SAFE** |
| Turbo `/v8/artifacts` | `_anonymous` | skipped | client `?teamId=` query | **EXPLOITABLE (verified)** |
| CAS `/v1/cas/:tenant/:hash` | `_anonymous` (`reapi_v1`) | skipped | `cas.rs:193` reads `Path((tenant,hash))`, header IGNORED, path tenant→caller_tenant (cas.rs:205) | **HANDLER VULNERABLE as-written; prod-reachability = open Q** |
| AC `/v1/ac/:tenant/:digest` | `_anonymous` (`reapi_v1`) | skipped | `ac.rs:179` reads `Path((tenant,…))`, header IGNORED, path tenant→caller_tenant (ac.rs:190) | **HANDLER VULNERABLE as-written; prod-reachability = open Q** |
| npm/pip/brew/cargo `/x/<tenant>` | path tenant | enforced | (container) | likely SAFE (guarded) — confirm |
| OCI `/v2/<tenant>` | path tenant | enforced | (container) | likely SAFE (guarded) — confirm |

CAS is the core product; if CAS/AC key off the path tenant and ignore the
header, they share the turbo defect and this is a **platform-critical**
pre-launch isolation gap, not a contained one. This must be confirmed before
launch (the `cas.rs` path appears to be `:tenant/:hash` while the live REAPI
path is `/v1/cas/blobs/<digest>/<size>` — the exact mount must be traced).

## SOTA fix (class-level, not a turbo point-patch)

1. **Introduce the shared container tower middleware / typed extractor** that
   reads `x-corelink-tenant-id`, **fail-closed** (missing/`_unknown`/`_anonymous`
   → 401/deny), and exposes the authenticated tenant to handlers. This is the
   "production wiring … from a tower middleware" the `cas.rs`/`ac.rs`/`turbo_v8`
   comments already promise. Mirrors Bazel's `caller_tenant()` helper, hoisted to
   a single shared layer so the defect class cannot recur.
2. **Every cache surface keys isolation off that authenticated tenant.** For
   turbo, the storage tenant becomes the PAT tenant (UUID ⇒ injective
   `derive_prefix` HMAC); `teamId` is preserved as a logical **sub-namespace**
   within the tenant (`write(authed_tenant, "<teamId>/<hash>", …)`), never the
   isolation boundary. The tautological `team_id == caller_tenant` check is
   removed.
3. Land **#141 (durable R2)** only after 1–2, so durability never persists a
   cross-tenant bleed.

## Full surface audit (2026-06-05) — TWO defect classes

Read-only audit of every mounted container surface against "what is the
isolation/authority source, and can the client control it?"

### Class 1 — tenant-binding (container ignored the trustworthy header)
The Worker SETS `x-corelink-tenant-id` and **overwrites it unconditionally**
(`index.ts:1433`), so the client cannot control it — it is trustworthy. These
surfaces ignored it and keyed off a client path/query value instead. Fix =
`AuthTenant` extractor (read the header; reject client-controlled mismatch).

| Surface | Source as-found | Verdict | Fix |
|---|---|---|---|
| Turbo `/v8/artifacts` | client `?teamId=` | DEFECT | FIXED (auth.0 = tenant; teamId → sub-namespace) |
| CAS `/v1/cas/:tenant/:hash` | path `:tenant` | DEFECT | FIXED (AuthTenant, path==auth.0 else 403) |
| AC `/v1/ac/:tenant/:digest` | path `:tenant` | DEFECT | FIXED (AuthTenant) |
| **audit_export `/v1/audit/:tenant/export`** | path `:tenant` (the `?tenant=` cross-check only fires if the attacker *opts in*) | **DEFECT** | PENDING (same AuthTenant fix) |
| Bazel `/bazel/v2/:instance/*` | header `caller_tenant()`, `instance==header` | SAFE (exemplar) | — |
| customer `/v1/customer/*` | header `tenant()` | SAFE | — |
| audit_analytics `/v1/audit/analytics/*` | header `X-Tenant-Id` | SAFE | — |

### Class 2 — privilege-header forgery (container trusts a client-forgeable header)
Distinct and more severe: unlike `x-corelink-tenant-id`, the admin headers
`x-admin-scope` / `x-admin-principal` / `x-admin-tenant` are **never set or
stripped** by the Worker or DO on the public path (grep of `worker/src` for
`x-admin*` = empty; Worker copies ALL client headers `index.ts:1424`; DO
forwards verbatim `durable_object.ts:219`). The container's
`require_admin_scope` (`admin_pilot.rs:802`) authorizes purely on the presence
of `x-admin-scope: corelink:admin:pilots` — no PAT-scope check.

| Surface | Authority source | Client-forgeable? | Verdict |
|---|---|---|---|
| **admin_pilot `/v1/admin/pilots/:tenant/{grant-tier,checkin}`** | header `x-admin-scope` | **YES** (not set/stripped by Worker/DO) | **PRIV-ESC + cross-tenant mutate** |
| admin `/v1/admin/*` (read/mutate) | TBD — must confirm if it also trusts a forgeable header | ? | MUST-VERIFY |

**Exploit (Class 2):** any valid PAT (free tier) + `x-admin-scope:
corelink:admin:pilots` + `x-admin-principal: x` → `POST
/v1/admin/pilots/<any-tenant>/grant-tier` passes `require_admin_scope` → mutates
arbitrary tenant tier/cap. admin_pilot is on the PUBLIC router
(`routes.rs:220` → `main.rs:289`, single port, no internal gate — contrast
`internal_pat` which IS gated by `CORELINK_INTERNAL_AUTH_KEY`).

**Class-2 fix (touches the Worker — core/money-path lane):** the Worker must
derive admin scope from the authenticated PAT (D1 scopes) and **set/overwrite
`x-admin-scope` while stripping any client-supplied value** — exactly as it does
for `x-corelink-tenant-id`. Alternatively the container independently verifies
the PAT's admin scope. Until then, `/v1/admin/pilots/*` must not be publicly
reachable (gate it behind the internal-auth key like `internal_pat`).

### Not-mounted (documented, not a launch risk)
npm/pip/cargo/brew/oci_v2/reapi_v2: worker-classified but NO container route
module mounted in `routes.rs::build_with_factory` ⇒ would 404. Track as
"declared, not implemented."

## Not introduced by #141

This is the existing Turbo **Phase 0** design (the route comment flags the
missing auth threading). #141 does not create it — but it materially worsens the
blast radius, which is why the fix should precede the durability switch.
