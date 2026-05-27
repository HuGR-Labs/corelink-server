# Design Pattern 04 — Asymmetric Dual-Write with Truth Hierarchy

**Location:** `crates/corelink-audit-chain/src/neon_shadow.rs` + `neon_shadow/{real,real_tokio_pg,tenant_region}.rs`
**Work item:** Wave 18 GA (initial), Wave 20 (real driver), Wave 21 (per-tenant region resolver)
**Invariants protected:** `INV-OBS-AUDIT-CHAIN-INTEGRITY` (HIGH), `INV-DATA-RESIDENCY` (CRITICAL), `INV-AUTH-SCHEMA-RLS-DEFAULT-ON` (CRITICAL)
**Pattern category:** Storage architecture — multi-tier write with explicit truth hierarchy

---

## 1. Problem

Append-only audit chain stores need two qualitatively different read paths:

| Read shape | Storage that serves it well | Storage that serves it poorly |
|---|---|---|
| **Integrity walk** (sequential, BLAKE3 link recompute, 7-year retention, immutable) | Object store (R2): cheap retention, native NDJSON streaming, byte-stable archive | Postgres: expensive retention, schema rigidity, no native immutability proof |
| **Analytics** (multi-event aggregation, time-range filtering, tenant cross-referencing, ad-hoc JOIN) | Postgres (Neon): SQL, JSONB indexes, query planner, materialized views | R2: every query reads + parses GBs of NDJSON, no indexes, no joins |

A naive single-tier choice fails at one end:
- R2-only → customer analytics queries are painful and expensive at scale.
- Postgres-only → 7-year retention cost explodes, and integrity proof depends on the same engine that stores the data (not a strong attestation posture).

The common "solution" is **dual-write** — write to both stores on every event. But the naive dual-write has three classes of failure:

1. **Cascading critical failure.** If Postgres write is treated as equally critical to object-store write, a Postgres outage aborts the audit emit. Now your compliance log can be DoS'd by an analytics outage.
2. **Implicit truth.** Without an explicit "which one is canonical?" declaration, operators reading the analytics tier during an incident treat it as truth — and silent divergence becomes silent corruption.
3. **Symmetric SEV.** Page-on-everything-fails creates alert fatigue and hides the real signal.

## 2. Solution architecture

Asymmetric dual-write with **declared truth hierarchy**:

```
event_emit(audit_event)
    │
    ├─→ R2 NDJSON archive  ─── canonical, integrity-bearing, SEV-0 on fail
    │
    └─→ Neon Postgres shadow ── analytics-only, best-effort, SEV-2 on fail
```

Three asymmetries are **structural**, not advisory:

| Axis | R2 (canonical) | Neon (shadow) |
|---|---|---|
| Truth status | Authoritative source of chain | Convenience projection; non-authoritative |
| Failure severity | SEV-0 page (chain break) | SEV-2 page (analytics lag) |
| Retention | 7+ years | Operational window (90-365d typical) |
| Verifier scope | Daily chain verifier walks R2 | Verifier never walks Neon |
| Recovery posture | Restore from R2 backups | Rebuild from R2 archive |

The shadow lag is **structurally bounded** but not blocking:

```rust
pub const SHADOW_LAG_NOMINAL_MAX_MS: u64 = 5 * 60 * 1_000;     // 5 min
pub const SHADOW_LAG_SEV2_THRESHOLD_MS: u64 = 60 * 60 * 1_000; // 60 min
```

Nominal lag matches the R2 archive producer's 5-min flush cadence — the shadow can be at most one flush behind in steady state. Above 60 min lag, an SEV-2 alerts operators that Neon is degraded, **without** escalating to a chain-integrity event.

## 3. Why each asymmetry is load-bearing

### 3.1 Truth declared in code, not in docs

```rust
//! The R2 NDJSON archive (`archive_producer.rs`) is the **canonical**
//! chain-integrity store. The daily-verify cron walks R2 and treats any
//! chain break there as SEV-0.
//!
//! The shadow is **analytics-only** — never authoritative for chain
//! integrity.
```

Most dual-write systems treat both stores as "the database" and operators have to discover the asymmetry empirically (usually during an incident). Declaring truth at module-level doc-comment + enforcing via the verifier wiring (verifier never reads Neon) makes the asymmetry **discoverable from the source tree**, not from a runbook.

### 3.2 SEV asymmetry prevents DoS-via-analytics

A symmetric SEV-0 dual-write creates the following attack surface:

```
Attacker degrades Neon → R2 write blocks → audit emit blocks → CAS write blocks
```

