---
id: "AUDIT-2026-05-27-OPENAPI-ROUTE-INVENTORY"
type: "audit"
doc_status: "ACTIVE"
audit_status: "SEALED"
version: "1.0.0"
created: "2026-05-27"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
tags: ["audit", "openapi", "axum-routes", "drift-inventory", "post-w36"]
references:
  - "specs/_audits/2026-05-27-sdk-openapi-drift-audit.md"
---

# OpenAPI ↔ axum routes inventory SEAL

## §1 Scope

Complete tabulation of HTTP routes exposed by the workspace's axum servers, vs the canonical OpenAPI spec (`openapi/corelink-v1.yaml` + `tools/openapi::paths`), vs the client paths used by the Go/Python SDK quickstart examples. READ-ONLY pass — no source modified, no codegen executed.

Inputs:

- `git rev-parse --abbrev-ref HEAD` → `worktree-agent-a017c98531cb42a4a` (clean tree, no untracked).
- Server route discovery: `grep -rEn '\.route\(' --include="*.rs" crates/ apps/` (axum 0.7 Router-builder pattern).
- OpenAPI: `openapi/corelink-v1.yaml` (1429 LOC, hand-authored 3.1) + `tools/openapi/src/lib.rs::paths::ALL` (24 constants).
- SDK paths: REST quickstarts under `tools/sdks/{python,go}/examples/` (URL literals extracted via grep).

This document follows up on, and does NOT supersede, the SDK-DRIFT report at `specs/_audits/2026-05-27-sdk-openapi-drift-audit.md`. It refines that report's §3.3 list by separating *inbound axum routes* (true server surface) from *outbound HTTP client paths* (calls into Stripe, Drata, the REAPI peer), which the prior audit mixed together.

## §2 Server routes (axum)

The workspace runs **seven distinct axum routers**, four of which are package-registry adapters mounted on dedicated ports (not part of the customer REST surface):

| # | METHOD | PATH | HANDLER (function) | CRATE | FILE:LINE |
|---|---|---|---|---|---|
| **Customer REST surface — `corelink-container::routes::build()`** ||||||
| 1 | GET | `/v1/cas/:tenant/:hash` | `handle_read` | corelink-container | `src/routes/cas.rs:105` |
| 2 | GET / PUT | `/v1/ac/:tenant/:action_digest` | `handle_lookup` / `handle_update` | corelink-container | `src/routes/ac.rs:116` |
| 3 | GET | `/v1/admin/read/:resource` | `handle_read` | corelink-container | `src/routes/admin.rs:109` |
| 4 | POST | `/v1/admin/mutate` | `handle_mutate` | corelink-container | `src/routes/admin.rs:110` |
| 5 | GET | `/v1/admin/pilots` | `handle_list` | corelink-container | `src/routes/admin_pilot.rs:571` |
| 6 | POST | `/v1/admin/pilots/:tenant_id/grant-tier` | `handle_grant_tier` | corelink-container | `src/routes/admin_pilot.rs:572` |
| 7 | POST | `/v1/admin/pilots/:tenant_id/checkin` | `handle_checkin` | corelink-container | `src/routes/admin_pilot.rs:573` |
| 8 | GET | `/v1/audit/export` | `handle_export` | corelink-container | `src/routes/audit_export/state.rs:146` |
| 9 | GET | `/v1/audit/analytics/event-count` | `handle_event_count` | corelink-container | `src/routes/audit_analytics/state.rs:76` |
| 10 | GET | `/v1/audit/analytics/timeline` | `handle_timeline` | corelink-container | `src/routes/audit_analytics/state.rs:77` |
| 11 | POST | `/v1/signup/pilot/:token` | `handle_pilot_signup` | corelink-container | `src/routes/signup.rs:712` |
| **Webhook surface — `corelink-container::webhook::router()`** ||||||
| 12 | POST | `/v1/billing/stripe-webhook` | `stripe_webhook_handler` | corelink-container | `src/webhook.rs:88` |
| **Read-API surface — `corelink-reapi::http_read::router()`** ||||||
| 13 | GET | `/v1/cas/:digest` | `handle_cas_get` | corelink-reapi | `src/http_read.rs:165` |
| **Package-registry adapters (separate listeners, non-REST surface)** ||||||
| 14 | GET / PUT / HEAD | `/:key` (cargo registry) | `handle_get`/`handle_put`/`handle_head` | corelink-adapter-host | `src/cargo/server.rs:80` |
| 15 | GET | `/` (homebrew bottles) | `handle_bottle_request` | corelink-adapter-host | `src/brew/server.rs:78` |
| 16 | GET | `/*path` (homebrew bottles) | `handle_bottle_request` | corelink-adapter-host | `src/brew/server.rs:79` |
| 17 | GET | `/v2/` (OCI) | `api_version` | corelink-adapter-host | `src/oci/server.rs:39` |
| 18 | GET | `/v2` (OCI) | `api_version` | corelink-adapter-host | `src/oci/server.rs:40` |
| 19 | GET | `/v2/_catalog` (OCI) | `catalog` | corelink-adapter-host | `src/oci/server.rs:41` |
| 20 | GET | `/token` (OCI auth) | `token` | corelink-adapter-host | `src/oci/server.rs:42` |
| 21 | ANY | `/v2/*rest` (OCI dispatch) | `dispatch_v2` | corelink-adapter-host | `src/oci/server.rs:43` |
| 22 | GET | `/-/ping` (npm) | `handle_ping` | corelink-adapter-host | `src/npm/server.rs:66` |
| 23 | GET | `/-/v1/search` (npm) | `handle_search_not_implemented` | corelink-adapter-host | `src/npm/server.rs:67` |
| 24 | GET | `/:pkg` (npm metadata) | `handle_metadata` | corelink-adapter-host | `src/npm/server.rs:68` |
| 25 | GET | `/:pkg/-/:tarball` (npm tarball) | `handle_tarball` | corelink-adapter-host | `src/npm/server.rs:71` |
| 26 | GET | `/simple/:project/` (pip) | `handle_index` | corelink-adapter-host | `src/pip/server.rs:48` |
| 27 | GET | `/pkg/:sha256/:filename` (pip wheel) | `handle_wheel` | corelink-adapter-host | `src/pip/server.rs:49` |
| 28 | GET | `/healthz` (pip adapter liveness) | `handle_health` | corelink-adapter-host | `src/pip/server.rs:50` |

