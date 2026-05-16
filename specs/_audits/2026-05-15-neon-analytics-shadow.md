# Audit-Chain Neon Analytics Shadow Sync — 2026-05-15

> **Doc kind:** evidence / audit attestation (no canonical front matter required — `_audits/` is excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Owner:** Gustavo Schneiter (Security Lead).
>
> **Trigger:** GA Wave 18 — Audit-Chain Neon analytics shadow sync.
>
> **Related WIs:** WI-S09-004 (CloudEvents emitter + R2 hash chain + daily verifier), Wave 15 (R2 NDJSON archive producer), Wave-15.3 (`/v1/audit/export`), Wave 18 (this — Neon analytics shadow sync + customer SQL endpoints).
>
> **Related controls:** CTRL-AUDIT-001 (R2 Object Lock 7y retention — unchanged), CTRL-COMPLIANCE-SOC2-CC72 (immutable audit log — R2 source-of-truth preserved), CTRL-PRIV-031 (data residency — per-region Neon project), INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH — R2 is canonical), INV-DATA-RESIDENCY (CRITICAL — Schrems II + LGPD Art. 33), INV-AUTH-SCHEMA-RLS-DEFAULT-ON (CRITICAL — tenant isolation), INV-AUDIT-APPEND-ONLY (CRITICAL — shadow INSERT-only).

## 1. Scope

This audit memo documents the **Neon analytics shadow** of the audit chain shipped Wave 18. The shadow is a read-tier projection of the canonical R2 NDJSON archive (Wave 15) into a Neon Postgres table (`audit_events_shadow`), exposed to customers via two SQL-backed aggregate endpoints:

- `GET /v1/audit/analytics/event-count?from=&to=&event_type=`
- `GET /v1/audit/analytics/timeline?from=&to=&granularity=`

The motivation is purely UX: customer analytics queries (multi-event aggregation, time-range filtering, tenant-cross-referencing) are painful on R2 NDJSON archives. Neon Postgres serves the same data set with sub-second SQL aggregates, while R2 remains the canonical chain-integrity store.

## 2. Architecture

```text
    ┌──────────────────────────────────────────────────┐
    │  ArchiveProducer (Wave 15) — per-tenant + region │
    │  buffers PersistedAuditLines + flushes chunks    │
    └────────────────────┬─────────────────────────────┘
                         │  on flush success
            ┌────────────┴────────────┐
            ▼                         ▼
    ┌───────────────┐         ┌───────────────────┐
    │ R2 NDJSON     │         │ Neon shadow sink  │
    │ (CANONICAL —  │         │ (ANALYTICS ONLY — │
    │  Object Lock  │         │  per-region Neon  │
    │  Governance   │         │  project; RLS     │
    │  Mode 7y)     │         │  + INSERT-only)   │
    └───────────────┘         └─────────┬─────────┘
            ▲                           │
            │                           ▼
    ┌───────┴────────┐         ┌──────────────────────────┐
    │ corelink audit │         │ /v1/audit/analytics/*    │
    │ verify (CLI)   │         │ event-count + timeline   │
    │ — chain        │         │ — SQL aggregates over    │
    │ integrity      │         │ Neon shadow              │
    │ source-of-     │         │                          │
    │ truth          │         └──────────────────────────┘
    └────────────────┘
```

**Source-of-truth discipline:** R2 = chain integrity (SEV-0 on break). Neon shadow = analytics convenience (SEV-2 on sync failure / lag breach). A customer auditor pursuing compliance evidence runs `corelink audit verify` against R2; the Neon shadow is never used as authoritative input to any compliance attestation.

**Eventual consistency:** ≤ 5 min nominal (matches the `archive_producer::DEFAULT_FLUSH_AFTER_MS` cadence). The shadow lags the canonical archive by at most one flush window in steady state. Lag ≥ 60 min surfaces a SEV-2 PagerDuty alert (`dashboards/alerts/dash-neon-shadow-lag.yml` — to be wired in a follow-on Lote alongside the `RealNeonShadowSink` driver implementation).

## 3. Schema + invariant enforcement

The Neon migration is `migrations/neon/0001_audit_events_shadow.sql` (additive-only per INV-AUTH-MIGRATION-ADDITIVE; verified by `scripts/check_migrations_additive.py` — updated this wave to include `migrations/neon/`).

**`audit_events_shadow` columns:**

| Column | Type | Notes |
|---|---|---|
| `tenant_id` | `UUID NOT NULL` | Per-tenant chain partition; part of PRIMARY KEY |
| `seq` | `BIGINT NOT NULL` | Monotonic chain sequence; part of PRIMARY KEY |
| `event_time` | `TIMESTAMPTZ NOT NULL` | Event wall-clock instant |
| `event_type` | `TEXT NOT NULL` | Canonical CloudEvents `type` |
| `prev_hash` | `BYTEA NOT NULL` | 32-byte BLAKE3 link of previous event |
| `link_hash` | `BYTEA NOT NULL` | 32-byte BLAKE3 link of this event |
| `payload_jsonb` | `JSONB NOT NULL` | Full NDJSON payload — SQL-query-able |
| `synced_at` | `TIMESTAMPTZ NOT NULL DEFAULT now()` | Shadow-sync wall-clock |

