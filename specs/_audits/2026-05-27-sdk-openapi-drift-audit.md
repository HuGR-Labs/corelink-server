# SDK + OpenAPI drift audit — post Waves 33–36

**Date:** 2026-05-27
**Auditor:** SDK-DRIFT-AUDIT agent (autonomous, read-only)
**Scope:** `tools/sdks/{python,go}/`, `tools/openapi/`, sibling `openapi/corelink-v1.{yaml,json}`
**Mandate:** Surface drift between SDK + OpenAPI artifacts and current gRPC/REST handlers after the W33 Mod-Mono reorg (107 → 19 crates) and subsequent W34–W36 work. **No regeneration.** Catalogue only; recommend a regeneration sprint if drift is non-trivial.

---

## §1 — Files audited

| Path | LOC | Class |
|---|---:|---|
| `openapi/corelink-v1.yaml` | 1429 | Hand-authored OpenAPI 3.1 spec (source of truth per `tools/openapi/Cargo.toml`) |
| `openapi/corelink-v1.json` | 2673 | Generated sibling (round-trip checked by `tools/openapi`) |
| `tools/openapi/src/lib.rs` | 198 | Embedded spec + path constants (`Paths::*`) |
| `tools/openapi/Cargo.toml` | — | Crate manifest |
| `tools/sdks/python/src/lib.rs` | 326 | PyO3 extension (`corelink-py`) wrapping `corelink-client-verify` |
| `tools/sdks/python/corelink.pyi` | 122 | Type stubs |
| `tools/sdks/python/Cargo.toml` | — | Crate manifest |
| `tools/sdks/python/pyproject.toml` | — | maturin/PyPI metadata |
| `tools/sdks/python/build.rs` | — | Linker workaround |
| `tools/sdks/python/examples/*.py` | 10 files | REST quickstarts (no PyO3) |
| `tools/sdks/go/src/lib.rs` | 28 | cgo bridge facade re-export |
| `tools/sdks/go/src/go_bridge.rs` | 393 | C-ABI for Go SDK (`corelink-go`) wrapping `corelink-client-verify` |
| `tools/sdks/go/Cargo.toml` | — | Crate manifest |
| `tools/sdks/go/examples/*.go` | 10 files | REST quickstarts (no cgo) |

Total: **20 source + spec files + 20 example files** audited.

---

## §2 — Crate-path drift (absorbed crates from W33)

**Per §3 of mandate — search for absorbed-crate references:**

```
corelink_handler_cas | corelink_handler_ac | corelink_handler_admin
corelink_chunker | corelink_dedup | corelink_quota
corelink_byok_aws | corelink_byok_gcp | corelink_byok_azure
corelink_byok_vault | corelink_byok_core | corelink_byok_revocation
```

Recursive grep over `tools/sdks/` and `tools/openapi/`:

```
$ grep -rn "<patterns>" tools/sdks tools/openapi
(no matches)
```

**Result: ZERO stale absorbed-crate references in any SDK/OpenAPI source.**

The Python/Go SDKs ship as thin FFI wrappers around a single Rust dependency (`corelink-client-verify`, declared workspace-style in both `tools/sdks/{go,python}/Cargo.toml`). They never imported `corelink_handler_*`, `corelink_chunker`, `corelink_dedup`, `corelink_quota`, or any of the W33-absorbed `corelink_byok_*` family — they have always consumed only the consolidated verifier surface. Likewise the OpenAPI crate (`tools/openapi/`) only embeds the YAML/JSON and exports path constants; it does not depend on any handler crate.

**No regeneration is required from a W33-reorg perspective.** The SDK surface is structurally insulated from internal crate boundary churn.

---

## §3 — REST path drift (independent of W33 — pre-existing)

While there is no W33-reorg drift, the audit incidentally surfaces **substantial pre-existing drift between OpenAPI/SDK REST paths and what the current axum router actually serves.** This is unrelated to W33 absorption but is the higher-severity finding and should be captured.

### §3.1 — Three-way mismatch on the admin audit endpoint

