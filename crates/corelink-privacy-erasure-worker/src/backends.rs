//! 12-arm backend adapter trait + per-backend in-memory implementations.
//!
//! Per WI-S11-002 §6.3 the canonical orchestrator fans out to 12
//! independent backend adapters (8 effective + 4 pseudonymized;
//! privacy_model.md §6.2 source-of-truth pós Lote 10.11.0-bis). Every
//! adapter exposes the same trait surface so the orchestrator's fanout
//! step is structurally 12-deep without per-backend special-case
//! branching.
//!
//! ## Effective vs pseudonymized branch
//!
//! Per [`crate::event::BackendKind::is_effective`]:
//!
//! - **Effective** (8): a successful run returns
//!   [`crate::event::BackendErasureOutcome::Erased`] with
//!   `records_deleted >= 0` and the verification fingerprint equals
//!   [`CANONICAL_EMPTY_TENANT_HASH`].
//! - **Pseudonymized** (4): a successful run returns
//!   [`crate::event::BackendErasureOutcome::Pseudonymized`] with
//!   `records_redacted >= 0` and the canonical
//!   [`crate::pseudonymize::PII_REDACTED_MARKER_KEY`] presence on
//!   every retained row.
//!
//! ## Refcount-aware semantics (R2 CAS — S-07 dedup safety)
//!
//! The R2 CAS adapter never hard-deletes a blob shared between tenants
//! (cross-tenant break is CRITICAL per WI-S11-002 §28 R-003). Per
//! AC-003: the adapter decrements refcount via the shared chunks
//! table, then `subject_unaffiliated` (refcount > 0 post-decrement)
//! returns [`crate::event::BackendErasureOutcome::NotApplicable`] +
//! the blob remains; `subject_dedicated` (refcount = 0) returns
//! [`crate::event::BackendErasureOutcome::Erased`] + the blob is
//! tombstoned for GC sweep grace 72h.
//!
//! ## Stripe adapter — `Customer.update` NOT delete
//!
//! Per WI-S11-002 §9.3 + AC-004: Stripe customer.delete is irreversible
//! and breaks invoice integrity (GAAP ASC 606 + LGPD Art. 16 fiscal
//! 5y). The adapter pseudonymizes email/name/address via
//! `Customer.update` but the Stripe customer object survives so the
//! invoice trail remains queryable.
//!
//! ## Loki adapter — `/loki/api/v1/delete` HTTP API
//!
//! Per WI-S11-002 §6 step 13 + AC-005/006: the Loki adapter calls
//! `/loki/api/v1/delete` which queues a deletion for the next
//! retention compaction tick. The 24h verification job sweep DOES
//! account for the cold archive settle delay; verification mismatch
//! pre-24h is NOT an outcome failure — it is `Erased { records_deleted: N }`
//! with the canonical 24h-grace verification fingerprint to be
//! resolved by the verification cron at `queued_at + 24h`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::error::ErasureBackendError;
use crate::event::{BackendErasureOutcome, BackendKind};

pub mod d1;
pub mod kv;
pub mod loki;
pub mod neon_billing;
pub mod neon_main;
pub mod neon_pitr_pseudo;
pub mod r2_ac;
pub mod r2_audit_pseudo;
pub mod r2_cas;
pub mod r2_cas_legalhold_pseudo;
pub mod r2_evidence_pseudo;
pub mod stripe;

/// Canonical "no rows for tenant" sentinel: BLAKE3-256 of the
/// canonical zero-byte preimage. Effective backends compare their
/// remaining-rows fingerprint against this on the 24h verification
/// sweep; mismatch triggers
/// [`crate::event::BackendErasureOutcome::PartialFailure`].
///
/// Hard-coded so the verifier + the adapter never disagree on the
/// sentinel even if the blake3 crate changes its internal hash mode
/// in a future major version. Pinned by
/// `tests::empty_tenant_hash_matches_blake3_empty`.
pub const CANONICAL_EMPTY_TENANT_HASH: [u8; 32] = [
    0xaf, 0x13, 0x49, 0xb9, 0xf5, 0xf9, 0xa1, 0xa6, 0xa0, 0x40, 0x4d, 0xea, 0x36, 0xdc, 0xc9, 0x49,
    0x9b, 0xcb, 0x25, 0xc9, 0xad, 0xc1, 0x12, 0xb7, 0xcc, 0x9a, 0x93, 0xca, 0xe4, 0x1f, 0x32, 0x62,
];

