# CF Worker Fetch-Handler Request-Prelude Prefetch Wire — 2026-05-16 (wave-26)

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave-26 — closes the wave-25 audit-doc §8 caveat ("CF Worker fetch-handler prefetch wiring [...] non-trivial restructure deferred to a future wave"), itself the residual orchestration glue of the wave-21 §7 caveat.
>
> **Related WIs:** WI-S09-NEON-ANALYTICS-SHADOW (wave-18 trait surface), Wave-19 (production driver), Wave-20 (`TokioPostgresExecutor` binder), Wave-21 (`TenantRegionResolver` trait + `D1TenantRegionResolver`), Wave-25 (`D1TenantConfigStore` + `build_tenant_region_resolver` factory), Wave-26 (this — CF Worker fetch-handler prefetch orchestration glue).
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock 7y retention — unchanged), CTRL-PRIV-031 (data residency — per-region Neon project enforced at the resolver + the migration's per-row `region` column), CTRL-PRIV-001 (audit-payload PII scrub — synthetic emission carries only the validated tenant label + diagnostic), INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — per-txn `set_config('app.current_tenant', $1, true)` envelope).

## 1. Scope

Closes wave-25 §8 caveat by wiring `D1TenantConfigStore::prefetch` into the CF Worker fetch handler's request-prelude (`crates/corelink-clerk-cf/src/health.rs::main`). Until this wave, the wave-25 wire was FUNCTIONALLY CALLABLE but the fetch handler did not invoke it — `ShadowSinkFactory::for_tenant` consumers fell back to the cache-miss path, which routed every legacy tenant through the configured `Region::Iad` fallback. After this wave the CF Worker boot path:

1. Resolves the tenant via the wave-25 `TenantContext::from_header_value` (unchanged).
2. Builds the wave-15 `CfRealBindings` bundle (unchanged).
3. **NEW** — calls `prod_wiring::prefetch_request_prelude` which:
   - Derives a deterministic `Uuid` cache key from the tenant label (`Uuid::new_v5(NAMESPACE_OID, label)`).
   - Issues the canonical `SELECT region FROM tenant_config WHERE tenant_id = ?` via the audit-fenced + tenant-scoped `CfD1DatabaseReal`.
   - Populates the per-request `D1TenantConfigStore` cache.
   - Synchronously resolves the region through the wave-25 `D1TenantRegionResolver` factory.
   - **On ANY failure** (D1 prefetch backend error, resolver `Unresolved`, resolver `BackendUnavailable`): emits a `tenant_region_unresolved` audit row via the canonical `AuditSink::emit_synthetic` (Logpush ingest) AND returns `PrefetchWireError`. The fetch handler maps this to HTTP **503 fail-CLOSED** — NEVER a silent fallback to `Region::Iad`.
4. Holds the resulting `RequestPrelude` (region + populated store + resolver) alongside `CfRealBindings` for the duration of the request so downstream handler-chain code branches on `prelude.region` without re-resolving.

Out of scope:
- Wave-25 trait surface, `D1TenantConfigStore`, `build_tenant_region_resolver` factory — consumed unchanged.
- Migration `0052_tenant_config_region.sql` — already shipped wave-21.
- Native gRPC server wire (`apps/server::main.rs`) — unchanged; wave-21 wire applies.
- The wasm32 build target (`cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf`) — pre-existing broken on the e9ee8eb baseline per wave-25 §8 (`getrandom 0.4.2 backends::inner_u64`); separately tracked under wave-26 stream #2 wasm32 baseline fix.

## 2. Deliverables (this wave)

1. **`tenant_uuid_for_label(label: &str) -> Uuid`** — `crates/corelink-clerk-cf/src/prod_wiring.rs`. Deterministic `Uuid::new_v5(NAMESPACE_OID, label.as_bytes())` derivation. Pure (no I/O, no clock); the same `label` always derives the same `Uuid`. Used as the per-request cache key so the async prefetch site and the synchronous `region_label` lookup site agree on the key without round-tripping a separate identifier through the request.
2. **`PrefetchWireError`** — `crates/corelink-clerk-cf/src/prod_wiring.rs`. `#[non_exhaustive]` enum with two variants: `PrefetchBackend(String)` (D1 prefetch refused) and `Unresolved { tenant_id_label, diagnostic }` (resolver chain failed). Both translate to HTTP 503 at the fetch handler.
3. **`RequestPrelude`** — `crates/corelink-clerk-cf/src/prod_wiring.rs`. Bundle of `{ tenant: TenantContext, tenant_uuid: Uuid, region: Region, store: Arc<D1TenantConfigStore>, resolver: Arc<dyn TenantRegionResolver> }`. Held by the handler chain so the resolved region propagates without re-resolution; the store/resolver are retained so downstream `ShadowSinkFactory::for_tenant` dispatches re-use the populated cache.
4. **`prefetch_request_prelude(tenant, &d1, &audit_sink, fallback_region) -> Result<RequestPrelude, PrefetchWireError>`** — `crates/corelink-clerk-cf/src/prod_wiring.rs`. Async orchestration entry point. Sequence:
   1. Derive deterministic v5 cache key.
   2. Build empty `D1TenantConfigStore`.
   3. Async `store.prefetch(d1, tenant_uuid, &tenant_label).await` — populates the cache from D1. On failure → emit audit + return `PrefetchBackend`.
   4. Build `D1TenantRegionResolver` via wave-25 factory.
   5. Synchronous `resolver.resolve_region(&tenant_uuid)`. On failure → emit audit + return `Unresolved`.
   6. Return `RequestPrelude { tenant, tenant_uuid, region, store, resolver }`.
5. **`emit_tenant_region_unresolved` (module-private)** — `crates/corelink-clerk-cf/src/prod_wiring.rs`. Builds a synthetic `AuditEvent { surface: "d1", op: "tenant_region_unresolved", tenant: label, subject: diagnostic }` and emits via the canonical `AuditSink::emit_synthetic` path. Operators key the Logpush dashboard on the `tenant_region_unresolved` op label.
6. **`AuditSink::emit_synthetic(event)`** — `crates/corelink-clerk-cf/src/audit_sink.rs`. New public helper that emits a pre-built `AuditEvent` directly through the configured backend, bypassing the per-binding `AuditFn` adapter layer. The per-binding `D1Op`/`R2Op`/`KvOp`/`DoOp` enums intentionally do not include the `tenant_region_unresolved` variant (their variants gate hot-path wrapper ops, not orchestration-level events); the synthetic path lets the prefetch wire emit through the same NDJSON sink without expanding those enums. Infallible by design — the recorder backend's mutex-poison case degrades to a silent drop (prefetch wire is already on the fail-CLOSED path; an additional log failing must not double-fault the 503 response).
7. **CF Worker fetch-handler integration** — `crates/corelink-clerk-cf/src/health.rs::main`. Calls `prefetch_request_prelude` between `build_real_bindings` and the `handle_health_real` dispatch (feature-gated on `tenant-region-real`). On `PrefetchWireError` returns `worker::Response::error(format!("tenant_region_unresolved: {e}"), 503)`. When the feature is OFF the prefetch step compiles away — the `/health` handler ships its legacy behaviour unchanged.
8. **`uuid` workspace feature add** — `crates/corelink-clerk-cf/Cargo.toml`. Added `"v5"` to the `tenant-region-real`-gated `uuid` dep so the deterministic v5 namespace derivation is reachable. The existing `v4`/`v7`/`serde`/`js` features in the root workspace dep are unaffected.
9. **Integration tests** — `crates/corelink-clerk-cf/tests/cf_worker_prefetch_wire.rs`. Three host-CI tests:
   - `prefetch_request_prelude_success_cached_region_propagates` — pre-seeds the store with a `Some("FRA")` pin, validates the wire-internal resolver factory routes to `Region::Fra` and the cache survives. Also asserts the v5 derivation is deterministic.
   - `prefetch_request_prelude_backend_failure_fails_closed` — drives the native-stub D1 wrapper through the actual orchestration entry point; the native stub's `WasmOnly: …` surface forces the wire to return `PrefetchWireError::PrefetchBackend` AND emit a `tenant_region_unresolved` row through the recorder sink. Explicit assertion that the audit row carries the tenant label + diagnostic and that the resolver did NOT silently fall back to `Region::Iad`.
   - `prefetch_request_prelude_cached_region_survives_handler_chain` — simulates the post-prefetch state and validates a downstream sync `region_label` lookup against the same store key returns the cached entry without further D1 round-trips. Pins the sync/async boundary resolution (the prefetch runs ONCE per request; any number of downstream `ShadowSinkFactory::for_tenant` calls consult the same cache).

## 3. Charter constraints

- **`#![forbid(unsafe_code)]`** — inherited from `crate::lib.rs`; no unsafe added.
- **No `unwrap` / `expect` / `panic` in src** — wire code uses `match` + `Result` propagation throughout. The synthetic audit emission's recorder-poison drop is the only intentional infallibility (documented).
- **Audit fail-CLOSED on resolver failure** — the wire MUST NOT silently fall back to `Region::Iad`. Every failure mode emits `tenant_region_unresolved` AND surfaces a typed error to the fetch handler, which translates to HTTP 503. Pinned by `prefetch_request_prelude_backend_failure_fails_closed`.
- **Cache key determinism** — `tenant_uuid_for_label` is pure. Pinned by the determinism assertion in `prefetch_request_prelude_success_cached_region_propagates`.
- **No `block_on` inside the Worker isolate** — the sync/async boundary is resolved by running the async prefetch ONCE in the request-prelude before any synchronous `ShadowSinkFactory::for_tenant` dispatch. The synchronous `region_label` lookup reads the populated in-memory cache without crossing the futures runtime boundary.
- **DCO sign-off + Co-Authored-By** — applied on the wave-26 commit.

## 4. Sync/async boundary resolution

The wave-25 `TenantConfigStore` trait is **sync** (`region_label(&Uuid) -> Result<Option<String>, String>`) because the wave-21 `ShadowSinkFactory::for_tenant` consumer is sync and dispatches `resolve_region` synchronously. The `worker::D1Database::prepare(...).first(...)` API is **async**, and CF Workers' wasm32 runtime forbids `block_on` inside the isolate. The wave-26 wire resolves this tension by **separating the I/O from the lookup**:

| Step | Async/Sync | Where | Notes |
|---|---|---|---|
| `tenant_uuid_for_label` | sync (pure) | request-prelude | deterministic v5 derivation, no I/O |
| `D1TenantConfigStore::prefetch` | **async** | request-prelude | runs the `worker::D1Database::first` future |
| `D1TenantConfigStore::insert` (cache write) | sync | inside `prefetch` | populates the per-request `Arc<Mutex<HashMap<…>>>` |
| `D1TenantRegionResolver::resolve_region` | **sync** | request-prelude | reads the populated cache (cache-hit guaranteed) |
| `D1TenantConfigStore::region_label` (downstream) | sync | handler-chain | re-reads the SAME cache; no further D1 round-trips |

The cache is **per-request** — each CF Worker `fetch` invocation builds a fresh `D1TenantConfigStore` via `prefetch_request_prelude`, ensuring `tenant_config.region` UPDATE statements take effect on the next request without any TTL coherence concerns. Long-lived caching across requests is intentionally out of scope (the wave-25 audit doc §2 fixed this stance).

## 5. RLS / tenant-isolation contract (unchanged)

Tenant isolation continues to be enforced at TWO independent layers:

1. **App layer** — `CfD1DatabaseReal::scoped_query` rejects any SELECT lacking `WHERE tenant_id = ?`; `CfD1DatabaseReal::verify_first_bind` constant-time-compares the FIRST bound parameter against the anchored tenant-id. The wave-26 wire's prefetch passes the tenant-id as the first (and only) parameter, so cross-tenant probing is blocked at the wrapper.
2. **Storage layer** — D1 is per-Worker-binding scoped (CF Workers enforce the `CLERK_DB` binding mapping at the platform level).

A wiring bug that leaks across tenants surfaces as `D1Error::Backend("tenant_bind: …")` from the wrapper → `PrefetchError::Backend(…)` from the store → `PrefetchWireError::PrefetchBackend(…)` from the wire → HTTP 503 + `tenant_region_unresolved` audit row. Never a silent cross-tenant row.

## 6. CTRL-PRIV-001 — audit-payload PII scrub

The synthetic `tenant_region_unresolved` audit row carries:

- `surface = "d1"` — static.
- `op = "tenant_region_unresolved"` — static.
- `tenant = <validated tenant label>` — already non-secret (the `TenantContext::from_header_value` constructor rejects empty / NUL / separator inputs at the binding-anchor boundary).
- `subject = <backend diagnostic string>` — sourced from `PrefetchError::Backend(msg)` or `TenantRegionError::{Unresolved|BackendUnavailable}::to_string()`. NEVER raw D1 row payloads, JWT bytes, or blob contents.

The `AuditSink::emit_synthetic` helper documents the CTRL-PRIV-001 invariant inline.

## 7. Tests (net-new)

| Test | Loc | What it pins |
|---|---|---|
| `prefetch_request_prelude_success_cached_region_propagates` | `tests/cf_worker_prefetch_wire.rs` | Cache pre-seed → resolver → prelude region propagates; v5 derivation is deterministic. |
| `prefetch_request_prelude_backend_failure_fails_closed` | `tests/cf_worker_prefetch_wire.rs` | Native-stub `WasmOnly: …` → `PrefetchWireError::PrefetchBackend` + `tenant_region_unresolved` audit row; NEVER silent IAD fallback. |
| `prefetch_request_prelude_cached_region_survives_handler_chain` | `tests/cf_worker_prefetch_wire.rs` | Sync `region_label` lookup against the post-prefetch store returns the cached entry without further D1 round-trips. |

Total wave-26 net-new: 3 integration tests. Test count: 37 (wave-25 baseline) → **40** passing with `--features tenant-region-real`.

## 8. Gates

| Gate | Status | Notes |
|---|---|---|
| `cargo build -p corelink-clerk-cf --features tenant-region-real` | ✅ green | Wave-26 wire compiles clean. |
| `cargo build --workspace --features neon-real` | ✅ green | Workspace remains buildable end-to-end. |
| `cargo test -p corelink-clerk-cf --features tenant-region-real` | ✅ 40 passing (37 → 40) | Wave-26 +3 integration tests; wave-25 tests unchanged. |
| `cargo clippy -p corelink-clerk-cf --features tenant-region-real --all-targets -- -D warnings` | ✅ clean | No new lints introduced. |
| `cargo clippy --workspace --all-targets -- -D warnings` | ✅ clean | Workspace-wide. |
| `validate_specs.py` | ✅ green | 446 schema + 9 YAML-only = 455. |
| `validate_references.py` | ✅ green | No dangling refs. |
| `cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf --features tenant-region-real` | ⚠️ pending wave-26 stream #2 wasm32 baseline fix | Pre-existing `getrandom 0.4.2 backends::inner_u64` failure on the e9ee8eb baseline; tracked under the parallel wave-26 wasm32 baseline stream. Per the task charter: "wasm32 build pending wave-26 #2 SEAL". |

## 9. Caveats / follow-ons

- **wasm32 build** — still blocked on the pre-existing `getrandom 0.4.2` failure (wave-25 §8 caveat). The wave-26 wire code itself is wasm32-clean; the failure is upstream in a transitive dep and reproducible on `main 2a4e00c` before any wave-26 change. Tracked under wave-26 stream #2.
- **Handler-chain ShadowSinkFactory dispatch wiring** — **CLOSED-WAVE-27** (`specs/_audits/sealed/2026-05-16-shadow-sink-consumer-adoption.md`). The wave-27 wire adds `ShadowSinkFactory::for_tenant_in_region(tenant, region)` (default-impl delegate to `for_tenant`) + an `Option<Extension<RequestPrelude>>` extractor on the `/v1/audit/analytics/*` route handlers. When the extension is present the route skips the per-request `TenantRegionResolver::resolve_region` round-trip; when absent the route falls back through the legacy path with a `tracing::warn!` + `request_prelude_missing` audit row (NOT a silent IAD fallback). The fallback path is the active code path on the native gRPC server boot today; the route becomes shadow-aware automatically when the CF Worker dispatch site attaches the extension.
  - *Original caveat (for context):* `prefetch_request_prelude` returns a `RequestPrelude` carrying the resolver, but the actual `corelink_audit_chain::ShadowSinkFactory::for_tenant` integration call site in `health::handle_health_real` is not yet shadow-aware (the `/health` probe never opens a shadow analytics row). This is by design — `/health` is the proving ground for the wire; the shadow-factory consumer call sites land when the analytics endpoints (`/v1/audit/analytics/*`) graduate from native-only to the CF Worker boot path on a later wave. The wire is FUNCTIONALLY CALLABLE today; this caveat tracks the consumer-side adoption, not the wire itself.
- **Tenant identifier shape** — wave-26 derives the cache key via `Uuid::new_v5(NAMESPACE_OID, tenant_label.as_bytes())`. This is one-way (no `Uuid → label` recovery) and collision-resistant for the 16-hex tenant prefix cardinality. If a future tenant scheme switches to UUIDs natively (e.g. v7 ids from a centralised tenant registry), the derivation can drop the v5 step in favour of a `Uuid::parse_str(label)` path. The trait surface (`region_label(&Uuid)`) is unchanged.

## 10. Sign-off

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.

Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>.

---

## Wave-26 closure relationship to wave-25 audit doc

This wave-26 audit doc is the canonical closure for the deferred §8 caveat in `specs/_audits/sealed/2026-05-16-tenant-config-cf-prod-wire.md` (wave-25 closure-note). The wave-25 doc is updated in the same commit to flip the §8 caveat to **CLOSED-WAVE-26** with a forward pointer to this doc.
