//! Wave-20 ephemeral-Postgres integration suite for the production
//! [`RealNeonShadowSink`] driver.
//!
//! Replaces the 5 wave-19 `#[ignore]`-by-default Neon-staging tests
//! with a self-contained testcontainer harness. Every test boots its
//! own `postgres:16-alpine` container via
//! [`harness::pg_container::spawn_ephemeral_postgres`] — no external
//! Neon project, no OIDC, no per-CI secret rotation.
//!
//! ## Charter
//!
//! Every test is annotated `#[cfg_attr(not(feature = "live-pg"),
//! ignore)]` so the default `cargo test -p corelink-audit-chain` run
//! skips the suite when Docker is unavailable; `--features live-pg`
//! opts in. The suite contract is:
//!
//! 1. **`real_sync_chunk_persists_and_queries_back`** — end-to-end
//!    INSERT + aggregate query against real Postgres.
//! 2. **`real_idempotent_insert_on_retry`** — `ON CONFLICT
//!    (tenant_id, seq) DO NOTHING` semantics; INV-AUDIT-APPEND-ONLY
//!    + retry-safe.
//! 3. **`real_rls_enforces_tenant_isolation`** — tenant_b SELECT
//!    returns zero rows of tenant_a's data even when querying the
//!    same table; load-bearing INV-AUTH-SCHEMA-RLS-DEFAULT-ON.
//! 4. **`real_sync_chunk_rejects_cross_tenant_at_pre_check`** —
//!    constant-time tenant_eq_ct fires BEFORE any SQL is emitted;
//!    defense in depth on top of RLS.
//! 5. **`real_aggregate_timeline_matches_in_memory_fake`** — the
//!    SQL `aggregate_timeline` returns the same bucket layout as
//!    the in-memory fake (cross-driver parity).
//!
//! Canonical reference: `specs/_audits/2026-05-16-neon-shadow-pg-testharness.md`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code
)]

// Inline `harness::pg_container` re-export. The harness file lives at
// `tests/harness/pg_container.rs` (per the wave-20 audit doc), but
// Rust's tests-dir module resolution + the workspace
// `clippy::mod_module_files = "deny"` lint forbid the obvious
// `tests/harness/mod.rs` layout, so we wire it up here via `#[path]`.
mod harness {
    #![allow(dead_code)]
    // Inside an inline `mod harness { ... }`, `#[path]` is resolved
    // relative to the implicit `harness/` directory next to the
    // containing file (`tests/`), so `pg_container.rs` lands at the
    // canonical `tests/harness/pg_container.rs` path.
    #[cfg(feature = "live-pg")]
    #[path = "pg_container.rs"]
    pub mod pg_container;
}

#[cfg(feature = "live-pg")]
use std::sync::Arc;

#[cfg(feature = "live-pg")]
use corelink_analytics::Region;
#[cfg(feature = "live-pg")]
use corelink_audit_chain::{
    ArchiveReceipt, ChainHash, EventCountBucket, InMemoryNeonShadowSink,
    InMemoryShadowSyncAuditSink, NeonExecutor, NeonShadowError, NeonShadowSink,
    RealNeonShadowSink, ShadowEventRow, SQL_BEGIN_TXN, SQL_COMMIT_TXN,
    SQL_INSERT_SHADOW_ROW, SQL_SET_RLS_TENANT_GUC,
};
#[cfg(feature = "live-pg")]
use uuid::Uuid;

#[cfg(feature = "live-pg")]
use harness::pg_container::{
    build_runtime, count_visible_rows, spawn_ephemeral_postgres, TokioPostgresExecutor,
};

#[cfg(feature = "live-pg")]
const BASE_MS: u64 = 1_700_000_000_000;