/// Verification context handed to a backend adapter at the 24h sweep.
/// Per WI-S11-002 AC-005/006 the canonical contract is "every backend
/// returns its own remaining-rows fingerprint; the orchestrator
/// compares against the canonical sentinel".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerificationContext {
    /// Canonical UUIDv7 DSR id (matches the request).
    pub dsr_id: Uuid,
    /// Tenant id (S-03 inheritance — NEVER the request body).
    pub tenant_id: Uuid,
    /// Data subject id (PAT principal post-authn).
    pub subject_id: Uuid,
    /// Wall-clock instant the canonical verification sweep started
    /// (Unix epoch ms; production wiring `now_ms = queued_at + 24h`).
    pub now_ms: u64,
}

/// Per-backend erasure adapter trait. Production wiring at WI-S11-008
/// binds the 12 canonical adapters to live transport (Neon HTTP via
/// wasm-bindgen / R2 DeleteObject / D1 row purge / KV key prefix
/// DELETE / Stripe `Customer.update` via S-10 StripeClient wrapper /
/// Loki `/loki/api/v1/delete` HTTP API / R2 audit Object Lock 7y HKDF
/// pseudonymization / Neon PITR backup tombstone replay / R2 CAS
/// legal_hold partition pseudonymize / R2 evidence-* buckets
/// pseudonymize).
pub trait BackendErasureAdapter: Send + Sync + core::fmt::Debug {
    /// Canonical 12-arm backend kind this adapter implements.
    fn kind(&self) -> BackendKind;

    /// Erase (effective backends) or pseudonymize (pseudonymized
    /// backends) every row associated with `(tenant_id, subject_id)`.
    /// The orchestrator dispatches this AFTER the canonical
    /// `dev.hugr.corelink.dsr.erasure.started.v1` audit row emitted +
    /// AFTER the per-instance F-001 mutex acquired.
    ///
    /// # Errors
    ///
    /// - [`ErasureBackendError::Transport`] for transient transport
    ///   failures (orchestrator retries up to retry_count = 5).
    /// - [`ErasureBackendError::NotApplicable`] when the backend has
    ///   no rows matching `(tenant_id, subject_id)` (returned as
    ///   [`BackendErasureOutcome::NotApplicable`] with reason context).
    fn erase(
        &self,
        tenant_id: Uuid,
        subject_id: Uuid,
        erasure_salt: &[u8; 32],
        legal_hold: bool,
    ) -> Result<BackendErasureOutcome, ErasureBackendError>;

    /// Compute the canonical remaining-rows fingerprint for
    /// `(tenant_id, subject_id)` post-erasure. Called by the 24h
    /// verification sweep; the orchestrator compares against
    /// [`CANONICAL_EMPTY_TENANT_HASH`] (effective) or asserts marker
    /// presence (pseudonymized) per WI AC-005.
    ///
    /// # Errors
    ///
    /// - [`ErasureBackendError::Transport`] for transient transport
    ///   failures.
    fn verification_hash(&self, ctx: VerificationContext) -> Result<[u8; 32], ErasureBackendError>;
}

/// Canonical in-memory backend adapter. Stores per-(tenant, subject)
/// row sets in a `HashMap<(tenant_id, subject_id), Vec<...>>` and
/// performs an effective DELETE (clears the slot) or pseudonymize
/// (leaves rows + records the canonical marker). The fanout-12-deep
/// orchestrator instantiates 12 of these — one per `BackendKind`.
#[derive(Debug, Clone)]
pub struct InMemoryBackendErasureAdapter {
    kind: BackendKind,
    rows: RowsHandle,
    /// When true, the adapter pretends to be unreachable (transport
    /// failure) on every call. Used by the chaos test fixtures.
    fail_transport: Arc<Mutex<bool>>,
}

