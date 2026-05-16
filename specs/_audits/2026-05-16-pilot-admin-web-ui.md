# Wave-29 stream-3 — Pilot Admin Web UI

> **Doc kind:** wave-29 R-prep audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-29 stream-3 pilot-admin-web-ui agent (Claude Opus 4.7) — branch `wt/r-prep-pilot-admin-web-ui`.
> **Base:** `main` @ `365dd38`.
> **Scope:** replace the wave-27 placeholder shell scripts (`grant-pilot-tier.sh`, `list-pilot-tenants.sh`, `pilot-24h-checkin.sh`) with proper admin endpoints + a Docusaurus admin page that operators consume from a browser. Closes the wave-23 audit §8 "production wiring lands when the admin endpoints are formalized" expectation.
> **Cross-ref:** `specs/_audits/2026-05-16-pilot-onboarding-e2e.md` §8 (wave-23 journey + wave-27 placeholders), `specs/_audits/2026-05-16-pilot-signup-pipeline.md` (wave-27 placeholder contract), `apps/server/src/routes/admin.rs` (wave-15 admin handler discipline this module mirrors).

---

## 1. What this wave ships

Three admin HTTP routes + one Docusaurus admin page + ten integration tests:

| Surface | Path | Replaces (wave-27) |
|---|---|---|
| List pilots | `GET  /v1/admin/pilots?state=<STATE>` | `scripts/admin/list-pilot-tenants.sh` |
| Grant pilot tier | `POST /v1/admin/pilots/{tenant_id}/grant-tier` | `scripts/admin/grant-pilot-tier.sh` |
| 24h check-in | `POST /v1/admin/pilots/{tenant_id}/checkin` | `scripts/admin/pilot-24h-checkin.sh` |
| Operator web UI | `apps/docs/src/pages/admin/pilots.tsx` | (none — net new; the wave-27 scripts assumed operator-shell-only) |

All three routes are gated by:

1. **JWT scope claim** `corelink:admin:pilots` (delivered via the `X-Admin-Scope` header that the production tower middleware pins from the Clerk session).
2. **5-Layer Defense** — every route runs the L2 scope re-check + L3 tenant-scope gate + L4 input validation + L5 fail-CLOSED audit emit.
3. **Audit fail-CLOSED ordering** — every state mutation emits its `corelink.admin.pilot_*.v1` audit row BEFORE the mutation lands; if the sink returns `Err`, the route surfaces `503 Service Unavailable` and the store mutation does NOT apply.

## 2. Audit event surface

Three new event types, one per action + two security anchors:

- `corelink.admin.pilot_list.v1` — emitted on every successful list query (one row per request).
- `corelink.admin.pilot_tier_granted.v1` — emitted TWICE per grant-tier request: once at `attempt` BEFORE the store mutation, once at `ok` after the mutation commits. The attempt row is the SEC team's hard anchor for "operator tried to grant a tier" regardless of outcome.
- `corelink.admin.pilot_checkin.v1` — emitted on every check-in run; `exit_status` distinguishes `ok` (healthy tenant) from `alert` (24h overdue, no first blob).
- `corelink.security.admin_pilot_unauthorized.v1` — emitted BEFORE the 403 on any caller missing the required scope claim.
- `corelink.security.admin_pilot_cross_tenant.v1` — emitted BEFORE the 403 on any tenant-scoped admin attempting a cross-tenant operation.

The event-type strings + their payloads are pinned as `pub const` in `apps/server/src/routes/admin_pilot.rs` so the audit team's SIEM rules bind against the canonical literals.

## 3. 5-Layer Defense ladder

| Layer | Mechanism | Code location |
|---|---|---|
| L1 — JWT signature + expiry | Tower middleware (out of scope; production-wired upstream) | n/a |
| L2 — scope re-check | `require_admin_scope` re-parses `X-Admin-Scope`, rejects on absence of `corelink:admin:pilots` | `admin_pilot.rs::require_admin_scope` |
| L3 — tenant-scope gate | `PilotAdminScope::allows_tenant` rejects cross-tenant probes when the JWT carries an `X-Admin-Tenant` bound id | `admin_pilot.rs::PilotAdminScope` |
| L4 — input validation | `parse_tenant_uuid` (UUID v4/v7 lowercase hyphenated) + `PilotState::parse` (canonical uppercase) + body shape checks (`tier == "pilot"`, `cap_bytes > 0`) | `admin_pilot.rs::parse_tenant_uuid`, `PilotState::parse`, handler bodies |
| L5 — fail-CLOSED audit emit | Every non-happy-path path routes through `emit_or_503`; every mutation emits BEFORE the mutation lands | `admin_pilot.rs::emit_or_503`, `handle_grant_tier` |

## 4. Wave-27 placeholder script semantics — parity matrix

