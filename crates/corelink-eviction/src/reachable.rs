//! Race-aware reachable check probe — the load-bearing INV-GC-001
//! inheritance gate.
//!
//! ## Canonical SQL (WI-S07-002 §6.1.6)
//!
//! ```sql
//! -- evict_started_at_ms captured BEFORE scan via UPDATE eviction_run;
//! -- commit-then-scan ordering.
//! SELECT COUNT(*) AS active_refcount
//! FROM ac_meta a, json_each(a.blob_refs) j
//! WHERE a.tenant_id = ?
//!   AND j.value = ?                       -- blob_digest
//!   AND a.deleted_at IS NULL
//!   AND a.created_at < ?;                  -- evict_started_at_ms
//! ```
//!
//! ## Strict `<` evict arm + `>=` protect arm
//!
//! The race-aware predicate is the contrapositive of S-06 INV-GC-004:
//! the GC sweep PROTECTS when `ac.created_at >= mark_started_at_ms`
//! (canonical TLA `gc_correctness.tla` L152-154 protect-if-equal-or-newer).
//! Eviction's evict arm uses the contrapositive `ac.created_at <
//! evict_started_at_ms` — strict less-than — so a newly-created AC
//! reference (`ac.created_at == evict_started_at_ms` OR newer)
//! AUTOMATICALLY surfaces the blob as still-reachable.
//!
//! Off-by-one (`<=` instead of `<`) on the evict arm is a data-loss
//! bug: a blob just-re-referenced at the eviction-start instant would
//! be deleted. Pinned by the property test
//! `prop_evict_protect_if_re_referenced_strict_boundary` at boundary
//! offsets `0`, `-1`, `+1`.
//!
//! ## BLOB-only scope (Lote 10.7bis P0-8)
//!
//! The probe iterates `ac_meta.blob_refs` (BLOB-level references); the
//! `chunks` table (refcount-managed; lifecycle owned by S-06 GC) is
//! NEVER consulted by the eviction reachable check. Conversely, the
//! S-06 GC mark phase consults `chunks.refcount` for chunk-level
//! reachability — distinct domains, distinct invariants.

use std::collections::BTreeMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::blob_meta::EvictionBlobDigest;
use crate::error::EvictionError;

/// Forensic projection of an `ac_meta` row that satisfies the evict-arm
/// predicate (`a.created_at < evict_started_at_ms` AND
/// `digest \in blob_refs`). Surfaced via [`AcReferenceProbe::find_active_reference`]
/// when the evict arm fires; carried into the `skipped_reachable` audit
/// record per WI §6.1.7.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcReferenceWitness {
    /// `ac_meta.action_digest` of the offending row.
    pub action_digest: String,
    /// `ac_meta.created_at_ms` (will satisfy
    /// `< evict_started_at_ms`).
    pub created_at_ms: u64,
}

/// Trait surfaced by every `ac_meta` reachable-probe backend (D1
/// reader / in-memory fake).
///
/// The single method [`AcReferenceProbe::find_active_reference`]
/// mirrors the canonical SQL `SELECT COUNT(*)` race-aware predicate.
/// The trait is tenant-scoped at the API surface so a Layer 4 envelope
/// violation surfaces as a fail-closed `Backend` error.
pub trait AcReferenceProbe: Send + Sync + core::fmt::Debug {
    /// Probe whether any `ac_meta` row satisfies BOTH:
    ///
    /// 1. `a.tenant_id = $1`
    /// 2. `digest \in blob_refs` (json_each contains; canonical idiom
    ///    per WI §6.1.6)
    /// 3. `a.deleted_at IS NULL`
    /// 4. **`a.created_at < evict_started_at_ms`** — STRICT less-than
    ///    (Lote 10.7bis P0-6 fix); the contrapositive of S-06
    ///    INV-GC-004 protect-if-`>=` per `gc_correctness.tla` L152-154.
    ///
    /// Returns:
    /// - `Ok(Some(witness))` — the evict arm fires with at least one
    ///   active reference; eviction MUST NOT delete; emit
    ///   `corelink.evict.skipped_reachable`.
    /// - `Ok(None)` — the evict arm is empty (eviction proceeds with
    ///   the soft-delete UPDATE).
    ///
    /// # Errors
    ///
    /// Backend transport errors. Cross-tenant attempts surface as a
    /// fail-closed [`EvictionError::Backend`] error.
    fn find_active_reference(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        evict_started_at_ms: u64,
    ) -> Result<Option<AcReferenceWitness>, EvictionError>;