/// Canonical row store handle: per-(tenant, subject) row sets in a
/// shared map under a per-instance mutex. Pinned as a typed alias to
/// keep the underlying generics in one place.
type RowsHandle = Arc<Mutex<HashMap<(Uuid, Uuid), Vec<InMemoryRow>>>>;

/// In-memory row payload (canonical opaque shape; the orchestrator
/// only inspects the canonical `pii_redacted` marker).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryRow {
    /// Free-form payload bytes (test fixture for the canonical row
    /// payload — production wiring would carry typed tenant data).
    pub payload: Vec<u8>,
    /// Whether the row carries the canonical `pii_redacted=true`
    /// marker (set by pseudonymize) — drives the verification sweep
    /// for pseudonymized backends.
    pub pii_redacted: bool,
    /// Optional canonical pseudonym (sha256 hex 64 chars) when
    /// `pii_redacted == true`.
    pub pseudonym: Option<String>,
    /// Per-row refcount (used by the R2 CAS refcount-aware adapter to
    /// model S-07 dedup safety). Defaults to 1 (single-tenant).
    pub refcount: u32,
}

impl InMemoryRow {
    /// Construct a canonical fresh row with refcount = 1 + no marker.
    #[must_use]
    pub fn new(payload: Vec<u8>) -> Self {
        Self {
            payload,
            pii_redacted: false,
            pseudonym: None,
            refcount: 1,
        }
    }

    /// Builder: set the canonical refcount (>= 1).
    #[must_use]
    pub fn with_refcount(mut self, refcount: u32) -> Self {
        self.refcount = refcount;
        self
    }
}

impl InMemoryBackendErasureAdapter {
    /// Construct a fresh per-`kind` in-memory adapter.
    #[must_use]
    pub fn new(kind: BackendKind) -> Self {
        Self {
            kind,
            rows: Arc::new(Mutex::new(HashMap::new())),
            fail_transport: Arc::new(Mutex::new(false)),
        }
    }

    /// Insert canonical test rows for `(tenant_id, subject_id)`.
    pub fn insert_rows(&self, tenant_id: Uuid, subject_id: Uuid, rows: Vec<InMemoryRow>) {
        if let Ok(mut g) = self.rows.lock() {
            g.entry((tenant_id, subject_id)).or_default().extend(rows);
        }
    }

    /// Snapshot the canonical row count for `(tenant_id, subject_id)`.
    #[must_use]
    pub fn row_count(&self, tenant_id: Uuid, subject_id: Uuid) -> usize {
        match self.rows.lock() {
            Ok(g) => g
                .get(&(tenant_id, subject_id))
                .map_or(0, std::vec::Vec::len),
            Err(_) => 0,
        }
    }

    /// Whether 100% of remaining rows for `(tenant_id, subject_id)`
    /// carry the canonical `pii_redacted=true` marker. The 24h
    /// verification sweep asserts `true` for pseudonymized backends.
    #[must_use]
    pub fn all_redacted(&self, tenant_id: Uuid, subject_id: Uuid) -> bool {
        match self.rows.lock() {
            Ok(g) => g
                .get(&(tenant_id, subject_id))
                .map_or(true, |rows| rows.iter().all(|r| r.pii_redacted)),
            Err(_) => false,
        }
    }

    /// Toggle the canonical chaos-test transport-failure flag. When
    /// `true`, every subsequent `erase` / `verification_hash` call
    /// returns [`ErasureBackendError::Transport`].
    pub fn set_transport_failure(&self, fail: bool) {
        if let Ok(mut g) = self.fail_transport.lock() {
            *g = fail;
        }
    }
}

impl BackendErasureAdapter for InMemoryBackendErasureAdapter {
    fn kind(&self) -> BackendKind {
        self.kind
    }

