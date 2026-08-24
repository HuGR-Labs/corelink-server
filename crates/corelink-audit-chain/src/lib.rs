//! `corelink-audit-chain` — CloudEvents 1.0 audit emitter + per-region
//! BLAKE3 hash chain + daily verify primitive (WI-S09-004).
//!
//! # What this crate ships
//!
//! Per the corelink autonomous execution charter
//! (`trait-abstraction-defer`), this crate ships the **pure-logic
//! skeleton** of the audit-chain emit + verify primitive: trait surfaces
//! every production CF R2 PutObject + scheduled verify Cron + SIEM
//! fan-out via Cloudflare Queue will satisfy, plus an in-memory
//! orchestrator that exercises every load-bearing invariant the
//! production wiring relies on. Property tests pinned at 10k iter
//! against the orchestrator cover INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH;
//! per-tenant chain hash unbroken; daily verifier 7d clean per WI §6.1.5
//! DoD gate), INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (audit-of-audit fail-
//! closed envelope; emit BEFORE state mutation), INV-TENANT-ISOLATION
//! (per-tenant chain partitioning; cross-tenant slice rejected at the
//! verifier boundary), and the canonical 8-element CNCF subject
//! taxonomy per `observability_model.md §7` + sprint contract §5.4
//! R-S09-10.
//!
//! Specifically, the crate ships:
//!
//! 1. The [`event`] module ships [`AuditEvent`] (CloudEvents 1.0
//!    aligned: `specversion` / `type` / `source` / `subject` / `id` /
//!    `time_ms` / `datacontenttype` / `data` + CoreLink extensions
//!    `tenant_id` / `region` / `sequence_number` / `prev_hash`) +
//!    [`AuditEventKind`] `#[non_exhaustive]` 8-canonical CNCF subject
//!    taxonomy (`tenant` / `cas:put` / `cas:get` / `ac:lookup` /
//!    `gc:purge` / `auth:login` / `quota:exceeded` / `abuse:detected`)
//!    + [`ChainHash`] 32-byte BLAKE3 newtype (hex-rendered on the wire)
//!    + canonical NDJSON serializer.
//! 2. The [`chain`] module ships [`HashChainBuilder`] (per-tenant chain
//!    state machine: `head` + `next_sequence`; appends events
//!    enforcing sequence + `prev_hash` integrity at the boundary;
//!    BLAKE3-256 link hash via RFC 8785 JCS canonicalization) +
//!    [`compute_canonical_bytes`] / [`link_chain_hash`] /
//!    [`verify_chain_link`] primitives.
//! 3. The [`sink`] module ships [`R2AuditSink`] trait +
//!    [`InMemoryR2AuditSink`] orchestrator (canonicalize → audit emit
//!    BEFORE state mutation → R2 NDJSON write → chain head advance) +
//!    [`canonical_r2_key`] (canonical
//!    `audit/{tenant_id}/{date}/{seq:08}.cloudevent.ndjson` layout per
//!    WI §6.1.4) + [`canonical_date_yyyy_mm_dd`] pure-logic UTC date
//!    helper + [`PersistedAuditLine`] + [`CapturedR2AuditSink`] +
//!    [`FailingR2AuditSink`].
//! 4. The [`verifier`] module ships [`ChainVerifier`] (daily-verify
//!    routine; walks `[start, end]` slice; recomputes BLAKE3 links;
//!    fail-CLOSED on first mismatch with canonical
//!    `corelink.audit_chain.chain_break_detected` SEV-0 audit emit) +
//!    [`VerifyOutcome`] (`events_verified_count` /
//!    `last_verified_hash` / `first_break_at_seq`).
//! 5. The [`audit`] module ships [`AuditChainAuditEventType`]
//!    (`#[non_exhaustive]` 4-event taxonomy:
//!    `corelink.audit_chain.{event_appended, chain_verified_ok,
//!    chain_break_detected, sink_failure}`) +
//!    [`AuditChainAuditRecord`] + [`AuditChainAuditSink`] +
//!    [`InMemoryAuditChainAuditSink`] capture sink (fail-closed
//!    envelope per `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER`).
//! 6. The [`error`] module ships the canonical [`AuditChainError`]
//!    `#[non_exhaustive]` taxonomy (canonicalization /
//!    sequence-ordering / chain-break / tenant-isolation / audit /
//!    sink / internal).
//!
//! # Why audit-chain is `trait + fake` here, real CF R2 PutObject in
//! # WI-S09-007
//!
//! S-09 lands without Cloudflare R2 + DO + Cron bindings wired into CI
//! (no remote + Cloudflare Workers + R2 staging are HARD inflection
//! points per `corelink_autonomous_execution_charter.md`). The fake
//! covers the algorithmic invariants that a production binding bug
//! would expose: per-tenant chain partitioning + cross-tenant
//! isolation; BLAKE3 link recomputation determinism; tamper detection
//! at first divergence sequence; sequence monotonicity; genesis-zero
//! convention; JCS canonicalization byte-stability; audit-of-audit
//! fail-closed envelope on every decision arm. The live R2 PutObject
//! (with Object Lock Governance Mode 7y retention) + scheduled DO
//! verifier Cron + SIEM fan-out via Cloudflare Queue + Terraform IaC
//! integration tests run alongside WI-S09-007 (PRR ship gate).
//!
//! # Cripto-driven invariants enforced
//!
//! - INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH; sprint contract §8 NEW per
//!   §3.12; SOC 2 CC7.2 audit-chain-integrity discipline): per-tenant
//!   hash chain unbroken; daily verifier walks chain start → end +
//!   asserts every link's recomputed hash matches the claimed hash.
//!   Pinned by `prop_chain_verify_passes_on_unmodified` (10k iter PR
//!   gate; nightly 100k via `PROPTEST_CASES` env var).
//! - INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+ proven em Lote 6.2 from
//!   S-06 inheritance): R2 Object Lock Governance Mode 7y enforces
//!   append-only at storage level; tampering detected at verify time
//!   via hash chain integrity. Pinned by `prop_chain_append_only`.
//! - INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from S-07 P1-1 fix
//!   plus Lote 10.6bis pattern): audit-of-audit emit BEFORE state
//!   mutation on every decision arm (`event_appended`,
//!   `chain_verified_ok`, `chain_break_detected`, `sink_failure`);
//!   audit failure aborts the chain emit + returns a typed error.
//!   Pinned by `prop_audit_emit_per_event_type`.
//! - INV-TENANT-ISOLATION (CRITICAL, TLA+; lift from invariant
//!   registry §3.7): per-tenant chain partitioning; tenant A's chain
//!   never references tenant B's events. Pinned by
//!   `prop_tenant_isolation`.
//! - JCS canonicalization byte-stability (informational invariant; RFC
//!   8785 canonical): the same `AuditEvent` value produces the same
//!   canonical bytes across all platforms, all serde versions
//!   compatible with `serde_jcs = "0.2"`, and all process invocations.
//!   Pinned by `prop_jcs_canonicalization_deterministic`.
//! - Sequence monotonicity (informational invariant): chain sequence
//!   numbers strictly increase by 1 starting from `0`. Pinned by
//!   `prop_chain_sequence_monotonic`.
//! - Genesis zero-hash convention (informational invariant): the first
//!   event in any chain has `prev_hash == [0u8; 32]`. Pinned by
//!   `prop_genesis_zero_prev_hash`.
//! - Tamper detection at first divergence (informational invariant):
//!   modifying any event's data / `prev_hash` / sequence surfaces a
//!   `ChainBreak` at the first divergent sequence. Pinned by
//!   `prop_chain_break_detected_on_tamper`.
//!
//! # Production wiring (deferred to WI-S09-007)
//!
//! - CF R2 PutObject + Object Lock Governance Mode 7y retention via
//!   `worker::send_future` fire-and-forget per Lote 10.7bis R5 P0-3
//!   (NEVER `tokio::spawn`).
//! - Terraform IaC for R2 bucket Object Lock + lifecycle (per WI
//!   §6.1.3).
//! - DO `AuditChainVerifier-<region>` per-region; cron alarm 24h at
//!   UTC 02:00 (low-traffic window); alarm re-arm AT START (Lote
//!   10.4bis lesson).
//! - SIEM fan-out via Cloudflare Queue → customer webhook (best-effort;
//!   NOT fail-closed; audit already committed em R2 quando emit returns
//!   Ok).
//! - CI hook `.github/workflows/audit-chain-cloudevents-gate.yml`
//!   cloudevents-cli validation against the canonical schema.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(missing_debug_implementations)]

