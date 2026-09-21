use super::*;
use std::sync::Arc;

use crate::audit::InMemoryAuditSink;
use crate::observer::InMemorySliObserver;

#[derive(Debug)]
struct EffectAwareFailingCas {
    effect: EffectAwareFailure,
}

#[derive(Debug, Clone, Copy)]
enum EffectAwareFailure {
    Committed,
    Pending(uuid::Uuid),
}

impl CasWriteHandler for EffectAwareFailingCas {
    fn write(&self, _req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal("post-commit audit failed".into()))
    }

    fn write_with_effect(
        &self,
        _req: CasWriteRequest,
    ) -> Result<CasWriteResponse, crate::CasWriteFailure> {
        let cause = CasHandlerError::Internal("post-commit audit failed".into());
        match self.effect {
            EffectAwareFailure::Committed => Err(crate::CasWriteFailure::committed(cause)),
            EffectAwareFailure::Pending(intent_id) => {
                Err(crate::CasWriteFailure::pending(cause, intent_id))
            }
        }
    }
}

#[derive(Debug)]
struct LegacyFailingCas;

impl CasWriteHandler for LegacyFailingCas {
    fn write(&self, _req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        Err(CasHandlerError::Internal("legacy storage error".into()))
    }
}

fn write_request() -> CasWriteRequest {
    CasWriteRequest::new("t1", "a".repeat(64), b"body".to_vec(), "p1", "t1", 1)
}

#[test]
fn explicit_write_effects_cross_trait_boundary() {
    let handler: Arc<dyn CasWriteHandler> = Arc::new(EffectAwareFailingCas {
        effect: EffectAwareFailure::Committed,
    });

    let failure = handler
        .write_with_effect(write_request())
        .expect_err("effect-aware failure");

    assert_eq!(failure.effect, crate::MutationEffect::Committed);
    assert!(matches!(failure.cause, CasHandlerError::Internal(_)));

    let intent_id = uuid::Uuid::new_v4();
    let handler: Arc<dyn CasWriteHandler> = Arc::new(EffectAwareFailingCas {
        effect: EffectAwareFailure::Pending(intent_id),
    });
    let failure = handler
        .write_with_effect(write_request())
        .expect_err("effect-aware pending failure");

    assert_eq!(failure.effect, crate::MutationEffect::Pending { intent_id });
}

#[test]
fn default_write_effect_is_unknown_without_a_durable_intent() {
    let handler: Arc<dyn CasWriteHandler> = Arc::new(LegacyFailingCas);

    let failure = handler
        .write_with_effect(write_request())
        .expect_err("legacy write failure");

    assert_eq!(failure.effect, crate::MutationEffect::Unknown);
    assert_ne!(failure.effect, crate::MutationEffect::Committed);
}

fn fixture() -> (
    Arc<InMemoryAuditSink>,
    Arc<InMemorySliObserver>,
    InMemoryCasHandler,
) {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let h = InMemoryCasHandler::new(audit.clone(), sli.clone());
    (audit, sli, h)
}

#[test]
fn read_happy_path_emits_sli_avail_and_correctness() {
    let (audit, sli, h) = fixture();
    let bytes = b"hello".to_vec();
    let hash = fake_hash(&bytes);
    h.seed("t1", &hash, bytes.clone()).expect("seed");

    let resp = h
        .read(CasReadRequest {
            tenant: "t1".into(),
            hash: hash.clone(),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            at_unix_ms: 1,
            algo: crate::DigestAlgo::Blake3,
            max_bytes: None,
        })
        .expect("read");
    assert_eq!(resp.bytes, bytes);
    assert_eq!(resp.content_hash, hash);

    let obs = sli.snapshot().expect("snapshot");
    let avail_count = obs
        .iter()
        .filter(|o| o.sli == Sli::AvailCasGet && !o.is_error)
        .count();
    assert_eq!(avail_count, 1, "exactly one AvailCasGet ok observation");
    let lat_count = obs
        .iter()
        .filter(|o| o.sli == Sli::LatencyCasGetP99)
        .count();
    assert_eq!(lat_count, 1);

    let rows = audit.snapshot().expect("audit");
    // ReadAttempted then ReadServed in order — INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER
    // ordering pinned.
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, AuditEventKind::ReadAttempted);
    assert_eq!(rows[1].kind, AuditEventKind::ReadServed);
}

