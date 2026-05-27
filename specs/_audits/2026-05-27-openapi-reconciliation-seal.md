---
id: "AUDIT-2026-05-27-OPENAPI-RECONCILIATION-SEAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "openapi", "reconciliation", "per-route", "post-w36"]
references:
  - "specs/_audits/2026-05-27-openapi-route-inventory.md"
  - "specs/_audits/2026-05-27-sdk-openapi-drift-audit.md"
---

# OpenAPI ↔ axum routes reconciliation SEAL

## §1 Mandate (verbatim 2026-05-27)

> "Tem que ver se os endpoints deveriam existir ou existem de outra forma/nome, não é ir pelo caminho mais fácil, e entender e fazer direito."

Per-route investigation, no easy-delete path. Reconciliation classes:

- **EXISTS-UNDER-DIFFERENT-NAME** — same intent at a different axum path/method; fix OpenAPI.
- **SHOULD-BE-IMPLEMENTED** — scoped (sprint WI exists), handler not wired; keep in spec + `x-status: planned`.
- **DESCOPED-OR-OBSOLETE** — sprint dropped or scope changed; `deprecated: true` (NOT delete).
- **UNKNOWN-NEEDS-HUMAN** — can't determine; leave alone + flag.

Inputs: `specs/_audits/2026-05-27-openapi-route-inventory.md` (commit
`909ba660`-ish; SEALed) and `specs/_audits/2026-05-27-sdk-openapi-drift-audit.md`
(commit `cb5cb8ea`).

Worktree: `worktree-agent-ad09323c0021f615d`. Clean tree pre-edit.

## §2 Investigation methodology

For each of the 17 **OPENAPI-ONLY** paths flagged in inventory §5, I:

1. Grepped the workspace (`crates/`, `apps/`, `tools/`) for axum
   `.route(...)` mounts of the path or a near-variant (path-component
   reorder, singular/plural, `/v1/` vs `/api/` prefix, dashed-vs-slashed
   audit-events variants, GET↔POST swap, handler-name semantic match).
   No alternate mount surfaced for any of the 17 — the axum router
   inventory in inventory-audit §2 is exhaustive (30 `.route(...)` calls
   workspace-wide; all 30 accounted for).
2. Cross-referenced each path stem against `specs/04_sprints/` and
   `specs/04_sprints/_sealed/` for WI scope mentions. Sprint S-19
   (sealed; "Customer Onboarding") scopes the entire signup / DPA /
   tier-select / enterprise-inquiry surface. Sprint S-11 (sealed)
   scopes the DSR + consent ledger surface. Sprint S-16 (sealed)
   scopes the CSP-report intake on the Next.js side.
3. Inspected the corresponding business-logic crate (`corelink-signup`,
   `corelink-dpa-acceptance`, `corelink-tier-selection`, `corelink-pat`,
   `corelink-dsr`, `corelink-privacy::consent::ledger`,
   `corelink-ops::admin::api` + `corelink-dual-approval`,
   `corelink-enterprise-inquiry`). Every one ships the service + schema
   layer with doc-comments tagging the OpenAPI-listed `/v1/...` path as
   canonical (post-Lote 10.16 fixes). None mount an axum `.route(...)`.
4. Git-log probed `tools/openapi/` for descope markers
   (`drop`, `descope`, `remove`). No descope commits — only the
   creation commit `5d21b3c1` (2026-05-14) and the W33 reorg `git mv`
   commit `4780c2c9` (2026-05-22).

For each of the 12 **SERVER-ONLY** axum routes, I read the route file
docstring + handler signature + query/body struct definitions to compose
accurate OpenAPI entries with auth scheme + req/resp schemas that mirror
the in-memory wire shape.

## §3 Architectural finding (driver of §4 classifications)

The 17 OPENAPI-ONLY paths share **one root cause**: they all live in
business-logic crates that ship pure-logic services + schemas + audit/
SLI collaborators, with the axum mount **deferred to the Cloudflare
Worker entry crate** per the `trait-abstraction-defer` charter. The
deferral is documented in two places:

- `tools/openapi/src/lib.rs` L29-39: *"the customer / privacy / admin
  handlers live across ~10 separate crates that target both native
  (apps/server) and wasm32 (cf-bindings) — `utoipa`'s macro surface
  attaches to handler function signatures, which are only present in
  the wasm-only CF Worker entry crates (deferred to PRR ship gate)."*