#[cfg(feature = "live-pg")]
fn dummy_receipt(tenant: Uuid, first: u64, last: u64) -> ArchiveReceipt {
    ArchiveReceipt {
        r2_key: format!("audit/2026/05/16/{:08}.ndjson", first),
        tenant_id: tenant,
        first_event_time_ms: BASE_MS,
        last_event_time_ms: BASE_MS + (last - first) * 10,
        first_sequence_number: first,
        last_sequence_number: last,
        prev_hash_anchor: ChainHash::genesis(),
        chain_head_after: ChainHash([0xCD; 32]),
        bytes_written: 100,
        events_written: last - first + 1,
    }
}

#[cfg(feature = "live-pg")]
fn make_row(tenant: Uuid, seq: u64, time_ms: u64, ty: &str, region: Region) -> ShadowEventRow {
    // ShadowEventRow is `#[non_exhaustive]`; use the canonical
    // constructor (added wave-18 for exactly this test-harness use
    // case — see `crates/corelink-audit-chain/src/neon_shadow.rs:162`).
    ShadowEventRow::new(
        tenant,
        seq,
        time_ms,
        ty.to_string(),
        ChainHash::genesis(),
        ChainHash([(seq as u8); 32]),
        region,
        format!("{{\"i\":{seq}}}"),
    )
}

// ---------------------------------------------------------------------------
// Test 1 — end-to-end INSERT + aggregate query
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(not(feature = "live-pg"), ignore)]
fn real_sync_chunk_persists_and_queries_back() {
    #[cfg(feature = "live-pg")]
    {
        let rt = build_runtime();
        let harness = rt.block_on(spawn_ephemeral_postgres(Arc::clone(&rt)));
        let tenant = Uuid::now_v7();
        let region = Region::Iad;

        let exec: Arc<dyn NeonExecutor> = Arc::new(TokioPostgresExecutor::new(
            harness.client(),
            Arc::clone(&rt),
        ));
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, region, exec, audit.clone());

        let rows = (0..10)
            .map(|i| make_row(tenant, i, BASE_MS + i * 10, "cas.put.v1", region))
            .collect::<Vec<_>>();
        let receipt = dummy_receipt(tenant, 0, 9);
        let r = sink
            .sync_chunk(&receipt, &rows, BASE_MS + 500)
            .expect("sync_chunk");
        assert_eq!(r.rows_persisted, 10);
        assert_eq!(r.region, region);

        // Aggregate event-count should return 1 bucket × 10 events.
        let buckets = sink
            .aggregate_event_count(BASE_MS, BASE_MS + 1_000, None)
            .expect("aggregate");
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].event_type, "cas.put.v1");
        assert_eq!(buckets[0].count, 10);

        // Filtered query for an absent event_type should return 0 buckets.
        let none = sink
            .aggregate_event_count(BASE_MS, BASE_MS + 1_000, Some("absent.kind"))
            .expect("filtered aggregate");
        assert!(none.is_empty());

        // Audit emits: 1 SHADOW_SYNCED row.
        let snap = audit.snapshot().expect("snap");
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].event_type, "corelink.audit.neon_shadow_synced.v1");
    }
}

// ---------------------------------------------------------------------------
// Test 2 — idempotent INSERT on retry
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(not(feature = "live-pg"), ignore)]
fn real_idempotent_insert_on_retry() {
    #[cfg(feature = "live-pg")]
    {
        let rt = build_runtime();
        let harness = rt.block_on(spawn_ephemeral_postgres(Arc::clone(&rt)));
        let tenant = Uuid::now_v7();
        let region = Region::Iad;

        let exec: Arc<dyn NeonExecutor> = Arc::new(TokioPostgresExecutor::new(
            harness.client(),
            Arc::clone(&rt),
        ));
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(tenant, region, exec, audit);

        let rows = (0..5)
            .map(|i| make_row(tenant, i, BASE_MS + i * 10, "cas.get.v1", region))
            .collect::<Vec<_>>();
        let receipt = dummy_receipt(tenant, 0, 4);

        let r1 = sink.sync_chunk(&receipt, &rows, BASE_MS + 100).expect("1st");
        assert_eq!(r1.rows_persisted, 5);

        // 2nd call: same (tenant, seq) PK → ON CONFLICT DO NOTHING.
        // The sink reports `rows_persisted = rows.len()` (the LOGICAL
        // count of rows the caller handed in); the row count in the
        // table stays at 5.
        let r2 = sink.sync_chunk(&receipt, &rows, BASE_MS + 200).expect("2nd");
        assert_eq!(r2.rows_persisted, 5);

        // Query back: still 5 rows total.
        let buckets = sink
            .aggregate_event_count(BASE_MS, BASE_MS + 1_000, None)
            .expect("aggregate");
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].count, 5, "duplicate INSERTs must be deduped");
    }
}