    fn erase(
        &self,
        tenant_id: Uuid,
        subject_id: Uuid,
        erasure_salt: &[u8; 32],
        legal_hold: bool,
    ) -> Result<BackendErasureOutcome, ErasureBackendError> {
        // Chaos test transport failure pre-check.
        let fail = match self.fail_transport.lock() {
            Ok(g) => *g,
            Err(_) => false,
        };
        if fail {
            return Err(ErasureBackendError::Transport(format!(
                "induced transport failure for backend={}",
                self.kind
            )));
        }

        let mut guard = self.rows.lock().map_err(|_| {
            ErasureBackendError::Transport("in-memory adapter mutex poisoned".to_string())
        })?;

        // Legal hold gate: effective backends skip with NotApplicable
        // (CTRL-PRIV-033 + AC-010); pseudonymized backends still
        // execute (Object Lock retains the original; the secondary
        // index update preserves immutability).
        if legal_hold && self.kind.is_effective() {
            return Ok(BackendErasureOutcome::NotApplicable);
        }

        let key = (tenant_id, subject_id);

        // R2 CAS refcount-aware: emulate S-07 dedup safety. If the
        // single canonical row carries refcount > 1 we decrement +
        // return NotApplicable (subject_unaffiliated); refcount == 1
        // we delete + Erased.
        if matches!(self.kind, BackendKind::R2Cas) {
            let removed = if let Some(rows) = guard.get_mut(&key) {
                let mut deleted: u64 = 0;
                rows.retain_mut(|r| {
                    if r.refcount > 1 {
                        r.refcount = r.refcount.saturating_sub(1);
                        true // keep (subject_unaffiliated)
                    } else {
                        deleted = deleted.saturating_add(1);
                        false // delete (subject_dedicated)
                    }
                });
                if rows.is_empty() {
                    guard.remove(&key);
                }
                deleted
            } else {
                0
            };
            if removed == 0 {
                return Ok(BackendErasureOutcome::NotApplicable);
            }
            return Ok(BackendErasureOutcome::Erased {
                records_deleted: removed,
            });
        }

        // Pseudonymized backends: substitute PII fields per row +
        // insert canonical marker (pii_redacted=true + sha256
        // pseudonym). Record count is preserved (Object Lock 7y).
        if self.kind.is_pseudonymized() {
            let pseudonym =
                corelink_privacy_pseudonymize::pseudonymize_subject_id(subject_id, erasure_salt);
            let pseudonym_hex = pseudonym.to_hex();

            let count = if let Some(rows) = guard.get_mut(&key) {
                let mut redacted: u64 = 0;
                for r in rows.iter_mut() {
                    if !r.pii_redacted {
                        r.pii_redacted = true;
                        r.pseudonym = Some(pseudonym_hex.clone());
                        redacted = redacted.saturating_add(1);
                    }
                }
                redacted
            } else {
                0
            };

            return Ok(BackendErasureOutcome::Pseudonymized {
                records_redacted: count,
            });
        }

        // Effective backends (non-R2-CAS): hard-DELETE every row for
        // (tenant, subject).
        let removed = guard.remove(&key);
        let count: u64 = match removed {
            Some(rows) => u64::try_from(rows.len()).unwrap_or(u64::MAX),
            None => 0,
        };

        if count == 0 {
            return Ok(BackendErasureOutcome::NotApplicable);
        }

        Ok(BackendErasureOutcome::Erased {
            records_deleted: count,
        })
    }

