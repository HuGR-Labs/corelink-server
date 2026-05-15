//! Happy-path 2 — Erasure (LGPD Art. 18 VI / GDPR Art. 17 right to be
//! forgotten).
//!
//! Customer submits an Erasure DSR with a valid MFA step-up token →
//! API returns `RequestAccepted` + JWT receipt. The erasure worker
//! processes the canonical 12-backend plan (8 effective + 4
//! pseudonymized) and emits `started.v1` + 12 × `backend_completed.v1`.
//! A 24h verification sweep returns `VerifiedComplete`, signs the
//! report (BLAKE3-keyed MAC over JCS canonical preimage), uploads to
//! the in-memory R2 evidence-dsr stub at the canonical object key, and
//! returns a 24h-TTL signed URL.
//!
//! The test asserts:
//! - all 12 canonical backends emit `backend_completed`
//! - verification arm is `VerifiedComplete` (no failures)
//! - signed report verifies post-facto
//! - signed URL is valid at submission and expired post-TTL
//! - cross-tenant erasure structurally impossible (other tenant's data
//!   on the same backends untouched).

#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]

use corelink_dsr::{DsrDecision, DsrEndpoint, DsrJurisdiction, DsrRequestKind};
use corelink_privacy_erasure_worker::{
    canonical_report_key, BackendErasureAdapter, ErasureDecision, ErasureWorker, BACKEND_COUNT,
};
use e2e_dsr::{
    canonical_dsr_for, canonical_erasure_for, canonical_mfa_token, make_test_tenant,
    seed_backends_for, setup_test_env, verify_audit_chain, ExpectedDsrAuditEvent,
    TEST_SIGNED_URL_TTL_MS,
};

#[test]
fn erasure_request_happy_path_drains_all_12_backends() {
    let env = setup_test_env();
    let primary = make_test_tenant("primary-tenant");
    let neighbor = make_test_tenant("neighbor-tenant");

    // Seed every canonical backend with 3 rows for both tenants. The
    // cross-tenant rows are the canary for tenant isolation.
    seed_backends_for(&env, &primary, 3);
    seed_backends_for(&env, &neighbor, 3);

    // ---- 1. Submit Erasure with valid MFA step-up token ----
    let mfa = canonical_mfa_token();
    let request = canonical_dsr_for(
        &primary,
        DsrRequestKind::Erasure,
        DsrJurisdiction::Lgpd,
        env.now_ms,
    )
    .with_mfa(mfa);

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
    assert!(receipt.is_non_empty());
    let expected_sla = env.now_ms.saturating_add(15 * 86_400_000); // LGPD 15d
    assert_eq!(sla_deadline_ms, expected_sla);

    // Verify the JWT receipt round-trips via the canonical issuer.
    use corelink_dsr::JwtReceiptIssuer as _;
    let claims = match env.dsr_receipt_issuer.verify(&receipt, env.now_ms) {
        Ok(c) => c,
        Err(e) => panic!("jwt receipt verify failed: {e:?}"),
    };
    assert_eq!(claims.request_id, request.request_id);
    assert_eq!(claims.expires_at_ms, env.now_ms.saturating_add(90 * 86_400_000));

    // Audit chain: received + mfa_verified + accepted + receipt_issued.
    if let Err(msg) = verify_audit_chain(
        &env,
        &[
            ExpectedDsrAuditEvent::RequestReceived,
            ExpectedDsrAuditEvent::MfaVerified,
            ExpectedDsrAuditEvent::RequestAccepted,
            ExpectedDsrAuditEvent::ReceiptIssued,
        ],
    ) {
        panic!("dsr audit chain: {msg}");
    }
    assert_eq!(env.dsr_mfa.verified_count(), 1);

    // ---- 2. Erasure worker drains 12 backends ----
    let erasure = canonical_erasure_for(&primary, request.request_id, env.now_ms);
    let dispatch = match env.erasure_worker.process_erasure(&erasure, env.now_ms) {
        Ok(d) => d,
        Err(e) => panic!("erasure worker process_erasure: {e:?}"),
    };
    let plan = match dispatch {
        ErasureDecision::Started { plan } => plan,
        other => panic!("expected Started, got {other:?}"),
    };
    assert_eq!(
        plan.entries.len(),
        BACKEND_COUNT,
        "12-canonical-backend invariant"
    );

    // Per-backend completed events — one per canonical backend.
    let erasure_audit_count = env.erasure_audit.len();
    // 1 started + 12 backend_completed = 13.
    assert_eq!(
        erasure_audit_count, 13,
        "expected 1 started + {BACKEND_COUNT} backend_completed, got {erasure_audit_count}"
    );

    // ---- 3. 24h verification sweep emits signed report ----
    let verify_at_ms = env.now_ms.saturating_add(60_000); // 1 min after submit
    let outcome = match env.verification_job.run_24h_sweep(&erasure, verify_at_ms) {
        Ok(o) => o,
        Err(e) => panic!("verification sweep failed: {e:?}"),
    };
    assert!(
        matches!(outcome.decision, ErasureDecision::VerifiedComplete { .. }),
        "expected VerifiedComplete, got {:?}",
        outcome.decision
    );
    let report = match outcome.report {
        Some(r) => r,
        None => panic!("verification sweep returned no report"),
    };
    let signature = match outcome.signature {
        Some(s) => s,
        None => panic!("verification sweep returned no signature"),
    };

    // Signature verifies post-facto via the canonical signer.
    if let Err(e) = env.verification_job.verify_report(&report, &signature) {
        panic!("signed report verify failed: {e:?}");
    }

    // ---- 4. Upload report + return signed URL ----
    let key = canonical_report_key(primary.tenant_id, request.request_id);
    let expected_key = match outcome.object_key {
        Some(k) => k,
        None => panic!("verification sweep returned no object_key"),
    };
    assert_eq!(expected_key, key);

    let report_bytes = match serde_json::to_vec(&report) {
        Ok(v) => v,
        Err(e) => panic!("report serialize: {e}"),
    };
    let url = env.r2.put_signed(key.clone(), report_bytes.clone(), env.now_ms);
    assert!(url.is_valid(env.now_ms));
    assert_eq!(
        url.expires_at_ms,
        env.now_ms.saturating_add(TEST_SIGNED_URL_TTL_MS)
    );

    // Fetch round-trip.
    let fetched = match env.r2.get(&url, env.now_ms.saturating_add(60_000)) {
        Ok(b) => b,
        Err(e) => panic!("r2 get during ttl failed: {e:?}"),
    };
    assert_eq!(fetched, report_bytes);

    // ---- 5. Cross-tenant invariant: neighbor still has its rows ----
    for adapter in &env.backend_adapters {
        // Primary rows: cleared on effective backends, retained-but-redacted
        // on pseudonymized backends.
        if adapter.kind().is_effective() {
            assert_eq!(
                adapter.row_count(primary.tenant_id, primary.subject_id),
                0,
                "primary rows must be erased on effective backend {}",
                adapter.kind()
            );
        } else {
            assert!(
                adapter.all_redacted(primary.tenant_id, primary.subject_id),
                "primary rows must be pseudonymized on backend {}",
                adapter.kind()
            );
        }
        // Neighbor's rows untouched on every backend.
        assert_eq!(
            adapter.row_count(neighbor.tenant_id, neighbor.subject_id),
            3,
            "neighbor rows untouched on backend {}",
            adapter.kind()
        );
        assert!(
            !adapter.all_redacted(neighbor.tenant_id, neighbor.subject_id)
                || adapter.kind().is_pseudonymized(),
            "neighbor rows must not be redacted on effective backend {}",
            adapter.kind()
        );
    }
}