    /// Count active references — used for the `active_refcount`
    /// surfaced in `EvictionError::BlobReachable`. Returns the same
    /// boundary semantics as [`Self::find_active_reference`].
    ///
    /// # Errors
    ///
    /// Backend transport errors.
    fn count_active_references(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        evict_started_at_ms: u64,
    ) -> Result<u32, EvictionError>;
}

/// In-memory `ac_meta` reachable-probe fake. Tests push `(action_digest,
/// blob_refs, created_at_ms, deleted_at_ms)` triples per tenant; the
/// probe filters by tenant + blob-ref membership + deleted-at-NULL +
/// strict `<` boundary deterministically.
#[derive(Debug, Default)]
pub struct InMemoryAcReferenceProbe {
    inner: Mutex<BTreeMap<Uuid, Vec<AcReferenceRow>>>,
}

#[derive(Clone, Debug)]
struct AcReferenceRow {
    action_digest: String,
    blob_refs: Vec<EvictionBlobDigest>,
    created_at_ms: u64,
    deleted_at_ms: Option<u64>,
}

impl InMemoryAcReferenceProbe {
    /// Construct an empty probe.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an `ac_meta` row tied to a tenant. The `action_digest` is
    /// surfaced by the witness on the evict-arm path; tests can use any
    /// deterministic string (production wiring uses the canonical
    /// 64-char hex form).
    ///
    /// # Errors
    ///
    /// Returns [`EvictionError::Backend`] on mutex poisoning.
    pub fn push_ac_row(
        &self,
        tenant_id: Uuid,
        action_digest: impl Into<String>,
        blob_refs: Vec<EvictionBlobDigest>,
        created_at_ms: u64,
        deleted_at_ms: Option<u64>,
    ) -> Result<(), EvictionError> {
        let mut g = self.inner.lock().map_err(|_| {
            EvictionError::Backend(
                "ac reference probe mutex poisoned".to_string(),
            )
        })?;
        g.entry(tenant_id).or_default().push(AcReferenceRow {
            action_digest: action_digest.into(),
            blob_refs,
            created_at_ms,
            deleted_at_ms,
        });
        Ok(())
    }

    fn snapshot_for_tenant(
        &self,
        tenant_id: Uuid,
    ) -> Result<Vec<AcReferenceRow>, EvictionError> {
        let g = self.inner.lock().map_err(|_| {
            EvictionError::Backend(
                "ac reference probe mutex poisoned".to_string(),
            )
        })?;
        Ok(g.get(&tenant_id).cloned().unwrap_or_default())
    }
}

impl AcReferenceProbe for InMemoryAcReferenceProbe {
    fn find_active_reference(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        evict_started_at_ms: u64,
    ) -> Result<Option<AcReferenceWitness>, EvictionError> {
        let rows = self.snapshot_for_tenant(tenant_id)?;
        for row in rows {
            // Strict `<` evict arm (Lote 10.7bis P0-6); rows with
            // `created_at >= evict_started_at_ms` are PROTECTED (the
            // contrapositive of S-06 INV-GC-004).
            if row.deleted_at_ms.is_none()
                && row.created_at_ms < evict_started_at_ms
                && row.blob_refs.contains(digest)
            {
                return Ok(Some(AcReferenceWitness {
                    action_digest: row.action_digest.clone(),
                    created_at_ms: row.created_at_ms,
                }));
            }
        }
        Ok(None)
    }

