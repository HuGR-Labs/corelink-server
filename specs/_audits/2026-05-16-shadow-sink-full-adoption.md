# ShadowSinkFactory Full Adoption — `TokioPgShadowSinkFactory::for_tenant_in_region` Override — 2026-05-16 (wave-29)

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave-29 stream-4 — closes the wave-27 §9 caveat #2 ("Production `TokioPgShadowSinkFactory` override"). Wave-27 wired the `RequestPrelude` extension into the `/v1/audit/analytics/*` route handlers and added the `ShadowSinkFactory::for_tenant_in_region` trait method whose default impl delegates back to `for_tenant`. The wave-27 caveat: the production `TokioPgShadowSinkFactory` (nested inside `apps/server/src/main.rs`) kept that default delegate, so the prelude-attached hot path STILL paid a `TenantRegionResolver::resolve_region` round-trip per request. Wave-29 ships the override + adds the `region_source` audit telemetry field so the dashboard can plot the rate of each dispatch path.
>
> **Freeze classification:** §3.b (P1 GA-blocker prep). Closes the wave-27 caveat on consumer-side cost + observability. The override eliminates one D1 round-trip per `/v1/audit/analytics/*` request when the wave-26 prelude is populated and surfaces the dispatch path as a per-request telemetry field.
>
> **Related WIs:** WI-S09-NEON-ANALYTICS-SHADOW (wave-18 trait surface), wave-19 (production driver), wave-20 (`TokioPostgresExecutor` binder), wave-21 (`TenantRegionResolver`), wave-25 (`D1TenantConfigStore` + `build_tenant_region_resolver` factory), wave-26 (CF Worker fetch-handler prefetch wire — `RequestPrelude` populator), wave-27 (consumer adoption — server-side prelude trait surface + route extractor), **wave-29 stream-4 (this — production factory override + telemetry)**.
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock 7y retention — unchanged), CTRL-PRIV-031 (data residency — per-region Neon project, still enforced by construction-time region pin on `RealNeonShadowSink::new`), CTRL-PRIV-001 (audit-payload PII scrub — the new `region_source` field carries a 2-value enum-like string with no tenant data), INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33; the wave-29 override is a strict refinement that REMOVES a redundant resolver round-trip without weakening residency enforcement — the prelude region IS the residency-pinned region resolved at the CF Worker request-prelude boundary), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — per-txn `set_config('app.current_tenant', $1, true)` envelope; the `for_tenant_in_region` impl chains through the same `RealNeonShadowSink::new(tenant, region, exec, audit)` constructor, so the per-txn RLS envelope is preserved).

## 1. Scope

Closes wave-27 §9 caveat #2 by overriding `ShadowSinkFactory::for_tenant_in_region` on the production `TokioPgShadowSinkFactory` so the per-request `TenantRegionResolver::resolve_region` round-trip is eliminated on the wave-26 prelude hot path. Also adds a `region_source: Option<String>` field on `AnalyticsAuditRow` so the canonical `corelink.audit.analytics_query.v1` happy-path emit carries `"prelude"` vs `"fallback"` telemetry — closing the dashboard-visibility loop on the wave-26 / wave-27 / wave-29 progression.

Before this wave:

- The wave-27 `for_tenant_in_region` trait method had a default impl that delegated back to `for_tenant`. The production factory kept the default delegate.
- Net effect: a prelude-attached `/v1/audit/analytics/*` request paid one redundant `TenantRegionResolver::resolve_region` round-trip (a D1 `SELECT region FROM tenant_config WHERE tenant_id = ?` on the production CF Worker boot path) AFTER the CF Worker request-prelude prefetch had already resolved the region.
- The success emit row did NOT distinguish prelude-vs-fallback dispatch, so the dashboard widget on the consumer-adoption rate (a key SEV-3 health metric) had no signal to plot.

After this wave:

