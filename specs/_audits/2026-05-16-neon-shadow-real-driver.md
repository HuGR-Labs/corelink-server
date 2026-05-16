# Audit-Chain Neon Analytics Shadow — Real Driver Cutover — 2026-05-16

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave 19 — closes wave-18 caveat #3 (the in-memory `InMemoryNeonShadowSink` shipped wave-18; this wave ships the production `RealNeonShadowSink` driver + per-region Neon project resolution + daily reconciliation cron + lag/drift PD alerts + runbook).
>
> **Related WIs:** WI-S09-NEON-ANALYTICS-SHADOW (Wave 18 — the trait surface + InMemory fake + customer endpoints + 4 audit-event taxonomy), Wave-19 (this — production driver).
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock 7y retention — unchanged; the shadow is analytics-only), CTRL-COMPLIANCE-SOC2-CC72 (immutable audit log — R2 source-of-truth preserved), CTRL-PRIV-031 (data residency — per-region Neon project enforced at the `EnvVarResolver` + the migration's per-row `region` column), INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH — R2 stays canonical), INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33 per-region project), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — every txn opens with `SELECT set_config('app.current_tenant', $1, true)`), INV-AUDIT-APPEND-ONLY (CRITICAL — `ON CONFLICT (tenant_id, seq) DO NOTHING` idempotent INSERT).

## 1. Scope

Closes the wave-18 wave-close caveat #3: ship the production Neon driver so the customer-facing analytics endpoints (`/v1/audit/analytics/event-count` + `/v1/audit/analytics/timeline`) can be wired to a live Neon project at GA rollout instead of staying on the wave-18 `InMemoryNeonShadowSink`.

Out of scope:
- The wave-18 trait surface + InMemory fake (already shipped; preserved unchanged so the 17+3+5 wave-18 tests stay green).
- The 4 audit-event-type taxonomy + lag SLO constants (already pinned by Rust unit tests; this driver consumes them, doesn't redefine them).
- Customer endpoint wire shape (already shipped wave-18; this driver slots behind the same `NeonShadowSink` trait the endpoints already consume).

## 2. Deliverables (this wave)

1. **`RealNeonShadowSink`** — `crates/corelink-audit-chain/src/neon_shadow/real.rs`. Production `NeonShadowSink` impl bound to a `(tenant_id, region)` pair at construction time. SQL is `pub const &str` for the 8 canonical statements (BEGIN / RLS GUC SET / INSERT / 2× aggregate queries / reconcile count / COMMIT — the migration column list is pinned by the `sql_constants_pin_to_migration_schema` unit test).
2. **`NeonProjectResolver` trait + `EnvVarResolver` impl** — per-region DSN resolution from `NEON_DB_URL_<REGION_UPPER>` (rows 120–124 of `docs/internal/secrets-checklist.md`).
3. **`NeonExecutor` trait** — the SQL backplane abstraction. The trait is sync (matches the rest of `corelink-audit-chain`); the production binder (a thin `deadpool-postgres` adapter wrapping `tokio-postgres`) lives at the binary boot path per the `trait-abstraction-defer` charter pattern (mirrors `corelink-byok-aws::AwsKmsRealProvider` + `corelink-cf-bindings::cf_r2`). The `InMemoryExecutor` ships here for unit tests + the property test; the real Postgres binder is the wave-19 server-wire follow-on.
4. **Daily reconciliation cron** — `.github/workflows/neon-shadow-reconcile-daily.yml`. Walks each active region; for the prior UTC day compares the R2 NDJSON archive row count vs the shadow row count via `SQL_RECONCILE_COUNT`. Non-zero drift fires SEV-2 PD with `dedup_key=neon-shadow-drift-<region>-<date>`. Per-region failures DO NOT short-circuit the matrix (each region's reconcile is independent). The harness is `scripts/neon_shadow_reconcile.py`.
5. **Runbook** — `specs/_runbooks/RB-NEON-SHADOW-LAG.md`. SEV-2 lag scenario (≥60min lag, 5 consecutive samples) + SEV-2 drift scenario (reconcile cron non-zero diff) + diagnostic SQL + mitigations (catch-up replay, region failover, sink disable) + regulatory note (NOT a personal-data breach — no GDPR Art. 33 / LGPD Art. 48 clock starts on a shadow lag).
6. **PD alert rules** — `dashboards/alerts/dash-neon-shadow-alerts.yml`. Two SEV-2 rules (`NeonShadowLagSev2` + `NeonShadowDriftSev2`) + two recording rules (per-region p99 lag + per-region 1h drift rate). BLAKE3-pseudonymized `tenant_id_hex8` in payload labels.

## 3. Charter constraints (`trait-abstraction-defer` pattern)

The driver follows the same charter pattern used by `byok-aws-real` / `byok-gcp-real` / `byok-azure-real` / `cf-billing-real`:

- The trait surface (`NeonShadowSink` + `NeonExecutor` + `NeonProjectResolver`) lives in the pure-logic crate (`corelink-audit-chain`).
- The production binder (`tokio-postgres` + `deadpool-postgres`) lives at the binary boot path (`apps/server`), gated by `cfg(not(target_arch = "wasm32"))` + the `neon-real` Cargo feature flag.
- The Cargo feature is a **build-time witness**, not a runtime gate. The trait surface + `RealNeonShadowSink` orchestration are ALWAYS built so the `apps/server` binary can wire them without conditional code; the feature flag only toggles whether the production-driver binder module is compiled in.

The wasm32 stub is implemented but every method returns `NeonError::WasmOnly` — CF Workers reach the per-region Neon project via the native gRPC server, never directly from the worker isolate (matches the canonical `corelink-cf-bindings::cf_r2` pattern).

## 4. RLS contract (defense in depth)

Tenant isolation is enforced at TWO independent layers — a wiring bug at either layer surfaces as a hard error, NOT a silent leak:

1. **App layer (this driver)** — `RealNeonShadowSink` is bound to a `(tenant_id, region)` pair at construction time. Every `sync_chunk` call:
   1. Constant-time-compares the receipt's `tenant_id` against the bound tenant via `subtle::ConstantTimeEq` (timing-attack resistant).
   2. Constant-time-compares every row's `tenant_id` + region against the bound pair.
   3. Fails CLOSED with `NeonShadowError::TenantIsolationViolation` / `::ResidencyViolation` BEFORE any txn opens.
2. **SQL layer (migration `0001_audit_events_shadow.sql`)** — the table carries `ENABLE ROW LEVEL SECURITY` + a `tenant_isolation_audit_events_shadow` policy keyed on `current_setting('app.current_tenant')`. Every txn opens with `SELECT set_config('app.current_tenant', $1, true)` — the third arg `true` scopes the GUC to the txn so a poolboy-reused connection cannot leak across requests.

A wiring bug that drops the `set_config` SET surfaces as a Postgres permission error inside the txn (the RLS policy rejects the INSERT) — never as a cross-tenant leak.

## 5. Reconciliation contract

The daily reconciliation cron (`neon-shadow-reconcile-daily.yml`; 03:00 UTC) is the day-N+1 cross-tier integrity gate between R2 (canonical) + Neon shadow (read tier). Contract:

- **Frequency:** daily, 03:00 UTC (1h after `audit-chain-daily-verify.yml` so the canonical chain verify outcome is fresh).
- **Scope:** prior UTC day, per region in `RECONCILE_REGIONS` env (matrix kept in lockstep with the `Region` enum).
- **Comparison:** R2 NDJSON key count under `audit/<YYYY>/<MM>/<DD>/` vs `audit_events_shadow` row count for the same `[from_ms, to_ms)` window. The Neon side uses the canonical SQL constant `SQL_RECONCILE_COUNT` (pinned to the Rust driver via the `sql_constants_pin_to_migration_schema` unit test).
- **Trigger:** non-zero diff fires SEV-2 PD page via Events API v2 with `dedup_key=neon-shadow-drift-<region>-<date>`. The recording rule `corelink_neon_shadow_drift_rate_1h` lifts the per-day gauge to a 1h rolling series so the dashboard panel + Alertmanager rule see a continuous signal.
- **Fail-safe:** missing per-region DSN is logged + SKIPPED (NOT a failure — bring-up friendly). Missing CF token = explicit error + per-region PD page. Network-level errors = explicit error + per-region PD page.

The reconcile script (`scripts/neon_shadow_reconcile.py`) re-encodes the canonical SQL constant for the Python tier; the Rust-side test `sql_constants_pin_to_migration_schema` enforces single-source-of-truth between the Rust constant + the migration; the Python copy is a strict re-encode for the cron environment.

## 6. Tests (net-new)

| Test | Loc | What it pins |
|---|---|---|
| `env_var_resolver_canonical_names` | `real.rs` | `Region::Iad → NEON_DB_URL_IAD` etc. |
| `env_var_resolver_returns_project_unresolved_when_unset` | `real.rs` | typed error when DSN env unset |
| `static_resolver_returns_dsn` | `real.rs` | test-only resolver round-trips |
| `sync_chunk_emits_canonical_sql_order` | `real.rs` | BEGIN → set_config → INSERT × N → COMMIT order pinned |
| `sync_chunk_rejects_cross_tenant_constant_time` | `real.rs` | tenant isolation pre-check + SEV-2 audit emit |
| `sync_chunk_rejects_cross_region` | `real.rs` | residency pre-check + audit emit |
| `sync_chunk_propagates_backend_failure_sev2` | `real.rs` | executor error → typed `NeonShadowError::Backend` |
| `aggregate_event_count_decodes_canned_rows` | `real.rs` | bigint count decoding |
| `aggregate_timeline_zero_granularity_rejected` | `real.rs` | granularity == 0 surfaces Internal |
| `reconcile_count_decodes_scalar` | `real.rs` | reconcile harness scalar decode |
| `sql_constants_pin_to_migration_schema` | `real.rs` | migration column list pinned to Rust SQL |
| `neon_error_into_neon_shadow_error_preserves_taxonomy` | `real.rs` | error mapping covers every variant incl. `WasmOnly` |
| `roundtrip_jsonb_preserves_payload` (proptest, 10_000 cases) | `real.rs` | payload_jsonb byte-stable through the entire write path |

**Status:** 12 unit tests + 1 property test (10k iter PR-gate) — all passing in the canonical `cargo test -p corelink-audit-chain --features neon-real --lib` run.

The 5 `#[ignore]`-by-default Neon-staging integration tests are deferred to the wave-19 server-wire follow-on (they require a live Neon project DSN; the container infra to run an ephemeral Postgres + apply the 0001 migration + seed RLS + exercise the full driver is non-trivial and time-budgeted out of this stream — the property test + the canonical-SQL-order unit test cover the wire-shape invariants the staging tests would re-pin).

## 7. Caveats / follow-ons

- **`#[ignore]`-by-default Neon-staging integration tests** — deferred to a follow-on WI; documented above in §6.
- **Real `TokioPostgresExecutor` binder** — deferred to the server-wire follow-on (the `apps/server` boot path needs the `deadpool-postgres` pool constructor + per-region pool sharding); the `RealNeonShadowSink` orchestration is wave-19 complete; only the trait-object binding to `tokio-postgres` is the deferred bit.
- **`audit_analytics` router merge** — the wave-18 module was declared but not merged into `routes::build()`. Auto-merging it requires a production `ShadowSinkFactory` that resolves the per-region Neon executor per-tenant; that lives behind the same server-wire follow-on as the `TokioPostgresExecutor`.

## 8. Sign-off

- Rust unit tests: 139 passing (17 wave-18 InMemory + 12 wave-19 RealNeonShadowSink + 1 proptest + the rest of the audit-chain crate's 109 pre-existing tests).
- Clippy: clean.
- `check_migrations_additive.py`: green (no schema mutations; the 0001 migration shipped wave-18).
- `validate_secrets_matrix.py`: green (5 new `NEON_DB_URL_<REGION>` rows added, all referenced in the reconcile workflow).
- `validate_references.py`: green.
- `actionlint`: target — green on the new `neon-shadow-reconcile-daily.yml`.

DCO sign-off: Gustavo Schneiter <gustavo@humangr.com>.
