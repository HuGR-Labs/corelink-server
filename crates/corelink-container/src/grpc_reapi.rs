//! Authenticated REAPI cache ingress for CoreLink's shared cache plane.
//!
//! State-changing RPCs delegate to the decorated handlers assembled for REST,
//! retaining their tenant isolation, byte accounting, audit, tombstone, and
//! BYOK behaviour.  No gRPC handler calls the REST transport.

use std::sync::Arc;

use corelink_handler_ac::{
    AcHandlerError, AcLookupHandler, AcLookupRequest, AcUpdateHandler, AcUpdateRequest,
};
use corelink_handler_cas::{
    CasHandlerError, CasReadHandler, CasReadRequest, CasWriteHandler, CasWriteRequest, DigestAlgo,
};
use corelink_reapi::proto::bytestream::byte_stream_server::{ByteStream, ByteStreamServer};
use corelink_reapi::proto::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};
use corelink_reapi::proto::reapi::action_cache_server::{ActionCache, ActionCacheServer};
use corelink_reapi::proto::reapi::capabilities_server::{Capabilities, CapabilitiesServer};
use corelink_reapi::proto::reapi::content_addressable_storage_server::{
    ContentAddressableStorage, ContentAddressableStorageServer,
};
use corelink_reapi::proto::reapi::{
    batch_read_blobs_response, batch_update_blobs_response, ActionCacheUpdateCapabilities,
    ActionResult, BatchReadBlobsRequest, BatchReadBlobsResponse, BatchUpdateBlobsRequest,
    BatchUpdateBlobsResponse, CacheCapabilities, Digest, FindMissingBlobsRequest,
    FindMissingBlobsResponse, GetActionResultRequest, GetCapabilitiesRequest, ServerCapabilities,
    UpdateActionResultRequest,
};
use futures::StreamExt;
use prost::Message;
use sha2::{Digest as _, Sha256};
use tonic::{async_trait, Code, Request, Response, Status, Streaming};

use crate::adapter_pat::{PatVerifier, VerifyError};
use crate::routes::build::ReapiBridgeDeps;

const MAX_BLOB_BYTES: usize = corelink_hash::CACHE_ENTRY_MAX_BYTES;
const MAX_BATCH_BYTES: usize = 4 * 1024 * 1024;
const READ_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
struct Ingress {
    cas_read: Arc<dyn CasReadHandler>,
    cas_write: Arc<dyn CasWriteHandler>,
    ac_lookup: Arc<dyn AcLookupHandler>,
    ac_update: Arc<dyn AcUpdateHandler>,
    quota: Option<crate::routes::QuotaGate>,
    pat: Arc<PatVerifier>,
}

impl Ingress {
    fn from_deps(deps: &ReapiBridgeDeps) -> Option<Self> {
        Some(Self {
            cas_read: Arc::clone(&deps.cas_read),
            cas_write: Arc::clone(&deps.cas_write),
            ac_lookup: Arc::clone(&deps.ac_lookup),
            ac_update: Arc::clone(&deps.ac_update),
            quota: deps.quota.clone(),
            pat: Arc::clone(deps.pat.as_ref()?),
        })
    }