| Wave-27 script flag | Wave-29 route equivalent |
|---|---|
| `list-pilot-tenants.sh --state NEW \| ACTIVE \| GRADUATED \| TERMINATED` | `GET /v1/admin/pilots?state=<S>&offset=<N>&limit=<N>` (also supports `RESERVED` + `PROVISIONED` per the wave-27 schema extension) |
| `list-pilot-tenants.sh --format {table,json}` | Response is JSON; the web UI renders the table client-side |
| `grant-pilot-tier.sh --tenant-id <uuid> --tier pilot --cap 100GB` | `POST /v1/admin/pilots/<uuid>/grant-tier` with body `{"tier": "pilot", "cap_bytes": 100000000000}` |
| `grant-pilot-tier.sh --dry-run` | (deferred — Phase 2 will accept `?dry_run=true` if operator demand surfaces; the web UI replaces this with a confirmation modal) |
| `pilot-24h-checkin.sh --tenant-id <uuid>` | `POST /v1/admin/pilots/<uuid>/checkin` |
| `pilot-24h-checkin.sh --all-active` | Operator iterates from the list endpoint (the web UI auto-polls every 30s and highlights overdue rows in red — no per-tenant manual fan-out needed) |

The wave-27 scripts remain in-tree as the operator escape hatch for the period before the production tower middleware lands. Once the Clerk-issued admin JWT is wired end-to-end (Phase 2), the scripts become deprecated and migrate to `scripts/admin/legacy/`.

## 5. Test surface

`apps/server/tests/admin_pilot.rs` — 12 async integration tests covering:

1. List happy path (rows + ordering + audit emit).
2. Grant-tier happy path (NEW → ACTIVE + dual audit rows + store mutation).
3. Check-in happy path (alert emitted for overdue ACTIVE tenant with no first blob).
4. Non-admin JWT rejected with audit BEFORE 403.
5. Missing principal rejected with audit.
6. Cross-tenant grant-tier rejected with audit BEFORE 403 + no store mutation.
7. Same-tenant admin grant-tier succeeds (control case).
8. Audit fail-CLOSED on grant-tier → 503 + no store mutation.
9. Audit fail-CLOSED on list → 503.
10. Invalid state filter → 400.
11. Invalid tenant UUID → 400.
12. Double grant-tier (already ACTIVE) → 409 conflict.

Plus 9 unit tests in `apps/server/src/routes/admin_pilot.rs::tests` covering the store, audit sink, scope guard, state round-trip, and route constants.

## 6. Charter compliance

- `#![forbid(unsafe_code)]` (inherited crate-wide via `apps/server/src/lib.rs`).
- No `unwrap()` / `expect()` / `panic!()` on the request path — all `Result`s map to a typed error → HTTP response.
- DCO sign-off + Co-Authored-By land at commit time.
- Synchronous bash only — no background scripts spawned by the routes.

## 7. Migrations

No database migration required. The existing `tenants` schema (wave-27 §6) already carries `pilot_state`, `tier`, `cap_bytes`, `signup_at_ms`, `tier_granted_at_ms`, `first_blob_at_ms` columns. The wave-29 routes consume the same columns through the `PilotStore` trait; production wiring binds `Arc<dyn PilotStore>` against the D1 driver, tests bind the in-memory fake.

The audit events flow through the existing audit-chain pipeline (wave-15) via a new `Arc<dyn PilotAuditSink>` trait — production wiring connects this to the audit-chain producer, tests bind `InMemoryPilotAuditSink`. No new audit table required: the events ride the canonical `audit_chain` ledger.

## 8. Open questions / follow-ups

- **Phase 2 swap** — `apps/docs/src/pages/admin/pilots.tsx` currently pins admin headers from env-derived defaults. Swap to `@clerk/clerk-react`'s `useUser()` + `getToken()` once the Clerk admin instance is provisioned (out-of-scope for this wave — operator-bound provisioning).
- **D1 binding** — `InMemoryPilotStore` is the dev/CI default. Production wiring needs a `D1PilotStore` adapter that issues the canonical SQL emitted by the wave-27 placeholder scripts (§4 of `2026-05-16-pilot-signup-pipeline.md`).
- **Slack alerter** — the 24h check-in surfaces `alert_emitted: bool` in the response. The Slack POST step from `pilot-24h-checkin.sh` is NOT yet wired in this route; operators currently consume the alert via the web UI's red-row highlight. Phase 2 lands a `SlackAlerter` trait + cron schedule (`apps/server/cron/pilot_checkin.rs`).
- **Pagination cursor** — the list route accepts `offset` + `limit` (capped at 200). Cursor-based pagination is deferred; the customer-success cohort never exceeds 200 simultaneous pilots so offset-pagination is structurally adequate.

## 9. Freeze posture

Per the wave-29 freeze list, this change lands in §3.b (GA-blocker prep). It does not touch the customer-data plane (`apps/server/src/routes/cas.rs`, `ac.rs`) and does not modify the audit-chain wire format. The change is observable end-to-end: every audit row emitted carries a canonical event type the SEC team can grep for.
