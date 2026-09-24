//! REAPI CAS unary operations over the authenticated decorated cache plane.
//!
//! This module implements only `FindMissingBlobs`, `BatchReadBlobs`, and
//! `BatchUpdateBlobs`.  It owns no storage and exposes no listener: a later
//! composition contract supplies its [`CasUnaryService::into_server`] result
//! only after the transport and sibling service gates are green.

use std::collections::{HashMap, HashSet};

use corelink_reapi::proto::google_rpc::Status as RpcStatus;
use corelink_reapi::proto::reapi::content_addressable_storage_server::{
    ContentAddressableStorage, ContentAddressableStorageServer,
};
use corelink_reapi::proto::reapi::{
    batch_read_blobs_response, batch_update_blobs_request, batch_update_blobs_response,
    digest_function, BatchReadBlobsRequest, BatchReadBlobsResponse, BatchUpdateBlobsRequest,
    BatchUpdateBlobsResponse, Digest, FindMissingBlobsRequest, FindMissingBlobsResponse,
};
use corelink_reapi::{
    MAX_BATCH_TOTAL_SIZE_BYTES, MAX_CAS_BLOB_SIZE_BYTES, MAX_FIND_MISSING_BATCH_SIZE,
};
use tonic::{async_trait, Code, Request, Response, Status};

use crate::reapi_ingress::{validate_digest, Access, AdmittedIngress, ReapiIngress};

const IDENTITY_COMPRESSOR: i32 = 0;

/// Unmounted REAPI CAS implementation backed by one shared ingress kernel.
#[derive(Clone, Debug)]
pub struct CasUnaryService {
    ingress: ReapiIngress,
}

impl CasUnaryService {
    /// Bind CAS RPCs to the production-decorated ingress dependency bundle.
    #[must_use]
    pub fn new(ingress: ReapiIngress) -> Self {
        Self { ingress }
    }

    /// Produce the tonic service for a future, separately authorized mount.
    #[must_use]
    pub fn into_server(self) -> ContentAddressableStorageServer<Self> {
        ContentAddressableStorageServer::new(self)
    }
}

#[async_trait]
impl ContentAddressableStorage for CasUnaryService {
    async fn find_missing_blobs(
        &self,
        request: Request<FindMissingBlobsRequest>,
    ) -> Result<Response<FindMissingBlobsResponse>, Status> {
        let (metadata, _extensions, body) = request.into_parts();
        require_sha256(body.digest_function)?;
        require_batch_count(body.blob_digests.len())?;
        let digests = validate_find_missing_digests(&body.blob_digests)?;
        let admitted = self
            .ingress
            .authorize(&metadata, &body.instance_name, Access::Read)
            .await?;

        let mut missing = Vec::new();
        for digest in digests {
            match read_masked(&admitted, &digest).await {
                Ok(()) => {}
                Err(MaskedMiss::Missing) => missing.push(digest),
                Err(MaskedMiss::Failure(error)) => return Err(error),
            }
        }
        Ok(Response::new(FindMissingBlobsResponse {
            missing_blob_digests: missing,
        }))
    }

