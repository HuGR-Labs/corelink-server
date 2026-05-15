//! Minimal in-memory R2 CAS surface for the E2E harness.
//!
//! Production R2 access goes through `corelink-r2-multipart` (multipart
//! lifecycle) and the BlobStoreWrite trait from `corelink-hash`
//! (CTRL-CAS-001). For the E2E harness we only need the **happy-path**
//! CAS contract:
//!
//! - PUT requires PAT-authentication (PAT hash bound to a tenant) +
//!   client-side BLAKE3 verification (wrapped via
//!   [`corelink_hash::VerifiedBody`] at construction).
//! - GET returns the bytes for a previously-put digest, scoped to the
//!   originating tenant (tenant-prefix scoping per
//!   `INV-MULTIPART-PATH-TENANT-SCOPED`).
//! - `stat` reports the cache hit/miss tally so the test can assert the
//!   PUT-then-GET path counts as a single PUT + one GET hit.

use std::collections::HashMap;
use std::sync::Mutex;

use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};

/// Errors raised by [`InMemoryR2Client`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum R2Error {
    /// PAT did not match any provisioned tenant.
    #[error("unauthorized: PAT not recognised")]
    Unauthorized,
    /// Tenant attempted to read a digest belonging to another tenant
    /// (mirrors the production tenant-prefix scoping invariant).
    #[error("forbidden: digest does not belong to caller tenant")]
    Forbidden,
    /// Requested digest is not present in the in-memory store.
    #[error("not found: digest absent from CAS")]
    NotFound,
    /// BLAKE3 digest of the GET response did not match the requested
    /// digest (defence in depth — the in-memory client never lies, so
    /// this branch only fires if a caller tampers with the bytes after
    /// the put roundtrip).
    #[error("digest mismatch on GET — INV-CAS-INTEGRITY violated")]
    DigestMismatch,
}

/// Per-tenant stat snapshot returned by [`InMemoryR2Client::stat`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct R2StatReport {
    /// Number of successful PUT operations for the tenant.
    pub puts: u64,
    /// Number of successful GET operations resolved from the CAS (hits).
    pub get_hits: u64,
    /// Number of GET operations that missed the CAS.
    pub get_misses: u64,
}

/// `tenant_id || ":" || digest_hex` → object bytes mapping plus a
/// per-tenant stat counter.
#[derive(Debug, Default)]
struct Inner {
    objects: HashMap<String, Bytes>,
    /// PAT hash → tenant id binding (mirrors the production PAT auth
    /// resolver shape; the orchestrator's first-PAT hash is the only
    /// credential the harness exercises).
    pat_to_tenant: HashMap<String, String>,
    stats: HashMap<String, R2StatReport>,
}

/// In-memory R2 CAS client used by the harness.
#[derive(Debug, Default)]
pub struct InMemoryR2Client {
    inner: Mutex<Inner>,
}

impl InMemoryR2Client {
    /// Construct an empty client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a PAT hash to a tenant id (mirrors the WI-S19-006 reveal
    /// flow — once the first PAT is materialised the auth resolver
    /// stores `(pat_hash → tenant_id)`).
    pub fn bind_pat(&self, pat_hash: &str, tenant_id: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.pat_to_tenant
                .insert(pat_hash.to_owned(), tenant_id.to_owned());
            g.stats.entry(tenant_id.to_owned()).or_default();
        }
    }

    /// PUT a blob to the CAS. The body is wrapped via
    /// [`VerifiedBody::new`] at the call site; the digest is the only
    /// key recorded.
    ///
    /// # Errors
    ///
    /// Returns [`R2Error::Unauthorized`] if `pat_hash` is not bound to
    /// a tenant.
    pub fn put(&self, pat_hash: &str, vb: &VerifiedBody) -> Result<Digest, R2Error> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| R2Error::Unauthorized)?;
        let tenant_id = g
            .pat_to_tenant
            .get(pat_hash)
            .cloned()
            .ok_or(R2Error::Unauthorized)?;
        let digest = *vb.digest();
        let key = compose_key(&tenant_id, &digest);
        g.objects.insert(key, vb.body().clone());
        let stat = g.stats.entry(tenant_id).or_default();
        stat.puts = stat.puts.saturating_add(1);
        Ok(digest)
    }

    /// GET a blob from the CAS. The returned bytes are re-verified
    /// against the requested digest using BLAKE3 client-side.
    ///
    /// # Errors
    ///
    /// Returns [`R2Error::Unauthorized`] if `pat_hash` is unknown,
    /// [`R2Error::NotFound`] if no object is present, and
    /// [`R2Error::DigestMismatch`] if the bytes have been corrupted.
    pub fn get(&self, pat_hash: &str, digest: &Digest) -> Result<Bytes, R2Error> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| R2Error::Unauthorized)?;
        let tenant_id = g
            .pat_to_tenant
            .get(pat_hash)
            .cloned()
            .ok_or(R2Error::Unauthorized)?;
        let key = compose_key(&tenant_id, digest);
        let result = g.objects.get(&key).cloned();
        let stat = g.stats.entry(tenant_id).or_default();
        match result {
            Some(bytes) => {
                // Re-verify the body client-side. This is the
                // INV-CAS-INTEGRITY round-trip the harness pins.
                let recomputed = Digest::compute(&bytes);
                if !recomputed.verify_constant_time(digest) {
                    return Err(R2Error::DigestMismatch);
                }
                stat.get_hits = stat.get_hits.saturating_add(1);
                Ok(bytes)
            }
            None => {
                stat.get_misses = stat.get_misses.saturating_add(1);
                Err(R2Error::NotFound)
            }
        }
    }

    /// Snapshot the per-tenant stat report (creates a zeroed entry if
    /// the tenant has never been touched).
    #[must_use]
    pub fn stat(&self, tenant_id: &str) -> R2StatReport {
        match self.inner.lock() {
            Ok(g) => g.stats.get(tenant_id).cloned().unwrap_or_default(),
            Err(_) => R2StatReport::default(),
        }
    }
}

fn compose_key(tenant_id: &str, digest: &Digest) -> String {
    format!("{}:{}", tenant_id, digest.to_hex())
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

    #[test]
    fn unbound_pat_unauthorized() {
        let r2 = InMemoryR2Client::new();
        let body = Bytes::from_static(b"hello");
        let digest = Digest::compute(&body);
        let vb = VerifiedBody::new(body, digest).unwrap();
        let err = r2.put("pat-unknown", &vb).unwrap_err();
        assert!(matches!(err, R2Error::Unauthorized));
    }

    #[test]
    fn put_then_get_hit_increments_stats() {
        let r2 = InMemoryR2Client::new();
        r2.bind_pat("pat-1", "tenant-1");
        let body = Bytes::from_static(b"corelink");
        let digest = Digest::compute(&body);
        let vb = VerifiedBody::new(body.clone(), digest).unwrap();
        let _ = r2.put("pat-1", &vb).unwrap();
        let got = r2.get("pat-1", &digest).unwrap();
        assert_eq!(got, body);
        let st = r2.stat("tenant-1");
        assert_eq!(st.puts, 1);
        assert_eq!(st.get_hits, 1);
        assert_eq!(st.get_misses, 0);
    }
}