**Customer REST surface total: 11 unique paths / 13 method-path tuples** (rows 1–11; AC counts twice for GET+PUT, plus row 12 stripe webhook = 13 method-paths; row 13 is the parallel read-api binary, not the container).

**Adapter surface total: 15 routes across cargo / brew / OCI / npm / pip** (rows 14–28; live on dedicated registry ports and intentionally outside the OpenAPI contract since they implement third-party package-manager protocols).

## §3 OpenAPI paths

Source: `openapi/corelink-v1.yaml` `paths:` block. Constants mirrored in `tools/openapi/src/lib.rs::paths::ALL`.

| METHOD | PATH | operationId | YAML LINE |
|---|---|---|---|
| POST | `/v1/signup` | `signup` | 3 |
| POST | `/v1/dpa/accept` | `dpaAccept` | 56 |
| POST | `/v1/dpa/re-accept` | `dpaReAccept` | 89 |
| POST | `/v1/onboarding/tier-select` | `tierSelect` | 121 |
| POST | `/v1/pats` | `patIssue` | 156 |
| GET | `/v1/pats` | `patList` | 181 |
| DELETE | `/v1/pats/{pat_id}` | `patRevoke` | 197 |
| POST | `/v1/billing/stripe-webhook` | `stripeWebhook` | 211 |
| POST | `/v1/privacy/dsr/{action}` | `dsrSubmit` | 260 |
| GET | `/v1/privacy/dsr/{request_id}/status` | `dsrStatus` | 310 |
| POST | `/v1/consent/{purpose}` | `consentGrant` | 333 |
| DELETE | `/v1/consent/{purpose}` | `consentRevoke` | 360 |
| GET | `/v1/consent` | `consentHistory` | 387 |
| GET | `/v1/consent/verify` | `consentVerifyGrant` | 401 |
| GET | `/v1/consent/revocation/verify` | `consentVerifyRevocation` | 433 |
| POST | `/v1/admin/ops` | `adminOpsSubmit` | 455 |
| GET | `/v1/admin/ops/{op_id}` | `adminOpGet` | 494 |
| POST | `/v1/admin/ops/{op_id}/approve` | `adminOpApprove` | 513 |
| POST | `/v1/admin/ops/{op_id}/reject` | `adminOpReject` | 538 |
| GET | `/v1/admin/audit/events` | `adminAuditEvents` | 563 |
| GET | `/v1/admin/tenants` | `adminTenantsList` | 595 |
| POST | `/v1/enterprise/inquire` | `enterpriseInquire` | 626 |
| GET | `/v1/users/me` | `usersMeGet` | 651 |
| PATCH | `/v1/users/me` | `usersMePatch` | 664 |
| GET | `/v1/data-categories` | `dataCategoriesList` | 683 |
| GET | `/api/health` | `apiHealth` | 698 |
| POST | `/api/csp-report` | `cspReport` | 711 |