    async fn batch_read_blobs(
        &self,
        request: Request<BatchReadBlobsRequest>,
    ) -> Result<Response<BatchReadBlobsResponse>, Status> {
        let (metadata, _extensions, body) = request.into_parts();
        require_sha256(body.digest_function)?;
        require_batch_count(body.digests.len())?;
        require_identity_acceptable(&body.acceptable_compressors)?;
        let dispatch_budget = batch_read_dispatch_budget(&body.digests);
        let admitted = self
            .ingress
            .authorize(&metadata, &body.instance_name, Access::Read)
            .await?;

        let mut responses = Vec::with_capacity(body.digests.len());
        let mut returned_bytes = 0_i64;
        for (digest, within_declared_budget) in body.digests.into_iter().zip(dispatch_budget) {
            let result = if within_declared_budget {
                batch_read_entry(&admitted, &digest).await
            } else {
                Err(Status::new(
                    Code::FailedPrecondition,
                    "REAPI batch response requires ByteStream read",
                ))
            };
            let (data, status) = match result {
                Ok(data) => {
                    let data_len = i64::try_from(data.len()).map_err(|_| {
                        Status::new(Code::ResourceExhausted, "REAPI batch is too large")
                    })?;
                    if returned_bytes
                        .checked_add(data_len)
                        .is_some_and(|total| total <= MAX_BATCH_TOTAL_SIZE_BYTES)
                    {
                        returned_bytes += data_len;
                        (data, Status::new(Code::Ok, ""))
                    } else {
                        (
                            Vec::new(),
                            Status::new(
                                Code::FailedPrecondition,
                                "REAPI batch response requires ByteStream read",
                            ),
                        )
                    }
                }
                Err(error) => (Vec::new(), error),
            };
            responses.push(batch_read_blobs_response::Response {
                digest: Some(digest),
                data,
                compressor: IDENTITY_COMPRESSOR,
                status: Some(rpc_status(status)),
            });
        }
        Ok(Response::new(BatchReadBlobsResponse { responses }))
    }

    async fn batch_update_blobs(
        &self,
        request: Request<BatchUpdateBlobsRequest>,
    ) -> Result<Response<BatchUpdateBlobsResponse>, Status> {
        let (metadata, _extensions, body) = request.into_parts();
        require_sha256(body.digest_function)?;
        require_batch_count(body.requests.len())?;
        require_update_batch_bytes(&body.requests)?;
        let duplicate_hashes = duplicate_hashes(&body.requests);
        let admitted = self
            .ingress
            .authorize(&metadata, &body.instance_name, Access::Write)
            .await?;

        let mut responses = Vec::with_capacity(body.requests.len());
        for entry in body.requests {
            let digest = entry.digest.clone();
            let result = batch_update_entry(&admitted, entry, &duplicate_hashes).await;
            responses.push(batch_update_blobs_response::Response {
                digest,
                status: Some(rpc_status(result)),
            });
        }
        Ok(Response::new(BatchUpdateBlobsResponse { responses }))
    }
}

fn require_sha256(declared_digest_function: i32) -> Result<(), Status> {
    if declared_digest_function == digest_function::Value::Sha256 as i32 {
        Ok(())
    } else {
        Err(Status::new(
            Code::InvalidArgument,
            "only SHA-256 REAPI digests are supported",
        ))
    }
}

fn require_batch_count(count: usize) -> Result<(), Status> {
    if count <= MAX_FIND_MISSING_BATCH_SIZE {
        Ok(())
    } else {
        Err(Status::new(
            Code::ResourceExhausted,
            "REAPI batch contains too many entries",
        ))
    }
}

fn require_identity_acceptable(compressors: &[i32]) -> Result<(), Status> {
    if compressors.is_empty() || compressors.contains(&IDENTITY_COMPRESSOR) {
        Ok(())
    } else {
        Err(Status::new(
            Code::FailedPrecondition,
            "REAPI batch reads require identity compression",
        ))
    }
}

/// Reserve the inline response budget from valid declared digest sizes before
/// any decorated read is dispatched. Invalid entries remain eligible for
/// per-entry validation below, where they receive their protocol status.
fn batch_read_dispatch_budget(digests: &[Digest]) -> Vec<bool> {
    let mut declared_bytes = 0_i64;
    digests
        .iter()
        .map(|digest| {
            if validate_cas_digest(digest).is_err() {
                return true;
            }
            let Some(next) = declared_bytes.checked_add(digest.size_bytes) else {
                return false;
            };
            if next > MAX_BATCH_TOTAL_SIZE_BYTES {
                return false;
            }
            declared_bytes = next;
            true
        })
        .collect()
}