| Artifact | Path |
|---|---|
| `openapi/corelink-v1.yaml` L624 | `/v1/admin/audit/events` |
| `tools/openapi/src/lib.rs` L93 (`ADMIN_AUDIT_EVENTS`) | `/v1/admin/audit/events` |
| `tools/sdks/go/examples/quickstart_audit.go` L32 | `/v1/admin/audit-events` |
| `tools/sdks/python/examples/quickstart_audit.py` L31 | `/v1/admin/audit-events` |
| `tools/cli/examples/quickstart_audit.rs` L31 | `/v1/admin/audit-events` |
| `apps/admin-ui/.../api-mocks.ts` L245, L270 | `/v1/admin/audit`, `/v1/admin/audit/<id>` |
| Actual axum routes (`grep -rE '"/v1/' crates/`) | **none of the above** — server exposes `/v1/audit/analytics/event-count`, `/v1/audit/analytics/timeline`, `/v1/audit/export` |

Four distinct conventions in flight (`/v1/admin/audit`, `/v1/admin/audit/events`, `/v1/admin/audit-events`, `/v1/audit/...`). The server-side router does **not** mount any of the four under `/v1/admin/audit*`. Clients calling the SDK quickstart will 404.

### §3.2 — OpenAPI paths with no corresponding handler

Cross-referencing `Paths::ALL` in `tools/openapi/src/lib.rs` vs `grep -rE '"/v1/' crates/`:

| OpenAPI / SDK path | Mounted in crates? |
|---|---|
| `/v1/signup` | NO (only `/v1/signup/pilot/:token` exists) |
| `/v1/dpa/accept` | NO |
| `/v1/dpa/re-accept` | YES (`corelink-privacy::dpa::versioning::middleware::RE_ACCEPT_PATH`) |
| `/v1/onboarding/tier-select` | NO |
| `/v1/pats` | NO |
| `/v1/pats/{pat_id}` | NO |
| `/v1/billing/stripe-webhook` | YES |
| `/v1/privacy/dsr/{action}` | NO |
| `/v1/privacy/dsr/{request_id}/status` | NO |
| `/v1/consent/{purpose}` | NO |
| `/v1/consent` | NO |
| `/v1/consent/verify` | YES (referenced via `consent::ledger`; mounted indirectly) |
| `/v1/consent/revocation/verify` | NO |
| `/v1/admin/ops` (+ `{op_id}`, `/approve`, `/reject`) | NO (server uses `/v1/admin/read/:resource`, `/v1/admin/mutate`) |
| `/v1/admin/audit/events` | NO (see §3.1) |
| `/v1/admin/tenants` | NO |
| `/v1/enterprise/inquire` | NO |
| `/v1/users/me` | NO |
| `/v1/data-categories` | NO |
| `/api/health` | NO (server exposes `/healthz` and CF Worker `/health`) |
| `/api/csp-report` | NO |

**Result: 17 of 21 OpenAPI Paths constants reference routes that do not exist in current axum routers.** Of the four that match, two (`stripe-webhook`, `dpa/re-accept`) match exactly and two (`consent/verify`, `signup/*`) match only partially.

### §3.3 — Server-side routes not represented in OpenAPI

Conversely, the server mounts several routes the OpenAPI spec does not document at all:

```
/v1/ac/:tenant/:action_digest
/v1/admin/mutate
/v1/admin/pilots[/:tenant_id/grant-tier|/checkin]
/v1/admin/read/:resource
/v1/audit/analytics/event-count
/v1/audit/analytics/timeline
/v1/audit/export
/v1/billing_portal/sessions
/v1/cas/:digest
/v1/cas/:tenant/:hash
/v1/cas/put
/v1/checkout/sessions
/v1/customers
/v1/evidence/access-reviews
/v1/evidence/audit-logs
/v1/evidence/change-management
/v1/evidence/credential-management
/v1/evidence/incident-response
/v1/evidence/vulnerability-management
/v1/objects/abc
/v1/signup/pilot/:token
/v1/subscriptions
```

None of these appear in the OpenAPI `paths:` block. The hand-authored YAML describes a customer/privacy/admin REST surface that has materially diverged from what the workspace actually serves.

---

## §4 — gRPC drift

The SDKs (`corelink-py`, `corelink-go`) do **not** expose gRPC at all today — both are pure FFI wrappers over `corelink-client-verify` (digest/verify utilities), plus REST-only examples that use `urllib`/`net/http` directly. There is therefore no gRPC stub drift to catalogue: nothing has been generated to drift from.

