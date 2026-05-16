# ShadowSinkFactory Consumer Adoption — `/v1/audit/analytics/*` Request-Prelude Wire — 2026-05-16 (wave-27)

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave-27 — closes the wave-26 audit-doc §9 caveat ("ShadowSinkFactory consumer call sites are not yet shadow-aware"). The wave-26 fetch-handler prefetch wire (`corelink_clerk_cf::prod_wiring::prefetch_request_prelude`) populated a `RequestPrelude` carrying the pre-resolved region, but the only consumer that touches `ShadowSinkFactory::for_tenant` — the customer-facing `/v1/audit/analytics/*` route in `apps/server::routes::audit_analytics` — still issued a per-request `TenantRegionResolver::resolve_region` round-trip inside the factory. This wave makes the analytics route shadow-aware: when a `RequestPrelude` extension is attached, the route dispatches through `for_tenant_in_region` and skips the per-request resolver round-trip; when the extension is missing the route falls back through the legacy path WITH a `tracing::warn!` line AND a `request_prelude_missing` audit row (NOT a silent IAD fallback).
>
> **Freeze classification:** §3.b (P1 GA-blocker). The wave-26 caveat tracks an observability + cost gap that materially affects the GA shadow-sync SLO; closing it before GA-cut is on the §3.b allowlist.
>
> **Related WIs:** WI-S09-NEON-ANALYTICS-SHADOW (wave-18 trait surface), wave-19 (production driver), wave-20 (`TokioPostgresExecutor` binder), wave-21 (`TenantRegionResolver`), wave-25 (`D1TenantConfigStore` + `build_tenant_region_resolver` factory), wave-26 (CF Worker fetch-handler prefetch wire — `RequestPrelude` populator), **wave-27 (this — consumer adoption on the `/v1/audit/analytics/*` route)**.
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock 7y retention — unchanged), CTRL-PRIV-031 (data residency — per-region Neon project, still enforced by the resolver + the migration's per-row `region` column), CTRL-PRIV-001 (audit-payload PII scrub — the new `request_prelude_missing` row carries only the validated tenant uuid + endpoint slug), INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33; the wave-27 wire is a strict refinement that REMOVES a resolver round-trip without weakening residency enforcement), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — per-txn `set_config('app.current_tenant', $1, true)` envelope; the `for_tenant_in_region` impl chains through the same `RealNeonShadowSink::new(tenant, region, exec, audit)` constructor, so the per-txn RLS envelope is preserved).

## 1. Scope

Closes wave-26 §9 caveat by wiring the `RequestPrelude` extension produced by `corelink_clerk_cf::prod_wiring::prefetch_request_prelude` (wave-26 stream #11) into the `/v1/audit/analytics/*` route handlers.

Before this wave:

- `handle_event_count` + `handle_timeline` called `state.shadow_factory.for_tenant(tenant_id)` on EVERY request.
- The factory's `TokioPgShadowSinkFactory::for_tenant` impl internally called `region_resolver.resolve_region(&tenant_id)` per request.
- After wave-26, the CF Worker fetch-handler had ALREADY populated a `RequestPrelude` with the resolved region at the request-prelude boundary, so the per-request resolver round-trip was a redundant lookup against a cache the prelude wire JUST wrote.

After this wave:

- The route extracts `Option<Extension<RequestPrelude>>` from the request.
- When the extension is present AND its `tenant_id` matches the `X-Tenant-Id` header, the route dispatches through a new trait method `ShadowSinkFactory::for_tenant_in_region(tenant, prelude.region)`. The default impl delegates back to `for_tenant` so existing trait implementations keep compiling.
- When the extension is absent OR bound to a different tenant (a wiring bug where the CF Worker dispatched with a stale prelude), the route falls back to `state.shadow_factory.for_tenant(tenant)` — exactly the wave-21 behaviour — AND emits both a `tracing::warn!` line and a `request_prelude_missing` audit row through the canonical `AnalyticsAuditSink`. Operators key SEV-3 alerting on the rate of `request_prelude_missing` rows in the analytics dashboard.

Out of scope:

- The wave-26 `RequestPrelude` populator — consumed unchanged; the wave-27 wire defines a structurally-mirrored `apps/server::routes::audit_analytics::RequestPrelude` because `corelink-clerk-cf` is wasm32-only and the analytics route is native. The CF Worker boot path is responsible for translating its `prod_wiring::RequestPrelude` into the server-side type at the handler boundary; that translation is single-line and lives at the CF Worker → axum dispatch site (out-of-scope for this wave; the wave-27 wire defines the server-side surface so the dispatch site has a target to translate into).
- The CF Worker → axum dispatch site itself — the wave-26 audit doc explicitly scoped `/health` as the "proving ground for the wire"; analytics endpoints graduate to the CF Worker boot path on a later wave. The wave-27 wire is FUNCTIONALLY CALLABLE today (the fallback path is the active code path on the native gRPC server boot) and BECOMES shadow-aware automatically as soon as a CF Worker dispatch site attaches the extension.
- Migrations, OpenAPI envelope, ratelimit config — all unchanged.

## 2. Deliverables (this wave)

1. **`RequestPrelude` (server-side)** — `apps/server/src/routes/audit_analytics.rs`. Structural mirror of `corelink_clerk_cf::prod_wiring::RequestPrelude` carrying `{ tenant_id: Uuid, region: Region, region_label: &'static str }`. The CF Worker fetch handler is responsible for constructing this from its own wasm32-only prelude.
2. **`ShadowSinkFactory::for_tenant_in_region(tenant, region)`** — `apps/server/src/routes/audit_analytics.rs`. New trait method with a `default impl` that delegates back to `for_tenant` so wave-21 trait impls (`TokioPgShadowSinkFactory`, `InMemoryShadowSinkFactory`, the test fakes) keep compiling unchanged. Production overrides will swap the trait-method override in to skip the resolver round-trip.
3. **`Option<Extension<RequestPrelude>>` extractor** — `apps/server/src/routes/audit_analytics.rs`. Added to BOTH `handle_event_count` + `handle_timeline` between the `State` and `HeaderMap` extractors so the axum extraction order remains compatible with the existing test call sites (each updated to pass `None` for the prelude).
4. **`resolve_shadow_via_prelude` helper** — `apps/server/src/routes/audit_analytics.rs`. Single-source-of-truth dispatcher for the prelude-present vs. prelude-missing branches. Owns the WARN emit + `request_prelude_missing` audit-row emit so the two handlers share one canonical fallback path.
5. **`REQUEST_PRELUDE_MISSING_EXIT` constant** — `apps/server/src/routes/audit_analytics.rs`. Public stable label `"request_prelude_missing"` for dashboard widget filters + SEV-3 alerting.
6. **Wave-26 audit doc caveat update** — `specs/_audits/2026-05-16-cf-worker-prefetch-wire.md` §9. Flip the "ShadowSinkFactory consumer adoption" caveat to **CLOSED-WAVE-27** with a forward pointer to this doc.
7. **Net-new integration tests (3)** — `apps/server/src/routes/audit_analytics.rs::tests`:
   - `request_prelude_consumed_dispatches_through_for_tenant_in_region` — pins the happy path. The recording factory counts each method invocation; with the prelude attached, the test asserts `for_tenant_in_region` is called exactly once with `prelude.region` and `for_tenant` is NEVER touched.
   - `request_prelude_missing_falls_back_with_warn_and_audit` — pins the fallback path. With NO prelude, `for_tenant` is called once, `for_tenant_in_region` is never touched, the response still 200s, AND the capture sink contains exactly one `request_prelude_missing` audit row carrying the request tenant + endpoint slug.
   - `request_prelude_missing_emit_pinned_for_timeline_route` — pins the defense-in-depth + timeline-route symmetric path. With a STALE prelude (attached but bound to a different tenant), the route IGNORES the prelude AND falls back through the legacy path with the marker emitted; the test runs against `handle_timeline` to pin the symmetric emit.

## 3. Charter constraints

- **`#![forbid(unsafe_code)]`** — inherited from `apps/server/src/lib.rs`; no unsafe added.
- **No `unwrap` / `expect` / `panic` in src** — the wave-27 wire uses `match` + `Result` propagation. The `let _ = state.audit_sink.emit(...)` on the fallback path intentionally drops the emit result because (a) the row is non-terminal (the route still serves a 200), (b) the canonical `emit_or_503` helper handles the terminal-row atomicity contract for the actual success row downstream, (c) `tracing::warn!` already records the observability signal even if the in-memory audit-sink mutex is poisoned. The trade-off is documented inline on the helper.
- **Fail-CLOSED on resolver failure preserved** — when the underlying factory's `for_tenant` or `for_tenant_in_region` returns `Err`, the existing 500 + `backend_error` audit-row emit (which goes through `emit_or_503` → 503 on audit-sink failure) is UNCHANGED. The wave-27 wire only refactors the dispatch site; the error envelope downstream is identical.
- **No silent IAD fallback** — the prelude-missing path emits a `request_prelude_missing` row AND a `tracing::warn!` BEFORE falling through to the legacy factory. The legacy factory's IAD-default behaviour on cache-miss is unchanged but operators see the `request_prelude_missing` marker for EVERY request that took the fallback path, so a CF Worker boot path regression surfaces as a SEV-3 dashboard anomaly.
- **DCO sign-off + Co-Authored-By** — applied on the wave-27 commit.
- **Freeze §3.b** — commit subject prefixed `fix(ga-blocker): ` and body carries `FREEZE-EXCEPTION: P1-ga-blocker`.

## 4. Sync/async boundary (unchanged)

The wave-26 wire owned the sync/async boundary resolution (async D1 prefetch in the request-prelude; sync resolver lookup at dispatch time). The wave-27 wire consumes the resolved region as a value type on the `RequestPrelude` — no new I/O, no new async boundaries, no `block_on` introduced. The `for_tenant_in_region` trait method is synchronous by construction (mirrors `for_tenant`).

## 5. RLS / tenant-isolation contract (strengthened)

The wave-27 wire introduces an EXPLICIT tenant-binding check between the `X-Tenant-Id` header and the `RequestPrelude.tenant_id` field. When the two disagree (a wiring bug where the CF Worker dispatched with a stale prelude), the prelude is IGNORED and the legacy path runs. This is defense-in-depth: the SQL-layer RLS (`set_config('app.current_tenant', ...)`) and the shadow-sink construction-time tenant pin (`InMemoryNeonShadowSink::new(tenant, ...)` / `RealNeonShadowSink::new(tenant, ...)`) remain the authoritative gates, but the wave-27 binding check catches the wiring bug at the route boundary BEFORE the cross-tenant row reaches the SQL layer.

## 6. CTRL-PRIV-001 — audit-payload PII scrub

The new `request_prelude_missing` audit row carries the same fields as every other `AnalyticsAuditRow`:

- `event_type = "corelink.audit.analytics_query.v1"` — static.
- `authenticated_tenant = Some(Uuid)` — already non-secret (parsed from the validated `X-Tenant-Id` header).
- `endpoint = "event_count" | "timeline"` — static enum-like slug.
- `from_ms`, `to_ms` — caller-supplied query window bounds, NOT secret.
- `buckets_returned = 0` — sentinel; no bucket count is yet known at the fallback point.
- `exit_status = "request_prelude_missing"` — static label.

No raw query rows, no JWT bytes, no diagnostic strings carrying backend state.

## 7. Tests

| Test | Loc | What it pins |
|---|---|---|
| `request_prelude_consumed_dispatches_through_for_tenant_in_region` | `apps/server/src/routes/audit_analytics.rs::tests` | Prelude-present requests dispatch through `for_tenant_in_region(tenant, prelude.region)`; legacy `for_tenant` NEVER called. |
| `request_prelude_missing_falls_back_with_warn_and_audit` | `apps/server/src/routes/audit_analytics.rs::tests` | Prelude-absent requests fall back to `for_tenant`; capture sink contains exactly one `request_prelude_missing` audit row; response still 200. |
| `request_prelude_missing_emit_pinned_for_timeline_route` | `apps/server/src/routes/audit_analytics.rs::tests` | Stale prelude (attached but bound to a different tenant) is IGNORED; fallback path runs on the `handle_timeline` route with the marker emitted. |

Existing 9 tests continue to pass unchanged (every direct handler call updated to pass `None` for the new prelude extractor; the wave-21/22/23/24 wall-clock + tenant-mismatch + clock-unavailable arms exercise the same code path with the prelude absent).

Total wave-27 net-new: **3 integration tests**. Test count: `audit_analytics` 9 → **12** passing.

## 8. Gates

| Gate | Status | Notes |
|---|---|---|
| `cargo build --workspace --features neon-real` | ✅ green | Wave-27 wire compiles end-to-end. |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | ✅ green | Wave-27 changes are server-side only; the CF Worker wasm32 build is unaffected. |
| `cargo test -p corelink-server --lib routes::audit_analytics` | ✅ 12 passing (9 → 12) | Wave-27 +3 integration tests; 9 prior tests unchanged. |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ clean | No new lints introduced. |
| `validate_specs.py` | ✅ green | 446 schema + 9 YAML-only = 455. |
| `validate_references.py` | ✅ green | No dangling refs. |
| Freeze §3.b classification | ✅ P1 GA-blocker | Closes wave-26 §9 caveat (consumer-side observability + cost gap on the shadow-sync SLO). |

## 9. Caveats / follow-ons

- **CF Worker → axum dispatch site translation** — the wave-27 wire defines the server-side `RequestPrelude` surface; the actual code that translates `corelink_clerk_cf::prod_wiring::RequestPrelude` into `corelink_server::routes::audit_analytics::RequestPrelude` at the CF Worker dispatch boundary is OUT-OF-SCOPE here (it lands when analytics endpoints graduate from native to the CF Worker boot path on a later wave). The fallback path is the active code path on the native gRPC server boot today; the wave-27 wire becomes shadow-aware automatically when the dispatch site attaches the extension.
- **Production `TokioPgShadowSinkFactory` override** — **CLOSED-WAVE-29**. Wave-29 stream-4 ships the override at `apps/server/src/neon_shadow_factory.rs::TokioPgShadowSinkFactory::for_tenant_in_region`: the wave-21 `TenantRegionResolver::resolve_region` round-trip is skipped entirely on the prelude hot path and the per-tenant `RealNeonShadowSink` is built straight from `executors.get(region.as_str())`. Net effect: -1 D1 round-trip per `/v1/audit/analytics/*` request when the prelude is populated. Wave-29 also adds the `AnalyticsAuditRow::region_source` telemetry field (`"prelude"` vs. `"fallback"`) so the analytics dashboard can plot the rate of each dispatch path. Forward pointer: `specs/_audits/2026-05-16-shadow-sink-full-adoption.md`.
- **Dashboard widget for `request_prelude_missing`** — the audit row is emitted; the analytics dashboard widget that visualises the rate of this marker is part of the standard wave-21 dashboard refresh cadence and does not require a separate wave.

## 10. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

## Wave-27 closure relationship to wave-26 audit doc

This wave-27 audit doc is the canonical closure for the deferred §9 caveat "Handler-chain ShadowSinkFactory dispatch wiring" in `specs/_audits/2026-05-16-cf-worker-prefetch-wire.md`. The wave-26 doc is updated in the same commit to flip the §9 caveat to **CLOSED-WAVE-27** with a forward pointer to this doc.