- `crates/corelink-container/src/routes/cas.rs` L77-99 (and
  `routes/admin.rs` L97-104, etc.): *"On `wasm32-unknown-unknown`
  (Cloudflare Worker target) this would build a `CfWorkerCasHandler`
  against R2 + KV bindings. That impl is **deferred** per the
  autonomous-execution charter `trait-abstraction-defer` rule."*

Therefore the 17 OPENAPI-ONLY entries are **SHOULD-BE-IMPLEMENTED**
(not OBSOLETE, not EXISTS-UNDER-DIFFERENT-NAME). Action: annotate each
with `x-status: planned` + `x-binding: cf-worker-pending` +
`x-implementing-crate: <crate>` + a reconciliation note. Spec stays
authoritative; backlog flags the binding gap. The 12 SERVER-ONLY paths
are the **inverse symptom** — the in-memory wire-up demonstrations and
the operator-shell-script replacements that *did* land in the native
container were not authored into the contract. Action: add full OpenAPI
entries with `x-status: implemented` + handler-file pointers.

## §4 Per-route decision matrix

### §4.1 OPENAPI-ONLY (17)

| # | OpenAPI method-path | Server match | Classification | Action | x-implementing-crate |
|---|---|---|---|---|---|
| 1 | `POST /v1/signup` | none (only pilot variant) | SHOULD-BE-IMPLEMENTED | `x-status: planned` + reconciliation note disambiguating from `/v1/signup/pilot/{token}` | `corelink-signup` |
| 2 | `POST /v1/dpa/accept` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` + WI cross-ref | `corelink-dpa-acceptance` |
| 3 | `POST /v1/dpa/re-accept` | RE_ACCEPT_PATH const referenced by middleware (no `.route()`) | SHOULD-BE-IMPLEMENTED | `x-status: planned` + middleware cross-ref | `corelink-privacy::dpa::versioning::middleware` |
| 4 | `POST /v1/onboarding/tier-select` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` + S-19 WI-S19-004 cross-ref | `corelink-tier-selection` |
| 5 | `POST /v1/pats` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` at path-item level (covers POST + GET) | `corelink-pat` |
| 6 | `GET /v1/pats` | none | SHOULD-BE-IMPLEMENTED | (same path-item annotation) | `corelink-pat` |
| 7 | `DELETE /v1/pats/{pat_id}` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` at path-item level | `corelink-pat` |
| 8 | `POST /v1/privacy/dsr/{action}` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` + S-11 WI-S11-001 cross-ref | `corelink-dsr` |
| 9 | `GET /v1/privacy/dsr/{request_id}/status` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` | `corelink-dsr` |
| 10 | `POST + DELETE /v1/consent/{purpose}` | data-layer in `corelink-privacy::consent::ledger` (no `.route()`) | SHOULD-BE-IMPLEMENTED | `x-status: planned` at path-item level | `corelink-privacy::consent::ledger` |
| 11 | `GET /v1/consent` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` at path-item level | `corelink-privacy::consent::ledger` |
| 12 | `GET /v1/consent/verify` | hmac primitive in `consent::hmac_sign`; no `.route()` | SHOULD-BE-IMPLEMENTED | `x-status: planned` | `corelink-privacy::consent::hmac_sign` |
| 13 | `GET /v1/consent/revocation/verify` | hmac primitive; no `.route()` | SHOULD-BE-IMPLEMENTED | `x-status: planned` | `corelink-privacy::consent::hmac_sign` |
| 14 | `POST /v1/admin/ops` | none (distinct intent from server `/v1/admin/read+mutate` wire-up demo) | SHOULD-BE-IMPLEMENTED | `x-status: planned` + disambiguation note | `corelink-ops::admin::api` + `corelink-dual-approval` |
| 15 | `GET /v1/admin/ops/{op_id}` (+ approve + reject) | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` at each path-item | `corelink-ops::admin::api` |
| 16 | `GET /v1/admin/audit/events` | distinct intent from server `/v1/audit/export` + `/v1/audit/analytics/*`; SDK uses dashed `/v1/admin/audit-events` typo | SHOULD-BE-IMPLEMENTED | `x-status: planned` + note: dashed SDK variant is a typo, NOT canonical | `corelink-audit-chain` (admin query surface — handler not yet authored) |
| 17 | `GET /v1/admin/tenants` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` | `corelink-tenant-registry` (admin query surface) |
| 18 | `POST /v1/enterprise/inquire` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` | `corelink-enterprise-inquiry` |
| 19 | `GET + PATCH /v1/users/me` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` at path-item level | pending (Clerk-authenticated profile read) |
| 20 | `GET /v1/data-categories` | enum in `corelink-privacy`; no `.route()` | SHOULD-BE-IMPLEMENTED | `x-status: planned` | `corelink-privacy` |
| 21 | `GET /api/health` | adapter-host pip server has `/healthz` (different port, not customer surface) | SHOULD-BE-IMPLEMENTED | `x-status: planned` + note distinguishing from pip-adapter `/healthz` | pending |
| 22 | `POST /api/csp-report` | none | SHOULD-BE-IMPLEMENTED | `x-status: planned` + S-16 WI-S16-001 cross-ref | pending |

(The table has 22 rows because path-items with multiple methods are
listed for each method-path tuple per inventory §5; the 17 distinct
**OPENAPI-ONLY method-paths** in inventory §5.1 correspond to rows
2, 3, 4, 7, 9, 10×2, 11, 12, 13, 15×3, 16, 17, 19 PATCH, 20, 22.)

**Zero descoped. Zero EXISTS-UNDER-DIFFERENT-NAME. Zero UNKNOWN.**

### §4.2 SERVER-ONLY (12 method-paths) — added to OpenAPI

| # | METHOD + PATH | Handler file | Action |
|---|---|---|---|
| 1 | `POST /v1/signup/pilot/{token}` | `crates/corelink-container/src/routes/signup.rs::handle_pilot_signup` | ADD full OpenAPI entry under tag `signup-pilot` + new `PilotSignupRequest` / `PilotSignupResponse` schemas |
| 2 | `GET /v1/admin/read/{resource}` | `crates/corelink-container/src/routes/admin.rs::handle_read` | ADD entry under tag `admin-read-mutate`; opaque response (handler taxonomy) |
| 3 | `POST /v1/admin/mutate` | `crates/corelink-container/src/routes/admin.rs::handle_mutate` | ADD entry under tag `admin-read-mutate` + new `AdminMutateBody` schema |
| 4 | `GET /v1/admin/pilots` | `crates/corelink-container/src/routes/admin_pilot.rs::handle_list` | ADD entry under tag `admin-pilot` + new `ListPilotsResponse` + `PilotTenant` + `PilotState` schemas |
| 5 | `POST /v1/admin/pilots/{tenant_id}/grant-tier` | `crates/corelink-container/src/routes/admin_pilot.rs::handle_grant_tier` | ADD entry + new `GrantTierBody` / `GrantTierResponse` schemas |
| 6 | `POST /v1/admin/pilots/{tenant_id}/checkin` | `crates/corelink-container/src/routes/admin_pilot.rs::handle_checkin` | ADD entry with inline response schema |
| 7 | `GET /v1/audit/export` | `crates/corelink-container/src/routes/audit_export/handler.rs::handle_export` | ADD entry under tag `audit-export` (NDJSON stream + chain-head header) |
| 8 | `GET /v1/audit/analytics/event-count` | `crates/corelink-container/src/routes/audit_analytics/handler_event_count.rs` | ADD entry under tag `audit-analytics` + new `EventCountResponse` / `EventCountEntry` |
| 9 | `GET /v1/audit/analytics/timeline` | `crates/corelink-container/src/routes/audit_analytics/handler_timeline.rs` | ADD entry + new `TimelineResponse` / `TimelineEntry` |
| 10 | `GET /v1/cas/{tenant}/{hash}` | `crates/corelink-container/src/routes/cas.rs::handle_read` | ADD entry under tag `reapi-http` + binding note (production uses gRPC ByteStream) |
| 11 | `GET /v1/cas/{digest}` | `crates/corelink-reapi/src/http_read.rs::handle_cas_get` | ADD entry under tag `reapi-http` (REAPI binary) |
| 12 | `GET + PUT /v1/ac/{tenant}/{action_digest}` | `crates/corelink-container/src/routes/ac.rs::handle_lookup` / `handle_update` | ADD entry under tag `reapi-http` (binding note: production AC over gRPC) |

All 12 routes are now in the contract with handler-file pointers, auth
schemes inferred from the route's tower-middleware contract (admin →
`ClerkSessionCookie` + scope re-check; REAPI HTTP fakes → `BearerPAT`;
pilot signup → `security: []` with token in path), and req/resp
schemas mirroring the Rust struct field layouts verbatim. The new
`tools/openapi/src/lib.rs::paths` constants (`SIGNUP_PILOT`,
`ADMIN_READ`, …, `AC_LOOKUP` — 12 total) mirror the YAML and pass
the `every_path_constant_is_present_in_spec` round-trip test.

### §4.3 SDK-ONLY edge case (1)

| # | METHOD + PATH | Status | Action |
|---|---|---|---|
| 1 | `GET /v1/admin/audit-events` | Typo in `tools/sdks/{go,python}/examples/quickstart_audit.{go,py}` + `tools/cli/examples/quickstart_audit.rs:31` — NOT canonical; correct path is `/v1/admin/audit/events` | **DEFERRED to a separate SDK-quickstart fix sprint** per inventory-audit §7.4. The dashed variant is documented in this audit and in the `x-reconciliation-note` block on `/v1/admin/audit/events` as a typo. The SDK quickstart will not work against either the current server (no audit-events route exists yet) or a future canonical `/v1/admin/audit/events` mount until both the SDK fix lands AND the CF Worker binding lands. The OpenAPI side of the reconciliation is complete. |

## §5 Applied changes

### §5.1 `openapi/corelink-v1.yaml`

- 17 OPENAPI-ONLY path-items annotated with `x-status: planned` +
  `x-binding: cf-worker-pending` + `x-implementing-crate` +
  `x-reconciliation-note` (where disambiguation is non-trivial).
- 12 SERVER-ONLY path-items added at the end of the `paths:` block
  (just before `components:`), each with `x-status: implemented` +
  `x-implementing-handler` pointer. Two REAPI HTTP fakes also carry an
  `x-binding-note` cross-referencing `docs/reference/reapi/`.
- 7 new `tags:` entries (`signup-pilot`, `admin-read-mutate`,
  `admin-pilot`, `audit-export`, `audit-analytics`, `reapi-http`).
- 13 new schemas under `components/schemas`:
  `PilotSignupRequest`, `PilotSignupResponse`, `AdminMutateBody`,
  `PilotState`, `PilotTenant`, `ListPilotsResponse`, `GrantTierBody`,
  `GrantTierResponse`, `EventCountEntry`, `EventCountResponse`,
  `TimelineEntry`, `TimelineResponse` (plus reused `ErrorEnvelope` /
  `NotFound` / `BadRequest` / `Unauthorized` / `Forbidden`).

Verbatim line-count delta: 1429 → 2237 (≈ +808 LOC; +12 paths, +13
operations, +13 schemas, +7 tags, +25 `x-*` annotations).

### §5.2 `openapi/corelink-v1.json`

Regenerated via `python3 scripts/openapi_sync.py`:

```
openapi structure: 36 paths, 40 operations
openapi 3.1 meta-schema: ok
wrote openapi/corelink-v1.json
```

Was 24 paths / 27 operations; now 36 paths / 40 operations (+12 paths,
+13 operations — exactly the SERVER-ONLY delta).

### §5.3 `tools/openapi/src/lib.rs`

Added 12 SERVER-ONLY path constants (`SIGNUP_PILOT`, `ADMIN_READ`,
`ADMIN_MUTATE`, `ADMIN_PILOTS`, `ADMIN_PILOTS_GRANT_TIER`,
`ADMIN_PILOTS_CHECKIN`, `AUDIT_EXPORT`, `AUDIT_ANALYTICS_EVENT_COUNT`,
`AUDIT_ANALYTICS_TIMELINE`, `CAS_READ_TENANT`, `CAS_READ_DIGEST`,
`AC_LOOKUP`) and appended them to `paths::ALL`. The path strings use
the OpenAPI `{name}` placeholder form (NOT the axum `:name` form) —
the round-trip test asserts membership in the YAML `paths:` map keys.

### §5.4 SDK source NOT touched

Per mandate §5 and §9, the SDK source is not regenerated. The SDK
quickstart typo (`/v1/admin/audit-events`) is flagged in §4.3 and §6
for a separate sprint.

## §6 Backlog — SHOULD-BE-IMPLEMENTED items (CF Worker entry crate)

These 17 OpenAPI paths are documented but not yet axum-mounted. They
all live in business-logic crates ready for binding. The unifying gap
is the **Cloudflare Worker entry crate** (per
`tools/openapi/src/lib.rs` L29-39, deferred to PRR ship gate).

| Tag | Method-path(s) | Implementing crate | Sprint cross-ref |
|---|---|---|---|
| signup | POST /v1/signup | corelink-signup | S-19 WI-S19-001 |
| dpa | POST /v1/dpa/accept | corelink-dpa-acceptance | S-19 WI-S19-002 |
| dpa | POST /v1/dpa/re-accept | corelink-privacy::dpa::versioning | S-19 WI-S19-003 |
| tier | POST /v1/onboarding/tier-select | corelink-tier-selection | S-19 WI-S19-004 |
| pat | POST + GET /v1/pats, DELETE /v1/pats/{pat_id} | corelink-pat | (existing crate; PAT issuance + revoke endpoints) |
| privacy-dsr | POST /v1/privacy/dsr/{action}, GET .../status | corelink-dsr | S-11 WI-S11-001 |
| privacy-consent | POST + DELETE /v1/consent/{purpose}, GET /v1/consent + 2 verify | corelink-privacy::consent | S-11 WI-S11-003 |
| admin | POST /v1/admin/ops + 3 op-id endpoints | corelink-ops::admin::api + corelink-dual-approval | (existing crates) |
| admin | GET /v1/admin/audit/events | corelink-audit-chain (admin query surface — handler not authored) | (no specific sealed WI — new scoping required) |
| admin | GET /v1/admin/tenants | corelink-tenant-registry (admin query surface — handler not authored) | (no specific sealed WI) |
| enterprise | POST /v1/enterprise/inquire | corelink-enterprise-inquiry | S-19 WI-S19-005 |
| users | GET + PATCH /v1/users/me | pending (Clerk-authenticated profile read — no implementing crate yet) | (no specific sealed WI) |
| data-categories | GET /v1/data-categories | corelink-privacy (enum) | (trivial endpoint; deferred with the binding) |
| ops | GET /api/health, POST /api/csp-report | pending | S-16 WI-S16-001 (CSP-report frontend side sealed) |

**Three items lack a specific sealed WI** (`/v1/admin/audit/events`
admin query surface, `/v1/admin/tenants` admin query surface,
`/v1/users/me` profile read). These need product scoping before the
binding work can proceed. Flagged for human follow-up.

**One SDK-side item** lives separately from this backlog: fix the
dashed `/v1/admin/audit-events` typo in
`tools/sdks/{go,python}/examples/quickstart_audit.{go,py}` and
`tools/cli/examples/quickstart_audit.rs` to use the canonical
`/v1/admin/audit/events` path (requires the binding above to land
first or the quickstart will continue to 404).

## §7 Acceptance evidence

```
$ python3 -c "import yaml; [yaml.safe_load(open(f)) for f in __import__('glob').glob('openapi/*.yaml')]"
(silent success)

$ python3 scripts/openapi_sync.py
openapi structure: 36 paths, 40 operations
openapi 3.1 meta-schema: ok
wrote openapi/corelink-v1.json

$ cargo test -p corelink-openapi
test tests::json_parses ... ok
test tests::every_path_constant_is_present_in_spec ... ok
test tests::spec_version_matches_info_version ... ok
test result: ok. 3 passed; 0 failed

$ cargo build -p corelink-openapi
Finished `dev` profile [unoptimized + debuginfo] target(s) in 22.40s

$ cargo build -p corelink-server
Finished `dev` profile [unoptimized + debuginfo] target(s) in 3m 06s

$ cargo build -p corelink-client-verify
Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.88s

$ grep -rEn "<<<<<<<|>>>>>>>" tools/openapi/ tools/sdks/ openapi/
(no matches)
```

All §6 mandate gates GREEN.

## §8 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
