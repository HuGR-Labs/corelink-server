//! Shared fixtures for OCI adapter integration tests.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    dead_code,
    missing_docs,
    missing_debug_implementations,
    reason = "test fixtures"
)]

use std::sync::Arc;

use corelink_adapter_oci::config::defaults;
use corelink_adapter_oci::ports::testing::{
    InMemoryBlobStore, InMemoryKv, StaticTenantResolver,
};
use corelink_adapter_oci::{router, AppState, OciAdapterConfig};
use corelink_audit::ports::InMemoryAuditEmitter;
use corelink_core::{SecretWrap, TenantId};

/// Pinned clock value used by the test app-state.
pub const FIXED_NOW_MS: u64 = 1_700_000_000_000;

/// One-stop test rig.
pub struct TestRig {
    pub cas: Arc<InMemoryBlobStore>,
    pub kv: Arc<InMemoryKv>,
    pub resolver: Arc<StaticTenantResolver>,
    pub audit: Arc<InMemoryAuditEmitter>,
    pub state: AppState,
    pub tenant: TenantId,
    pub token_signing_key: SecretWrap,
    pub token_signing_key_str: String,
}

fn fixed_clock_ms() -> u64 {
    FIXED_NOW_MS
}

impl Default for TestRig {
    fn default() -> Self {
        Self::new()
    }
}

impl TestRig {
    pub fn new() -> Self {
        let cas: Arc<InMemoryBlobStore> = Arc::new(InMemoryBlobStore::default());
        let kv: Arc<InMemoryKv> = Arc::new(InMemoryKv::default());
        let resolver: Arc<StaticTenantResolver> = Arc::new(StaticTenantResolver::default());
        let audit: Arc<InMemoryAuditEmitter> = Arc::new(InMemoryAuditEmitter::default());
        let tenant = TenantId::from_uuid(uuid::Uuid::nil());
        let key_str = "x".repeat(32);
        let cfg = OciAdapterConfig::new(
            ([127u8, 0, 0, 1], 0).into(),
            String::from("http://localhost:5000/token"),
            defaults::BLOB_SIZE_LIMIT_BYTES,
            defaults::MULTIPART_CHUNK_SIZE_BYTES,
            false,
            defaults::TOKEN_TTL_SECS,
            SecretWrap::new(key_str.clone()),
            cas.clone(),
            kv.clone(),
            resolver.clone(),
            audit.clone(),
        );
        let state = AppState::new(Arc::new(cfg), fixed_clock_ms);
        Self {
            cas,
            kv,
            resolver,
            audit,
            state,
            tenant,
            token_signing_key: SecretWrap::new(key_str.clone()),
            token_signing_key_str: key_str,
        }
    }

    /// Build a bearer token for `scope` with the rig's signing key.
    /// `now_secs` mirrors the fixed-clock value to avoid expiry skew.
    pub fn mint_token(&self, scope: &corelink_adapter_oci::auth::OciScope) -> String {
        corelink_adapter_oci::auth::mint(
            &self.token_signing_key,
            &self.tenant,
            scope,
            FIXED_NOW_MS / 1000,
            defaults::TOKEN_TTL_SECS,
        )
        .expect("mint")
    }

    /// Hand the axum app shape ready to `tower::Service::call` against.
    pub fn app(&self) -> axum::Router {
        router(self.state.clone())
    }
}