**Indexes:** `(tenant_id, event_time)`, `(tenant_id, event_type)`, `(event_type, event_time)` — cover the analytics query shapes.

**RLS policy (INV-AUTH-SCHEMA-RLS-DEFAULT-ON CRITICAL):**

```sql
ALTER TABLE audit_events_shadow ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation_audit_events_shadow ON audit_events_shadow
    USING (tenant_id = current_setting('app.current_tenant', true)::uuid);
```

The production `RealNeonShadowSink` SETs `app.current_tenant` inside every transaction; the app-layer trait pin (`NeonShadowSink::tenant_id()`) provides defense-in-depth for wiring bugs.

**Residency (INV-DATA-RESIDENCY CRITICAL):**

The shadow lives in the tenant's pinned Neon region (one Neon project per region: `enam` / `weur` / `sam` / etc.). The sink trait carries `region()` so a cross-region write is caught at the type layer. Schrems II + LGPD Art. 33 alignment is identical to the auth-domain schema (migration 002).

**Auxiliary table `audit_shadow_lag`** — per `(tenant_id, region)` lag observation row used by the SEV-2 alert. Same RLS pattern.

## 4. Customer endpoints + rate limit

Both endpoints live in `apps/server/src/routes/audit_analytics.rs` and follow the wave-15.3 `audit_export.rs` pattern:

