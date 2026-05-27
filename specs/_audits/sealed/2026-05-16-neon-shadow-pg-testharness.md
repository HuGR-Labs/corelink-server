# Audit-Chain Neon Analytics Shadow — Ephemeral Postgres Test Harness — 2026-05-16

> **Doc kind:** evidence / audit attestation (`_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** Wave-20 test-infra stream #1 — lift the 5 wave-19 Neon-shadow integration tests out of `#[ignore]` purgatory by introducing a self-contained `testcontainers-rs` harness that boots a `postgres:16-alpine` container per test.
>
> **Related WIs:** WI-S09-NEON-ANALYTICS-SHADOW (Wave 18 — trait surface + InMemory fake), Wave-19 (production driver — closed wave-18 caveat #3), Wave-20 stream #1 (this — test infra) + Wave-20 stream #2 (canonical migration drift fix — see §5 caveat).
>
> **Related controls:** INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH — R2 stays canonical; this harness exercises only the analytics shadow), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — every txn opens with `SELECT set_config('app.current_tenant', $1, true)`), INV-TENANT-ISOLATION (CRITICAL — the cross-tenant test asserts RLS returns zero rows for a non-bound tenant), INV-AUDIT-APPEND-ONLY (CRITICAL — the idempotent-retry test asserts `ON CONFLICT DO NOTHING` semantics).

## 1. Scope

Wave-19 shipped `RealNeonShadowSink` + 5 integration tests that called out to a real Neon-staging project. Those tests were `#[ignore]`-by-default because the staging project required out-of-band provisioning (project creation, DSN distribution, OIDC federation, per-CI secret rotation). They never ran in CI; a developer had to opt in with `--ignored` + `NEON_DB_URL_IAD=...` to exercise them.

This wave-20 stream lifts that constraint. The new harness (`crates/corelink-audit-chain/tests/harness/pg_container.rs`) spins up an ephemeral `postgres:16-alpine` testcontainer per test, applies the wave-18 migration plus a wave-20 schema-drift compensation patch (see §5), creates a non-superuser `corelink_test` role so RLS is actually enforced, and exposes a `TokioPostgresExecutor` adapter that satisfies the `NeonExecutor` trait. The 5 wave-19 tests (now living at `crates/corelink-audit-chain/tests/neon_shadow_real.rs`) exercise `RealNeonShadowSink` end-to-end against the ephemeral container with NO external dependency beyond a reachable Docker daemon.

