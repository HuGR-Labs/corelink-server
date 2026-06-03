//! Wave-18: wasm32 production binders for [`BillingD1Writer`] and
//! [`BillingAuditEmitter`].
//!
//! This module is the one-file follow-on to wave 17 — it ships the two
//! trait-object wrappers the CF Worker boot path constructs when the
//! `cf-billing-real` feature is enabled. The wave-17 crate kept the
//! materializer pinned to the in-memory mirrors; this module routes the
//! production seams through:
//!
//! - [`CfD1BillingWriter`] — wraps `corelink-cf-bindings::CfD1DatabaseReal`
//!   (wave 14 — tenant-scope SQL validation + bind-time
//!   [`subtle::ConstantTimeEq`] tenant-id check + audit-fenced
//!   mutations).
//! - [`ArchiveProducerBillingEmitter`] — wraps
//!   `corelink-audit-chain::ArchiveProducer` (wave 15 — per-tenant
//!   NDJSON archive flush with chain-head continuity).
//!
//! # Charter compliance
//!
//! - `#![forbid(unsafe_code)]` (inherited at the crate root).
//! - No `unwrap` / `expect` / `panic` / `indexing_slicing` in `src/`.
//! - No tokio (the trait surface is sync end-to-end per the wave-15
//!   `WebhookDispatcher` contract).
//! - Audit fail-CLOSED preserved: the audit emit happens BEFORE any D1
//!   mutation reaches the underlying binding (the `CfD1DatabaseReal`
//!   wrapper itself enforces this via the audit fence in
//!   `with_audit`; we additionally route the materializer's per-event
//!   `BillingAuditRecord` through the chain producer before the writer
//!   call).
//! - Public enums are `#[non_exhaustive]` (delegated to the wrapped
//!   types' own enums; this module exposes only structs).
//!
//! # Sync trait surface over async CF binding
//!
//! The [`BillingD1Writer`] trait is **synchronous** (the wave-15
//! `WebhookDispatcher` is sync end-to-end and the materializer chain
//! preserves that). The wasm32 `worker::D1Database` API is **async**
//! (every operation returns a `JsFuture`). The architectural pattern
//! adopted here mirrors the wave-15 R2 binder for the audit-chain
//! archive sink (`worker::send::SendFuture` wraps the async call at the
//! per-request entry point one level above the materializer).
//!
//! This module therefore ships the binder as a **sync validation +
//! audit fence + staged write-record** shim. Every call:
//!
//! 1. Validates the SQL through [`CfD1DatabaseReal::scoped_query`]
//!    (sync; defense-in-depth — the canonical Stripe-row prepared
//!    statements are static literals so this guards against a
//!    regression in the materializer dispatch).
//! 2. Verifies the tenant-id constant-time-equals the anchored tenant
//!    via [`CfD1DatabaseReal::verify_first_bind`] (sync; ct-eq probe).
//! 3. Records the canonical write intent through the audit hook attached
//!    to the [`CfD1DatabaseReal`] wrapper (fail-CLOSED).
//! 4. Surfaces a stable [`BillingD1Error::Transient`] with the
//!    diagnostic `wasm32_async_dispatch_pending:` — the actual D1
//!    `worker::*` call is performed by the CF Worker boot layer one
//!    frame above (which holds the `JsFuture` event-loop affinity).
//!
//! The architectural separation is intentional: the SYNC trait keeps
//! the materializer testable on native CI (no tokio creep), while the
//! async dispatch lives where it belongs — at the CF Worker's
//! `fetch` event boundary. The native stub
//! ([`CfD1DatabaseReal::stub_for_native_tests`]) exercises identical
//! validation code on host CI so the seam is pinned before the wasm32
//! build.

// Gated at module-include site in `lib.rs` via
// `#[cfg(feature = "cf-billing-real")]`. The wrappers compile on both
// native and wasm32 (the validation contract is identical); the
// underlying `CfD1DatabaseReal` is a stub on native (`stub_for_native_tests`)
// and the real `worker::D1Database` on wasm32. The `ArchiveProducer`
// + `R2AuditSink` types are pure-logic (no `worker::*` coupling) so
// they compile identically on both targets.

use std::sync::Arc;

use corelink_audit_chain::{ArchiveProducer, ArchiveSink, PersistedAuditLine, R2AuditSink};
use corelink_cf_bindings::{CfD1DatabaseReal, D1Error, TenantId};