fn require_update_batch_bytes(
    requests: &[batch_update_blobs_request::Request],
) -> Result<(), Status> {
    let total = requests.iter().try_fold(0_i64, |total, request| {
        let bytes = i64::try_from(request.data.len())
            .map_err(|_| Status::new(Code::ResourceExhausted, "REAPI batch is too large"))?;
        total
            .checked_add(bytes)
            .ok_or_else(|| Status::new(Code::ResourceExhausted, "REAPI batch is too large"))
    })?;
    if total <= MAX_BATCH_TOTAL_SIZE_BYTES {
        Ok(())
    } else {
        Err(Status::new(
            Code::ResourceExhausted,
            "REAPI batch is too large",
        ))
    }
}

fn validate_find_missing_digests(digests: &[Digest]) -> Result<Vec<Digest>, Status> {
    digests
        .iter()
        .map(|digest| {
            validate_cas_digest(digest)?;
            Ok(digest.clone())
        })
        .collect()
}

fn validate_cas_digest(digest: &Digest) -> Result<(), Status> {
    validate_digest(&digest.hash, digest.size_bytes)?;
    if digest.size_bytes <= MAX_CAS_BLOB_SIZE_BYTES {
        Ok(())
    } else {
        Err(Status::new(
            Code::ResourceExhausted,
            "REAPI blob exceeds the CAS limit",
        ))
    }
}

fn duplicate_hashes(requests: &[batch_update_blobs_request::Request]) -> HashSet<String> {
    let mut counts = HashMap::new();
    for request in requests {
        if let Some(digest) = &request.digest {
            *counts.entry(digest.hash.clone()).or_insert(0_usize) += 1;
        }
    }
    counts
        .into_iter()
        .filter_map(|(hash, count)| (count > 1).then_some(hash))
        .collect()
}

async fn batch_update_entry(
    admitted: &AdmittedIngress,
    entry: batch_update_blobs_request::Request,
    duplicate_hashes: &HashSet<String>,
) -> Status {
    if entry.compressor != IDENTITY_COMPRESSOR {
        return Status::new(
            Code::InvalidArgument,
            "REAPI batch updates require identity compression",
        );
    }
    let Some(digest) = entry.digest else {
        return Status::new(Code::InvalidArgument, "REAPI digest is required");
    };
    if duplicate_hashes.contains(&digest.hash) {
        return Status::new(Code::InvalidArgument, "duplicate REAPI digest in batch");
    }
    if let Err(error) = validate_cas_digest(&digest) {
        return error;
    }
    if i64::try_from(entry.data.len()).ok() != Some(digest.size_bytes) {
        return Status::new(
            Code::InvalidArgument,
            "REAPI digest size does not match payload",
        );
    }
    match admitted
        .cas_write(&digest.hash, digest.size_bytes, entry.data)
        .await
    {
        Ok(_) => Status::new(Code::Ok, ""),
        Err(error) => error,
    }
}

async fn batch_read_entry(admitted: &AdmittedIngress, digest: &Digest) -> Result<Vec<u8>, Status> {
    validate_cas_digest(digest)?;
    if digest.size_bytes > MAX_BATCH_TOTAL_SIZE_BYTES {
        return Err(Status::new(
            Code::FailedPrecondition,
            "REAPI blob requires ByteStream read",
        ));
    }
    match admitted.cas_read(&digest.hash, digest.size_bytes) {
        Ok(response) => Ok(response.bytes),
        Err(error) if error.code() == Code::PermissionDenied => {
            Err(Status::new(Code::NotFound, "CAS object not found"))
        }
        Err(error) => Err(error),
    }
}

enum MaskedMiss {
    Missing,
    Failure(Status),
}

async fn read_masked(admitted: &AdmittedIngress, digest: &Digest) -> Result<(), MaskedMiss> {
    match admitted.cas_read(&digest.hash, digest.size_bytes) {
        Ok(_) => Ok(()),
        Err(error) if matches!(error.code(), Code::NotFound | Code::PermissionDenied) => {
            Err(MaskedMiss::Missing)
        }
        Err(error) => Err(MaskedMiss::Failure(error)),
    }
}

fn rpc_status(status: Status) -> RpcStatus {
    RpcStatus {
        code: status.code() as i32,
        message: status.message().to_owned(),
        details: Vec::new(),
    }
}