1. JWT-validated tenant injected via `X-Tenant-Id` header (production middleware; tests stub directly).
2. Rate-limit gate: `BucketKey::per_tenant_per_endpoint(tenant, "audit.analytics")`. Config: 10 queries / 60 s / tenant (looser than `/v1/audit/export` because analytics is cheaper — Postgres aggregate vs. R2 list+walk).
3. Audit emit `corelink.audit.analytics_query.v1` BEFORE returning bytes (fail-CLOSED ordering per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
4. Tenant-pin assertion: `NeonShadowSink::tenant_id() == authenticated_tenant` (defense-in-depth over the SQL-layer RLS).

**Defensive bucket-cardinality cap on the timeline endpoint:** `(to - from) / granularity ≤ MAX_TIMELINE_BUCKETS = 1_000`. Granularity ≤ 24 h. A single request cannot exhaust planner memory.

## 5. Failure modes + SEV classification

| Mode | SEV | Rationale |
|---|---|---|
| R2 chunk write fails | SEV-0 (chain break / fail-CLOSED transaction abort) | R2 is canonical; missing chunk = compliance gap |
| Neon shadow `INSERT` fails | SEV-2 (analytics lag) | R2 already committed; shadow is best-effort |
| Shadow lag breaches 60 min | SEV-2 (PagerDuty page) | Customer-visible analytics staleness |
| Cross-tenant shadow row | Fail-CLOSED at app layer + RLS at SQL layer; emits `corelink.audit.neon_shadow_sync_failed.v1` SEV-2 | Tenant isolation invariant |
| Cross-region shadow row | Fail-CLOSED at app layer; emits failure audit SEV-2 | INV-DATA-RESIDENCY |
| Analytics rate-limit deny | 429 + `corelink.audit.analytics_query.v1 exit_status=rate_limited` | Wave-18 SLA: 10 / min / tenant |

**SEV-2, not SEV-0:** the shadow is intentionally NOT in the chain-integrity critical path. A Neon outage degrades analytics UX (customers see stale aggregates) but the canonical chain in R2 is unaffected. Re-running `corelink audit verify` continues to work; the daily-verify cron (`audit-chain-daily-verify.yml`) walks R2 + emits SEV-0 only on R2 chain breaks.

**Reconciliation:** on Neon recovery the sink resumes; the `(tenant_id, seq)` PRIMARY KEY makes the INSERT idempotent (retries are safe — `ON CONFLICT DO NOTHING` semantics on the production driver; the in-memory fake mirrors this).

## 6. Audit event taxonomy (4 new event types)

| Type | Trigger | SEV |
|---|---|---|
| `corelink.audit.neon_shadow_synced.v1` | Successful chunk sync | `info` (or `sev-2` if observed_lag_ms ≥ 60 min) |
| `corelink.audit.neon_shadow_sync_failed.v1` | Tenant / residency / backend failure | `sev-2` |
| `corelink.audit.analytics_query.v1` (`/event-count`) | Analytics query served / rate-limited / 400 | info |
| `corelink.audit.analytics_query.v1` (`/timeline`) | Analytics query served / rate-limited / 400 | info |

(The `analytics_query.v1` type is shared between both endpoints; the `endpoint` field on the row disambiguates.)

## 7. Test coverage

Pin file: `crates/corelink-audit-chain/tests/neon_shadow.rs`.

- `roundtrip_hundred_events_through_archive_plus_shadow_sync_matches_aggregate` — 100-event end-to-end happy path through `ArchiveProducer` + 4 chunks of 25 events each + shadow sync + `aggregate_event_count` + `aggregate_timeline` assertions.
- `lag_detection_emits_sev2_when_observed_lag_breaches_60min` — simulates a 60-min sync delay; asserts SEV-2 audit emit.
- `tenant_isolation_cross_tenant_query_fails_closed` — asserts both write-side rejection (`NeonShadowError::TenantIsolationViolation`) AND read-side scope (a tenant-B sink sees zero tenant-A rows).

Unit tests in `src/neon_shadow.rs` cover residency violation, injected backend failure, duplicate-seq idempotency, empty-rows-slice rejection, timeline bucketing, event-type filter, audit constants, lag classifier boundaries, and trait object safety (17 tests in this module; 126 in the crate lib; 11 prop tests; 3 integration tests — total 167).

## 8. Quality gates (Wave 18 CI)

| Gate | Status |
|---|---|
| `cargo build -p corelink-audit-chain` | green |
| `cargo build -p corelink-server` | green |
| `cargo clippy -p corelink-audit-chain --tests -- -D warnings` | green |
| `cargo clippy -p corelink-server --tests -- -D warnings` | green |
| `cargo test -p corelink-audit-chain` (167 tests) | green |
| `cargo test -p corelink-server --lib routes::audit_analytics` (5 tests) | green |
| `python3 scripts/validate_specs.py` | green |
| `python3 scripts/check_migrations_additive.py` (+ `migrations/neon/` directory now scanned) | green |

## 9. Production wiring (deferred — `RealNeonShadowSink`)

The real Neon driver implementation is intentionally deferred (mirrors the wave-15 `trait-abstraction-defer` pattern for the CF R2 binding). The next WI lands:

1. `RealNeonShadowSink` using `tokio-postgres` for the native target / Neon serverless driver for the CF Worker target (the `worker::send_future` pattern wraps the sync trait surface).
2. Per-region Neon project resolution (`region → neon project id` map shipped via Terraform IaC, slotted under `corelink-iac`).
3. `SET LOCAL app.current_tenant = '<uuid>'` at transaction open (RLS gate).
4. Daily reconciliation cron `audit-chain-shadow-reconcile.yml` — walks R2 list, asserts every chunk's `(tenant_id, last_seq)` is reflected in `audit_events_shadow`. Negative delta surfaces SEV-2 (PagerDuty Events API v2 dispatch keyed `audit-shadow-lag-<YYYY-MM-DD>-<tenant>`).
5. PagerDuty alert rules + runbook `RB-NEON-SHADOW-LAG.md`.

The pure-logic primitive shipped this wave (`NeonShadowSink` + `InMemoryNeonShadowSink`) covers every invariant the production binding will need; the swap is a one-line constructor change at the route boot path (`apps/server/src/routes.rs::build`).

## 10. SOC 2 CC7.2 mapping (delta vs. Wave 15)

| CC7.2 control | Wave 15 mechanism | Wave 18 delta |
|---|---|---|
| Detect security events / failures | R2 daily-verify cron | unchanged (R2 = canonical) |
| Customer-initiated audit retrieval | `/v1/audit/export` NDJSON | adds `/v1/audit/analytics/*` SQL aggregates |
| Communicate disposition | dual-approval audit-viewer UI | unchanged |
| Resume normal operations | resumable verifier | adds idempotent shadow re-sync via `ON CONFLICT DO NOTHING` |

## 11. ISO 27001:2022 mapping

| Control | Mechanism |
|---|---|
| A.5.28 — Evidence collection | R2 Object Lock 7y (unchanged) |
| A.8.15 — Logging | CloudEvents 1.0 + R2 NDJSON (unchanged) + Neon shadow (NEW, analytics-only) |
| A.8.16 — Monitoring | SEV-2 alert on shadow lag ≥ 60 min |
| A.8.34 — Protection during audit testing | RLS at SQL layer + app-layer tenant pin |

## 12. Sign-off

- **Mechanism choice:** Neon Postgres analytics shadow with R2 retained as canonical chain-integrity store. SEV-2 (analytics-only) failure mode preserves Wave-15 SEV-0 chain-break discipline.
- **Mechanism owner:** Gustavo Schneiter (Security Lead).
- **Review cadence:** Annual (mirrors SOC 2 audit cycle); revisit after `RealNeonShadowSink` lands to update SLO measurement methodology.
- **Next review:** 2027-05-15.