#[test]
fn read_cross_tenant_audits_before_returning_denied() {
    let (audit, sli, h) = fixture();
    let bytes = b"abc".to_vec();
    let hash = fake_hash(&bytes);
    h.seed("victim", &hash, bytes).expect("seed");

    let err = h
        .read(CasReadRequest {
            tenant: "victim".into(),
            hash: hash.clone(),
            principal: "attacker".into(),
            caller_tenant: "attacker_tenant".into(),
            at_unix_ms: 1,
            algo: crate::DigestAlgo::Blake3,
            max_bytes: None,
        })
        .expect_err("denied");
    assert!(matches!(err, CasHandlerError::CrossTenantDenied { .. }));
    let rows = audit.snapshot().expect("audit");
    // Audit row is the FIRST thing emitted — fail-CLOSED ordering.
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
    let obs = sli.snapshot().expect("sli");
    assert!(
        obs.iter().any(|o| o.sli == Sli::AvailCasGet && o.is_error),
        "AvailCasGet error observation emitted on denial path"
    );
}

#[test]
fn read_not_found_returns_not_found_and_emits_sli_error() {
    let (_audit, sli, h) = fixture();
    let err = h
        .read(CasReadRequest {
            tenant: "t1".into(),
            hash: "deadbeef".repeat(8),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            at_unix_ms: 1,
            algo: crate::DigestAlgo::Blake3,
            max_bytes: None,
        })
        .expect_err("not found");
    assert!(matches!(err, CasHandlerError::NotFound { .. }));
    let obs = sli.snapshot().expect("sli");
    assert!(obs.iter().any(|o| o.sli == Sli::AvailCasGet && o.is_error));
}

#[test]
fn read_max_bytes_is_observed_before_copying_the_object() {
    let (_audit, _sli, h) = fixture();
    let bytes = b"five!".to_vec();
    let hash = fake_hash(&bytes);
    h.seed("t1", &hash, bytes).expect("seed");
    let err = h
        .read(CasReadRequest::new("t1", hash, "p1", "t1", 1).with_max_bytes(4))
        .expect_err("object must exceed the request ceiling");
    assert_eq!(
        err,
        CasHandlerError::ObjectTooLarge {
            actual_bytes: 5,
            limit_bytes: 4,
        }
    );
}

#[test]
fn read_correctness_injection_emits_correctness_sli_error() {
    let (audit, sli, h) = fixture();
    let bytes = b"x".to_vec();
    let hash = fake_hash(&bytes);
    h.seed("t1", &hash, bytes).expect("seed");
    h.inject_correctness_mismatch("t1", &hash, "ffff".repeat(16))
        .expect("inject");

    let err = h
        .read(CasReadRequest {
            tenant: "t1".into(),
            hash: hash.clone(),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            at_unix_ms: 1,
            algo: crate::DigestAlgo::Blake3,
            max_bytes: None,
        })
        .expect_err("mismatch");
    assert!(matches!(err, CasHandlerError::HashMismatch { .. }));
    let obs = sli.snapshot().expect("sli");
    assert!(obs
        .iter()
        .any(|o| o.sli == Sli::CorrectnessCas && o.is_error));
    // Audit emitted CorrectnessViolation row.
    let rows = audit.snapshot().expect("audit");
    assert!(
        rows.iter()
            .any(|r| r.kind == AuditEventKind::CorrectnessViolation),
        "correctness violation audit row emitted"
    );
}

#[test]
fn write_happy_path_emits_avail_put_and_correctness_ok() {
    let (audit, sli, h) = fixture();
    let bytes = b"hello".to_vec();
    let hash = fake_hash(&bytes);
    let resp = h
        .write(CasWriteRequest {
            tenant: "t1".into(),
            claimed_hash: hash.clone(),
            bytes: bytes.clone(),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            accounting_tenant: "t1".into(),
            at_unix_ms: 1,
            storage_quota_bytes: None,
            algo: crate::DigestAlgo::Blake3,
        })
        .expect("write");
    assert_eq!(resp.content_hash, hash);
    assert!(resp.durable);

    let rows = audit.snapshot().expect("audit");
    // WriteAttempted then WriteCommitted — INV ordering.
    assert_eq!(rows[0].kind, AuditEventKind::WriteAttempted);
    assert!(rows
        .iter()
        .any(|r| r.kind == AuditEventKind::WriteCommitted));
    assert!(sli
        .snapshot()
        .expect("sli")
        .iter()
        .any(|o| o.sli == Sli::AvailCasPut && !o.is_error));
}