    fn count_active_references(
        &self,
        tenant_id: Uuid,
        digest: &EvictionBlobDigest,
        evict_started_at_ms: u64,
    ) -> Result<u32, EvictionError> {
        let rows = self.snapshot_for_tenant(tenant_id)?;
        let mut n: u32 = 0;
        for row in rows {
            if row.deleted_at_ms.is_none()
                && row.created_at_ms < evict_started_at_ms
                && row.blob_refs.contains(digest)
            {
                n = n.saturating_add(1);
            }
        }
        Ok(n)
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
mod tests {
    use super::*;

    fn ten_a() -> Uuid {
        Uuid::from_u128(0xa)
    }

    fn ten_b() -> Uuid {
        Uuid::from_u128(0xb)
    }

    fn dig(seed: u8) -> EvictionBlobDigest {
        let bytes = [seed; 32];
        let mut s = String::with_capacity(64);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        EvictionBlobDigest::new(s).unwrap()
    }

    #[test]
    fn empty_probe_returns_none() {
        let p = InMemoryAcReferenceProbe::new();
        let r = p
            .find_active_reference(ten_a(), &dig(1), 1000)
            .unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn ac_row_below_anchor_is_active_reference() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        // ac.created_at = 999 < evict_started_at = 1000 → active.
        p.push_ac_row(ten_a(), "act-a", vec![d.clone()], 999, None)
            .unwrap();
        let w = p
            .find_active_reference(ten_a(), &d, 1000)
            .unwrap()
            .unwrap();
        assert_eq!(w.action_digest, "act-a");
        assert_eq!(w.created_at_ms, 999);
    }

    /// CRITICAL boundary — `ac.created_at == evict_started_at_ms` MUST
    /// be PROTECTED (canonical TLA `>=` protect arm; equivalent to
    /// strict `<` evict arm).
    #[test]
    fn ac_row_exactly_at_anchor_is_protected_offset_zero() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        // ac.created_at = 1000 == evict_started_at = 1000 → PROTECTED
        // (the strict `<` evict arm fails).
        p.push_ac_row(ten_a(), "act-boundary", vec![d.clone()], 1000, None)
            .unwrap();
        let r = p
            .find_active_reference(ten_a(), &d, 1000)
            .unwrap();
        assert!(
            r.is_none(),
            "boundary off-by-one: ac.created_at == evict_started_at MUST be PROTECTED \
             (i.e. NOT surface as active reference; eviction would delete a re-referenced blob)"
        );
    }

    /// CRITICAL boundary — `ac.created_at > evict_started_at_ms`
    /// (strictly newer) MUST be PROTECTED.
    #[test]
    fn ac_row_above_anchor_is_protected_offset_plus_one() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        p.push_ac_row(ten_a(), "act-newer", vec![d.clone()], 1001, None)
            .unwrap();
        let r = p
            .find_active_reference(ten_a(), &d, 1000)
            .unwrap();
        assert!(r.is_none());
    }

    /// CRITICAL boundary — `ac.created_at == evict_started_at_ms - 1`
    /// (strictly older) IS an active reference (eviction MUST refuse).
    #[test]
    fn ac_row_just_below_anchor_is_active_offset_minus_one() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        p.push_ac_row(ten_a(), "act-older", vec![d.clone()], 999, None)
            .unwrap();
        let r = p
            .find_active_reference(ten_a(), &d, 1000)
            .unwrap();
        assert!(r.is_some());
    }

    #[test]
    fn deleted_ac_row_does_not_protect() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        // ac.deleted_at = Some(_) → row is deleted; even if
        // created_at < anchor it does NOT protect.
        p.push_ac_row(
            ten_a(),
            "act-deleted",
            vec![d.clone()],
            500,
            Some(750),
        )
        .unwrap();
        let r = p
            .find_active_reference(ten_a(), &d, 1000)
            .unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn ac_row_for_different_digest_does_not_match() {
        let p = InMemoryAcReferenceProbe::new();
        p.push_ac_row(ten_a(), "act-other", vec![dig(2)], 500, None)
            .unwrap();
        let r = p
            .find_active_reference(ten_a(), &dig(1), 1000)
            .unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn cross_tenant_ac_row_does_not_protect() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        // Tenant B has the AC reference; Tenant A's eviction sees nothing.
        p.push_ac_row(ten_b(), "act-b", vec![d.clone()], 500, None)
            .unwrap();
        let r = p
            .find_active_reference(ten_a(), &d, 1000)
            .unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn count_active_references_aggregates() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        p.push_ac_row(ten_a(), "a1", vec![d.clone()], 100, None)
            .unwrap();
        p.push_ac_row(ten_a(), "a2", vec![d.clone()], 200, None)
            .unwrap();
        p.push_ac_row(ten_a(), "a3", vec![d.clone()], 300, None)
            .unwrap();
        // All three < anchor 1000.
        let n = p.count_active_references(ten_a(), &d, 1000).unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn count_active_references_excludes_at_anchor() {
        let p = InMemoryAcReferenceProbe::new();
        let d = dig(1);
        p.push_ac_row(ten_a(), "a1", vec![d.clone()], 999, None)
            .unwrap(); // active
        p.push_ac_row(ten_a(), "a2", vec![d.clone()], 1000, None)
            .unwrap(); // protected (== anchor)
        p.push_ac_row(ten_a(), "a3", vec![d.clone()], 1001, None)
            .unwrap(); // protected (> anchor)
        let n = p.count_active_references(ten_a(), &d, 1000).unwrap();
        assert_eq!(n, 1);
    }
}
