//! AC envelope persistence trait + in-memory implementation.
//!
//! Split from monolith `reapi/ac/handler.rs` (wave-33 stage 2.PRE-A.4).

#![allow(
    clippy::manual_async_fn,
    reason = "trait surface uses explicit `impl Future + Send + 'a` so the `Send` bound and lifetime are visible at the call site; matches the corelink-meta MetaStore + corelink-worker R2Backend canonical pattern"
)]

use core::fmt;
use core::future::Future;

use corelink_tenant_path::TenantPrefix;

use super::super::sig::AcEnvelope;
use crate::Region;

/// AC envelope persistence surface. Production wiring stores the
/// envelope at `ac-<region>/<tenant_prefix>/<action_digest>.json` in
/// R2; the in-memory fake [`InMemoryAcEnvelopeStore`] keeps the same
/// canonical key shape.
pub trait AcEnvelopeStore: Send + Sync {
    /// Atomic PUT of the envelope at the canonical key.
    /// `If-None-Match: *` semantics: present only on first write.
    /// Idempotent re-update is a no-op (key already exists; bytes
    /// content-stable per the canonical envelope shape).
    ///
    /// # Errors
    ///
    /// Returns a backend-class error string. The handler maps to
    /// 503 `COR_AC_BACKEND_UNAVAILABLE`.
    fn put<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
        envelope: AcEnvelope,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a;

    /// PK GET. Returns `None` when the key is absent.
    ///
    /// # Errors
    ///
    /// Returns a backend-class error string.
    fn get<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
    ) -> impl Future<Output = Result<Option<AcEnvelope>, String>> + Send + 'a;
}

/// In-memory AC envelope store. Preserves the canonical key shape
/// (`region::tenant_prefix::action_digest_hex`) so cross-tenant
/// isolation is honored at the storage seam too (defense-in-depth
/// against trait-surface bypass).
pub struct InMemoryAcEnvelopeStore {
    // DEBT-013 OPT-04 phase 1 — `parking_lot::Mutex` (no poisoning).
    // The "envelope store mutex poisoned" error string returned by
    // the trait impl below is now structurally unreachable on this
    // backend; retained on the surface for transport-class failures.
    inner: parking_lot::Mutex<std::collections::HashMap<String, AcEnvelope>>,
}

impl Default for InMemoryAcEnvelopeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for InMemoryAcEnvelopeStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryAcEnvelopeStore")
            .finish_non_exhaustive()
    }
}

impl InMemoryAcEnvelopeStore {
    /// Construct a fresh in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn key(region: Region, prefix: &TenantPrefix, hex: &str) -> String {
        format!(
            "ac-{}/{}/{}.json",
            region.bucket_suffix(),
            prefix.as_str(),
            hex
        )
    }

    /// Snapshot every persisted key (test diagnostics).
    pub fn keys(&self) -> Vec<String> {
        // parking_lot lock is infallible — no PoisonError arm.
        self.inner.lock().keys().cloned().collect()
    }

    /// Test-only mutator: tamper an envelope's signature byte to
    /// simulate envelope corruption.
    pub fn tamper_for_test(&self, region: Region, prefix: &TenantPrefix, hex: &str) -> bool {
        let key = Self::key(region, prefix, hex);
        let mut guard = self.inner.lock();
        if let Some(env) = guard.get_mut(&key) {
            env.signature[0] ^= 0xFF;
            true
        } else {
            false
        }
    }
}

impl AcEnvelopeStore for InMemoryAcEnvelopeStore {
    fn put<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
        envelope: AcEnvelope,
    ) -> impl Future<Output = Result<(), String>> + Send + 'a {
        async move {
            // parking_lot lock is infallible — the "envelope store
            // mutex poisoned" error string is structurally
            // unreachable here.
            let mut guard = self.inner.lock();
            let key = Self::key(region, tenant_prefix, action_digest_hex);
            // Idempotent overwrite — the envelope shape is content-
            // stable for the same (tenant, action_digest, result_hash)
            // tuple, so re-PUT under retry is safe.
            guard.insert(key, envelope);
            Ok(())
        }
    }

    fn get<'a>(
        &'a self,
        region: Region,
        tenant_prefix: &'a TenantPrefix,
        action_digest_hex: &'a str,
    ) -> impl Future<Output = Result<Option<AcEnvelope>, String>> + Send + 'a {
        async move {
            let guard = self.inner.lock();
            let key = Self::key(region, tenant_prefix, action_digest_hex);
            Ok(guard.get(&key).cloned())
        }
    }
}
