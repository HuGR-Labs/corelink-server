//! Happy-path 3 — Portability (LGPD Art. 18 V / GDPR Art. 20
//! machine-readable export).
//!
//! Customer submits a Portability DSR (read-only; skips MFA gate) →
//! API returns `RequestAccepted` with receipt + sla_deadline. The
//! worker builds a JSON export tarball-like payload, uploads to R2 at
//! the canonical evidence-dsr key, and returns a 24h signed URL. The
//! customer fetches the export via the signed URL.

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{DsrDecision, DsrEndpoint, DsrJurisdiction, DsrRequestKind};
use e2e_dsr::{
    canonical_dsr_for, make_test_tenant, setup_test_env, verify_audit_chain, ExpectedDsrAuditEvent,
};

#[test]
fn portability_request_returns_signed_export() {
    let env = setup_test_env();
    let tenant = make_test_tenant("portability-tenant");

    let request = canonical_dsr_for(
        &tenant,
        DsrRequestKind::Portability,
        DsrJurisdiction::Ccpa,
        env.now_ms,
    );
    let decision = match env.dsr.submit(&request) {
        Ok(d) => d,
        Err(e) => panic!("dsr submit failed: {e:?}"),
    };
    let (receipt, sla_deadline_ms) = match decision {
        DsrDecision::RequestAccepted {
            receipt,
            sla_deadline_ms,
        } => (receipt, sla_deadline_ms),
        other => panic!("expected RequestAccepted, got {other:?}"),
    };
    // CCPA SLA = 45 days.
    let expected_sla = env.now_ms.saturating_add(45 * 86_400_000);
    assert_eq!(sla_deadline_ms, expected_sla);
    assert!(receipt.is_non_empty());

    // No MFA gate for Portability.
    assert_eq!(env.dsr_mfa.verified_count(), 0);

    // Build a canonical JSON export payload (tarball stand-in) and
    // upload to the R2 stub.
    let export = serde_json::json!({
        "tenant_id": tenant.tenant_id,
        "subject_id": tenant.subject_id,
        "request_id": request.request_id,
        "data": [
            { "table": "accounts", "rows": [] },
            { "table": "consent_ledger", "rows": [] }
        ]
    });
    let payload = match serde_json::to_vec(&export) {
        Ok(v) => v,
        Err(e) => panic!("serialize export: {e}"),
    };
    let key = format!(
        "dsr-exports/{}/{}/portability.json",
        tenant.tenant_id.simple(),
        request.request_id.simple()
    );
    let url = env.r2.put_signed(key.clone(), payload.clone(), env.now_ms);
    assert!(url.is_valid(env.now_ms));

    // Customer fetches via signed URL during TTL.
    let fetched = match env.r2.get(&url, env.now_ms.saturating_add(60_000)) {
        Ok(b) => b,
        Err(e) => panic!("r2 get failed: {e:?}"),
    };
    assert_eq!(fetched, payload);

    // Audit chain.
    if let Err(msg) = verify_audit_chain(
        &env,
        &[
            ExpectedDsrAuditEvent::RequestReceived,
            ExpectedDsrAuditEvent::RequestAccepted,
            ExpectedDsrAuditEvent::ReceiptIssued,
        ],
    ) {
        panic!("audit chain: {msg}");
    }
}