    fn verification_hash(&self, ctx: VerificationContext) -> Result<[u8; 32], ErasureBackendError> {
        let fail = match self.fail_transport.lock() {
            Ok(g) => *g,
            Err(_) => false,
        };
        if fail {
            return Err(ErasureBackendError::Transport(format!(
                "induced transport failure for backend={}",
                self.kind
            )));
        }

        let guard = self.rows.lock().map_err(|_| {
            ErasureBackendError::Transport("in-memory adapter mutex poisoned".to_string())
        })?;

        let key = (ctx.tenant_id, ctx.subject_id);
        let rows = guard.get(&key);

        // Effective backend: 0 rows → canonical sentinel; non-zero →
        // BLAKE3 fingerprint of the row count + payload bytes (the
        // verification sweep flags this as VerificationMismatch).
        if self.kind.is_effective() {
            return Ok(match rows {
                None => CANONICAL_EMPTY_TENANT_HASH,
                Some(rs) if rs.is_empty() => CANONICAL_EMPTY_TENANT_HASH,
                Some(rs) => {
                    let mut h = blake3::Hasher::new();
                    h.update(self.kind.as_str().as_bytes());
                    h.update(&u64::try_from(rs.len()).unwrap_or(u64::MAX).to_le_bytes());
                    for r in rs {
                        h.update(&r.payload);
                    }
                    *h.finalize().as_bytes()
                }
            });
        }

        // Pseudonymized backend: 100% pii_redacted=true → canonical
        // sentinel; any non-redacted row → mismatch fingerprint.
        let all_redacted = match rows {
            None => true,
            Some(rs) => rs.iter().all(|r| r.pii_redacted),
        };
        if all_redacted {
            return Ok(CANONICAL_EMPTY_TENANT_HASH);
        }
        let mut h = blake3::Hasher::new();
        h.update(self.kind.as_str().as_bytes());
        h.update(b"pseudonymized-mismatch");
        if let Some(rs) = rows {
            for r in rs {
                let m: u8 = if r.pii_redacted { 1 } else { 0 };
                h.update(&[m]);
            }
        }
        Ok(*h.finalize().as_bytes())
    }
}