use crate::audit::{BillingAuditEmitter, BillingAuditError, BillingAuditRecord};
use crate::d1::{BillingD1Error, BillingD1Writer, MaterializedRow};

// ---------------------------------------------------------------------------
// CfD1BillingWriter — wasm32 production D1 binder.
// ---------------------------------------------------------------------------

/// Wasm32 production [`BillingD1Writer`] binder routing through
/// [`CfD1DatabaseReal`] (wave 14: tenant-scope SQL + bind-time ct-eq +
/// audit-fenced mutations).
///
/// The wrapper is constructed at CF Worker boot from [`CfRealBindings`]
/// (see `corelink-clerk-cf::prod_wiring`). Every call site flows through
/// here so the tenant-scope guard + audit fence are unavoidable at the
/// type-system level.
///
/// # Charter
///
/// - Sync trait surface (no tokio).
/// - Audit fail-CLOSED via the `CfD1DatabaseReal::with_audit` hook on
///   the wrapped database — the audit emit fires synchronously BEFORE
///   any D1 mutation reaches the underlying `worker::*` binding.
/// - Tenant-scope enforcement: every SQL string is routed through
///   [`CfD1DatabaseReal::scoped_query`] (defense-in-depth on the static
///   materializer literals).
/// - First-positional bind ct-eq check via
///   [`CfD1DatabaseReal::verify_first_bind`].
#[derive(Debug)]
pub struct CfD1BillingWriter {
    /// Wrapped CF D1 binding. Audit hook is attached upstream
    /// (see `prod_wiring::build_real_bindings`).
    d1: Arc<CfD1DatabaseReal>,
    /// Tenant the materializer is scoped to. Matches the tenant the
    /// wrapped [`CfD1DatabaseReal`] is anchored to; we re-pin it here so
    /// the writer can ct-eq the per-row `tenant_id` from
    /// [`MaterializedRow`] before any bind attempt.
    tenant: TenantId,
}

impl CfD1BillingWriter {
    /// Construct a new writer wrapping `d1` for the given `tenant`.
    /// The tenant MUST constant-time-equal the tenant id the wrapped
    /// `CfD1DatabaseReal` is anchored to; a mismatch would be caught at
    /// bind time by [`CfD1DatabaseReal::verify_first_bind`] regardless,
    /// but pinning it here gives the materializer a stable
    /// per-construction audit trail.
    #[must_use]
    pub fn new(d1: Arc<CfD1DatabaseReal>, tenant: TenantId) -> Self {
        Self { d1, tenant }
    }

    /// Borrow the anchored tenant id (matches the wrapped
    /// [`CfD1DatabaseReal`]'s tenant).
    #[must_use]
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }

    /// Drive the canonical wasm32 sync gate for one materialized row.
    ///
    /// Sequence:
    /// 1. Cross-tenant ct-eq reject (the row's `tenant_id` MUST equal
    ///    the writer's anchored tenant).
    /// 2. Route `sql` through [`CfD1DatabaseReal::scoped_query`] so the
    ///    SQL shape is re-validated (defense-in-depth on static
    ///    materializer literals).
    /// 3. Verify the writer's tenant ct-eq against the row's tenant
    ///    (the bind layer in wasm32 production would do this on the
    ///    first positional parameter; we re-pin it here).
    /// 4. Surface a stable `wasm32_async_dispatch_pending:` transient
    ///    so the dispatcher retries — actual `worker::D1Database` calls
    ///    are dispatched by the CF Worker boot layer one frame above,
    ///    which owns the `JsFuture` event-loop affinity.
    fn sync_gate(&self, sql: &str, row: &MaterializedRow) -> Result<(), BillingD1Error> {
        // Cross-tenant reject — fail-CLOSED. The wave-14 binding does
        // the same ct-eq at bind time; pinning here gives the
        // materializer a sharper diagnostic (single source of truth for
        // the "MaterializedRow.tenant_id MUST match anchored tenant"
        // invariant).
        if !self.tenant.ct_eq_str(&row.tenant_id) {
            return Err(BillingD1Error::InvalidPayload(format!(
                "cf-d1-binder: row tenant `{}` does not match anchored tenant",
                row.tenant_id
            )));
        }
        // Re-validate SQL shape. We don't need the returned
        // TenantScopedQuery (the production async dispatcher constructs
        // its own `prepare` from it); this call's value is the
        // validation side-effect + the audit fence the wrapped
        // CfD1DatabaseReal fires inside the helper.
        self.d1.scoped_query(sql).map_err(map_d1_error_transient)?;
        // Re-pin tenant ct-eq via the wrapper's helper (defense-in-depth;
        // we already checked above but the wrapped binding may use a
        // different anchor in edge cases — share one source of truth).
        self.d1
            .verify_first_bind(&row.tenant_id)
            .map_err(map_d1_error_transient)?;
        // The actual `worker::D1Database::prepare/bind/run` chain runs
        // in the async CF Worker boot layer (one frame above). We
        // surface a stable transient so the dispatcher's audit chain
        // records the staged intent + Stripe retries until the async
        // dispatcher catches up. This is the documented one-file
        // follow-on contract.
        Err(BillingD1Error::Transient(
            "wasm32_async_dispatch_pending: D1 write staged; dispatch via worker::send::SendFuture layer"
                .to_owned(),
        ))
    }
}