If a gRPC client surface is in scope for GA, that is a **new** sprint, not a regeneration.

---

## §5 — Type / schema drift

The Python `.pyi` stubs (`tools/sdks/python/corelink.pyi`) and the Rust extension module (`tools/sdks/python/src/lib.rs`) expose `Digest`, `ClientVerifier`, `VerifyConfig`, `VerifyError`. These names trace 1:1 to current symbols in `corelink-client-verify` (confirmed by grep of `use corelink_client_verify::{...}`). **No type-drift in the verify surface.**

The Go cgo bridge (`tools/sdks/go/src/go_bridge.rs`) re-exports `corelink_verifier_*` and adds `corelink_go_client_*` shims. All referenced `corelink-client-verify::ffi` symbols are present in current head. **No type-drift in the FFI surface.**

---

## §6 — Severity & recommendation

| Class | Severity | Notes |
|---|---|---|
| W33 absorbed-crate refs in SDK/OpenAPI | **NONE** | Audit objective satisfied — SDKs are insulated from internal reorg. |
| OpenAPI path drift vs actual axum routes | **HIGH** | 17/21 documented paths are 404; ~20 mounted routes undocumented; admin audit endpoint has four-way naming inconsistency. |
| SDK example path drift | **HIGH** | Go/Python/CLI quickstarts hit `/v1/admin/audit-events`, `/v1/users/me`, `/v1/pats`, etc. — none of which exist. These examples will fail in production. |
| FFI / type drift | **NONE** | `corelink-client-verify` surface is current. |

**Recommendation — DO NOT auto-regenerate.** Per §5 of the mandate, codegen is brittle in autonomous mode and the regeneration target itself (the hand-authored YAML) is the artifact furthest from reality. Auto-regeneration would propagate a stale spec.

**Recommend a dedicated regeneration / reconciliation sprint** with these phases:

1. **Spec ground-truthing** — enumerate actual axum routes via `inventory`/build-time route collection (already partially present per `apps/admin-ui/tests`), emit a machine-generated `routes.json` from the server binary at build time.
2. **Hand-authored YAML reconciliation** — rewrite `openapi/corelink-v1.yaml` against `routes.json`; delete the 17 ghost paths or implement the missing handlers (product decision required).
3. **SDK example regeneration** — once §6.2 is settled, regenerate `tools/sdks/{go,python}/examples/*` from the new spec.
4. **Path constant resync** — regenerate `tools/openapi/src/lib.rs` `Paths::*` from the new YAML; this crate already round-trips YAML↔JSON in tests, so a single `cargo test -p corelink-openapi` will gate the resync.
5. **Audit endpoint decision** — canonicalize one of `/v1/admin/audit`, `/v1/admin/audit/events`, `/v1/admin/audit-events`, `/v1/audit/export` and delete the rest from clients + admin-ui mocks.

Out of scope for this audit (per §5): touching `tools/openapi-internals`, modifying any SDK source, or running codegen.

---

## §7 — Files referenced

- `tools/openapi/src/lib.rs:59-105` — `Paths::*` constants
- `tools/openapi/Cargo.toml:1-15`
- `openapi/corelink-v1.yaml:64-820`
- `tools/sdks/python/src/lib.rs:1-50`
- `tools/sdks/python/corelink.pyi:1-122`
- `tools/sdks/go/src/lib.rs:1-28`
- `tools/sdks/go/src/go_bridge.rs:1-90`
- `tools/sdks/{go,python}/examples/quickstart_*.{go,py}` — REST examples (all 20)
- `tools/cli/examples/quickstart_audit.rs:1-40`
- `crates/corelink-container/src/routes.rs`, `routes/admin.rs`, `routes/admin_pilot.rs`, `routes/signup.rs`, `routes/audit_*.rs`
- `crates/corelink-privacy/src/dpa/versioning/middleware.rs:67`
- `crates/corelink-privacy/src/consent/ledger.rs:280,404`
- `apps/admin-ui/.../api-mocks.ts:245,270`, `apps/admin-ui/.../e2e-mock-fixtures.ts:352,368`

---

**End of audit.** No source modified. No codegen run. Report-only per mandate §4 + §5.
