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
use crate::error::{CasHandlerError, CasWriteFailure};
use crate::observer::{Sli, SliObservation, SliObserver};
use crate::request::{
    CasBlobEntry, CasDeleteRequest, CasDeleteResponse, CasListRequest, CasListResponse,
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

    /// Cheap existence probe — MUST NOT download or rehash the blob
    /// (HEAD semantics).
    ///
    /// Returns `Ok(true)` when the blob is present, `Ok(false)` when it
    /// is absent (the storage-layer `NotFound`), and propagates every
    /// other [`CasHandlerError`] (cross-tenant denial, audit failure,
    /// internal/transport error) unchanged so the fail-CLOSED contract
    /// of [`Self::read`] is preserved on the probe path.
    ///
    /// # Why this exists
    ///
    /// Bazel REAPI v2 `findMissingBlobs` is supposed to be a cheap
    /// HEAD/exists probe. Probing existence via [`Self::read`] forces a
    /// FULL blob download + SHA-256/BLAKE3 rehash per digest — looped up
    /// to the batch cap (4096) it becomes tens of GiB of R2 GET egress +
    /// rehash CPU per request, repeatable (r34 #10). Implementors backed
    /// by an object store MUST override this with a HEAD/metadata lookup
    /// that transfers no body and performs no rehash.
    ///
    /// # Default
    ///
    /// The default falls back to [`Self::read`] (downloads + rehashes)
    /// and merely maps the outcome to a boolean. It is CORRECT but NOT
    /// cheap — it exists only so existing in-process/test handlers keep
    /// working without an override. Every storage-backed handler
    /// (e.g. the R2 adapter) overrides it with a true HEAD.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CasHandlerError`] per the trait contract;
    /// `NotFound` is mapped to `Ok(false)` rather than propagated.
    fn exists(&self, req: CasReadRequest) -> Result<bool, CasHandlerError> {
        match self.read(req) {
            Ok(_) => Ok(true),
            Err(CasHandlerError::NotFound { .. }) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// OPTIONAL batch existence probe — the same question as
    /// [`Self::exists`], asked about many digests at once so an
    /// implementor can collapse the per-digest network round trips.
    ///
    /// # Contract
    ///
    /// * `None` — this handler provides no batch capability. The caller
    ///   MUST fall back to looping [`Self::exists`], which is the
    ///   behaviour every handler had before this method existed. This is
    ///   the default, so no existing implementor changes semantics.
    /// * `Some(Ok(flags))` — `flags.len() == reqs.len()` and `flags[i]`
    ///   answers `reqs[i]`, **in request order**. `true` = present,
    ///   `false` = absent (the storage-layer `NotFound`, absorbed exactly
    ///   as [`Self::exists`] absorbs it).
    /// * `Some(Err(e))` — the FIRST error in request order, mapped
    ///   exactly as the equivalent [`Self::exists`] call would map it.
    ///   One error fails the whole batch; no partial result is returned.
    ///
    /// # Invariants an implementor MUST preserve
    ///
    /// Every guarantee [`Self::exists`] makes per digest still holds per
    /// digest here — in particular a cross-tenant request is denied
    /// (and audited) BEFORE any storage work is dispatched, and the
    /// mandatory `ReadAttempted` audit rows must have COMMITTED before
    /// any probe result is returned (fail-CLOSED). Concurrency may change
    /// what gets *dispatched*; it must never change what can *reach the
    /// caller*.
    ///
    /// # Errors
    ///
    /// See the contract above — variants of [`CasHandlerError`], never a
    /// `NotFound` (absorbed into `Ok(false)` per digest).
    fn exists_batch(&self, reqs: &[CasReadRequest]) -> Option<Result<Vec<bool>, CasHandlerError>> {
        let _ = reqs;
        None
    }
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

    /// Serve one CAS write and report what the implementation can prove about
    /// any failed durable mutation.
    ///
    /// The default keeps existing implementors source-compatible.  A legacy
    /// [`Self::write`] error does not prove the R2 request had no effect and
    /// has no durable reconciliation intent, so it is reported as
    /// [`crate::MutationEffect::Unknown`]. Production implementations and
    /// accounting decorators MUST override this method before using the effect
    /// to settle a reservation. [`crate::MutationEffect::Pending`] is reserved
    /// for an implementation that has already persisted the exact intent ID.
    ///
    /// # Errors
    ///
    /// Returns [`CasWriteFailure`], preserving the legacy error in
    /// [`CasWriteFailure::cause`] and attaching its proved mutation effect.
    fn write_with_effect(&self, req: CasWriteRequest) -> Result<CasWriteResponse, CasWriteFailure> {
        self.write(req).map_err(CasWriteFailure::unknown)
    }
}

/// Trait every concrete CAS delete handler implements (D-8).
///
/// DELETE is **idempotent** by contract: deleting a present blob and
/// deleting an absent one both return `Ok` (the route maps both to HTTP
/// 204 No Content). Implementors **MUST**:
///
/// 1. On `CrossTenantDenied`, emit `AuditEventKind::DeleteDenied` BEFORE
///    returning (fail-CLOSED ordering, mirrors the write path).
/// 2. Emit `AuditEventKind::DeleteAttempted` BEFORE the storage delete.
/// 3. Emit `Sli::AvailCasPut` + `Sli::LatencyCasPutP99` observations on
///    EVERY return path (delete folds into the PUT availability bucket
///    — both are mutations on the canonical-18 SLI registry).
/// 4. On success emit `AuditEventKind::DeleteCommitted`.
pub trait CasDeleteHandler: Send + Sync + core::fmt::Debug {
    /// Delete one CAS blob (idempotent).
    ///
    /// # Errors
    ///
    /// Returns variants of [`CasHandlerError`] per the trait contract.
    fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError>;
}

/// Trait every concrete CAS list handler implements (D-8).
///
/// Enumeration MUST stay within the authenticated tenant's derived
/// storage prefix — a tenant must never enumerate another tenant's
/// blobs. Implementors **MUST**:
///
/// 1. On `CrossTenantDenied`, emit `AuditEventKind::ListDenied` BEFORE
///    returning.
/// 2. Emit `AuditEventKind::ListAttempted` BEFORE the enumeration.
/// 3. Emit `Sli::AvailCasGet` + `Sli::LatencyCasGetP99` observations on
///    EVERY return path (list folds into the GET availability bucket).
pub trait CasListHandler: Send + Sync + core::fmt::Debug {
    /// Enumerate one page of the tenant's CAS blobs.
    ///
    /// # Errors
    ///
    /// Returns variants of [`CasHandlerError`] per the trait contract.
    fn list(&self, req: CasListRequest) -> Result<CasListResponse, CasHandlerError>;
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
    pub fn new(audit: std::sync::Arc<dyn AuditSink>, sli: std::sync::Arc<dyn SliObserver>) -> Self {
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
        let bytes = match g.get(&(req.tenant.clone(), req.hash.clone())) {
            None => Err(CasHandlerError::NotFound {
                tenant: req.tenant.clone(),
                hash: req.hash.clone(),
            }),
            Some(bytes) => {
                if let Some(limit) = req.max_bytes {
                    let actual_bytes = bytes.len() as u64;
                    if actual_bytes > limit {
                        return {
                            drop(g);
                            emit(true);
                            Err(CasHandlerError::ObjectTooLarge {
                                actual_bytes,
                                limit_bytes: limit,
                            })
                        };
                    }
                }
                Ok(bytes.clone())
            }
        };
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
        if !req.is_authorized_for_caller() {
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

        // Verify claimed hash against bytes under the keyspace's tagged
        // digest function (`req.algo`): the deterministic `fake_hash`
        // stand-in for the native BLAKE3 keyspace (so tests can construct
        // mismatches without pulling blake3) or a real SHA-256 for the Bazel
        // REAPI `Sha256` keyspace. Production handler uses
        // `corelink_hash::CanonicalHash` / `sha2` respectively.
        let actual_hash = expected_content_hash(req.algo, &req.bytes);
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

impl CasDeleteHandler for InMemoryCasHandler {
    fn delete(&self, req: CasDeleteRequest) -> Result<CasDeleteResponse, CasHandlerError> {
        // Delete folds availability into the PUT (mutation) SLI bucket.
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

        // Cross-tenant — audit BEFORE rejection (fail-CLOSED ordering).
        if !Self::check_tenant(&req.tenant, &req.caller_tenant) {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::DeleteDenied,
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

        // DeleteAttempted audit BEFORE mutation.
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::DeleteAttempted,
                tenant: req.tenant.clone(),
                hash: req.hash.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(CasHandlerError::AuditFailed)?;

        // Idempotent delete: remove if present, no-op if absent. Capture the
        // removed blob's length for `reclaimed_bytes` (byte-accounting release).
        let (existed, reclaimed_bytes) = {
            let mut g = self
                .objects
                .lock()
                .map_err(|_| CasHandlerError::Internal("storage lock poisoned".into()))?;
            match g.remove(&(req.tenant.clone(), req.hash.clone())) {
                Some(bytes) => (true, bytes.len() as u64),
                None => (false, 0u64),
            }
        };

        // DeleteCommitted audit AFTER the delete (fires whether or not
        // the blob existed — DELETE is idempotent).
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::DeleteCommitted,
                tenant: req.tenant.clone(),
                hash: req.hash.clone(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(CasHandlerError::AuditFailed)?;
        emit(false);
        Ok(CasDeleteResponse::with_reclaimed(existed, reclaimed_bytes))
    }
}

impl CasListHandler for InMemoryCasHandler {
    fn list(&self, req: CasListRequest) -> Result<CasListResponse, CasHandlerError> {
        // List folds availability into the GET (read) SLI bucket.
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

        // Cross-tenant — audit BEFORE rejection.
        if !Self::check_tenant(&req.tenant, &req.caller_tenant) {
            self.audit
                .emit(AuditEvent {
                    kind: AuditEventKind::ListDenied,
                    tenant: req.tenant.clone(),
                    hash: String::new(),
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

        // ListAttempted audit BEFORE enumeration.
        self.audit
            .emit(AuditEvent {
                kind: AuditEventKind::ListAttempted,
                tenant: req.tenant.clone(),
                hash: String::new(),
                principal: req.principal.clone(),
                at_unix_ms: req.at_unix_ms,
            })
            .map_err(CasHandlerError::AuditFailed)?;

        // Enumerate this tenant's blobs, sorted by hash for a stable,
        // cursor-paginatable order. The opaque cursor is the last hash
        // returned on the prior page; this page starts strictly after it.
        let g = self
            .objects
            .lock()
            .map_err(|_| CasHandlerError::Internal("storage lock poisoned".into()))?;
        let mut hashes: Vec<(String, usize)> = g
            .iter()
            .filter(|((t, _), _)| t == &req.tenant)
            .map(|((_, h), bytes)| (h.clone(), bytes.len()))
            .collect();
        drop(g);
        hashes.sort_by(|a, b| a.0.cmp(&b.0));

        let after = req.cursor.clone();
        let limit = req.limit.max(1) as usize;
        let mut blobs = Vec::new();
        let mut next_cursor: Option<String> = None;
        for (h, size) in hashes
            .into_iter()
            .filter(|(h, _)| after.as_ref().map_or(true, |c| h > c))
        {
            if blobs.len() == limit {
                // There is at least one more entry beyond this page; the
                // last entry we emitted is the next cursor.
                next_cursor = blobs.last().map(|e: &CasBlobEntry| e.hash.clone());
                break;
            }
            // The in-memory fake has no real timestamp; emit the unix
            // epoch as a deterministic RFC-3339 stand-in (the R2 impl
            // surfaces the object's true last-modified).
            blobs.push(CasBlobEntry::new(h, size as u64, "1970-01-01T00:00:00Z"));
        }

        emit(false);
        Ok(CasListResponse { blobs, next_cursor })
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

/// Real SHA-256 of `bytes` as 64-char lowercase hex — the Bazel REAPI v2
/// content-addressing function. Used by the in-memory fake's write verify
/// when [`crate::DigestAlgo::Sha256`] is the request's keyspace tag.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// The content hash the in-memory fake expects for a given keyspace tag:
/// the deterministic [`fake_hash`] stand-in for [`crate::DigestAlgo::Blake3`]
/// (native/sccache), or a real [`sha256_hex`] for [`crate::DigestAlgo::Sha256`]
/// (the Bazel REAPI keyspace). Keeps the fake faithful to the surface-tagged
/// single-function keyspace partitioning.
#[must_use]
pub fn expected_content_hash(algo: crate::DigestAlgo, bytes: &[u8]) -> String {
    match algo {
        crate::DigestAlgo::Blake3 => fake_hash(bytes),
        crate::DigestAlgo::Sha256 => sha256_hex(bytes),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
#[path = "tests.rs"]
mod tests;
