//! In-memory R2 evidence-dsr signed URL stub. Mirrors the canonical
//! production surface from WI-S11-002 §1 — every completed verification
//! sweep uploads a JCS-canonical signed `erasure-report.json` to
//! `dsr-reports/{tenant}/{dsr_id}/erasure-report.json` + returns a 24h
//! TTL signed URL.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::TEST_SIGNED_URL_TTL_MS;

/// Errors emitted by the in-memory R2 evidence-dsr stub.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum R2EvidenceError {
    /// Object key not found.
    #[error("r2 evidence-dsr object key not found: {0}")]
    NotFound(String),
    /// Signed URL has expired.
    #[error("r2 evidence-dsr signed url expired (now {now_ms} >= exp {expires_at_ms})")]
    Expired {
        /// Wall-clock instant the URL was inspected at.
        now_ms: u64,
        /// Expiration instant baked into the signed URL.
        expires_at_ms: u64,
    },
}

/// Canonical signed URL returned by
/// [`InMemoryR2EvidenceClient::put_signed`]. Mirrors the production
/// shape: 24h TTL + canonical object key + opaque token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct R2SignedUrl {
    /// Canonical R2 object key (e.g.
    /// `dsr-reports/{tenant}/{dsr_id}/erasure-report.json`).
    pub object_key: String,
    /// Opaque signed-URL token (deterministic per object key for tests).
    pub token: String,
    /// Wall-clock instant the URL was issued at (Unix epoch ms).
    pub issued_at_ms: u64,
    /// Wall-clock instant the URL expires (= issued + 24h).
    pub expires_at_ms: u64,
}

impl R2SignedUrl {
    /// Whether this URL is still valid at `now_ms`.
    #[must_use]
    pub const fn is_valid(&self, now_ms: u64) -> bool {
        now_ms < self.expires_at_ms
    }
}

/// In-memory R2 evidence-dsr client. Cloning shares the underlying
/// object map.
#[derive(Clone, Debug, Default)]
pub struct InMemoryR2EvidenceClient {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemoryR2EvidenceClient {
    /// Construct a fresh client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Upload `payload` at `object_key` and return the canonical 24h
    /// signed URL.
    pub fn put_signed(
        &self,
        object_key: impl Into<String>,
        payload: Vec<u8>,
        now_ms: u64,
    ) -> R2SignedUrl {
        let object_key = object_key.into();
        if let Ok(mut g) = self.objects.lock() {
            g.insert(object_key.clone(), payload);
        }
        let token = format!(
            "r2sig-{}-{now_ms}",
            blake3::hash(object_key.as_bytes()).to_hex()
        );
        R2SignedUrl {
            object_key,
            token,
            issued_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(TEST_SIGNED_URL_TTL_MS),
        }
    }

    /// Fetch the canonical payload bound to `url` if the URL is still
    /// valid.
    ///
    /// # Errors
    ///
    /// - [`R2EvidenceError::Expired`] when `now_ms >= url.expires_at_ms`.
    /// - [`R2EvidenceError::NotFound`] when the object key is unknown.
    pub fn get(&self, url: &R2SignedUrl, now_ms: u64) -> Result<Vec<u8>, R2EvidenceError> {
        if !url.is_valid(now_ms) {
            return Err(R2EvidenceError::Expired {
                now_ms,
                expires_at_ms: url.expires_at_ms,
            });
        }
        let guard = match self.objects.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard
            .get(&url.object_key)
            .cloned()
            .ok_or_else(|| R2EvidenceError::NotFound(url.object_key.clone()))
    }

    /// Snapshot every (key, payload) pair captured so far (for tests).
    #[must_use]
    pub fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
        let guard = match self.objects.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Number of distinct object keys stored.
    #[must_use]
    pub fn len(&self) -> usize {
        let guard = match self.objects.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.len()
    }

    /// Whether the client is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