The asymmetric design breaks this chain at the audit-emit boundary:

```rust
// R2 commit FIRST, never aborted by shadow failure:
let r2_receipt = archive_producer.flush().await?;        // SEV-0 if fail
let _shadow_result = neon_shadow.sync_chunk(&r2_receipt) // SEV-2 if fail
    .await
    .or_log_shadow_failure();
```

Shadow failure emits `corelink.audit.neon_shadow_sync_failed.v1` and returns from `or_log_shadow_failure()` with `Ok(())` semantically. The CAS write path continues uninterrupted.

### 3.3 Tiered lag thresholds reflect different concerns

| Threshold | What it means operationally |
|---|---|
| ≤ 5 min (nominal) | "Healthy" — shadow keeps pace with R2 flush |
| 5-60 min | Degraded; investigate Neon health proactively but don't page |
| ≥ 60 min (SEV-2) | Neon is broken; customer analytics is stale; **still no integrity concern** |

The lag never escalates to integrity SEV because integrity is checked against R2 by an independent verifier. The shadow being 4 hours behind doesn't make a chain break invisible — it makes the analytics query stale by 4 hours. Different problem, different page tier.

## 4. Defense-in-depth tenant isolation

The shadow path enforces tenant isolation at **two layers** independently, so a bug at either layer fails closed:

| Layer | Mechanism | Enforced by |
|---|---|---|
| **Application** | `NeonShadowSink::sync_chunk(tenant_id, ...)` takes tenant as first arg; every shadow row carries it; in-memory fake rejects cross-tenant queries fail-CLOSED in tests | Rust type system + test harness |
| **SQL** | `ENABLE ROW LEVEL SECURITY` + `tenant_isolation_audit_events_shadow` policy keyed on `current_setting('app.current_tenant')`; production driver SETs the GUC inside every transaction | Postgres RLS |

A bug in the SQL RLS policy is caught by the app-layer fake. A bug in the app layer is caught by the SQL RLS policy. Both must be wrong simultaneously for cross-tenant leakage. The probability is the product of two independent failure modes.

## 5. Region pinning at the type system

```rust
pub struct ShadowEventRow {
    // ...
    pub region: Region,    // pinned at construction
}

impl TokioPgShadowSinkFactory {
    pub fn for_tenant(&self, tenant_id: Uuid) -> Result<Arc<dyn NeonShadowSink>, ...>
        // resolves the per-tenant region via TenantConfigStore
        // and instantiates a SinkArc that ONLY writes to that region's
        // Neon project endpoint
}
```

The factory wires a tenant to its pinned region at instantiation. A misrouted shadow write (e.g., a tenant assigned to `weur` having a chunk synced to the `wnam` Neon project) is caught at the type-system layer: the `Arc<dyn NeonShadowSink>` resolved for a `weur` tenant simply does not have a code path to the `wnam` endpoint.

Compare to the alternative (runtime region check in a single global sink): a missed branch or stale config silently lands customer data in the wrong region — a CRITICAL invariant violation for `INV-DATA-RESIDENCY`.

## 6. Wasm-clean trait surface

```rust
// No tokio in src/. The sink trait is synchronous; production CF Worker
// adapter wraps the call in worker::send::SendFuture in apps/server.
pub trait NeonShadowSink: Send + Sync {
    fn sync_chunk(
        &self,
        tenant_id: Uuid,
        rows: &[ShadowEventRow],
    ) -> Result<SyncOutcome, NeonError>;
}
```

The trait is **synchronous** in the crate (`src/`). The native production adapter (`real_tokio_pg.rs`) uses `tokio-postgres` + `deadpool-postgres` and is gated behind a `neon-real` Cargo feature; the wasm32 stub is always compiled in so the `Arc<dyn NeonShadowSink>` wiring type-checks on both targets. Stub methods surface `NeonError::WasmOnly`.

This design accommodates two constraints simultaneously:
1. Cloudflare Workers runtime (wasm32, no tokio in worker)
2. Native tooling (CLI verifier, integration test harness) that needs real Postgres

A simpler "async trait + tokio everywhere" design would force the Worker runtime to ship a tokio reactor (multi-MB binary bloat + cold-start cost). The split discipline keeps the wasm32 binary clean.

## 7. CloudEvents 1.0 emission on every outcome

```rust
pub const EVENT_TYPE_SHADOW_SYNCED: &str = "corelink.audit.neon_shadow_synced.v1";
pub const EVENT_TYPE_SHADOW_SYNC_FAILED: &str = "corelink.audit.neon_shadow_sync_failed.v1";
```

