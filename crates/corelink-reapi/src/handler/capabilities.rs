//! `CapabilitiesService` — `Capabilities.GetCapabilities` gRPC service.
//!
//! Extracted from the monolithic `handler.rs` per Wave 33 Stream A2.1c
//! file-size discipline. Unauthenticated by REAPI v2 conformance: every
//! client probes capabilities BEFORE attaching credentials.

use std::sync::Arc;

use corelink_meta::MetaStore;
use corelink_worker::storage::r2::R2Backend;
use tonic::{async_trait, Request, Response, Status};

use crate::capabilities::server_capabilities;
use crate::orchestrator::OrphanReconciler;
use crate::pat::PatValidator;
use crate::proto::reapi as reapi_proto;
use crate::proto::reapi::capabilities_server::{Capabilities, CapabilitiesServer};
use crate::proto::reapi::{GetCapabilitiesRequest, ServerCapabilities};

use super::{Clock, HandlerCore};

/// Concrete `Capabilities` service.
pub struct CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    core: Arc<HandlerCore<V, B, M, R, C>>,
}

impl<V, B, M, R, C> std::fmt::Debug for CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilitiesService")
            .finish_non_exhaustive()
    }
}

impl<V, B, M, R, C> Clone for CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    fn clone(&self) -> Self {
        Self {
            core: Arc::clone(&self.core),
        }
    }
}

impl<V, B, M, R, C> CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    /// Wrap a `HandlerCore` into the gRPC service.
    #[must_use]
    pub fn new(core: Arc<HandlerCore<V, B, M, R, C>>) -> Self {
        Self { core }
    }

    /// Wrap into a tonic-server-ready service.
    #[must_use]
    pub fn into_server(self) -> CapabilitiesServer<Self> {
        CapabilitiesServer::new(self)
    }
}

#[async_trait]
impl<V, B, M, R, C> Capabilities for CapabilitiesService<V, B, M, R, C>
where
    V: PatValidator + 'static,
    B: R2Backend + 'static,
    M: MetaStore + 'static,
    R: OrphanReconciler + 'static,
    C: Clock + 'static,
{
    async fn get_capabilities(
        &self,
        _request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<ServerCapabilities>, Status> {
        // GetCapabilities is unauthenticated by REAPI v2 conformance — every
        // client probes capabilities BEFORE attaching credentials so the
        // wire-side discovery path is symmetric. Per CoreLink S-01: we still
        // emit canonical values regardless of caller.
        let caps = server_capabilities();
        let proto = reapi_proto::ServerCapabilities {
            cache_capabilities: Some(reapi_proto::CacheCapabilities {
                digest_functions: caps
                    .cache_capabilities
                    .digest_functions
                    .iter()
                    .map(|f| *f as i32)
                    .collect(),
                action_cache_update_capabilities: None,
                cache_priority_capabilities: None,
                max_batch_total_size_bytes: caps.cache_capabilities.max_batch_total_size_bytes,
                symlink_absolute_path_strategy: 0,
                supported_compressors: vec![],
                supported_batch_update_compressors: vec![],
                max_cas_blob_size_bytes: caps.cache_capabilities.max_cas_blob_size_bytes,
            }),
            execution_capabilities: None,
            deprecated_api_version: None,
            low_api_version: Some(crate::proto::semver::SemVer {
                major: caps.api_version.major,
                minor: caps.api_version.minor,
                patch: caps.api_version.patch,
                prerelease: String::new(),
            }),
            high_api_version: Some(crate::proto::semver::SemVer {
                major: caps.api_version.major,
                minor: caps.api_version.minor,
                patch: caps.api_version.patch,
                prerelease: String::new(),
            }),
        };
        Ok(Response::new(proto))
    }
}
