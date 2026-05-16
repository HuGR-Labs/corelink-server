//! `CasReadHandler` + `CasWriteHandler` traits + `InMemoryCasHandler`
//! deterministic in-process fake.
//!
//! The two traits split read from write so an admin-read-only build
//! variant can omit the write half and so the `apps/server` route
//! table can compose them independently. Both halves share the same
//! `(audit, sli)` collaborator pair which keeps cross-handler
//! invariants (audit-fail-CLOSED ordering, SLI emit-on-entry)
//! provable by composition rather than by inheritance.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::audit::{AuditEvent, AuditEventKind, AuditSink};
use crate::error::CasHandlerError;
use crate::observer::{Sli, SliObservation, SliObserver};
use crate::request::{
    CasReadRequest, CasReadResponse, CasWriteRequest, CasWriteResponse,
};

/// Trait every concrete CAS read handler implements.
///
/// Implementors **MUST**:
///
/// 1. Emit `AuditEventKind::ReadAttempted` BEFORE the storage lookup.
/// 2. Emit `Sli::AvailCasGet` observation on EVERY return path
///    (`is_error` set appropriately).
/// 3. Emit `Sli::LatencyCasGetP99` observation on EVERY return path.
/// 4. On `CrossTenantDenied`, emit `AuditEventKind::ReadDenied`
///    BEFORE returning.
/// 5. On hash-mismatch reads, emit `Sli::CorrectnessCas` failure
///    observation BEFORE returning.
pub trait CasReadHandler: Send + Sync + core::fmt::Debug {
    /// Serve one CAS read.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CasHandlerError`] per the trait
    /// contract (see crate-level invariants).
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError>;
}

/// Trait every concrete CAS write handler implements. Same
/// emit-discipline as the read trait but with the audit pair
/// `WriteAttempted` / `WriteCommitted` / `WriteDenied` + the
/// SLI pair `Sli::AvailCasPut` / `Sli::LatencyCasPutP99`.
///
/// Implementors **MUST**:
///
/// 1. Emit `AuditEventKind::WriteAttempted` BEFORE the storage
///    mutation. If the audit emit fails, abort with
///    `CasHandlerError::AuditFailed` and **do not mutate**.
/// 2. Verify the claimed hash against the bytes BEFORE storing; on
///    mismatch emit `Sli::CorrectnessCas` failure observation +
///    `AuditEventKind::CorrectnessViolation` + return
///    [`CasHandlerError::HashMismatch`].
/// 3. Emit `Sli::AvailCasPut` + `Sli::LatencyCasPutP99` observations
///    on EVERY return path.
/// 4. On success emit `AuditEventKind::WriteCommitted` AFTER the
///    durable store (the row records the durable outcome — the
///    `WriteAttempted` row above already records the intent).
pub trait CasWriteHandler: Send + Sync + core::fmt::Debug {
    /// Serve one CAS write.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CasHandlerError`] per the trait
    /// contract (see crate-level invariants).
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError>;
}

/// Deterministic in-memory CAS handler. Intended for unit tests,
/// property tests, and the `apps/server` wire-up until the
/// `cfg(target_arch = "wasm32")` CF-Worker handler lands.
///
/// `InMemoryCasHandler` is `Clone`-free on purpose; it carries
/// `Arc<dyn AuditSink>` + `Arc<dyn SliObserver>` upward by reference
/// so the `apps/server` router can hand the same instance to a
/// per-request stack.
pub struct InMemoryCasHandler {
    objects: Mutex<HashMap<(String, String), Vec<u8>>>,
    audit: std::sync::Arc<dyn AuditSink>,
    sli: std::sync::Arc<dyn SliObserver>,
    /// Optional injection: if Some(actual_hash), the next read
    /// returns `HashMismatch { claimed = key.hash, actual }` so
    /// `SLO-CORRECT-CAS` proptests have a falsifiability handle.
    correctness_injection: Mutex<Option<(String, String, String)>>,
}

impl core::fmt::Debug for InMemoryCasHandler {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InMemoryCasHandler").finish_non_exhaustive()
    }
}

impl InMemoryCasHandler {
    /// Construct an empty handler bound to the supplied collaborators.
    #[must_use]
    pub fn new(
        audit: std::sync::Arc<dyn AuditSink>,
        sli: std::sync::Arc<dyn SliObserver>,
    ) -> Self {
        Self {
            objects: Mutex::new(HashMap::new()),
            audit,
            sli,
            correctness_injection: Mutex::new(None),
        }
    }

