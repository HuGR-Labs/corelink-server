# `D1TenantRegionResolver` CF Worker Production Wire — 2026-05-16 (wave-25)

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave-25 — closes the wave-21 audit-doc §7 caveat ("`corelink-clerk-cf::prod_wiring` not yet updated to construct `D1TenantRegionResolver`") deferred at wave-21 commit `b5c6bfa`.
>
> **Related WIs:** WI-S09-NEON-ANALYTICS-SHADOW (wave-18 trait surface + InMemory fake), Wave-19 (production driver), Wave-20 (`TokioPostgresExecutor` binder), Wave-21 (`TenantRegionResolver` trait + `D1TenantRegionResolver` impl), Wave-25 (this — CF Worker production wire).
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock 7y retention — unchanged; the shadow is analytics-only), CTRL-PRIV-031 (data residency — per-region Neon project enforced at the resolver + the migration's per-row `region` column), INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33 per-region project pin), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — every txn opens with `SELECT set_config('app.current_tenant', $1, true)`).

## 1. Scope

Closes wave-21 §7 caveat: wire the wave-21 `D1TenantRegionResolver` into the production CF Worker boot path (`corelink-clerk-cf::prod_wiring`). Until this wave, the CF Worker boot path stayed on `InMemoryTenantRegionResolver` with `Region::Iad` fallback — the same IAD-default that wave-20 hard-coded in `apps/server::main.rs` before wave-21 replaced it with the trait surface. The native gRPC server already wired the resolver at wave-21; this wave brings the CF Worker boot to parity.

Out of scope:
- Wave-21 trait surface + `D1TenantRegionResolver` impl (already shipped; consumed unchanged).
- Migration `0052_tenant_config_region.sql` (already shipped wave-21; this wire consumes the `tenant_config.region` column it added).
- The per-request cache-warming policy: this wave ships an in-memory `Arc<Mutex<HashMap<Uuid, Option<String>>>>` cache populated by an async `prefetch` helper; the CF Worker fetch handler will call `prefetch` in the request-prelude on a future wave when the handler chain is reshaped to thread the resolver through (the wire path is now CALLABLE — the orchestration follow-on is non-trivial because the synchronous `ShadowSinkFactory::for_tenant` boundary forbids `block_on` inside the worker isolate).

## 2. Deliverables (this wave)

1. **`D1TenantConfigStore`** — `crates/corelink-clerk-cf/src/tenant_region_real.rs`. Implements `corelink_audit_chain::TenantConfigStore` against a `CfD1DatabaseReal` (the wave-15 tenant-prefix-enforced + audit-fenced D1 wrapper). The sync `region_label(&Uuid)` trait method consults a per-request `Arc<Mutex<HashMap<Uuid, Option<String>>>>` cache; the async `prefetch(...)` helper populates the cache by issuing `SELECT region FROM tenant_config WHERE tenant_id = ?` through `CfD1DatabaseReal::scoped_query`. The split lets the resolver fit the wave-21 sync trait surface while the actual D1 read stays on the worker async runtime.
2. **`build_tenant_region_resolver`** — `crates/corelink-clerk-cf/src/prod_wiring.rs`. Returns `Arc<dyn TenantRegionResolver>` constructed from either:
   - `Some(D1TenantConfigStore)` → `D1TenantRegionResolver::new(store, fallback)` (production wire).
   - `None` → `InMemoryTenantRegionResolver::new().with_fallback(fallback)` (dev mode; CF Worker boot path runs without the `CLERK_DB` binding, e.g. local `wrangler dev`).
3. **`tenant-region-real` Cargo feature** — `crates/corelink-clerk-cf/Cargo.toml`. Gates the new module + the `corelink-audit-chain` + `uuid` dependency adds so the default-features wasm32 build surface stays unchanged.
4. **`corelink-audit-chain::Region` re-export** — `crates/corelink-audit-chain/src/lib.rs`. The wave-25 wire signature consumes a `Region` argument (the `D1TenantRegionResolver::fallback`); re-exporting from audit-chain avoids forcing every wire-site to add `corelink-analytics` as a direct dep.
5. **Integration tests** — `crates/corelink-clerk-cf/tests/tenant_region_wire.rs`. Two host-CI tests:
   - `build_tenant_region_resolver_with_store_constructs_d1_resolver` — pins the production wire branch (Some(store)) routes pinned tenants through D1 → FRA and uncached tenants through fallback → IAD.
   - `build_tenant_region_resolver_without_store_falls_back_to_inmemory` — pins the dev-mode wire branch (None) routes every tenant through the supplied fallback region.

## 3. Charter constraints (`trait-abstraction-defer` pattern)

The wire follows the same pattern wave-21 + earlier waves established:

- **Trait surface lives in `corelink-audit-chain`** — `TenantConfigStore` + `TenantRegionResolver` are in the pure-logic crate.
- **Production binder lives at the binding boundary** — `corelink-clerk-cf` is the CF Worker crate that owns the `worker::D1Database` binding lookup; it constructs the D1-backed store impl from the `CfRealBindings` bundle.
- **Cargo feature flag is a build-time witness, not a runtime gate** — `tenant-region-real` toggles whether the wire module is compiled in; the trait surface + `D1TenantRegionResolver` are always available in `corelink-audit-chain`.
- **Native binaries stay on InMemory mirrors** — `apps/server::main.rs` already runs the wave-21 native fallback (`InMemoryTenantRegionResolver` with `Uuid::nil() → IAD` + `Region::Iad` fallback). This wave does not touch that path.

## 4. Migration footprint

- **No new migrations.** `0052_tenant_config_region.sql` (wave-21) already creates `tenant_config(tenant_id PRIMARY KEY, region TEXT NOT NULL DEFAULT 'IAD' CHECK (region IN (…22 colocodes…)))`. The wire issues a parameterised `SELECT region FROM tenant_config WHERE tenant_id = ?` against this column. `check_migrations_additive.py` should remain green (no schema mutations).
- Migration `0053` is intentionally NOT introduced — the task brief allowed for it as a witness ("if not already in 0052"), but 0052 fully covers the CF Worker read path.

## 5. RLS / tenant-isolation contract (unchanged)

Tenant isolation continues to be enforced at TWO independent layers — this wave does not loosen either:

1. **App layer** — `CfD1DatabaseReal::scoped_query` rejects any SELECT lacking `WHERE tenant_id = ?`; `CfD1DatabaseReal::verify_first_bind` constant-time-compares the FIRST bound parameter against the anchored tenant-id. The wire's `prefetch` passes the tenant-id as the first (and only) parameter, so cross-tenant probing is blocked at the wrapper.
2. **Storage layer** — D1 is per-Worker-binding scoped (CF Workers enforce the `CLERK_DB` binding mapping at the platform level).

A wiring bug that leaks across tenants surfaces as `D1Error::Backend("tenant_bind: …")` from the wrapper, never as a silent cross-tenant row.

## 6. Tests (net-new)

| Test | Loc | What it pins |
|---|---|---|
| `store_returns_none_for_uncached_tenant` | `tenant_region_real.rs` | Cache-miss → `Ok(None)` (resolver applies fallback). |
| `store_returns_inserted_label` | `tenant_region_real.rs` | Cache-hit round-trips the inserted label verbatim. |
| `prefetch_native_stub_returns_wasm_only` | `tenant_region_real.rs` | Native stub runs the validate+audit contract then surfaces `WasmOnly:` (canonical native-stub pattern). |
| `build_tenant_region_resolver_with_store_constructs_d1_resolver` | `tests/tenant_region_wire.rs` | Production wire branch — D1-backed resolver, pinned tenant → FRA, uncached → IAD fallback. |
| `build_tenant_region_resolver_without_store_falls_back_to_inmemory` | `tests/tenant_region_wire.rs` | Dev-mode wire branch — InMemory fallback resolver, every tenant → configured fallback region. |

Total wave-25 net-new: 5 tests (3 unit + 2 integration); the brief required `+2 integration` minimum, which is met.

## 7. Gates

- `cargo build -p corelink-clerk-cf --features tenant-region-real`: green.
- `cargo build --workspace --features neon-real`: green.
- `cargo test -p corelink-clerk-cf --features tenant-region-real`: 23 unit + 6 + 6 + 2 integration = 37 passing (default-features baseline: 18 unit + 6 + 6 + 0 = 30 passing).
- `cargo clippy -p corelink-clerk-cf --features tenant-region-real --all-targets -- -D warnings`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `validate_specs.py` / `validate_references.py`: green.
- `check_migrations_additive.py`: green (no new migrations; 0052 column unchanged).

## 8. Caveats / follow-ons

- **`cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf` is pre-existing broken on the e9ee8eb baseline** — `getrandom 0.4.2` referenced via the transitive dependency tree fails to compile against the wasm32 target with the current Cargo.lock pin (`backends::fill_inner` not found). This is reproducible on `main e9ee8eb` BEFORE any wave-25 change and is therefore tracked outside this wave's deliverable surface. The `tenant-region-real` feature does NOT add the offending dep (the wave-25 `uuid` dep already shipped via `corelink-cf-bindings`-time deps); the failure surfaces identically whether the feature is on or off. **CLOSED-WAVE-26**: fixed by adding `.cargo/config.toml` setting `--cfg=getrandom_backend="wasm_js"` plus a wasm32-targeted `getrandom 0.4 wasm_js` direct dep in `corelink-cf-bindings`. See `specs/_audits/sealed/2026-05-16-wasm32-baseline-getrandom-fix.md`.
- **CF Worker fetch-handler prefetch wiring** — ~~`D1TenantConfigStore::prefetch` is callable but the CF Worker fetch handler (`corelink-clerk-cf::health::handle_health_real`) does not yet call it in the request-prelude.~~ **CLOSED wave-26 (2026-05-16).** The wave-26 wire (`prod_wiring::prefetch_request_prelude` + `RequestPrelude` + `tenant_uuid_for_label` + `PrefetchWireError`) is integrated into `health::main` between `build_real_bindings` and the handler dispatch. The sync/async boundary is resolved by running the async prefetch ONCE in the request-prelude, then any number of downstream synchronous `region_label` lookups consult the populated in-memory cache. On resolver failure the wire fails CLOSED with HTTP 503 + emits a `tenant_region_unresolved` audit row via `AuditSink::emit_synthetic` (NOT a silent fallback to `Region::Iad`). See `specs/_audits/sealed/2026-05-16-cf-worker-prefetch-wire.md` for the full closure note (deliverables, sync/async boundary table, +3 integration tests, fail-CLOSED audit emission contract).

## 9. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

## Wave-25 closure relationship to wave-21 audit doc

This wave-25 audit doc is the canonical closure for the deferred §7 caveat in `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md` (wave-21 closure-note). The wave-21 doc is updated in the same commit to flip the §7 caveat to CLOSED-WAVE-25.
