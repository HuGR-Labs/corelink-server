//! In-memory test fakes for the npm adapter port traits.
//!
//! All three adapter-local port traits (`CasStore`, `KvStore`,
//! `TenantResolver`) are backed by `HashMap` + `Mutex` so integration
//! tests never touch real I/O. Mirrors the pip adapter's `common.rs`
//! (commit `e279b296`) per the dispatch packet requirement.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::type_complexity,
    dead_code,
    reason = "test fakes are allowed to use these primitives; dead_code because \
              this module is only consumed via `mod common;` in sibling test files"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use corelink_adapter_host::npm::{
    error::NpmAdapterError,
    ports::{CasStore, KvStore, TenantResolver},
};
use corelink_core::types::{digest::Digest, tenant::TenantId};
use subtle::ConstantTimeEq;

/// In-memory CAS store backed by `HashMap<(tenant_str, digest_hex), bytes>`.
#[derive(Debug, Default)]
pub struct InMemCas {
    inner: Mutex<HashMap<(String, String), Vec<u8>>>,
}

impl InMemCas {
    /// Return a snapshot of all stored entries.
    pub fn snapshot(&self) -> HashMap<(String, String), Vec<u8>> {
        self.inner.lock().expect("poisoned").clone()
    }
}

#[async_trait]
impl CasStore for InMemCas {
    async fn get(
        &self,
        tenant: &TenantId,
        digest: &Digest,
    ) -> Result<Option<Vec<u8>>, NpmAdapterError> {
        let map = self
            .inner
            .lock()
            .map_err(|_| NpmAdapterError::Cas("poisoned".into()))?;
        Ok(map
            .get(&(tenant.to_string(), hex::encode(digest.as_bytes())))
            .cloned())
    }

    async fn put(
        &self,
        tenant: &TenantId,
        digest: &Digest,
        bytes: Vec<u8>,
    ) -> Result<(), NpmAdapterError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| NpmAdapterError::Cas("poisoned".into()))?;
        map.insert((tenant.to_string(), hex::encode(digest.as_bytes())), bytes);
        Ok(())
    }
}

/// In-memory KV store backed by `HashMap<(tenant_str, key), (bytes, ts)>`.
#[derive(Debug, Default)]
pub struct InMemKv {
    inner: Mutex<HashMap<(String, String), (Vec<u8>, u64)>>,
}

impl InMemKv {
    /// Return a snapshot of all stored entries.
    pub fn snapshot(&self) -> HashMap<(String, String), (Vec<u8>, u64)> {
        self.inner.lock().expect("poisoned").clone()
    }
}

#[async_trait]
impl KvStore for InMemKv {
    async fn get(
        &self,
        tenant: &TenantId,
        key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError> {
        let map = self
            .inner
            .lock()
            .map_err(|_| NpmAdapterError::Kv("poisoned".into()))?;
        Ok(map.get(&(tenant.to_string(), key.to_owned())).cloned())
    }

    async fn put(
        &self,
        tenant: &TenantId,
        key: &str,
        value: Vec<u8>,
        inserted_at_unix_ms: u64,
    ) -> Result<(), NpmAdapterError> {
        let mut map = self
            .inner
            .lock()
            .map_err(|_| NpmAdapterError::Kv("poisoned".into()))?;
        map.insert(
            (tenant.to_string(), key.to_owned()),
            (value, inserted_at_unix_ms),
        );
        Ok(())
    }
}

/// KV store whose `get` always misses and whose `put` always ERRORS — used to
/// simulate a metadata-cache backend OUTAGE so the smoke suite can prove the
/// metadata surface degrades to proxy-through (200 + body) instead of 503.
#[derive(Debug, Default)]
pub struct FailingKv;

#[async_trait]
impl KvStore for FailingKv {
    async fn get(
        &self,
        _tenant: &TenantId,
        _key: &str,
    ) -> Result<Option<(Vec<u8>, u64)>, NpmAdapterError> {
        Ok(None)
    }

    async fn put(
        &self,
        _tenant: &TenantId,
        _key: &str,
        _value: Vec<u8>,
        _inserted_at_unix_ms: u64,
    ) -> Result<(), NpmAdapterError> {
        Err(NpmAdapterError::Kv("simulated KV outage".into()))
    }
}

/// Fixed-tenant resolver that performs a constant-time PAT comparison
/// against a single configured fixture PAT.
#[derive(Debug)]
pub struct FixedTenant {
    /// The `TenantId` returned on a successful PAT match.
    pub tenant: TenantId,
    /// The expected PAT plaintext to compare against (constant-time).
    pub expected_pat: String,
}

impl FixedTenant {
    /// Construct a new `FixedTenant` with a random tenant id and the
    /// given expected PAT.
    pub fn new(expected_pat: impl Into<String>) -> Arc<Self> {
        Arc::new(Self {
            tenant: TenantId::from_uuid(uuid::Uuid::now_v7()),
            expected_pat: expected_pat.into(),
        })
    }
}

#[async_trait]
impl TenantResolver for FixedTenant {
    async fn resolve(&self, pat_plaintext: &str) -> Result<TenantId, NpmAdapterError> {
        if pat_plaintext
            .as_bytes()
            .ct_eq(self.expected_pat.as_bytes())
            .unwrap_u8()
            == 1
        {
            Ok(self.tenant)
        } else {
            Err(NpmAdapterError::Auth("invalid PAT".into()))
        }
    }
}