    /// Direct seed (test fixture): place `bytes` under `(tenant, hash)`
    /// without going through the write path (avoids audit-row noise).
    ///
    /// # Errors
    ///
    /// Returns [`CasHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn seed(
        &self,
        tenant: impl Into<String>,
        hash: impl Into<String>,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), CasHandlerError> {
        let mut g = self
            .objects
            .lock()
            .map_err(|_| CasHandlerError::Internal("storage lock poisoned".into()))?;
        g.insert((tenant.into(), hash.into()), bytes.into());
        Ok(())
    }

    /// Inject a hash-mismatch for one `(tenant, hash)` so the next
    /// read of that key surfaces a correctness violation. Drives
    /// `SLO-CORRECT-CAS` falsifiability proptests.
    ///
    /// # Errors
    ///
    /// Returns [`CasHandlerError::Internal`] if the storage lock is
    /// poisoned.
    pub fn inject_correctness_mismatch(
        &self,
        tenant: impl Into<String>,
        hash: impl Into<String>,
        actual_hash: impl Into<String>,
    ) -> Result<(), CasHandlerError> {
        let mut g = self
            .correctness_injection
            .lock()
            .map_err(|_| CasHandlerError::Internal("correctness inject lock poisoned".into()))?;
        *g = Some((tenant.into(), hash.into(), actual_hash.into()));
        Ok(())
    }

    fn check_tenant(req_tenant: &str, caller_tenant: &str) -> bool {
        req_tenant == caller_tenant
    }
}

impl CasReadHandler for InMemoryCasHandler {
    fn read(&self, req: CasReadRequest) -> Result<CasReadResponse, CasHandlerError> {
        // INV-HANDLER-SLI-EMIT-ENTRY — record the latency / availability
        // SLI tuple on EVERY return path. We pin `is_error` after we
        // know the outcome; the latency budget reflects the entry-to-
        // return interval (here represented by request `at_unix_ms`
        // since tests drive a logical clock).
        let emit = |outcome_is_err: bool| {
            self.sli.observe(SliObservation {
                sli: Sli::AvailCasGet,
                is_error: outcome_is_err,
                latency_us: 0,
            });
            self.sli.observe(SliObservation {
                sli: Sli::LatencyCasGetP99,
                is_error: outcome_is_err,
                latency_us: 0,
            });
        };

        // Cross-tenant access — audit BEFORE returning denial.
        if !Self::check_tenant(&req.tenant, &req.caller_tenant) {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::ReadDenied,
                    tenant: req.tenant.clone(),
                    hash: req.hash.clone(),
                    principal: req.principal.clone(),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // ReadAttempted audit BEFORE lookup.
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::ReadAttempted,
                tenant: req.tenant.clone(),
                hash: req.hash.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(CasHandlerError::AuditFailed)?;

        // Correctness injection (zero-budget SLO-CORRECT-CAS).
        {
            let g = self.correctness_injection.lock().map_err(|_| {
                CasHandlerError::Internal("correctness inject lock poisoned".into())
            })?;
            if let Some((t, h, actual)) = g.as_ref() {
                if t == &req.tenant && h == &req.hash {
                    // Audit + SLI correctness emit BEFORE returning.
                    self.audit
                        .emit(AuditEvent {
                            kind: AuditEventKind::CorrectnessViolation,
                            tenant: req.tenant.clone(),
                            hash: req.hash.clone(),
                            principal: req.principal.clone(),
                            at_unix_ms: req.at_unix_ms,
                        })
                        .map_err(CasHandlerError::AuditFailed)?;
                    self.sli.observe(SliObservation {
                        sli: Sli::CorrectnessCas,
                        is_error: true,
                        latency_us: 0,
                    });
                    emit(true);
                    return Err(CasHandlerError::HashMismatch {
                        claimed: req.hash,
                        actual: actual.clone(),
                    });
                }
            }
        }

        // Storage lookup.
        let g = self
            .objects
            .lock()
            .map_err(|_| CasHandlerError::Internal("storage lock poisoned".into()))?;
        let bytes = g
            .get(&(req.tenant.clone(), req.hash.clone()))
            .cloned()
            .ok_or_else(|| CasHandlerError::NotFound {
                tenant: req.tenant.clone(),
                hash: req.hash.clone(),
            });
        drop(g);

        match bytes {
            Ok(b) => {
                // Served audit AFTER the lookup; entry intent was
                // already recorded by ReadAttempted above.
                self.audit
                    .emit(AuditEvent {
                        kind: AuditEventKind::ReadServed,
                        tenant: req.tenant.clone(),
                        hash: req.hash.clone(),
                        principal: req.principal.clone(),
                        at_unix_ms: req.at_unix_ms,
                    })
                    .map_err(CasHandlerError::AuditFailed)?;
                // Correctness positive observation (informational —
                // numerator excludes errors).
                self.sli.observe(SliObservation {
                    sli: Sli::CorrectnessCas,
                    is_error: false,
                    latency_us: 0,
                });
                emit(false);
                Ok(CasReadResponse {
                    bytes: b,
                    content_hash: req.hash,
                })
            }
            Err(e) => {
                emit(true);
                Err(e)
            }
        }
    }
}

impl CasWriteHandler for InMemoryCasHandler {
    fn write(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasHandlerError> {
        let emit = |outcome_is_err: bool| {
            self.sli.observe(SliObservation {
                sli: Sli::AvailCasPut,
                is_error: outcome_is_err,
                latency_us: 0,
            });
            self.sli.observe(SliObservation {
                sli: Sli::LatencyCasPutP99,
                is_error: outcome_is_err,
                latency_us: 0,
            });
        };

        // Cross-tenant — audit BEFORE rejection.
        if !Self::check_tenant(&req.tenant, &req.caller_tenant) {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::WriteDenied,
                    tenant: req.tenant.clone(),
                    hash: req.claimed_hash.clone(),
                    principal: req.principal.clone(),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(CasHandlerError::AuditFailed)?;
            emit(true);
            return Err(CasHandlerError::CrossTenantDenied {
                caller: req.caller_tenant,
                requested_tenant: req.tenant,
            });
        }

        // WriteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::WriteAttempted,
                tenant: req.tenant.clone(),
                hash: req.claimed_hash.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(CasHandlerError::AuditFailed)?;

        // Verify claimed hash against bytes. The in-memory fake uses
        // a deterministic stand-in (`len`+`first byte hex`) so tests
        // can construct mismatches without pulling blake3. Production
        // handler uses `corelink_hash::CanonicalHash`.
        let actual_hash = fake_hash(&req.bytes);
        if actual_hash != req.claimed_hash {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::CorrectnessViolation,
                    tenant: req.tenant.clone(),
                    hash: req.claimed_hash.clone(),
                    principal: req.principal.clone(),
                    at_unix_ms: req.at_unix_ms,
                })
                .map_err(CasHandlerError::AuditFailed)?;
            self.sli.observe(SliObservation {
                sli: Sli::CorrectnessCas,
                is_error: true,
                latency_us: 0,
            });
            emit(true);
            return Err(CasHandlerError::HashMismatch {
                claimed: req.claimed_hash,
                actual: actual_hash,
            });
        }