#[test]
fn write_hash_mismatch_aborts_before_storing() {
    let (audit, sli, h) = fixture();
    let bytes = b"hello".to_vec();
    let real_hash = fake_hash(&bytes);
    let bogus = "f".repeat(64);

    let err = h
        .write(CasWriteRequest {
            tenant: "t1".into(),
            claimed_hash: bogus.clone(),
            bytes: bytes.clone(),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            accounting_tenant: "t1".into(),
            at_unix_ms: 1,
            storage_quota_bytes: None,
            algo: crate::DigestAlgo::Blake3,
        })
        .expect_err("mismatch");
    match err {
        CasHandlerError::HashMismatch { claimed, actual } => {
            assert_eq!(claimed, bogus);
            assert_eq!(actual, real_hash);
        }
        other => panic!("unexpected: {other:?}"),
    }
    // No WriteCommitted row.
    let rows = audit.snapshot().expect("audit");
    assert!(rows
        .iter()
        .all(|r| r.kind != AuditEventKind::WriteCommitted));
    // CorrectnessViolation row present.
    assert!(rows
        .iter()
        .any(|r| r.kind == AuditEventKind::CorrectnessViolation));
    // CorrectnessCas error observation present.
    assert!(sli
        .snapshot()
        .expect("sli")
        .iter()
        .any(|o| o.sli == Sli::CorrectnessCas && o.is_error));
}

#[test]
fn write_audit_failure_aborts_before_storing() {
    let audit = Arc::new(InMemoryAuditSink::new());
    let sli = Arc::new(InMemorySliObserver::new());
    let h = InMemoryCasHandler::new(audit.clone(), sli.clone());
    audit.inject_failure("d1 down").expect("inject");

    let bytes = b"hello".to_vec();
    let hash = fake_hash(&bytes);
    let err = h
        .write(CasWriteRequest {
            tenant: "t1".into(),
            claimed_hash: hash.clone(),
            bytes: bytes.clone(),
            principal: "p1".into(),
            caller_tenant: "t1".into(),
            accounting_tenant: "t1".into(),
            at_unix_ms: 1,
            storage_quota_bytes: None,
            algo: crate::DigestAlgo::Blake3,
        })
        .expect_err("audit closed");
    assert!(matches!(err, CasHandlerError::AuditFailed(_)));
    // Storage MUST be untouched (fail-CLOSED ordering).
    let g = h.objects.lock().expect("lock");
    assert!(g.is_empty());
}

#[test]
fn write_cross_tenant_audits_before_returning_denied() {
    let (audit, _sli, h) = fixture();
    let bytes = b"abc".to_vec();
    let hash = fake_hash(&bytes);
    let err = h
        .write(CasWriteRequest {
            tenant: "victim".into(),
            claimed_hash: hash,
            bytes,
            principal: "attacker".into(),
            caller_tenant: "attacker_tenant".into(),
            accounting_tenant: "victim".into(),
            at_unix_ms: 1,
            storage_quota_bytes: None,
            algo: crate::DigestAlgo::Blake3,
        })
        .expect_err("denied");
    assert!(matches!(err, CasHandlerError::CrossTenantDenied { .. }));
    let rows = audit.snapshot().expect("audit");
    assert_eq!(rows[0].kind, AuditEventKind::WriteDenied);
}

#[test]
fn public_write_requires_explicit_accounting_identity() {
    let (_audit, _sli, h) = fixture();
    let bytes = b"shared-public".to_vec();
    let hash = fake_hash(&bytes);

    // Merely changing the physical namespace to `_public` must not turn a
    // normal request into an authorized cross-tenant write: `new` records
    // the physical tenant as its accounting identity.
    let spoof = CasWriteRequest::new(
        "_public",
        hash.clone(),
        bytes.clone(),
        "attacker",
        "tenant-real",
        1,
    );
    assert!(!spoof.is_authorized_for_caller());
    assert!(matches!(
        h.write(spoof),
        Err(CasHandlerError::CrossTenantDenied { .. })
    ));

    // The dedicated constructor carries the authenticated tenant and is
    // the only valid shape for a public physical write.
    let req = CasWriteRequest::for_public_namespace(
        "tenant-real",
        hash,
        bytes,
        "tenant-real-principal",
        2,
    );
    assert!(req.is_authorized_for_caller());
    assert!(h.write(req).expect("authorized public write").durable);
}