Both success and failure paths emit a CloudEvents 1.0-shaped audit row. The success row carries `(tenant_id, first_seq, last_seq, observed_lag_ms, region)`; the failure row adds the error class. Operators querying the audit chain itself for "what did the shadow do?" get a complete record without having to scrape Neon logs.

The success emission costs ~10 microseconds (one BLAKE3 hash + one audit-sink push) — negligible compared to the multi-millisecond Postgres write it is documenting.

## 8. Idempotent ON CONFLICT INSERT on the Postgres side

The production driver (`real.rs` / `real_tokio_pg.rs`) issues:

```sql
INSERT INTO audit_events_shadow (
    tenant_id, seq, event_time_ms, event_type,
    prev_hash, link_hash, region, payload_json
) VALUES (...)
ON CONFLICT (tenant_id, seq) DO NOTHING
```

On retry storms (Neon transient errors, network blips, replay after shadow worker restart), the dedup is at the storage layer rather than the application layer. Combined with the immutability of `(tenant_id, seq)` once R2 has committed (the audit chain is append-only), the ON CONFLICT DO NOTHING is a structural correctness statement: a row that already exists is the same row, by definition.

## 9. Anti-patterns explicitly rejected

| Anti-pattern | Why rejected |
|---|---|
| Symmetric SEV dual-write | DoS via analytics outage; audit emit blocks on analytics |
| Treating shadow as authoritative | Bugs propagate from "convenience tier" to integrity claims |
| Single global sink with runtime region routing | Misroute silently violates residency CRITICAL |
| Async trait in `src/` with tokio dep | Wasm32 worker binary bloat + cold-start cost |
| ON CONFLICT UPDATE (instead of DO NOTHING) | Allows post-hoc mutation of "immutable" audit row |
| Reading Neon during integrity verify | Verifier sees the shadow as ground truth → never detects R2 chain break |
| App-layer-only tenant isolation (no SQL RLS) | Single point of failure; a controller bug leaks cross-tenant |
| SQL-RLS-only tenant isolation (no app check) | RLS policy bug silently leaks; not caught in unit tests |

## 10. The general shape (transferable)

This pattern generalises to **any system that needs to serve qualitatively different reads from the same write stream** while maintaining a clear authoritative source. Transferable rules:

1. **Declare truth in code, not docs** — the verifier wiring must literally not be able to read the shadow.
2. **SEV-asymmetry follows truth-asymmetry** — non-authoritative writes fail at a lower SEV than authoritative writes.
3. **Lag SLOs bound divergence visibly** — operators see drift before customers do.
4. **Idempotent writes on the shadow side** — retries are safe by construction.
5. **Defense-in-depth on cross-cutting invariants** (tenant isolation, residency) — both layers enforce, both must fail for leak.
6. **Region/scope pinning at type system** — runtime checks are weaker than compile-time impossibilities.
7. **Both success and failure emit auditable events** — the shadow's own operations are part of the audit story.

## 11. Verification

| Property | Test |
|---|---|
| Shadow failure does not abort R2 commit | `tests/shadow_failure_does_not_block_archive` |
| Lag threshold pin (5min nominal, 60min SEV-2) | `tests::shadow_lag_thresholds_pinned` |
| Cross-tenant query fail-CLOSED in fake | `tests::in_memory_rejects_cross_tenant` |
| Cross-tenant insert violates RLS policy | `migrations/neon/tests/0001_rls_policy.sql` |
| Region misroute caught at factory layer | `tests::factory_rejects_region_mismatch` |
| ON CONFLICT DO NOTHING on duplicate seq | `tests::shadow_idempotent_replay` |
| CloudEvents emit on both paths | `tests::emits_synced_and_failed_events` |
| Wasm32 stub surfaces WasmOnly error | `tests::wasm32_stub_errors_consistently` |

## 12. References

- `archive_producer.rs` — R2 NDJSON canonical store (Wave 15)
- `migrations/neon/0001_audit_events_shadow.sql` — Schema + RLS policy
- `migrations/neon/0052_tenant_config_region.sql` — Per-tenant region pinning (Wave 21)
- `specs/_audits/sealed/2026-05-16-neon-shadow-real-driver.md` — Wave 20/21 closure design
- Kleppmann, M. (2017). *Designing Data-Intensive Applications*. O'Reilly. Ch. 11 (Stream Processing) on derived data systems and source-of-truth disciplines.
- CloudEvents Specification 1.0 — `https://cloudevents.io/`
- PostgreSQL Row Level Security — `https://www.postgresql.org/docs/current/ddl-rowsecurity.html`