/// Build the canonical 12-adapter list (one in-memory adapter per
/// `BackendKind` arm, in canonical order). Pinned for the orchestrator
/// fanout step + chaos test fixtures.
#[must_use]
pub fn canonical_in_memory_adapters() -> Vec<Arc<InMemoryBackendErasureAdapter>> {
    crate::event::canonical_backend_kinds()
        .iter()
        .map(|k| Arc::new(InMemoryBackendErasureAdapter::new(*k)))
        .collect()
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

    fn fixed_uuid(seed: u8) -> Uuid {
        let mut b = [0u8; 16];
        for (i, x) in b.iter_mut().enumerate() {
            *x = seed.wrapping_add(i as u8);
        }
        Uuid::from_bytes(b)
    }

    #[test]
    fn canonical_adapters_count_matches_backend_count() {
        let v = canonical_in_memory_adapters();
        assert_eq!(v.len(), crate::event::BACKEND_COUNT);
    }

    #[test]
    fn canonical_adapters_in_canonical_order() {
        let v = canonical_in_memory_adapters();
        for (i, expected) in crate::event::canonical_backend_kinds().iter().enumerate() {
            assert_eq!(v.get(i).unwrap().kind(), *expected);
        }
    }

    #[test]
    fn empty_tenant_hash_matches_blake3_empty() {
        let computed = *blake3::hash(b"").as_bytes();
        assert_eq!(CANONICAL_EMPTY_TENANT_HASH, computed);
    }

    #[test]
    fn effective_adapter_erase_then_verify_empty() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::D1);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        adapter.insert_rows(t, s, vec![InMemoryRow::new(b"row1".to_vec())]);
        let salt = [1u8; 32];
        let outcome = adapter.erase(t, s, &salt, false).unwrap();
        let is_erased = matches!(
            outcome,
            BackendErasureOutcome::Erased { records_deleted: 1 }
        );
        assert!(is_erased);
        let hash = adapter
            .verification_hash(VerificationContext {
                dsr_id: fixed_uuid(3),
                tenant_id: t,
                subject_id: s,
                now_ms: 100,
            })
            .unwrap();
        assert_eq!(hash, CANONICAL_EMPTY_TENANT_HASH);
    }

    #[test]
    fn pseudonymized_adapter_erase_then_verify_redacted() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::R2AuditPseudo);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        adapter.insert_rows(
            t,
            s,
            vec![
                InMemoryRow::new(b"audit-row-1".to_vec()),
                InMemoryRow::new(b"audit-row-2".to_vec()),
            ],
        );
        let salt = [3u8; 32];
        let outcome = adapter.erase(t, s, &salt, false).unwrap();
        let is_pseudo = matches!(
            outcome,
            BackendErasureOutcome::Pseudonymized {
                records_redacted: 2
            }
        );
        assert!(is_pseudo);
        // Object Lock 7y: rows are retained, NOT deleted.
        assert_eq!(adapter.row_count(t, s), 2);
        assert!(adapter.all_redacted(t, s));
        let hash = adapter
            .verification_hash(VerificationContext {
                dsr_id: fixed_uuid(3),
                tenant_id: t,
                subject_id: s,
                now_ms: 100,
            })
            .unwrap();
        assert_eq!(hash, CANONICAL_EMPTY_TENANT_HASH);
    }

    #[test]
    fn r2_cas_refcount_aware_keeps_shared_blob() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::R2Cas);
        let t1 = fixed_uuid(1);
        let s = fixed_uuid(2);
        // Shared blob with refcount=2 between (t1, s) and another
        // tenant; the row is the canonical metadata edge for s in t1.
        adapter.insert_rows(
            t1,
            s,
            vec![InMemoryRow::new(b"shared-blob".to_vec()).with_refcount(2)],
        );
        let salt = [9u8; 32];
        let outcome = adapter.erase(t1, s, &salt, false).unwrap();
        let is_na = matches!(outcome, BackendErasureOutcome::NotApplicable);
        assert!(is_na);
        // The row remains in the adapter (refcount decremented to 1).
        assert_eq!(adapter.row_count(t1, s), 1);
    }

    #[test]
    fn r2_cas_refcount_aware_deletes_dedicated_blob() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::R2Cas);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        adapter.insert_rows(
            t,
            s,
            vec![InMemoryRow::new(b"dedicated-blob".to_vec()).with_refcount(1)],
        );
        let salt = [9u8; 32];
        let outcome = adapter.erase(t, s, &salt, false).unwrap();
        let is_erased = matches!(
            outcome,
            BackendErasureOutcome::Erased { records_deleted: 1 }
        );
        assert!(is_erased);
        assert_eq!(adapter.row_count(t, s), 0);
    }

    #[test]
    fn legal_hold_skips_effective_backend() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::NeonMain);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        adapter.insert_rows(t, s, vec![InMemoryRow::new(b"row".to_vec())]);
        let salt = [9u8; 32];
        let outcome = adapter.erase(t, s, &salt, true).unwrap();
        let is_na = matches!(outcome, BackendErasureOutcome::NotApplicable);
        assert!(is_na);
        // Row remains untouched.
        assert_eq!(adapter.row_count(t, s), 1);
    }

    #[test]
    fn legal_hold_does_not_skip_pseudonymized_backend() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::R2AuditPseudo);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        adapter.insert_rows(t, s, vec![InMemoryRow::new(b"row".to_vec())]);
        let salt = [9u8; 32];
        let outcome = adapter.erase(t, s, &salt, true).unwrap();
        let is_pseudo = matches!(
            outcome,
            BackendErasureOutcome::Pseudonymized {
                records_redacted: 1
            }
        );
        assert!(is_pseudo);
    }

    #[test]
    fn transport_failure_returns_error() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::D1);
        adapter.set_transport_failure(true);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        let salt = [1u8; 32];
        let err = adapter.erase(t, s, &salt, false).unwrap_err();
        let is_transport = matches!(err, ErasureBackendError::Transport(_));
        assert!(is_transport);
    }

    #[test]
    fn empty_backend_returns_not_applicable() {
        let adapter = InMemoryBackendErasureAdapter::new(BackendKind::Stripe);
        let t = fixed_uuid(1);
        let s = fixed_uuid(2);
        let salt = [1u8; 32];
        let outcome = adapter.erase(t, s, &salt, false).unwrap();
        let is_na = matches!(outcome, BackendErasureOutcome::NotApplicable);
        assert!(is_na);
    }
}
