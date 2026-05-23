//! Shared helpers for the `split_splice` test suites.
//!
//! Split from monolith `reapi/cas/split_splice.rs` (wave-33 stage
//! 2.PRE-A.2). Hosts `make_ctx`, `wire`, `blob_digest`, and `Wiring`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code: panics surface as test failures by design"
)]

use std::sync::Arc;

use corelink_hash::Digest;
use corelink_pat::{PatEnv, PatId, PatScopes};
use corelink_tenant_path::TenantDerivationKey;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::middleware::auth_ctx::__test_helpers::make_auth_ctx;
use crate::middleware::auth_ctx::{AuthCtx, AuthMethod, PrincipalId};
use crate::reapi::cas::assembler::InMemoryBlobAssembler;
use crate::reapi::cas::audit::InMemoryAuditSink;
use crate::reapi::cas::chunk_store::InMemoryChunkStore;
use crate::reapi::cas::session::InMemorySessionStore;
use crate::reapi::cas::types::BlobDigest;
use crate::Region;

use super::builder::{Clock, FakeClock, SplitSpliceHandlerBuilder};
use super::handler::SplitSpliceHandlerImpl;

pub(super) fn fixed_tdk() -> Arc<TenantDerivationKey> {
    Arc::new(TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32])))
}

pub(super) fn make_ctx(tenant: Uuid, region: Region, scopes: PatScopes) -> AuthCtx {
    let pat_id = PatId(Uuid::nil());
    make_auth_ctx(
        PrincipalId(Uuid::nil()),
        tenant,
        region,
        scopes,
        AuthMethod::Pat {
            env: PatEnv::Pat,
            pat_id,
        },
        fixed_tdk(),
    )
}

#[allow(dead_code, reason = "session + assembler handles kept on Arc for handler-shared lifecycle even when individual tests do not borrow them directly")]
pub(super) struct Wiring {
    pub(super) handler: SplitSpliceHandlerImpl<
        InMemorySessionStore,
        InMemoryChunkStore,
        InMemoryBlobAssembler,
        InMemoryAuditSink,
    >,
    pub(super) sessions: Arc<InMemorySessionStore>,
    pub(super) chunks: Arc<InMemoryChunkStore>,
    pub(super) assembler: Arc<InMemoryBlobAssembler>,
    pub(super) audit: Arc<InMemoryAuditSink>,
    pub(super) clock: Arc<FakeClock>,
}

pub(super) fn wire(region: Region) -> Wiring {
    let sessions = Arc::new(InMemorySessionStore::new());
    let chunks = Arc::new(InMemoryChunkStore::new());
    let assembler = Arc::new(InMemoryBlobAssembler::new(Arc::clone(&chunks)));
    let audit = Arc::new(InMemoryAuditSink::new());
    let clock = Arc::new(FakeClock::new(1_000_000));
    let handler = SplitSpliceHandlerImpl::new(SplitSpliceHandlerBuilder {
        region,
        sessions: Arc::clone(&sessions),
        chunks: Arc::clone(&chunks),
        assembler: Arc::clone(&assembler),
        audit: Arc::clone(&audit),
        clock: Arc::clone(&clock) as Arc<dyn Clock>,
    });
    Wiring {
        handler,
        sessions,
        chunks,
        assembler,
        audit,
        clock,
    }
}

pub(super) fn blob_digest(seed: &[u8]) -> BlobDigest {
    BlobDigest::new(Digest::compute(seed), seed.len() as u64)
}