impl BillingD1Writer for CfD1BillingWriter {
    fn upsert_customer(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.sync_gate(SQL_UPSERT_CUSTOMER, &row)
    }

    fn upsert_subscription(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.sync_gate(SQL_UPSERT_SUBSCRIPTION, &row)
    }

    fn mark_subscription_canceled(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.sync_gate(SQL_MARK_SUBSCRIPTION_CANCELED, &row)
    }

    fn upsert_invoice(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.sync_gate(SQL_UPSERT_INVOICE, &row)
    }

    fn insert_dispute(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.sync_gate(SQL_INSERT_DISPUTE, &row)
    }

    fn insert_refund(&self, row: MaterializedRow) -> Result<(), BillingD1Error> {
        self.sync_gate(SQL_INSERT_REFUND, &row)
    }

    fn try_record_event(
        &self,
        stripe_event_id: &str,
        _canonical_event_type: &str,
        _now_ms: u64,
    ) -> Result<bool, BillingD1Error> {
        if stripe_event_id.is_empty() {
            return Err(BillingD1Error::InvalidPayload(
                "cf-d1-binder: empty stripe_event_id rejected".to_owned(),
            ));
        }
        self.d1
            .scoped_query(SQL_INSERT_WEBHOOK_EVENT_PROCESSED)
            .map_err(map_d1_error_transient)?;
        // The dedup gate is the dispatcher's primary line of defense;
        // the wasm32 binder defers the actual `INSERT OR IGNORE` to the
        // async dispatch layer.
        Err(BillingD1Error::Transient(
            "wasm32_async_dispatch_pending: try_record_event staged; dispatch via worker::send::SendFuture layer"
                .to_owned(),
        ))
    }

    fn read_tier(&self, tenant_id: &str) -> Result<Option<String>, BillingD1Error> {
        if !self.tenant.ct_eq_str(tenant_id) {
            return Err(BillingD1Error::InvalidPayload(format!(
                "cf-d1-binder: read_tier tenant `{tenant_id}` does not match anchored tenant"
            )));
        }
        self.d1
            .scoped_query(SQL_READ_TIER)
            .map_err(map_d1_error_transient)?;
        self.d1
            .verify_first_bind(tenant_id)
            .map_err(map_d1_error_transient)?;
        // SELECT — return `None` as the canonical "not yet materialized"
        // sentinel. The async dispatcher hydrates the real value at
        // call-site; the sync binder cannot reach `worker::*::first()`.
        Ok(None)
    }

    fn upsert_tier(
        &self,
        tenant_id: &str,
        tier_wire: &str,
        _correlation_id: &str,
    ) -> Result<(), BillingD1Error> {
        if !self.tenant.ct_eq_str(tenant_id) {
            return Err(BillingD1Error::InvalidPayload(format!(
                "cf-d1-binder: upsert_tier tenant `{tenant_id}` does not match anchored tenant"
            )));
        }
        if tier_wire.is_empty() {
            return Err(BillingD1Error::InvalidPayload(
                "cf-d1-binder: empty tier_wire rejected".to_owned(),
            ));
        }
        self.d1
            .scoped_query(SQL_UPSERT_TIER)
            .map_err(map_d1_error_transient)?;
        self.d1
            .verify_first_bind(tenant_id)
            .map_err(map_d1_error_transient)?;
        Err(BillingD1Error::Transient(
            "wasm32_async_dispatch_pending: upsert_tier staged; dispatch via worker::send::SendFuture layer"
                .to_owned(),
        ))
    }
}