// ---------------------------------------------------------------------------
// Test 3 — RLS enforces tenant isolation (SQL layer)
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(not(feature = "live-pg"), ignore)]
fn real_rls_enforces_tenant_isolation() {
    #[cfg(feature = "live-pg")]
    {
        let rt = build_runtime();
        let harness = rt.block_on(spawn_ephemeral_postgres(Arc::clone(&rt)));
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let region = Region::Iad;

        // tenant_a persists 3 rows via the production sink.
        let exec_a: Arc<dyn NeonExecutor> = Arc::new(TokioPostgresExecutor::new(
            harness.client(),
            Arc::clone(&rt),
        ));
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink_a = RealNeonShadowSink::new(tenant_a, region, exec_a, audit);
        let rows_a = (0..3)
            .map(|i| make_row(tenant_a, i, BASE_MS + i * 10, "cas.put.v1", region))
            .collect::<Vec<_>>();
        sink_a
            .sync_chunk(&dummy_receipt(tenant_a, 0, 2), &rows_a, BASE_MS + 50)
            .expect("tenant_a sync");

        // Cross-tenant SELECT under RLS: a tenant_b-bound txn must
        // see ZERO rows of tenant_a's data.
        let count_b = rt
            .block_on(count_visible_rows(&harness, tenant_b))
            .expect("tenant_b count");
        assert_eq!(count_b, 0, "tenant_b must NOT see tenant_a rows under RLS");

        // tenant_a's own bound query sees 3 rows.
        let count_a = rt
            .block_on(count_visible_rows(&harness, tenant_a))
            .expect("tenant_a count");
        assert_eq!(count_a, 3);
    }
}

// ---------------------------------------------------------------------------
// Test 4 — sync_chunk rejects cross-tenant at the pre-check
// (defense-in-depth ABOVE the SQL RLS layer)
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(not(feature = "live-pg"), ignore)]
fn real_sync_chunk_rejects_cross_tenant_at_pre_check() {
    #[cfg(feature = "live-pg")]
    {
        let rt = build_runtime();
        let harness = rt.block_on(spawn_ephemeral_postgres(Arc::clone(&rt)));
        let tenant_a = Uuid::now_v7();
        let tenant_b = Uuid::now_v7();
        let region = Region::Iad;

        let exec = Arc::new(TokioPostgresExecutor::new(
            harness.client(),
            Arc::clone(&rt),
        ));
        let trace_handle: Arc<TokioPostgresExecutor> = Arc::clone(&exec);
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let sink = RealNeonShadowSink::new(
            tenant_a,
            region,
            exec as Arc<dyn NeonExecutor>,
            audit.clone(),
        );

        let cross_rows = vec![make_row(tenant_b, 0, BASE_MS, "x", region)];
        let err = sink
            .sync_chunk(
                &dummy_receipt(tenant_b, 0, 0),
                &cross_rows,
                BASE_MS + 10,
            )
            .expect_err("must reject cross-tenant");
        assert!(matches!(err, NeonShadowError::TenantIsolationViolation { .. }));

        // No SQL was emitted to Postgres — the pre-check fires
        // BEFORE BEGIN. The SQL trace must be empty.
        let trace = trace_handle.take_trace();
        assert!(
            trace.is_empty(),
            "no SQL must be emitted on cross-tenant; got {:?}",
            trace
        );

        // SHADOW_SYNC_FAILED audit row at SEV-2.
        let snap = audit.snapshot().expect("snap");
        assert_eq!(snap.len(), 1);
        assert_eq!(
            snap[0].event_type,
            "corelink.audit.neon_shadow_sync_failed.v1"
        );
        assert_eq!(snap[0].sev, "sev-2");

        // Belt-and-suspenders: nothing landed in the table either.
        let count = rt
            .block_on(count_visible_rows(&harness, tenant_a))
            .expect("count");
        assert_eq!(count, 0);
    }
}