**Total: 27 method-path operations across 24 unique paths.** Matches `paths::ALL.len() == 24` in `tools/openapi/src/lib.rs:109`.

## §4 SDK client methods

Both SDKs (`tools/sdks/python`, `tools/sdks/go`) ship as thin FFI wrappers around `corelink-client-verify`. The PyO3 / cgo bridges expose Digest/Verifier types ONLY — there is no REST-client code generated. The "SDK paths" actually exercised live in the `examples/quickstart_*.{py,go}` files, which build URLs by hand with `urllib` / `net/http`. Both example sets are mirror images of each other (same 10 verbs):

| Example | METHOD | PATH | py FILE:LINE | go FILE:LINE |
|---|---|---|---|---|
| signup | POST | `/v1/signup` | `quickstart_signup.py:41` | `quickstart_signup.go:54` |
| put (PAT issue) | POST | `/v1/pats` | `quickstart_put.py:31` | `quickstart_put.go:51` |
| list (PAT list) | GET | `/v1/pats` | `quickstart_list.py:28` | `quickstart_list.go:31` |
| get (user profile) | GET | `/v1/users/me` | `quickstart_get.py:28` | `quickstart_get.go:31` |
| dsr_submit | POST | `/v1/privacy/dsr/{action}` | `quickstart_dsr_submit.py:38` | `quickstart_dsr_submit.go:57` |
| team_invite | POST | `/v1/admin/ops` | `quickstart_team_invite.py:39` | `quickstart_team_invite.go:57` |
| byok_rotate | POST | `/v1/admin/ops` | `quickstart_byok_rotate.py:38` | `quickstart_byok_rotate.go:55` |
| portal_session | POST | `/v1/enterprise/inquire` | `quickstart_portal_session.py:40` | `quickstart_portal_session.go:54` |
| audit (stream) | GET | `/v1/admin/audit-events` | `quickstart_audit.py:31` | `quickstart_audit.go:32` |
| stats (health) | GET | `/api/health` | `quickstart_stats.py:22` | `quickstart_stats.go:26` |

CLI quickstart at `tools/cli/examples/quickstart_audit.rs:31` also points at `/v1/admin/audit-events` (the dashed variant).

## §5 Classification

The customer REST surface is the meaningful comparison set. Adapter routes (rows 14–28 of §2) are excluded — they implement cargo / brew / OCI / npm / pip wire protocols, are intentionally undocumented in the customer-facing OpenAPI, and are out of scope for this drift assessment.

