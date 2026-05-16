//! Stripe webhook **production** state materializers (wave-17 follow-on).
//!
//! Wave 15 (`corelink-stripe-real::webhook_dispatch`) shipped the
//! canonical pipeline + trait surface; wave 16 (`apps/server::webhook`)
//! bound the axum HTTP shell to that dispatcher. Both shipped with
//! `RecordingStateMaterializer` / `RecordingAuditEmitter` /
//! `InMemoryIdempotencyStore` as in-process placeholders pending the
//! S-13+ "trait-abstraction-defer" production wiring.
//!
//! This crate closes that gap. It provides:
//!
//! - [`BillingD1Writer`] — a thin tenant-scoped SQL writer trait the
//!   materializer drives. The wasm32 production binder wires this onto
//!   `corelink-cf-bindings::CfD1DatabaseReal` (which carries the
//!   tenant-prefix enforcement + audit fence); native CI / dev use
//!   [`InMemoryBillingD1`] which mirrors the same `INSERT OR IGNORE` /
//!   `UPSERT` semantics.
//! - [`BillingAuditEmitter`] — a small audit-sink trait whose production
//!   binder routes through `corelink-audit-chain`. The materializer
//!   emits one `corelink.billing.<event>.materialized.v1` row **before**
//!   returning so a downstream observer crash cannot lose the state-
//!   change trail.
//! - [`D1SubscriptionStateHandler`] — implements the
//!   [`corelink_stripe_real::webhook_dispatch::StateMaterializer`] trait
//!   from wave 15. Each of the five state-mutating Stripe events writes
//!   the canonical D1 row; audit-fail propagates as `Transient`
//!   materializer error → dispatcher returns 500.
//! - [`RealStripeAuditEmitter`] — implements the wave-15
//!   [`corelink_stripe_real::webhook_dispatch::AuditEmitter`] trait.
//!   Routes the dispatcher's per-delivery audit row through the same
//!   sink the materializer uses (so the dispatcher + materializer share
//!   one chain; verifier never sees a split topology).
//! - [`D1IdempotencyStore`] — implements
//!   [`corelink_stripe_real::webhook_dispatch::IdempotencyStore`]
//!   backed by the same [`BillingD1Writer`] (mirroring the
//!   `stripe_webhook_events_processed` table from migration
//!   `0044_stripe_webhook_events_processed.sql`).
//! - [`TierSelector`] — recomputes `tier_selections.tier` from
//!   subscription state and emits `corelink.tenant.tier_changed.v1`
//!   if the canonical tier kind changed. The default
//!   [`InMemoryTierSelector`] holds the plan-id → tier mapping; the
//!   production binder pins it from the workspace tier config.
//!
//! # Charter compliance
//!
//! - `#![forbid(unsafe_code)]`.
//! - No `unwrap` / `expect` / `panic` / `indexing_slicing` in `src/`.
//! - No tokio in `src/` (sync trait boundary preserved end-to-end).
//! - Every public enum + struct is `#[non_exhaustive]`.
//! - Stripe event id comparison via [`subtle::ConstantTimeEq`] in the
//!   idempotency-store dedup path (defense in depth — the BLAKE3 token
//!   comparison upstream is already collision-resistant, but the raw
//!   `event_id` stored in D1 is the operator-visible primary key).
//! - Audit emit happens **before** the state mutation surfaces success
//!   to the caller (fail-CLOSED ordering preserved from wave 15).

#![forbid(unsafe_code)]

mod audit;
mod d1;
mod handler;
mod idempotency;
mod tier;
#[cfg(feature = "cf-billing-real")]
mod wasm32_binders;

pub use audit::{
    AuditSeverity, BillingAuditEmitter, BillingAuditError, BillingAuditRecord,
    InMemoryBillingAuditEmitter, RealStripeAuditEmitter,
};
pub use d1::{BillingD1Error, BillingD1Writer, InMemoryBillingD1, MaterializedRow};
pub use handler::{D1SubscriptionStateHandler, EVENT_MATERIALIZATION_MATRIX};
pub use idempotency::D1IdempotencyStore;
pub use tier::{InMemoryTierSelector, TierSelector};

#[cfg(feature = "cf-billing-real")]
pub use wasm32_binders::{
    ArchiveProducerBillingEmitter, CfD1BillingWriter, ProductionArchiveSink, ProductionAuditLine,
};