- `TokioPgShadowSinkFactory::for_tenant_in_region(tenant, region)` constructs a `RealNeonShadowSink` straight from `executors.get(region.as_str())` without ever touching the resolver.
- The legacy `for_tenant(tenant)` arm remains the wave-21 resolver round-trip — used on the fallback path (no prelude attached, dev/test/native gRPC boot) and on the wave-27 defense-in-depth stale-prelude rejection.
- The canonical `corelink.audit.analytics_query.v1` happy-path emit carries `region_source = "prelude"` when the prelude was consumed; `region_source = "fallback"` when the legacy resolver round-trip ran.
- The factory now lives in a new public module `corelink_server::neon_shadow_factory` (gated by `cfg(feature = "neon-real")`) instead of as a nested struct inside `apps/server/src/main.rs`. This is the structural change required to make the override unit-testable: the wave-29 unit test pins that `for_tenant_in_region` invokes the resolver ZERO times.

Out of scope:

- The wave-26 `RequestPrelude` populator — consumed unchanged.
- The wave-27 server-side route plumbing — consumed unchanged; only the helper `resolve_shadow_via_prelude` signature changes from `Result<Arc<dyn NeonShadowSink>, &'static str>` to `Result<(Arc<dyn NeonShadowSink>, &'static str), &'static str>` so the canonical success emit threads the region-source telemetry tag.
- Migrations, OpenAPI envelope, rate-limit config — all unchanged.
- The `InMemoryShadowSinkFactory` (`apps/server/src/routes.rs`) — keeps the wave-27 default delegate. The in-memory factory is dev/CI-only; the prelude hot path is not load-bearing here.

## 2. Deliverables (this wave)

1. **`corelink_server::neon_shadow_factory::TokioPgShadowSinkFactory`** — `apps/server/src/neon_shadow_factory.rs`. New public module (gated by `cfg(feature = "neon-real")`) hosting the production factory + its wave-29 `for_tenant_in_region` override that skips the resolver. Replaces the wave-21 nested struct in `apps/server/src/main.rs`.
2. **`AnalyticsAuditRow::region_source`** — `apps/server/src/routes/audit_analytics.rs`. New `Option<String>` field carrying the canonical telemetry value (`"prelude"` / `"fallback"`). Additive — `#[non_exhaustive]` is preserved; existing call sites default to `None` via the canonical `AnalyticsAuditRow::new` constructor; resolver-path emits decorate via the new `AnalyticsAuditRow::with_region_source` builder.
3. **`REGION_SOURCE_PRELUDE` + `REGION_SOURCE_FALLBACK` constants** — `apps/server/src/routes/audit_analytics.rs`. Public stable labels for the dashboard widget filters.
4. **`resolve_shadow_via_prelude` tuple return** — `apps/server/src/routes/audit_analytics.rs`. The helper now returns `Result<(Arc<dyn NeonShadowSink>, &'static str), &'static str>` so the canonical success-path emit in both `handle_event_count` + `handle_timeline` threads the `region_source` tag through `.with_region_source(...)` decoration.
5. **Wave-27 audit doc caveat update** — `specs/_audits/2026-05-16-shadow-sink-consumer-adoption.md` §9 caveat #2 → **CLOSED-WAVE-29** with forward pointer to this doc.
6. **Net-new tests (4)**:
   - **`tokio_pg_shadow_sink_factory_for_tenant_in_region_skips_resolver_lookup`** — `apps/server/src/neon_shadow_factory.rs::tests`. A counting `TenantRegionResolver` fake asserts that `for_tenant_in_region` invokes the resolver ZERO times (and `for_tenant` invokes it exactly once on the symmetric `tokio_pg_shadow_sink_factory_for_tenant_still_invokes_resolver` test).
   - **`tokio_pg_shadow_sink_factory_for_tenant_still_invokes_resolver`** — symmetric pin so the wave-21 fallback path doesn't regress.
   - **`tokio_pg_shadow_sink_factory_for_tenant_in_region_surfaces_missing_pool`** — defense-in-depth pin: when the prelude region was never wired at boot, the override surfaces the canonical `region has no executor pool` stable error (same `&'static str` the wave-21 arm surfaces on the same condition).
   - **`audit_analytics_consumes_prelude_region_without_extra_d1_round_trip`** — `apps/server/src/routes/audit_analytics.rs::tests`. Integration pin: with a `RequestPrelude` attached, the recording factory observes ZERO `for_tenant` calls, exactly one `for_tenant_in_region` call with the prelude's region, AND the canonical success emit carries `region_source = "prelude"` (and NO `request_prelude_missing` marker row).
   - **`audit_analytics_fallback_path_tags_region_source_fallback`** — symmetric integration pin: with NO prelude, the success emit carries `region_source = "fallback"`.