| Route | Server | OpenAPI | SDK example | Classification |
|---|:-:|:-:|:-:|---|
| POST `/v1/signup` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| POST `/v1/signup/pilot/:token` | YES | NO | NO | **SERVER-ONLY** |
| POST `/v1/dpa/accept` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/dpa/re-accept` | NO¹ | YES | NO | **OPENAPI-ONLY** (¹ referenced by middleware as a gate token, never `.route()`-mounted) |
| POST `/v1/onboarding/tier-select` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/pats` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| GET `/v1/pats` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| DELETE `/v1/pats/{pat_id}` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/billing/stripe-webhook` | YES | YES | NO | **MATCH (server+spec; not in SDK quickstarts — webhook ingress)** |
| POST `/v1/privacy/dsr/{action}` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| GET `/v1/privacy/dsr/{request_id}/status` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/consent/{purpose}` | NO² | YES | NO | **OPENAPI-ONLY** (² consent data layer present in `corelink-privacy::consent::ledger`; no axum mount) |
| DELETE `/v1/consent/{purpose}` | NO² | YES | NO | **OPENAPI-ONLY** |
| GET `/v1/consent` | NO² | YES | NO | **OPENAPI-ONLY** |
| GET `/v1/consent/verify` | NO² | YES | NO | **OPENAPI-ONLY** |
| GET `/v1/consent/revocation/verify` | NO² | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/admin/ops` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| GET `/v1/admin/ops/{op_id}` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/admin/ops/{op_id}/approve` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/admin/ops/{op_id}/reject` | NO | YES | NO | **OPENAPI-ONLY** |
| GET `/v1/admin/audit/events` | NO | YES | NO | **OPENAPI-ONLY** (SDK uses `/v1/admin/audit-events` — see §6) |
| GET `/v1/admin/audit-events` | NO | NO | YES (py+go+cli) | **SDK-ONLY (broken)** |
| GET `/v1/admin/tenants` | NO | YES | NO | **OPENAPI-ONLY** |
| POST `/v1/enterprise/inquire` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| GET `/v1/users/me` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| PATCH `/v1/users/me` | NO | YES | NO | **OPENAPI-ONLY** |
| GET `/v1/data-categories` | NO | YES | NO | **OPENAPI-ONLY** |
| GET `/api/health` | NO | YES | YES (py+go) | **OPENAPI+SDK only (server 404)** |
| POST `/api/csp-report` | NO | YES | NO | **OPENAPI-ONLY** |
| GET `/v1/cas/:tenant/:hash` | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/cas/:digest` (reapi binary) | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/ac/:tenant/:action_digest` | YES | NO | NO | **SERVER-ONLY** |
| PUT `/v1/ac/:tenant/:action_digest` | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/admin/read/:resource` | YES | NO | NO | **SERVER-ONLY** |
| POST `/v1/admin/mutate` | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/admin/pilots` | YES | NO | NO | **SERVER-ONLY** |
| POST `/v1/admin/pilots/:tenant_id/grant-tier` | YES | NO | NO | **SERVER-ONLY** |
| POST `/v1/admin/pilots/:tenant_id/checkin` | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/audit/export` | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/audit/analytics/event-count` | YES | NO | NO | **SERVER-ONLY** |
| GET `/v1/audit/analytics/timeline` | YES | NO | NO | **SERVER-ONLY** |

### §5.1 Counts

- **MATCH** (server ↔ OpenAPI, regardless of SDK): **1** — `POST /v1/billing/stripe-webhook` (Stripe webhook). This is the *only* fully-aligned customer REST route between server and OpenAPI in the workspace.
- **SERVER-ONLY** (axum route with no OpenAPI doc, no SDK call): **12** — rows 1–11 of §2 minus the one MATCH, plus `/v1/cas/:digest` (reapi binary), plus the two AC method splits.
- **OPENAPI-ONLY** (documented but no axum mount and no SDK quickstart): **17** distinct method-paths (`/v1/dpa/accept`, `/v1/dpa/re-accept`, `/v1/onboarding/tier-select`, `DELETE /v1/pats/{pat_id}`, `GET /v1/privacy/dsr/{request_id}/status`, `POST` + `DELETE /v1/consent/{purpose}`, `GET /v1/consent`, `GET /v1/consent/verify`, `GET /v1/consent/revocation/verify`, `GET /v1/admin/ops/{op_id}`, `POST /v1/admin/ops/{op_id}/approve`, `POST /v1/admin/ops/{op_id}/reject`, `GET /v1/admin/audit/events`, `GET /v1/admin/tenants`, `PATCH /v1/users/me`, `GET /v1/data-categories`, `POST /api/csp-report`).
- **OPENAPI + SDK, no server** (broken quickstarts): **8** — `POST /v1/signup`, `POST /v1/pats`, `GET /v1/pats`, `POST /v1/privacy/dsr/{action}`, `POST /v1/admin/ops`, `POST /v1/enterprise/inquire`, `GET /v1/users/me`, `GET /api/health`.
- **SDK-ONLY** (no server, not even in OpenAPI): **1** — `GET /v1/admin/audit-events` (the dashed naming variant the audit example, the team-invite example etc. *do not* hit; only `quickstart_audit.{py,go}` and `quickstart_audit.rs` hit this).

Sum-check: 1 MATCH + 12 SERVER-ONLY + 17 OPENAPI-ONLY + 8 (OPENAPI+SDK no server) + 1 SDK-ONLY = **39 distinct method-path tuples** across all three artifacts. (OpenAPI contributes 27, server REST surface contributes 13, SDK quickstarts contribute 10 — union with overlaps deduped lands at 39.)

## §6 Audit-endpoint naming split detail

Per §3.1 of the SDK-DRIFT audit, the workspace ships **four distinct conventions** for "audit events" endpoint, none of which align across server / OpenAPI / SDK:

| Variant | Appears in | Status |
|---|---|---|
| `/v1/admin/audit` | `apps/admin-ui/.../api-mocks.ts:245` | UI mock fixture only — no server, no OpenAPI, no SDK |
| `/v1/admin/audit/events` | `openapi/corelink-v1.yaml:562` + `tools/openapi/src/lib.rs:93` (`ADMIN_AUDIT_EVENTS`) | OpenAPI canonical — 404 in server, never called by SDK |
| `/v1/admin/audit-events` | `tools/sdks/{python,go}/examples/quickstart_audit.{py,go}` + `tools/cli/examples/quickstart_audit.rs` | SDK + CLI canonical — 404 in server, undocumented in OpenAPI |
| `/v1/audit/export`, `/v1/audit/analytics/event-count`, `/v1/audit/analytics/timeline` | `crates/corelink-container/src/routes/audit_export/types.rs:14`, `audit_analytics/types.rs:15,18` | Server canonical — three orthogonal routes (export bytes / event counts / timeline); none aliased to `/v1/admin/audit*`. Not in OpenAPI, not in SDK |

The UI mocks add a fifth variant (`/v1/admin/audit/<id>` at `api-mocks.ts:270`) but it is an admin-UI internal contract not exposed to clients.

**Server does NOT mount any path under `/v1/admin/audit*` or `/v1/admin/audit-events`.** All audit functionality lives under the top-level `/v1/audit/*` namespace (`audit_export`, `audit_analytics`).

## §7 Recommended reconciliation sprint scope

The drift is structural, not cosmetic: the hand-authored OpenAPI describes a customer/privacy/admin product surface (signup, PATs, DSR, consent ledger, dual-approval admin ops, audit events) that has **never been mounted as axum routes** in the container crate, while the surface that *is* mounted (CAS read, AC lookup/update, admin read/mutate, pilot lifecycle, audit export+analytics, Stripe webhook) is documented nowhere in the spec. Of 27 OpenAPI method-paths, only the Stripe webhook actually serves traffic. A reconciliation sprint should therefore:

1. **Decide the customer surface first** — for each OPENAPI-ONLY row, mark `implement` (mount handler, keep spec) or `delete from spec` (product hasn't shipped this; the YAML is aspirational). Without this product call, no codegen pass can succeed.
2. **Annotate SERVER-ONLY rows** — author OpenAPI entries for the 12 routes the container actually serves (admin/read, admin/mutate, admin/pilots/*, audit/export, audit/analytics/*, signup/pilot/:token, cas, ac); fold the reapi-binary `/v1/cas/:digest` into the same spec or document it separately.
3. **Canonicalize the audit-events naming** — pick one of the four variants in §6 (likely `/v1/admin/audit/events` to match OpenAPI, with server adding routes that proxy/rename to the existing audit_export+analytics handlers). Update SDK + CLI + admin-ui mocks in the same PR.
4. **Repoint the 8 OPENAPI+SDK broken quickstarts** — once §7.1 lands, regenerate `tools/sdks/{go,python}/examples/*` so signup / pats / dsr / users-me / enterprise-inquire / admin-ops / api-health resolve correctly.
5. **Re-emit `tools/openapi::paths::ALL`** mechanically from the reconciled YAML; the `spec_roundtrip` test (`tools/openapi/src/lib.rs:174–186`) gates this for free.
6. **Defer adapter-host routes** (cargo / brew / OCI / npm / pip — rows 14–28 of §2) — these implement third-party wire protocols and should remain outside the customer OpenAPI document.

Sprint sizing: 4 of the 5 phases above are mechanical once the §7.1 product decision is in. The §7.1 decision is the only non-engineering input the reconciliation needs and is the gating item for everything downstream.

## §8 DCO sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.