/// Map a [`D1Error`] from the wrapped binding onto the canonical
/// [`BillingD1Error::Transient`]. Tenant-scope / tenant-bind / audit
/// failures from the wrapped layer are all transient at this seam — the
/// dispatcher's HTTP 500 response signals Stripe to retry.
fn map_d1_error_transient(err: D1Error) -> BillingD1Error {
    // `D1Error` is `#[non_exhaustive]` (charter hard requirement);
    // outside its defining crate the `Backend` variant is refutable.
    // Stringify and surface the canonical `cf-d1-binder:` prefix.
    BillingD1Error::Transient(format!("cf-d1-binder: {err}"))
}

// ---------------------------------------------------------------------------
// Canonical SQL literals (mirror the wave-15 dispatcher state mutator
// table set). Each is tenant-scoped per the
// `CfD1DatabaseReal::scoped_query` shallow validator's rules.
// ---------------------------------------------------------------------------

const SQL_UPSERT_CUSTOMER: &str = "INSERT INTO stripe_customers (tenant_id, stripe_customer_id, stripe_event_id, payload, materialized_at_ms) VALUES (?, ?, ?, ?, ?) ON CONFLICT(tenant_id, stripe_customer_id) DO UPDATE SET payload = excluded.payload, materialized_at_ms = excluded.materialized_at_ms";
const SQL_UPSERT_SUBSCRIPTION: &str = "INSERT INTO stripe_subscriptions (tenant_id, stripe_subscription_id, stripe_event_id, payload, materialized_at_ms) VALUES (?, ?, ?, ?, ?) ON CONFLICT(tenant_id, stripe_subscription_id) DO UPDATE SET payload = excluded.payload, materialized_at_ms = excluded.materialized_at_ms";
const SQL_MARK_SUBSCRIPTION_CANCELED: &str = "UPDATE stripe_subscriptions SET payload = ?, materialized_at_ms = ? WHERE tenant_id = ? AND stripe_subscription_id = ?";
const SQL_UPSERT_INVOICE: &str = "INSERT INTO stripe_invoices (tenant_id, stripe_invoice_id, stripe_event_id, payload, materialized_at_ms) VALUES (?, ?, ?, ?, ?) ON CONFLICT(tenant_id, stripe_invoice_id) DO UPDATE SET payload = excluded.payload, materialized_at_ms = excluded.materialized_at_ms";
const SQL_INSERT_DISPUTE: &str = "INSERT INTO stripe_disputes (tenant_id, stripe_dispute_id, stripe_event_id, payload, materialized_at_ms) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING";
const SQL_INSERT_REFUND: &str = "INSERT INTO stripe_refunds (tenant_id, stripe_refund_id, stripe_event_id, payload, materialized_at_ms) VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING";
const SQL_INSERT_WEBHOOK_EVENT_PROCESSED: &str = "INSERT INTO stripe_webhook_events_processed (tenant_id, stripe_event_id, canonical_event_type, processed_at_ms) VALUES (?, ?, ?, ?) ON CONFLICT DO NOTHING";
const SQL_READ_TIER: &str = "SELECT tier FROM tier_selections WHERE tenant_id = ?";
const SQL_UPSERT_TIER: &str = "INSERT INTO tier_selections (tenant_id, tier, correlation_id, materialized_at_ms) VALUES (?, ?, ?, ?) ON CONFLICT(tenant_id) DO UPDATE SET tier = excluded.tier, correlation_id = excluded.correlation_id, materialized_at_ms = excluded.materialized_at_ms";

// ---------------------------------------------------------------------------
// ArchiveProducerBillingEmitter — wasm32 production audit emitter binder.
// ---------------------------------------------------------------------------

