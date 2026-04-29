//! Concrete in-repo evidence of CTRL-CAS-001 type-driven enforcement.
//!
//! Defines a [`MemoryBlobStore`] fake that implements
//! [`BlobStoreWrite`](corelink_hash::BlobStoreWrite); the trait signature
//! requires a `&VerifiedBody`, so this test demonstrates the architectural
//! claim that downstream storage adapters (R2, KV, …) **cannot** accept
//! raw bytes without first running `VerifiedBody::new`. The S-01 R2 adapter
//! lands in WI-S01-003 implementing this same trait; until then, the
//! contract is exercised here against a controlled in-memory backend.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::missing_docs_in_private_items,
    missing_docs,
    reason = "test code; trait-impl assertions are the contract"
)]

use std::collections::HashMap;
use std::sync::Mutex;

use bytes::Bytes;
use corelink_hash::{BlobStoreWrite, Digest, VerifiedBody};
use thiserror::Error;

#[derive(Debug, Default)]
struct MemoryBlobStore {
    inner: Mutex<HashMap<String, Bytes>>,
}

#[derive(Debug, Error)]
enum MemoryStoreError {
    #[error("store mutex poisoned")]
    Poisoned,
}

impl BlobStoreWrite for MemoryBlobStore {
    type Error = MemoryStoreError;

    async fn put_verified(&self, vb: &VerifiedBody) -> Result<(), Self::Error> {
        let mut guard = self.inner.lock().map_err(|_| MemoryStoreError::Poisoned)?;
        // Key off the verified digest's hex (canonical-content addressing).
        guard.insert(vb.digest().to_hex(), vb.body().clone());
        Ok(())
    }
}

#[tokio::test]
async fn put_verified_persists_under_digest_key() {
    let store = MemoryBlobStore::default();
    let body = Bytes::from_static(b"hello world");
    let claimed = Digest::from_hex(
        "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
    )
    .expect("static hex");
    let vb = VerifiedBody::new(body.clone(), claimed).expect("verifies");

    store.put_verified(&vb).await.expect("put ok");
    let inner = store.inner.lock().expect("lock");
    assert_eq!(inner.get(&claimed.to_hex()), Some(&body));
}

#[tokio::test]
async fn put_verified_rejects_garbage_via_constructor() {
    // The trait can only be reached through `&VerifiedBody`. The Gherkin
    // story "VerifiedBody envelope is unconstructible without verify"
    // (WI-S01-002 §8) is enforced at the type level by the constructor +
    // private fields; this test verifies the *user-facing* error pathway
    // when a caller tries to construct one with mismatched bytes.
    let body = Bytes::from_static(b"a");
    let wrong = Digest::compute(b"b");
    assert!(VerifiedBody::new(body, wrong).is_err());
}