    async fn authenticate<T>(&self, request: &Request<T>, write: bool) -> Result<String, Status> {
        let raw = request
            .metadata()
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("missing authorization"))?;
        let token = raw
            .strip_prefix("Bearer ")
            .ok_or_else(|| Status::unauthenticated("authorization must use Bearer"))?
            .trim();
        let (tenant, can_write) =
            self.pat
                .verify_capability(token)
                .await
                .map_err(|error| match error {
                    VerifyError::InvalidPat => Status::unauthenticated("invalid authorization"),
                    VerifyError::Backend(_) => Status::unavailable("authorization unavailable"),
                })?;
        if write && !can_write {
            return Err(Status::permission_denied("cache:write scope required"));
        }
        Ok(tenant)
    }

    async fn charge(&self, tenant: &str) -> Result<(), Status> {
        if let Some(gate) = self.quota.as_ref() {
            if let Some(rejection) = gate.check(tenant).await {
                return Err(match rejection.status() {
                    axum::http::StatusCode::PAYMENT_REQUIRED => {
                        Status::resource_exhausted("cache quota exceeded")
                    }
                    _ => Status::unavailable("cache quota unavailable"),
                });
            }
        }
        Ok(())
    }

    fn require_instance(instance: &str, tenant: &str) -> Result<(), Status> {
        if instance == tenant {
            Ok(())
        } else {
            Err(Status::unauthenticated(
                "instance does not match authenticated tenant",
            ))
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn validate_digest(digest: Option<Digest>) -> Result<(String, i64), Status> {
    let digest = digest.ok_or_else(|| Status::invalid_argument("digest is required"))?;
    let too_large = usize::try_from(digest.size_bytes).map_or(true, |size| size > MAX_BLOB_BYTES);
    if digest.size_bytes < 0
        || too_large
        || digest.hash.len() != 64
        || !digest
            .hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(Status::invalid_argument(
            "digest must be lowercase SHA-256 within the CAS byte limit",
        ));
    }
    Ok((digest.hash, digest.size_bytes))
}

fn require_sha256(function: i32) -> Result<(), Status> {
    if function == 1 {
        Ok(())
    } else {
        Err(Status::invalid_argument(
            "only SHA256 digest_function is supported",
        ))
    }
}

fn validate_inline_digest(bytes: &[u8], digest: Option<Digest>) -> Result<(), Status> {
    let (hash, size) = validate_digest(digest)?;
    if size != i64::try_from(bytes.len()).unwrap_or(i64::MAX)
        || format!("{:x}", Sha256::digest(bytes)) != hash
    {
        return Err(Status::invalid_argument(
            "inline result data does not match its digest",
        ));
    }
    Ok(())
}

fn validate_action_result(result: &ActionResult) -> Result<(), Status> {
    if result.encoded_len() > MAX_BLOB_BYTES {
        return Err(Status::resource_exhausted(
            "action result exceeds cache entry limit",
        ));
    }
    for file in &result.output_files {
        validate_digest(file.digest.clone())?;
        if !file.contents.is_empty() {
            validate_inline_digest(&file.contents, file.digest.clone())?;
        }
    }
    for directory in &result.output_directories {
        validate_digest(directory.tree_digest.clone())?;
        if let Some(root) = directory.root_directory_digest.clone() {
            validate_digest(Some(root))?;
        }
    }
    if !result.stdout_raw.is_empty() && result.stdout_digest.is_some() {
        validate_inline_digest(&result.stdout_raw, result.stdout_digest.clone())?;
    } else if result.stdout_digest.is_some() {
        validate_digest(result.stdout_digest.clone())?;
    }
    if !result.stderr_raw.is_empty() && result.stderr_digest.is_some() {
        validate_inline_digest(&result.stderr_raw, result.stderr_digest.clone())?;
    } else if result.stderr_digest.is_some() {
        validate_digest(result.stderr_digest.clone())?;
    }
    Ok(())
}

fn wire_status(status: Status) -> corelink_reapi::proto::google_rpc::Status {
    corelink_reapi::proto::google_rpc::Status {
        code: status.code() as i32,
        message: status.message().to_owned(),
        details: vec![],
    }
}

fn map_cas_error(error: CasHandlerError) -> Status {
    match error {
        CasHandlerError::NotFound { .. } | CasHandlerError::CrossTenantDenied { .. } => {
            Status::not_found("CAS blob not found")
        }
        CasHandlerError::HashMismatch { .. } => Status::invalid_argument("digest mismatch"),
        CasHandlerError::ObjectTooLarge { .. } => Status::resource_exhausted("CAS blob too large"),
        CasHandlerError::AuditFailed(_) => Status::unavailable("CAS audit unavailable"),
        CasHandlerError::Internal(_) => Status::internal("CAS failure"),
        _ => Status::internal("CAS failure"),
    }
}

fn map_ac_error(error: AcHandlerError) -> Status {
    match error {
        AcHandlerError::Miss { .. } => Status::not_found("action result not found"),
        AcHandlerError::CrossTenantDenied { .. } => Status::unauthenticated("tenant mismatch"),
        AcHandlerError::DivergentBody { .. } => {
            Status::already_exists("action result already exists")
        }
        AcHandlerError::AuditFailed(_) => Status::unavailable("action cache audit unavailable"),
        AcHandlerError::Internal(_) => Status::internal("action cache failure"),
        _ => Status::internal("action cache failure"),
    }
}

fn read_request(tenant: &str, hash: String) -> CasReadRequest {
    CasReadRequest::new(tenant, hash, "grpc-pat", tenant, now_ms()).with_algo(DigestAlgo::Sha256)
}

fn write_request(tenant: &str, hash: String, bytes: Vec<u8>) -> CasWriteRequest {
    CasWriteRequest::new(tenant, hash, bytes, "grpc-pat", tenant, now_ms())
        .with_algo(DigestAlgo::Sha256)
}

/// ContentAddressableStorage backed by the shared decorated native handlers.
#[derive(Clone)]
pub struct CasService(Ingress);

impl CasService {
    #[must_use]
    pub fn from_deps(deps: &ReapiBridgeDeps) -> Option<Self> {
        Ingress::from_deps(deps).map(Self)
    }
    #[must_use]
    pub fn into_server(self) -> ContentAddressableStorageServer<Self> {
        ContentAddressableStorageServer::new(self)
    }
}

#[async_trait]
impl ContentAddressableStorage for CasService {
    async fn batch_update_blobs(
        &self,
        request: Request<BatchUpdateBlobsRequest>,
    ) -> Result<Response<BatchUpdateBlobsResponse>, Status> {
        let tenant = self.0.authenticate(&request, true).await?;
        let body = request.into_inner();
        Ingress::require_instance(&body.instance_name, &tenant)?;
        require_sha256(body.digest_function)?;
        let total = body
            .requests
            .iter()
            .try_fold(0_usize, |sum, item| sum.checked_add(item.data.len()))
            .ok_or_else(|| Status::resource_exhausted("batch too large"))?;
        if total > MAX_BATCH_BYTES {
            return Err(Status::resource_exhausted("batch exceeds negotiated limit"));
        }
        let mut responses = Vec::with_capacity(body.requests.len());
        for item in body.requests {
            let digest = item.digest.clone();
            let result = match validate_digest(item.digest) {
                Ok((hash, size))
                    if item.compressor == 0
                        && size == i64::try_from(item.data.len()).unwrap_or(i64::MAX) =>
                {
                    self.0.charge(&tenant).await.and_then(|_| {
                        tokio::task::block_in_place(|| {
                            self.0
                                .cas_write
                                .write(write_request(&tenant, hash, item.data))
                        })
                        .map(|_| ())
                        .map_err(map_cas_error)
                    })
                }
                Ok(_) if item.compressor != 0 => Err(Status::invalid_argument(
                    "compressed batch uploads unsupported",
                )),
                Ok(_) => Err(Status::invalid_argument(
                    "digest size does not match blob bytes",
                )),
                Err(error) => Err(error),
            };
            let status = result.err().unwrap_or_else(|| Status::new(Code::Ok, ""));
            responses.push(batch_update_blobs_response::Response {
                digest,
                status: Some(wire_status(status)),
            });
        }
        Ok(Response::new(BatchUpdateBlobsResponse { responses }))
    }

    async fn batch_read_blobs(
        &self,
        request: Request<BatchReadBlobsRequest>,
    ) -> Result<Response<BatchReadBlobsResponse>, Status> {
        let tenant = self.0.authenticate(&request, false).await?;
        let body = request.into_inner();
        Ingress::require_instance(&body.instance_name, &tenant)?;
        require_sha256(body.digest_function)?;
        self.0.charge(&tenant).await?;
        if body.digests.len() > 1000 {
            return Err(Status::resource_exhausted("too many digests"));
        }
        let mut responses = Vec::with_capacity(body.digests.len());
        for digest in body.digests {
            let result = validate_digest(Some(digest.clone())).and_then(|(hash, size)| {
                if usize::try_from(size).map_or(true, |value| value > MAX_BATCH_BYTES) {
                    return Err(Status::resource_exhausted("use ByteStream for this blob"));
                }
                tokio::task::block_in_place(|| self.0.cas_read.read(read_request(&tenant, hash)))
                    .map_err(map_cas_error)
            });
            match result {
                Ok(blob)
                    if blob.bytes.len()
                        == usize::try_from(digest.size_bytes).unwrap_or(usize::MAX) =>
                {
                    responses.push(batch_read_blobs_response::Response {
                        digest: Some(digest),
                        data: blob.bytes,
                        compressor: 0,
                        status: Some(wire_status(Status::new(Code::Ok, ""))),
                    });
                }
                Ok(_) => responses.push(batch_read_blobs_response::Response {
                    digest: Some(digest),
                    data: vec![],
                    compressor: 0,
                    status: Some(wire_status(Status::internal("stored blob size mismatch"))),
                }),
                Err(error) => responses.push(batch_read_blobs_response::Response {
                    digest: Some(digest),
                    data: vec![],
                    compressor: 0,
                    status: Some(wire_status(error)),
                }),
            }
        }
        Ok(Response::new(BatchReadBlobsResponse { responses }))
    }

    async fn find_missing_blobs(
        &self,
        request: Request<FindMissingBlobsRequest>,
    ) -> Result<Response<FindMissingBlobsResponse>, Status> {
        let tenant = self.0.authenticate(&request, false).await?;
        let body = request.into_inner();
        Ingress::require_instance(&body.instance_name, &tenant)?;
        require_sha256(body.digest_function)?;
        self.0.charge(&tenant).await?;
        if body.blob_digests.len() > 1000 {
            return Err(Status::resource_exhausted("too many digests"));
        }
        let mut missing = Vec::new();
        for digest in body.blob_digests {
            let (hash, _) = validate_digest(Some(digest.clone()))?;
            let exists =
                tokio::task::block_in_place(|| self.0.cas_read.exists(read_request(&tenant, hash)))
                    .map_err(map_cas_error)?;
            if !exists {
                missing.push(digest);
            }
        }
        Ok(Response::new(FindMissingBlobsResponse {
            missing_blob_digests: missing,
        }))
    }
}

/// ActionCache backed by the shared decorated native handlers.
#[derive(Clone)]
pub struct ActionCacheService(Ingress);

impl ActionCacheService {
    #[must_use]
    pub fn from_deps(deps: &ReapiBridgeDeps) -> Option<Self> {
        Ingress::from_deps(deps).map(Self)
    }
    #[must_use]
    pub fn into_server(self) -> ActionCacheServer<Self> {
        ActionCacheServer::new(self)
    }
}

#[async_trait]
impl ActionCache for ActionCacheService {
    async fn get_action_result(
        &self,
        request: Request<GetActionResultRequest>,
    ) -> Result<Response<ActionResult>, Status> {
        let tenant = self.0.authenticate(&request, false).await?;
        let body = request.into_inner();
        Ingress::require_instance(&body.instance_name, &tenant)?;
        let (hash, _) = validate_digest(body.action_digest)?;
        self.0.charge(&tenant).await?;
        let lookup = AcLookupRequest::new(&tenant, hash, "grpc-pat", &tenant, now_ms());
        let result = tokio::task::block_in_place(|| self.0.ac_lookup.lookup(lookup))
            .map_err(map_ac_error)?;
        let result = ActionResult::decode(result.result_payload.as_slice())
            .map_err(|_| Status::internal("stored action result is malformed"))?;
        validate_action_result(&result)
            .map_err(|_| Status::internal("stored action result is incompatible"))?;
        Ok(Response::new(result))
    }

    async fn update_action_result(
        &self,
        request: Request<UpdateActionResultRequest>,
    ) -> Result<Response<ActionResult>, Status> {
        let tenant = self.0.authenticate(&request, true).await?;
        let body = request.into_inner();
        Ingress::require_instance(&body.instance_name, &tenant)?;
        let (hash, _) = validate_digest(body.action_digest)?;
        let result = body
            .action_result
            .ok_or_else(|| Status::invalid_argument("action_result is required"))?;
        validate_action_result(&result)?;
        self.0.charge(&tenant).await?;
        let update = AcUpdateRequest::new(
            &tenant,
            hash,
            result.encode_to_vec(),
            "grpc-pat",
            &tenant,
            now_ms(),
        );
        tokio::task::block_in_place(|| self.0.ac_update.update(update)).map_err(map_ac_error)?;
        Ok(Response::new(result))
    }
}

fn parse_resource(resource: &str, write: bool) -> Result<(String, String, i64), Status> {
    let parts: Vec<_> = resource.split('/').collect();
    let (instance, hash, size) = if write {
        if parts.len() != 6 || parts[1] != "uploads" || parts[2].is_empty() || parts[3] != "blobs" {
            return Err(Status::invalid_argument(
                "invalid ByteStream write resource",
            ));
        }
        (parts[0], parts[4], parts[5])
    } else {
        if parts.len() != 4 || parts[1] != "blobs" {
            return Err(Status::invalid_argument("invalid ByteStream read resource"));
        }
        (parts[0], parts[2], parts[3])
    };
    let size = size
        .parse::<i64>()
        .map_err(|_| Status::invalid_argument("invalid ByteStream digest size"))?;
    let (hash, checked_size) = validate_digest(Some(Digest {
        hash: hash.to_owned(),
        size_bytes: size,
    }))?;
    Ok((instance.to_owned(), hash, checked_size))
}

/// Bounded ByteStream adapter. QueryWriteStatus is explicitly unsupported;
/// callers must retry complete uploads instead of assuming resume state exists.
#[derive(Clone)]
pub struct ByteStreamService(Ingress);

impl ByteStreamService {
    #[must_use]
    pub fn from_deps(deps: &ReapiBridgeDeps) -> Option<Self> {
        Ingress::from_deps(deps).map(Self)
    }
    #[must_use]
    pub fn into_server(self) -> ByteStreamServer<Self> {
        ByteStreamServer::new(self)
    }
}

#[async_trait]
impl ByteStream for ByteStreamService {
    type ReadStream = futures::stream::BoxStream<'static, Result<ReadResponse, Status>>;

    async fn read(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<Self::ReadStream>, Status> {
        let tenant = self.0.authenticate(&request, false).await?;
        let body = request.into_inner();
        let (instance, hash, declared_size) = parse_resource(&body.resource_name, false)?;
        Ingress::require_instance(&instance, &tenant)?;
        if body.read_offset < 0 || body.read_limit < 0 {
            return Err(Status::out_of_range("negative ByteStream range"));
        }
        self.0.charge(&tenant).await?;
        let blob =
            tokio::task::block_in_place(|| self.0.cas_read.read(read_request(&tenant, hash)))
                .map_err(map_cas_error)?;
        if i64::try_from(blob.bytes.len()).unwrap_or(i64::MAX) != declared_size {
            return Err(Status::internal("stored blob size mismatch"));
        }
        let start = usize::try_from(body.read_offset)
            .map_err(|_| Status::out_of_range("read offset outside blob"))?;
        if start > blob.bytes.len() {
            return Err(Status::out_of_range("read offset outside blob"));
        }
        let end = if body.read_limit == 0 {
            blob.bytes.len()
        } else {
            start
                .saturating_add(usize::try_from(body.read_limit).unwrap_or(usize::MAX))
                .min(blob.bytes.len())
        };
        let responses: Vec<_> = blob.bytes[start..end]
            .chunks(READ_CHUNK_BYTES)
            .map(|data| {
                Ok(ReadResponse {
                    data: data.to_vec(),
                })
            })
            .collect();
        Ok(Response::new(futures::stream::iter(responses).boxed()))
    }

    async fn write(
        &self,
        request: Request<Streaming<WriteRequest>>,
    ) -> Result<Response<WriteResponse>, Status> {
        let tenant = self.0.authenticate(&request, true).await?;
        let mut stream = request.into_inner();
        let mut resource = None;
        let mut data = Vec::new();
        let mut expected_offset = 0_i64;
        let mut finished = false;
        while let Some(chunk) = stream.message().await? {
            if finished {
                return Err(Status::invalid_argument("chunk after finish_write"));
            }
            if resource.is_none() {
                if chunk.resource_name.is_empty() {
                    return Err(Status::invalid_argument(
                        "first write chunk requires resource_name",
                    ));
                }
                resource = Some(chunk.resource_name.clone());
            } else if !chunk.resource_name.is_empty()
                && resource.as_deref() != Some(chunk.resource_name.as_str())
            {
                return Err(Status::invalid_argument(
                    "write resource changed mid-stream",
                ));
            }
            if chunk.write_offset != expected_offset {
                return Err(Status::invalid_argument("write offset is not contiguous"));
            }
            if data
                .len()
                .checked_add(chunk.data.len())
                .map_or(true, |size| size > MAX_BLOB_BYTES)
            {
                return Err(Status::resource_exhausted("blob exceeds CAS byte limit"));
            }
            expected_offset =
                expected_offset.saturating_add(i64::try_from(chunk.data.len()).unwrap_or(i64::MAX));
            data.extend_from_slice(&chunk.data);
            finished = chunk.finish_write;
        }
        if !finished {
            return Err(Status::invalid_argument("write closed before finish_write"));
        }
        let resource = resource.ok_or_else(|| Status::invalid_argument("empty write stream"))?;
        let (instance, hash, size) = parse_resource(&resource, true)?;
        Ingress::require_instance(&instance, &tenant)?;
        if size != i64::try_from(data.len()).unwrap_or(i64::MAX) {
            return Err(Status::invalid_argument(
                "declared size does not match streamed bytes",
            ));
        }
        self.0.charge(&tenant).await?;
        tokio::task::block_in_place(|| self.0.cas_write.write(write_request(&tenant, hash, data)))
            .map_err(map_cas_error)?;
        Ok(Response::new(WriteResponse {
            committed_size: size,
        }))
    }

    async fn query_write_status(
        &self,
        request: Request<QueryWriteStatusRequest>,
    ) -> Result<Response<QueryWriteStatusResponse>, Status> {
        let tenant = self.0.authenticate(&request, true).await?;
        let body = request.into_inner();
        let (instance, _, _) = parse_resource(&body.resource_name, true)?;
        Ingress::require_instance(&instance, &tenant)?;
        Err(Status::unimplemented(
            "resumable ByteStream writes are unsupported",
        ))
    }
}

/// Cache-only capabilities: SHA-256 plus ActionCache updates, no execution.
#[derive(Clone)]
pub struct CapabilitiesService(Ingress);

impl CapabilitiesService {
    #[must_use]
    pub fn from_deps(deps: &ReapiBridgeDeps) -> Option<Self> {
        Ingress::from_deps(deps).map(Self)
    }

    #[must_use]
    pub fn into_server(self) -> CapabilitiesServer<Self> {
        CapabilitiesServer::new(self)
    }
}

#[async_trait]
impl Capabilities for CapabilitiesService {
    async fn get_capabilities(
        &self,
        request: Request<GetCapabilitiesRequest>,
    ) -> Result<Response<ServerCapabilities>, Status> {
        let tenant = self.0.authenticate(&request, false).await?;
        let body = request.into_inner();
        Ingress::require_instance(&body.instance_name, &tenant)?;
        Ok(Response::new(ServerCapabilities {
            cache_capabilities: Some(CacheCapabilities {
                digest_functions: vec![1],
                action_cache_update_capabilities: Some(ActionCacheUpdateCapabilities {
                    update_enabled: true,
                }),
                cache_priority_capabilities: None,
                max_batch_total_size_bytes: i64::try_from(MAX_BATCH_BYTES).unwrap_or(i64::MAX),
                symlink_absolute_path_strategy: 0,
                supported_compressors: vec![],
                supported_batch_update_compressors: vec![],
                max_cas_blob_size_bytes: i64::try_from(MAX_BLOB_BYTES).unwrap_or(i64::MAX),
            }),
            execution_capabilities: None,
            deprecated_api_version: None,
            low_api_version: Some(corelink_reapi::proto::semver::SemVer {
                major: 2,
                minor: 12,
                patch: 0,
                prerelease: String::new(),
            }),
            high_api_version: Some(corelink_reapi::proto::semver::SemVer {
                major: 2,
                minor: 12,
                patch: 0,
                prerelease: String::new(),
            }),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_digest_checks_hash_size_and_bounds() {
        let bytes = b"corelink";
        let digest = Digest {
            hash: format!("{:x}", Sha256::digest(bytes)),
            size_bytes: bytes.len() as i64,
        };
        assert!(validate_inline_digest(bytes, Some(digest.clone())).is_ok());

        let mut wrong_hash = digest.clone();
        let replacement = if wrong_hash.hash.starts_with('0') {
            "1"
        } else {
            "0"
        };
        wrong_hash.hash.replace_range(..1, replacement);
        assert!(validate_inline_digest(bytes, Some(wrong_hash)).is_err());

        let mut wrong_size = digest;
        wrong_size.size_bytes += 1;
        assert!(validate_digest(Some(wrong_size)).is_ok());
        assert!(validate_inline_digest(
            bytes,
            Some(Digest {
                hash: format!("{:x}", Sha256::digest(bytes)),
                size_bytes: bytes.len() as i64 + 1,
            })
        )
        .is_err());
        assert!(validate_digest(Some(Digest {
            hash: "a".repeat(64),
            size_bytes: i64::try_from(MAX_BLOB_BYTES).unwrap_or(i64::MAX) + 1,
        }))
        .is_err());
    }

    #[test]
    fn action_result_subset_round_trips_metadata_and_symlinks() {
        let result = ActionResult {
            exit_code: 17,
            execution_metadata: Some(corelink_reapi::proto::reapi::ExecutedActionMetadata {
                worker: "worker-7".to_owned(),
                ..Default::default()
            }),
            output_symlinks: vec![corelink_reapi::proto::reapi::OutputSymlink {
                path: "out/link".to_owned(),
                target: "../artifact".to_owned(),
            }],
            ..Default::default()
        };
        let decoded = ActionResult::decode(result.encode_to_vec().as_slice()).unwrap();
        assert_eq!(decoded, result);
        assert!(validate_action_result(&decoded).is_ok());
    }

    #[test]
    fn authenticated_instance_must_match_the_tenant() {
        assert!(Ingress::require_instance("tenant-a", "tenant-a").is_ok());
        assert_eq!(
            Ingress::require_instance("tenant-b", "tenant-a")
                .unwrap_err()
                .code(),
            Code::Unauthenticated
        );
    }

    #[test]
    fn byte_stream_resource_rejects_other_tenant_shapes() {
        let parsed = parse_resource(
            "tenant-a/blobs/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/3",
            false,
        )
        .expect("valid resource");
        assert_eq!(parsed.0, "tenant-a");
        assert!(parse_resource("tenant-a/blobs/not-a-sha/3", false).is_err());
        assert!(parse_resource(
            "tenant-a/uploads/id/blobs/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/-1",
            true,
        )
        .is_err());
    }

    #[test]
    fn only_sha256_is_negotiated() {
        assert!(require_sha256(1).is_ok());
        assert!(require_sha256(0).is_err());
        assert!(require_sha256(9).is_err());
    }
}