        // Durable store.
        let durable = {
            let mut g = self
                .objects
                .lock()
                .map_err(|_| CasHandlerError::Internal("storage lock poisoned".into()))?;
            g.insert((req.tenant.clone(), req.claimed_hash.clone()), req.bytes)
                .is_none()
        };

        // WriteCommitted audit AFTER durable store.
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::WriteCommitted,
                tenant: req.tenant.clone(),
                hash: req.claimed_hash.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(CasHandlerError::AuditFailed)?;
        self.sli.observe(SliObservation {
            sli: Sli::CorrectnessCas,
            is_error: false,
            latency_us: 0,
        });
        emit(false);
        Ok(CasWriteResponse {
            content_hash: req.claimed_hash,
            durable,
        })
    }
}

/// Deterministic stand-in hash for the in-memory fake; lower-case hex
/// of `len`-as-u64-be || `bytes.first()` padded to 64 chars. Real
/// handler uses `corelink_hash::CanonicalHash` (BLAKE3).
#[must_use]
pub fn fake_hash(bytes: &[u8]) -> String {
    let first = bytes.first().copied().unwrap_or(0u8);
    let mut s = format!("{:016x}{:02x}", bytes.len() as u64, first);
    while s.len() < 64 {
        s.push('0');
    }
    s
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::audit::InMemoryAuditSink;
    use crate::observer::InMemorySliObserver;

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
            })
            .expect_err("denied");
        assert!(matches!(err, CasHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        // Audit row is the FIRST thing emitted — fail-CLOSED ordering.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, AuditEventKind::ReadDenied);
        let obs = sli.snapshot().expect("sli");
        assert!(
            obs.iter()
                .any(|o| o.sli == Sli::AvailCasGet && o.is_error),
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
            })
            .expect_err("not found");
        assert!(matches!(err, CasHandlerError::NotFound { .. }));
        let obs = sli.snapshot().expect("sli");
        assert!(
            obs.iter()
                .any(|o| o.sli == Sli::AvailCasGet && o.is_error)
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
                at_unix_ms: 1,
            })
            .expect("write");
        assert_eq!(resp.content_hash, hash);
        assert!(resp.durable);

        let rows = audit.snapshot().expect("audit");
        // WriteAttempted then WriteCommitted — INV ordering.
        assert_eq!(rows[0].kind, AuditEventKind::WriteAttempted);
        assert!(rows.iter().any(|r| r.kind == AuditEventKind::WriteCommitted));
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
                at_unix_ms: 1,
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
        assert!(rows.iter().all(|r| r.kind != AuditEventKind::WriteCommitted));
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
                at_unix_ms: 1,
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
                at_unix_ms: 1,
            })
            .expect_err("denied");
        assert!(matches!(err, CasHandlerError::CrossTenantDenied { .. }));
        let rows = audit.snapshot().expect("audit");
        assert_eq!(rows[0].kind, AuditEventKind::WriteDenied);
    }
}