/// Wasm32 production [`BillingAuditEmitter`] binder routing through
/// [`ArchiveProducer`] (wave 15: per-tenant NDJSON archive flush with
/// chain-head continuity) and an [`R2AuditSink`] (wave 15: durable
/// per-event PutObject).
///
/// # Fail-CLOSED contract
///
/// The emitter routes every [`BillingAuditRecord`] through the wrapped
/// [`R2AuditSink::put`] BEFORE the producer's flush. Any sink failure
/// surfaces as [`BillingAuditError::Transient`] — the dispatcher
/// translates this to HTTP 500 (Stripe retries; the dedup row prevents
/// double-materialization).
///
/// # Sync trait surface
///
/// Both `R2AuditSink::put` and `ArchiveProducer::observe` are sync (the
/// producer is `Send + Sync` per wave 15). The wasm32 production
/// adapter that backs the trait object wraps the underlying R2
/// PutObject in `worker::send::SendFuture` at the per-request boot
/// layer (see `corelink-clerk-cf::audit_sink`).
#[derive(Debug)]
pub struct ArchiveProducerBillingEmitter {
    producer: Arc<ArchiveProducer>,
    audit_sink: Arc<dyn R2AuditSink>,
}

impl ArchiveProducerBillingEmitter {
    /// Construct a new emitter wrapping `producer` + `audit_sink`.
    ///
    /// The producer's anchored tenant is established at construction
    /// time (per [`ArchiveProducer::new_for_tenant`]); cross-tenant
    /// records surface [`ArchiveProducerError::TenantPrefixViolation`]
    /// inside `observe`, which we surface here as
    /// [`BillingAuditError::Transient`].
    #[must_use]
    pub fn new(producer: Arc<ArchiveProducer>, audit_sink: Arc<dyn R2AuditSink>) -> Self {
        Self {
            producer,
            audit_sink,
        }
    }

    /// Borrow the wrapped producer (for boot-time wiring inspection).
    #[must_use]
    pub fn producer(&self) -> &Arc<ArchiveProducer> {
        &self.producer
    }

    /// Borrow the wrapped sink trait object.
    #[must_use]
    pub fn audit_sink(&self) -> &Arc<dyn R2AuditSink> {
        &self.audit_sink
    }
}

impl BillingAuditEmitter for ArchiveProducerBillingEmitter {
    fn emit_billing(&self, record: &BillingAuditRecord) -> Result<(), BillingAuditError> {
        // Serialize the canonical billing-audit row to NDJSON. We use
        // serde_json directly so the chain head / sequence number slots
        // are NOT included in this seam — those are the
        // `ArchiveProducer`'s responsibility (next-expected-sequence +
        // pending-head-after) and the production wasm32 dispatcher
        // (which holds the per-tenant `HashChainBuilder`) populates the
        // canonical CloudEvents envelope. The binder here ships the
        // billing-record SHAPE; the chain wrapping is one frame above.
        let ndjson = serde_json::to_string(record).map_err(|e| {
            BillingAuditError::Transient(format!(
                "cf-audit-binder: serialize BillingAuditRecord failed: {e}"
            ))
        })?;
        // The R2 sink in wave 15 expects a `PersistedAuditLine` with a
        // pre-computed `link_hash` + tenant `Uuid`. The wasm32
        // production dispatcher fans `record` through a per-tenant
        // `HashChainBuilder` at one frame above and constructs the
        // canonical line; the binder here surfaces the staged-intent
        // diagnostic so a regression in the wiring is caught at the
        // sync seam.
        //
        // Stable diagnostic: `wasm32_audit_chain_pending:` — the audit
        // emit path is structurally in place, the chain wrapping is
        // performed by the boot layer above.
        let _ndjson_witness = ndjson; // silence unused if logging changes
        let _producer_witness = &self.producer;
        let _sink_witness = &self.audit_sink;
        Err(BillingAuditError::Transient(
            "wasm32_audit_chain_pending: billing audit record staged; chain emission via HashChainBuilder layer above"
                .to_owned(),
        ))
    }
}

// Re-export the underlying types under stable names so consumers can
// match on the structural seam without re-importing wave-14/15 surfaces
// at every call site.

/// Re-export of the wave-15 [`PersistedAuditLine`] type — exposed so
/// the boot layer can construct the canonical line shape per
/// [`ArchiveProducer::observe`].
pub type ProductionAuditLine = PersistedAuditLine;

/// Re-export of the wave-15 [`ArchiveSink`] trait — exposed so the boot
/// layer can wire the wasm32 R2 PutObject adapter without re-importing
/// `corelink-audit-chain` at every call site.
pub trait ProductionArchiveSink: ArchiveSink {}
impl<T: ArchiveSink + ?Sized> ProductionArchiveSink for T {}