## 3. Charter constraints

- **`#![forbid(unsafe_code)]`** — inherited from `apps/server/src/lib.rs`; no unsafe added.
- **No `unwrap` / `expect` / `panic` in src** — every fallible step in the new factory surfaces the canonical `&'static str` matching the wave-21 error contract (`region has no executor pool`, `tenant_region: unresolved`, `tenant_region: backend_unavailable`, `tenant_region: unknown`). Test-only code retains the `tests are allowed to use these primitives` waiver.
- **Fail-CLOSED on resolver failure preserved** — the wave-21 `for_tenant` arm is unchanged; the new `for_tenant_in_region` arm short-circuits on missing-pool with the same stable error the wave-21 arm surfaces. The route layer's `emit_or_503` wrap maps both to 503.
- **No silent IAD fallback** — the override does NOT default to `Region::Iad` on missing-pool; it surfaces the typed error so the route layer's 503 + audit emit chain runs.
- **Public API additivity** — `AnalyticsAuditRow` is `#[non_exhaustive]`; the `region_source` field is additive and the existing constructor surface is preserved. `with_region_source` is `must_use` so a builder chain that drops the result is caught by Clippy.
- **DCO sign-off + Co-Authored-By** — applied on the wave-29 commit.
- **Freeze §3.b** — wave-29 stream-4 is the GA-blocker prep stream; commit subject prefixed `fix(ga-blocker): ` and body carries `FREEZE-EXCEPTION: P1-ga-blocker`.

## 4. Sync/async boundary (unchanged)

The wave-26 wire owned the sync/async boundary resolution (async D1 prefetch in the request-prelude; sync resolver lookup at dispatch time). The wave-29 override is purely synchronous — the new path takes the pre-resolved region as a value type and dispatches straight to `executors.get(...)` (a `BTreeMap` lookup). No new I/O, no new async boundaries, no `block_on` introduced.

## 5. RLS / tenant-isolation contract (unchanged)

The wave-29 override does NOT weaken any tenant-isolation gate:

- **Construction-time tenant pin** — every shadow sink built via the override goes through `RealNeonShadowSink::new(tenant_id, region, exec, audit_sink)`, the same constructor the wave-21 `for_tenant` arm uses. The tenant pin is preserved end-to-end.
- **SQL-layer RLS (`app.current_tenant`)** — unchanged; the `RealNeonShadowSink` always SETs the GUC inside every transaction (`SQL_SET_RLS_TENANT_GUC`).
- **Residency pin** — the prelude's `region` value is the one the CF Worker request-prelude prefetch resolved from the same `tenant_config` D1 table the wave-21 resolver consults. Both paths surface the same authoritative region; the override merely eliminates the second lookup that yields the same answer the prelude already carries.
- **Stale-prelude rejection** — the wave-27 defense-in-depth check (`prelude.tenant_id != tenant`) remains the authoritative gate before `for_tenant_in_region` is called; a stale prelude triggers the fallback path with the `request_prelude_missing` marker emitted.

## 6. CTRL-PRIV-001 — audit-payload PII scrub

The new `region_source` field carries a 2-value enum-like string (`"prelude"` / `"fallback"`). It is:

- **Static** — neither value is caller-supplied; both are constants compiled into the route layer.
- **Non-PII** — describes the dispatch path, not any tenant attribute.
- **Bounded cardinality** — exactly 2 distinct values + `None` for pre-resolution exit arms.

No raw query rows, no JWT bytes, no diagnostic strings carrying backend state are added.

## 7. Tests

