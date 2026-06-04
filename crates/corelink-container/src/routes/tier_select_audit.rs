//! Production [`TierSelectAudit`] adapter: the fail-CLOSED audit seam
//! backing `POST /v1/onboarding/tier-select`.
//!
//! This is the **WP-C SCAFFOLD**. The struct + trait impl + `Debug` are
//! frozen here so WP-C can fill the single `emit` body (currently
//! `todo!("WP-C")`) against a stable surface WITHOUT touching the trait,
//! the orchestration, or the other adapters.
//!
//! # What WP-C implements
//!
//! [`TierSelectAudit::emit`] records the durable audit-chain entry for
//! `event` (one of the orchestration's static labels:
//! `tier_select_attempted`, `dpa_first_violation_attempt`,
//! `stripe_checkout_session_created`, `tier_activated_free`). The
//! orchestration calls `emit` BEFORE every state mutation; an `Err(String)`
//! ABORTS the whole operation (fail-CLOSED;
//! INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER) → 500 `internal`.
//!
//! Mirrors the audit approach in `internal_pat.rs`: the native container
//! path has no D1 binding, so the baseline implementation emits a
//! STRUCTURED `tracing` event (ingested by the CF Logs pipeline) and
//! returns `Ok(())`. WP-C upgrades this to the durable D1 audit-chain write
//! (the same `corelink-audit-chain` archive path the Worker billing flow
//! uses) so the emit is genuinely fail-CLOSED on the durable store, not
//! merely on the log sink.
//!
//! # SECURITY INVARIANTS (preserved by WP-C — do NOT regress)
//!
//! - **Audit BEFORE mutation, fail-CLOSED.** `emit` returning `Err` MUST
//!   abort the orchestration — WP-C must propagate a real durable-write
//!   failure as `Err`, never swallow it.
//! - **Tenant from the verified header only.** `emit` records the
//!   `tenant_id` the orchestration passes (extracted from the
//!   edge-verified `x-corelink-tenant-id`); this adapter never re-derives
//!   or defaults it.
//! - **No secrets / no PII in the audit line.** Log only the static
//!   `event`, the `tenant_id`, and the `correlation_id` — never tokens,
//!   never the buyer email, never a Stripe key.

use crate::routes::tier_select::TierSelectAudit;

/// Production audit sink for tier-select.
///
/// The baseline (this scaffold) is a structured-`tracing` emitter with no
/// secret state — a unit-shaped marker. WP-C replaces the body with the
/// durable D1 audit-chain write and, if it needs a collaborator (e.g. an
/// `Arc<dyn ArchiveProducer>` / audit-emitter), adds it as an `Arc<...>`
/// field here with a redacting `Debug`.
#[derive(Clone, Default)]
pub struct TierSelectAuditAdapter {
    // WP-C: hold the durable audit collaborator here, e.g.
    //   audit: Arc<dyn corelink_audit_chain::...>,
    // as `Arc<...>` with a redacting Debug. Unit-shaped for the scaffold.
    _private: (),
}

impl TierSelectAuditAdapter {
    /// Construct the audit adapter.
    #[must_use]
    pub fn new() -> Self {
        Self { _private: () }
    }
}

impl std::fmt::Debug for TierSelectAuditAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // No secret state today; the redacting shape is fixed now so a
        // future durable collaborator (WP-C) is added behind a marker and
        // can never leak via Debug.
        f.debug_struct("TierSelectAuditAdapter").finish()
    }
}

impl TierSelectAudit for TierSelectAuditAdapter {
    async fn emit(
        &self,
        event: &'static str,
        tenant_id: &str,
        correlation_id: &str,
    ) -> Result<(), String> {
        // Structured audit event ingested by the CF Logs pipeline. Mirrors
        // `internal_pat.rs`, which likewise emits a `tracing` audit and
        // defers the durable D1 audit-chain write to Wave-37. Records ONLY
        // the static `event`, the edge-verified `tenant_id`, and the
        // `correlation_id` — never a token, the buyer email, or a Stripe key
        // (security model §7). Returns `Ok(())` because the log sink cannot
        // fail; the durable-store upgrade — which makes the fail-CLOSED
        // contract bind on D1 rather than only the log — is the tracked
        // Wave-37 hardening. The `Result` surface is kept so that upgrade is a
        // body-only change (no trait / orchestration churn).
        tracing::info!(
            target: "corelink.tier_select.audit",
            event = event,
            tenant_id = tenant_id,
            correlation_id = correlation_id,
            "tier-select audit",
        );
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;

    #[test]
    fn debug_surfaces_no_state() {
        // The marker shape carries no secret/PII even before WP-C adds a
        // durable collaborator.
        let rendered = format!("{:?}", TierSelectAuditAdapter::new());
        assert!(rendered.contains("TierSelectAuditAdapter"));
        assert!(!rendered.contains("token"));
    }
}