// ---------------------------------------------------------------------------
// Test 5 — aggregate_timeline against real Postgres matches the
// in-memory fake byte-for-byte
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(not(feature = "live-pg"), ignore)]
fn real_aggregate_timeline_matches_in_memory_fake() {
    #[cfg(feature = "live-pg")]
    {
        let rt = build_runtime();
        let harness = rt.block_on(spawn_ephemeral_postgres(Arc::clone(&rt)));
        let tenant = Uuid::now_v7();
        let region = Region::Iad;

        // Real driver against ephemeral Postgres.
        let exec: Arc<dyn NeonExecutor> = Arc::new(TokioPostgresExecutor::new(
            harness.client(),
            Arc::clone(&rt),
        ));
        let audit = Arc::new(InMemoryShadowSyncAuditSink::new());
        let real_sink = RealNeonShadowSink::new(tenant, region, exec, audit.clone());

        // In-memory parity sink (canonical fake).
        let fake_sink = InMemoryNeonShadowSink::new(tenant, region, audit);

        // Build 20 events spaced 10ms apart starting at BASE_MS.
        let rows = (0..20)
            .map(|i| make_row(tenant, i, BASE_MS + i * 10, "cas.put.v1", region))
            .collect::<Vec<_>>();
        let receipt = dummy_receipt(tenant, 0, 19);

        real_sink
            .sync_chunk(&receipt, &rows, BASE_MS + 1_000)
            .expect("real sync");
        fake_sink
            .sync_chunk(&receipt, &rows, BASE_MS + 1_000)
            .expect("fake sync");

        // 4 buckets × 50ms granularity over a 200ms window → 5 rows each.
        let real_buckets = real_sink
            .aggregate_timeline(BASE_MS, BASE_MS + 200, 50)
            .expect("real timeline");
        let fake_buckets = fake_sink
            .aggregate_timeline(BASE_MS, BASE_MS + 200, 50)
            .expect("fake timeline");

        assert_eq!(
            real_buckets.len(),
            fake_buckets.len(),
            "bucket count mismatch real={:?} fake={:?}",
            real_buckets,
            fake_buckets
        );
        for (i, (r, f)) in real_buckets
            .iter()
            .zip(fake_buckets.iter())
            .enumerate()
        {
            assert_eq!(r.bucket_start_ms, f.bucket_start_ms, "bucket[{i}] start");
            assert_eq!(r.count, f.count, "bucket[{i}] count");
        }

        // Same parity check for aggregate_event_count.
        let real_counts: Vec<EventCountBucket> = real_sink
            .aggregate_event_count(BASE_MS, BASE_MS + 200, None)
            .expect("real counts");
        let fake_counts: Vec<EventCountBucket> = fake_sink
            .aggregate_event_count(BASE_MS, BASE_MS + 200, None)
            .expect("fake counts");
        assert_eq!(real_counts.len(), fake_counts.len());
        for (r, f) in real_counts.iter().zip(fake_counts.iter()) {
            assert_eq!(r.event_type, f.event_type);
            assert_eq!(r.count, f.count);
        }
    }
}

// Module-load-time witness: a compile-only assertion that the
// `SQL_*` constants the harness exercises are the same ones the
// production sink emits. If any const goes missing the test crate
// stops compiling.
#[cfg(feature = "live-pg")]
const _: &str = SQL_BEGIN_TXN;
#[cfg(feature = "live-pg")]
const _: &str = SQL_SET_RLS_TENANT_GUC;
#[cfg(feature = "live-pg")]
const _: &str = SQL_INSERT_SHADOW_ROW;
#[cfg(feature = "live-pg")]
const _: &str = SQL_COMMIT_TXN;