pub mod archive_producer;
pub mod audit;
pub mod chain;
pub mod error;
pub mod event;
pub mod exporter;
pub mod neon_shadow;
pub mod sealed_archive;
pub mod sink;
pub mod verifier;

pub use archive_producer::{
    archive_chunk_key, chain_hashes_eq_ct, ArchiveProducer, ArchiveProducerError, ArchiveReceipt,
    ArchiveSink, CapturedChunk, FailingArchiveSink, FlushPolicy, InMemoryArchiveSink,
    DEFAULT_FLUSH_AFTER_MS, DEFAULT_MAX_BYTES_PER_CHUNK, DEFAULT_MAX_EVENTS_PER_CHUNK,
};
pub use audit::{
    canonical_audit_event_strings, AuditChainAuditEmitError, AuditChainAuditEventType,
    AuditChainAuditRecord, AuditChainAuditSink, FailingAuditChainAuditSink,
    InMemoryAuditChainAuditSink,
};
pub use chain::{
    compute_canonical_bytes, link_chain_hash, link_chain_hash_from_canonical, verify_chain_link,
    HashChainBuilder,
};
pub use error::{AuditChainAuditSinkError, AuditChainError, R2AuditSinkError};
pub use event::{
    canonical_audit_event_kinds, AuditEvent, AuditEventKind, ChainHash,
    CLOUDEVENTS_DATACONTENTTYPE, CLOUDEVENTS_SPECVERSION, EVENT_TYPE_PREFIX, GENESIS_PREV_HASH,
    GENESIS_SEQUENCE_NUMBER,
};
pub use neon_shadow::real::{
    EnvVarResolver, ExecutorParam, ExecutorRow, InMemoryExecutor, NeonError, NeonExecutor,
    NeonProjectResolver, RealNeonShadowSink, StaticResolver, SQL_BEGIN_TXN, SQL_COMMIT_TXN,
    SQL_INSERT_SHADOW_ROW, SQL_QUERY_EVENT_COUNT, SQL_QUERY_EVENT_COUNT_FILTERED,
    SQL_QUERY_TIMELINE, SQL_RECONCILE_COUNT, SQL_SET_RLS_TENANT_GUC,
};
pub use neon_shadow::tenant_region::{
    parse_region_label, D1TenantRegionResolver, InMemoryTenantRegionResolver, TenantConfigStore,
    TenantRegionError, TenantRegionResolver,
};
pub use neon_shadow::{
    EventCountBucket, InMemoryNeonShadowSink, InMemoryShadowSyncAuditSink, NeonShadowError,
    NeonShadowSink, ShadowEventRow, ShadowSyncAuditRow, ShadowSyncAuditSink, ShadowSyncReceipt,
    TimelineBucket, EVENT_TYPE_SHADOW_SYNCED, EVENT_TYPE_SHADOW_SYNC_FAILED,
    SHADOW_LAG_NOMINAL_MAX_MS, SHADOW_LAG_SEV2_THRESHOLD_MS,
};
// Wave-25 follow-on: re-export `Region` from `corelink-analytics` so
// downstream callers of `D1TenantRegionResolver` (notably
// `corelink-clerk-cf::prod_wiring::build_tenant_region_resolver`)
// don't need to pull `corelink-analytics` as a direct dep alongside
// `corelink-audit-chain` — the resolver's `fallback` argument is a
// `Region`, so a clean wire path needs the type in scope at the call
// site. The canonical type still lives in `corelink-analytics`; this
// is a re-export only (no behaviour change).
pub use corelink_analytics::Region;
pub use exporter::{
    hashes_eq_ct, verify_export_result, verify_inclusion_proof, AuditExporter, ExportAuditRecord,
    ExportManifest, ExportResult, ExportWindow, ExportedAuditEvent, InMemoryAuditExporter,
    InclusionProof, ProofSibling,
};
pub use sealed_archive::{
    sealed_chunk_key, serialize_chunk, split_into_chunks, split_verifying_prefix, verify_chunk,
    PrefixBreak, SealedArchiveError, SealedArchiveLine, DEFAULT_SEALED_MAX_BYTES_PER_CHUNK,
    DEFAULT_SEALED_MAX_LINES_PER_CHUNK, SEALED_LINE_SCHEMA,
};
pub use sink::{
    canonical_date_yyyy_mm_dd, canonical_r2_key, CapturedR2AuditSink, FailingR2AuditSink,
    InMemoryR2AuditSink, PersistedAuditLine, R2AuditSink,
};
pub use verifier::{ChainVerifier, VerifyOutcome};

/// Crate canonical version constant.
#[must_use]
pub const fn audit_chain_schema_version() -> u32 {
    1
}