| Test | Loc | What it pins |
|---|---|---|
| `tokio_pg_shadow_sink_factory_for_tenant_in_region_skips_resolver_lookup` | `apps/server/src/neon_shadow_factory.rs::tests` | The override does NOT invoke `TenantRegionResolver::resolve_region` — the canonical wave-29 closure pin. |
| `tokio_pg_shadow_sink_factory_for_tenant_still_invokes_resolver` | `apps/server/src/neon_shadow_factory.rs::tests` | The wave-21 fallback arm still invokes the resolver exactly once (no regression). |
| `tokio_pg_shadow_sink_factory_for_tenant_in_region_surfaces_missing_pool` | `apps/server/src/neon_shadow_factory.rs::tests` | Missing-pool surfaces the canonical `region has no executor pool` stable error. |
| `audit_analytics_consumes_prelude_region_without_extra_d1_round_trip` | `apps/server/src/routes/audit_analytics.rs::tests` | Integration: prelude-attached requests skip the legacy `for_tenant`, dispatch through `for_tenant_in_region(prelude.region)`, AND tag the success emit `region_source = "prelude"`. |
| `audit_analytics_fallback_path_tags_region_source_fallback` | `apps/server/src/routes/audit_analytics.rs::tests` | Symmetric integration: no-prelude requests tag the success emit `region_source = "fallback"`. |

Existing wave-27 tests (`request_prelude_consumed_dispatches_through_for_tenant_in_region`, `request_prelude_missing_falls_back_with_warn_and_audit`, `request_prelude_missing_emit_pinned_for_timeline_route`) continue to pass unchanged — the helper signature change is internal; the trait surface and route extractor are untouched.

Total wave-29 net-new: **5 tests** (3 unit on the factory module + 2 integration on the route). Test count: `audit_analytics` 12 → **14** passing; `neon_shadow_factory` 0 → **3** passing.

## 8. Gates

| Gate | Status | Notes |
|---|---|---|
| `cargo build --workspace --features neon-real` | ✅ green | Wave-29 wire compiles end-to-end. |
| `cargo test -p corelink-audit-chain --features neon-real` | ✅ green | Audit-chain crate unchanged. |
| `cargo test -p corelink-server --lib audit_analytics` | ✅ green (12 → 14 passing) | Wave-29 +2 integration tests. |
| `cargo test -p corelink-server --lib neon_shadow_factory --features neon-real` | ✅ green (0 → 3 passing) | Wave-29 +3 unit tests on the new factory module. |
| `cargo clippy --workspace --all-targets --features neon-real -- -D warnings` | ✅ clean | No new lints introduced. |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` | ✅ green | Wave-29 changes are native-only (gated by `cfg(feature = "neon-real")`); the CF Worker wasm32 build is unaffected. |
| `validate_specs.py` | ✅ green | `_audits/` is in `SKIP_ALL`. |
| `validate_references.py` | ✅ green | Forward pointer wave-27 → wave-29 added; back pointer wave-29 → wave-27. |
| Freeze §3.b classification | ✅ P1 GA-blocker prep | Closes wave-27 §9 caveat #2. |

## 9. Caveats / follow-ons

- **Dashboard widget for `region_source`** — the audit row now carries the field; the analytics dashboard widget that plots the `prelude:fallback` ratio is part of the standard wave-21 dashboard refresh cadence and does not require a separate wave.
- **`InMemoryShadowSinkFactory` override** — the dev/CI factory in `apps/server/src/routes.rs` keeps the wave-27 default delegate. The in-memory factory is not load-bearing for prelude-vs-fallback dispatch (it has no underlying resolver round-trip to eliminate). Closing the in-memory factory's override would be cosmetic.
- **`RequestPrelude` translation at the CF Worker dispatch site** — still tracked at wave-27 §9 caveat #1 ("CF Worker → axum dispatch site translation"). Wave-29 stream-4 is purely server-side; the dispatch site translation lands when analytics endpoints graduate to the CF Worker boot path.

## 10. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

## Wave-29 closure relationship to wave-27 audit doc

This wave-29 audit doc is the canonical closure for the wave-27 §9 caveat #2 ("Production `TokioPgShadowSinkFactory` override") in `specs/_audits/2026-05-16-shadow-sink-consumer-adoption.md`. The wave-27 doc is updated in the same commit to flip the §9 caveat to **CLOSED-WAVE-29** with a forward pointer to this doc.