Out of scope:
- The `RealNeonShadowSink` driver + production wiring (already shipped wave-19).
- The wave-18 in-memory test surface (preserved unchanged; the 3 + 27 wave-18 tests still pass without `live-pg`).
- The wave-20 canonical migration fix (`0002_audit_events_shadow_with_check.sql` adds the missing `region` column to the canonical migration — that lands in stream #2). Until stream #2 merges, the harness applies a local compensation patch; see §5.

## 2. Deliverables

1. **`live-pg` cargo feature** — `crates/corelink-audit-chain/Cargo.toml`. OFF by default so `cargo test` on a developer laptop without Docker stays green; ON via `--features live-pg` to opt into the testcontainer suite.
2. **Dev-dependencies** — `testcontainers = "0.21"`, `testcontainers-modules = { version = "0.9", features = ["postgres"] }`, `tokio = { version = "1" }` (workspace-pinned), `tokio-postgres = { version = "0.7", features = ["with-uuid-1", "with-serde_json-1"] }`. All four are dev-deps — never linked into the library or CF Worker bundle.
3. **Harness module** — `crates/corelink-audit-chain/tests/harness/pg_container.rs` (~470 LOC):
   - `spawn_ephemeral_postgres(runtime: Arc<Runtime>) -> PgHarness` boots the container, applies migrations, creates the app role, returns the harness handle.
   - `PgHarness::client() -> Arc<Client>` exposes the long-lived non-superuser tokio-postgres client.
   - `TokioPostgresExecutor` implements `NeonExecutor` by translating each `(sql, params)` call to a `Client::prepare_typed` + `execute`/`query` (the typed-prepare step is load-bearing — see §4).
   - `count_visible_rows(harness, tenant) -> i64` is the RLS-binding verification helper. Opens a fresh connection, BEGINs, calls `set_config('app.current_tenant', $1, true)`, SELECTs `COUNT(*)`, COMMITs.
4. **5 integration tests** — `crates/corelink-audit-chain/tests/neon_shadow_real.rs`:
   1. `real_sync_chunk_persists_and_queries_back` — end-to-end INSERT + aggregate query against real Postgres.
   2. `real_idempotent_insert_on_retry` — `ON CONFLICT (tenant_id, seq) DO NOTHING`; second sync of the same rows leaves the row count unchanged.
   3. `real_rls_enforces_tenant_isolation` — tenant_b SELECT returns ZERO rows of tenant_a data under the RLS policy.
   4. `real_sync_chunk_rejects_cross_tenant_at_pre_check` — constant-time tenant_eq_ct fires BEFORE any SQL is emitted; the SQL trace is empty.
   5. `real_aggregate_timeline_matches_in_memory_fake` — the real driver and the in-memory fake produce byte-for-byte identical timeline buckets.

   Every test is annotated `#[cfg_attr(not(feature = "live-pg"), ignore)]` so they are listed as `ignored` on a default `cargo test` run.
5. **Cargo.toml `[[test]]` entry** — `name = "neon_shadow_real"`, `path = "tests/neon_shadow_real.rs"`. Documents the suite contract inline.

## 3. How to run

### Default (no Docker needed)

```sh
cargo test -p corelink-audit-chain
```

Outcome: 139 lib + 27 mutation_kills + 3 in-memory neon_shadow + 11 prop_audit_chain = **180 tests pass**, plus **5 ignored** in `neon_shadow_real`. Matches wave-19 baseline. Zero new dependencies linked into the library binary.

### With ephemeral Postgres (Docker required)

```sh
docker info >/dev/null || open -a Docker   # ensure daemon is up
cargo test -p corelink-audit-chain --features live-pg --test neon_shadow_real -- --test-threads=1
```

Outcome: **5 tests pass** in ~65 s on a 2024 MacBook Pro M3 (Docker Desktop 28.3, postgres:16-alpine image already cached). First-run cold start adds ~10 s for the image pull.

### Diagnosing a failing test

The `TokioPostgresExecutor` records every SQL string in an internal trace; the cross-tenant test asserts the trace is empty by calling `TokioPostgresExecutor::take_trace()`. To debug a passing-but-suspicious test:

```rust
let exec = TokioPostgresExecutor::new(harness.client(), runtime.clone());
let trace_handle = Arc::clone(&exec);   // before moving exec into Arc<dyn NeonExecutor>
// ... run test ...
eprintln!("SQL trace: {:#?}", trace_handle.take_trace());
```

The trace gives a 1:1 audit of which SQL statements the production sink emitted in which order — the canonical wave-19 `BEGIN → set_config → INSERT × N → COMMIT` order is asserted by the unit tests in `src/neon_shadow/real.rs`.

To inspect the live Postgres state mid-test, set a breakpoint or insert a long `tokio::time::sleep` and connect manually:

```sh
# The PgHarness exposes the bootstrap superuser DSN — grab the port from the test logs:
psql "postgres://postgres:postgres@localhost:<port>/postgres"
```

## 4. Engineering caveats

### 4.1 `prepare_typed` is load-bearing

The wave-19 SQL constants compose multiple casts on the same parameter — e.g. `SQL_INSERT_SHADOW_ROW` binds `event_time_ms` as `$3` and uses it inside `to_timestamp($3::double precision / 1000.0)`. When the executor sends the query via `Client::execute(&sql, &params)`, tokio-postgres asks the server to infer parameter types, and Postgres picks `float8` for the `::double precision` slot — but the harness binds the parameter as an `i64`. The `i64` `ToSql` impl errors with `"error serializing parameter 2"` because it only knows how to encode for `INT8` / `bigint`.

The fix is `Client::prepare_typed(sql, &[Type::INT8, ...])` — explicit parameter OIDs pin the shape so Postgres uses `bigint::double precision` (a valid SQL cast chain). The `param_pg_type` helper in `pg_container.rs` maps every `ExecutorParam` variant to the canonical `tokio_postgres::types::Type`. The production binder MUST replicate this pattern; the same panic would surface there.

### 4.2 `ContainerAsync::drop` requires a tokio runtime in scope

`testcontainers::core::async_drop::async_drop` calls `tokio::runtime::Handle::current()` at drop time, which panics if no runtime is in scope. The harness holds the runtime via `Arc<Runtime>` and explicitly drops the container inside `runtime.enter()` in `PgHarness::drop`. Without this, every test panics during cleanup with `"there is no reactor running, must be called from the context of a Tokio 1.x runtime"`.

### 4.3 Module-files lint vs `tests/harness/pg_container.rs` path

The workspace lints `clippy::mod_module_files = "deny"` forbids `tests/harness/mod.rs`. The audit task spec called out `tests/harness/pg_container.rs` as the canonical path; we preserve that path via an inline `mod harness { #[path = "pg_container.rs"] pub mod pg_container; }` in `neon_shadow_real.rs`. The `#[path]` is resolved relative to the implicit `harness/` directory beside the test file.

## 5. Schema-drift caveat (wave-20 stream #2 follow-up)

`migrations/neon/0001_audit_events_shadow.sql` (wave-18) does NOT declare a `region` column on `audit_events_shadow`, but the production `SQL_INSERT_SHADOW_ROW` constant binds `region` as parameter `$8` and inserts into a `region` column. The wave-18 / wave-19 in-memory unit tests pass because the `InMemoryExecutor` never checks column existence. **Real Postgres rejects the INSERT immediately.**

Wave-20 stream #2 (`wt/r-prep-neon-shadow-rls-with-check-and-emit-discipline`) lifts the column into a canonical migration `0002_audit_events_shadow_with_check.sql` along with the wave-20 `WITH CHECK` clause on the RLS policy. Until stream #2 merges, this harness applies a local **compensation patch** (`HARNESS_SHADOW_SCHEMA_PATCH` in `pg_container.rs`) that:

1. `DROP TABLE IF EXISTS audit_events_shadow CASCADE` (removes the wave-18 baseline shape).
2. Re-creates `audit_events_shadow` with the missing `region TEXT NOT NULL` column + `prev_hash`/`link_hash` as `TEXT` (the production driver binds them as hex strings via `ExecutorParam::HexBytes`).
3. Re-attaches the secondary indexes + the RLS policy (with the wave-20 `WITH CHECK` clause pre-applied).

When stream #2 lands, the compensation patch becomes redundant. Lifting it is a 5-line delete in `pg_container.rs`; the audit doc gains a "SUPERSEDED §5 — wave-20 stream #2 closed the drift" note. No production code changes.

## 6. Quality gates

| Gate                                                                                            | Status     |
|-------------------------------------------------------------------------------------------------|------------|
| `cargo build --workspace`                                                                       | pass       |
| `cargo test -p corelink-audit-chain` (default — 180 tests + 5 ignored)                          | pass       |
| `cargo test -p corelink-audit-chain --features live-pg --test neon_shadow_real -- --test-threads=1` | pass (5/5) |
| `cargo clippy --workspace --all-targets -- -D warnings`                                          | pass       |
| `cargo clippy -p corelink-audit-chain --all-targets --features live-pg -- -D warnings`           | pass       |
| Live-pg suite wall-clock                                                                         | ~65 s      |

## 7. Charter compliance

- `#![forbid(unsafe_code)]` preserved on every non-test crate (the harness lives entirely in `tests/`).
- Dev-deps are gated behind the `live-pg` feature flag at the cargo level; the default build does NOT compile `testcontainers` / `tokio-postgres` / `testcontainers-modules`.
- The harness honours the `trait-abstraction-defer` charter — `NeonExecutor` is the seam, the `TokioPostgresExecutor` adapter is test-local, the production binder still lives at the binary boot path (and will pick up the same `prepare_typed` pattern called out in §4.1).
- Wave-19 baseline test count preserved (180 passing, 1 ignored doc-test). Zero existing tests changed.

## 8. References

- `crates/corelink-audit-chain/tests/harness/pg_container.rs` — harness implementation.
- `crates/corelink-audit-chain/tests/neon_shadow_real.rs` — the 5 lifted tests.
- `crates/corelink-audit-chain/Cargo.toml` — `live-pg` feature + dev-deps.
- `migrations/neon/0001_audit_events_shadow.sql` — wave-18 canonical migration (referenced via `include_str!` by the harness).
- `crates/corelink-audit-chain/src/neon_shadow/real.rs` — `RealNeonShadowSink` + SQL constants exercised by the suite.
- `specs/_audits/2026-05-16-neon-shadow-real-driver.md` — wave-19 driver cutover doc (this harness is its test-infra follow-on).
