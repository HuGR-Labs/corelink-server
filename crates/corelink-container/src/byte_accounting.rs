//! Per-tenant **storage byte accounting** (red-team finding #1, HIGH).
//!
//! # Why this exists
//!
//! The `tenant_storage_state` row (migration 0008) carries the authoritative
//! `bytes_used` counter the eviction worker + storage-quota policy read to
//! decide the per-tier storage cap. But on the **container data plane** NOTHING
//! ever incremented it: a successful CAS/AC/Turbo write committed bytes to R2
//! and returned success WITHOUT touching `bytes_used`. The storage cap was
//! therefore structurally **inert** — a Free tenant could store unbounded TB at
//! `$0`, because the counter the cap reads never moved.
//!
//! [`ByteAccountant`] closes that gap. After a successful store write the
//! billable handler calls [`ByteAccountant::accrue`], which performs an ATOMIC
//! check-and-accrue against `tenant_storage_state`:
//!
//! ```sql
//! INSERT INTO tenant_storage_state
//!     (tenant_id, region, bytes_used, bytes_quota,
//!      bytes_used_updated_at_ms, last_synced_at_ms,
//!      bytes_reclaimed_lifetime, created_at_ms, updated_at_ms)
//!   VALUES (?1, ?2, ?3, 0, ?4, ?4, 0, ?4, ?4)
//!   ON CONFLICT(tenant_id, region) DO UPDATE SET
//!     bytes_used               = bytes_used + ?3,
//!     bytes_used_updated_at_ms = ?4,
//!     updated_at_ms            = ?4
//!   WHERE tenant_storage_state.bytes_quota = 0
//!      OR tenant_storage_state.bytes_used + ?3
//!         <= tenant_storage_state.bytes_quota
//!   RETURNING bytes_used
//! ```
//!
//! The add happens **in the DB** (`bytes_used = bytes_used + ?3`), not in the
//! app, so concurrent accruals sum instead of clobbering each other (the same
//! lost-update discipline as [`crate::tenant_quota::D1QuotaStore`]). The cap
//! check is **serialized with the increment** in the one statement, so two
//! concurrent over-cap writes cannot both read the same baseline and both pass.
//!
//! When the row's `bytes_quota` is `0` (not yet synced from `tenant_quota` by
//! the DO) the write is **uncapped** — accrual always succeeds; the counter
//! still moves so the cap becomes live the moment the DO populates the quota.
//!
//! # Fail-CLOSED
//!
//! The billable handler treats this as a money/quota gate: an [`AccrueOutcome`]
//! of [`AccrueOutcome::OverCap`] ⇒ the write must be REJECTED (the cache must
//! not serve unbounded storage), and a transport `Err` ⇒ the handler fails
//! CLOSED (503) — we do not return success for a write we could not account.
//!
//! # Deletes
//!
//! [`ByteAccountant::release`] is the inverse: a **saturating** decrement of
//! `bytes_used` (clamped at `0` by the table's
//! `CHECK (bytes_used >= 0)` and a `MAX(0, …)` in SQL) wired into the CAS/AC
//! delete handlers so reclaimed bytes free the tenant's headroom.
//!
//! # Env gating (dev/CI = absent)
//!
//! [`byte_accountant_from_env`] builds the production accountant from the same
//! [`crate::storage::StorageEnv`] the other D1 adapters use, returning `None`
//! when the env is unset (dev/CI) — exactly mirroring
//! [`crate::tenant_quota::quota_guard_from_env`] /
//! [`crate::routes::QuotaGate::from_env`]. Billable handlers hold an
//! `Option<Arc<ByteAccountant>>`; `None` ⇒ accounting is simply not enforced.

#![forbid(unsafe_code)]

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

#[cfg(test)]
use crate::customer_d1::ByokCryptoMode;
#[cfg(test)]
use crate::storage::byok_cas::{
    engagement_for, ByokConfigCache, ByokEngagement, BYOK_CLB1_OVERHEAD, BYOK_CLB2_OVERHEAD,
};

// B126-M2 REANCHOR MANIFEST (byte accounting).
// Ordered include fragments below are the sole composition point; this keeps
// the module namespace/API and execution order unchanged while preventing
// recomposition into a god-file. Symbols moved: region_from_env, STORAGE_QUOTA_HEADER, storage_quota_from_headers, AccrueOutcome, ByteStore, ByteAccountant, current_unix_ms, byte_accountant_from_env, D1ByteStore, OVER_CAP_SENTINEL, ACCT_UNAVAILABLE_SENTINEL, block_on_accrue, byok_committed_len, block_on_release, CAS_LOCK_SHARDS, AccountingCasHandler, AccountingAcHandler.
include!("byte_accounting/b126_m2_impl_01.rs");
include!("byte_accounting/b126_m2_impl_02.rs");

#[cfg(test)]
mod b126_m2_reanchor {
    #[test]
    fn implementation_fragments_are_wired() {
        let _ = [
            super::B126_M2_IMPL_1_REANCHOR,
            super::B126_M2_IMPL_2_REANCHOR,
        ];
    }
}
