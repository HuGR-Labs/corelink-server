//! Unmounted authenticated REAPI `google.bytestream.ByteStream` contract.
//!
//! This service consumes the frozen [`crate::reapi_ingress::ReapiIngress`]
//! boundary and therefore never creates a storage handler, cache, or upload
//! state of its own. It remains deliberately unmounted until #2176 supplies
//! the transport proof and the sibling service contracts are accepted.

use std::sync::Arc;

use corelink_reapi::proto::bytestream::byte_stream_server::ByteStream;
use corelink_reapi::proto::bytestream::{
    QueryWriteStatusRequest, QueryWriteStatusResponse, ReadRequest, ReadResponse, WriteRequest,
    WriteResponse,
};
use futures::{Stream, StreamExt};
use tonic::{Code, Request, Response, Status, Streaming};

use crate::reapi_ingress::{
    validate_blob_resource_name, Access, AdmittedIngress, ReapiIngress, ValidatedBlobResourceName,
};

/// One REAPI ByteStream payload is capped at the CAS single-blob ceiling.
pub const REAPI_BYTESTREAM_MAX_BUFFERED_BYTES: usize = 5 * 1024 * 1024;
/// Each successful read response contains at most this many bytes.
pub const REAPI_BYTESTREAM_CHUNK_BYTES: usize = 64 * 1024;
/// At most three full ByteStream bodies can be buffered by this service.
pub const REAPI_BYTESTREAM_CONCURRENCY_LIMIT: usize = 3;

const REAPI_BYTESTREAM_READ_PEAK_BYTES: u64 = crate::container_capacity::CAS_READ_COPY_MULTIPLIER
    * REAPI_BYTESTREAM_MAX_BUFFERED_BYTES as u64
    + REAPI_BYTESTREAM_CHUNK_BYTES as u64;
const REAPI_BYTESTREAM_WRITE_PEAK_BYTES: u64 = 2 * REAPI_BYTESTREAM_MAX_BUFFERED_BYTES as u64;

const _: () = assert!(REAPI_BYTESTREAM_CHUNK_BYTES > 0);
const _: () = assert!(REAPI_BYTESTREAM_CHUNK_BYTES <= REAPI_BYTESTREAM_MAX_BUFFERED_BYTES);
const _: () = assert!(REAPI_BYTESTREAM_CONCURRENCY_LIMIT > 0);
const _: () = assert!(
    REAPI_BYTESTREAM_MAX_BUFFERED_BYTES == corelink_reapi::MAX_CAS_BLOB_SIZE_BYTES as usize
);
const _: () = assert!(
    REAPI_BYTESTREAM_CONCURRENCY_LIMIT as u64 * REAPI_BYTESTREAM_READ_PEAK_BYTES
        <= crate::container_capacity::CAS_READ_GLOBAL_BUDGET_BYTES
);
const _: () = assert!(
    REAPI_BYTESTREAM_CONCURRENCY_LIMIT as u64 * REAPI_BYTESTREAM_WRITE_PEAK_BYTES
        <= crate::container_capacity::CAS_WRITE_GLOBAL_BUDGET_BYTES
);

/// Authenticated REAPI ByteStream operations over decorated tenant CAS.
#[derive(Clone, Debug)]
pub struct ReapiByteStreamService {
    ingress: ReapiIngress,
    buffered_operations: Arc<tokio::sync::Semaphore>,
}

impl ReapiByteStreamService {
    /// Bind the service to the route factory's unmounted ingress bundle.
    #[must_use]
    pub fn new(ingress: ReapiIngress) -> Self {
        Self {
            ingress,
            buffered_operations: Arc::new(tokio::sync::Semaphore::new(
                REAPI_BYTESTREAM_CONCURRENCY_LIMIT,
            )),
        }
    }

    async fn read_request(
        &self,
        metadata: &tonic::metadata::MetadataMap,
        request: ReadRequest,
    ) -> Result<ReapiReadStream, Status> {
        let instance = resource_instance(&request.resource_name)?;
        let admitted = self
            .ingress
            .authorize(metadata, instance, Access::Read)
            .await?;
        let resource =
            validate_blob_resource_name(&request.resource_name, admitted.tenant().tenant_id())?;
        require_read_resource(&resource)?;
        let declared_size = bounded_resource_size(&resource)?;
        validate_read_range(request.read_offset, request.read_limit, declared_size)?;
        let permit = self.acquire_buffer_permit()?;

        // `cas_read` is the single decorated read call. Its response has already
        // passed the handler's tenant, tombstone, integrity, audit, and byte
        // accounting gates before this transport layer emits bounded frames.
        let response = admitted.cas_read(resource.hash(), resource.size_bytes())?;
        let start = usize::try_from(request.read_offset)
            .map_err(|_| Status::new(Code::OutOfRange, "REAPI read offset is out of range"))?;
        let available = declared_size - start;
        let requested = if request.read_limit == 0 {
            available
        } else {
            usize::try_from(request.read_limit)
                .map_err(|_| Status::new(Code::InvalidArgument, "REAPI read limit is invalid"))?
                .min(available)
        };
        let end = start
            .checked_add(requested)
            .ok_or_else(|| Status::new(Code::OutOfRange, "REAPI read range is out of range"))?;

        Ok(Box::pin(futures::stream::unfold(
            ReadState {
                body: response.bytes,
                cursor: start,
                end,
                _admitted: admitted,
                _permit: permit,
            },
            |mut state| async move {
                if state.cursor == state.end {
                    return None;
                }
                let chunk_end = state
                    .cursor
                    .saturating_add(REAPI_BYTESTREAM_CHUNK_BYTES)
                    .min(state.end);
                let data = state.body[state.cursor..chunk_end].to_vec();
                state.cursor = chunk_end;
                Some((Ok(ReadResponse { data }), state))
            },
        )))
    }

    async fn write_stream<S>(
        &self,
        metadata: &tonic::metadata::MetadataMap,
        stream: S,
    ) -> Result<WriteResponse, Status>
    where
        S: Stream<Item = Result<WriteRequest, Status>> + Send,
    {
        futures::pin_mut!(stream);
        let first = stream
            .next()
            .await
            .transpose()?
            .ok_or_else(|| Status::new(Code::InvalidArgument, "REAPI write stream is empty"))?;
        let first_resource_name = first.resource_name.clone();
        let instance = resource_instance(&first_resource_name)?;
        let admitted = self
            .ingress
            .authorize(metadata, instance, Access::Write)
            .await?;
        let resource =
            validate_blob_resource_name(&first_resource_name, admitted.tenant().tenant_id())?;
        require_write_resource(&resource)?;
        let declared_size = bounded_resource_size(&resource)?;
        let permit = self.acquire_buffer_permit()?;

        let mut buffered = Vec::with_capacity(declared_size);
        consume_write_chunk(&mut buffered, &first, &first_resource_name, &resource, 0)?;
        let mut finished = first.finish_write;
        // Release the initial request frame after copying its bytes. Keeping
        // it alive would add an unbudgeted body-sized allocation beside the
        // assembled body while the handler persists the write.
        drop(first);

        while !finished {
            let chunk = stream.next().await.transpose()?.ok_or_else(|| {
                Status::new(
                    Code::InvalidArgument,
                    "REAPI write ended before finish_write",
                )
            })?;
            let expected_offset = i64::try_from(buffered.len()).map_err(|_| {
                Status::new(Code::ResourceExhausted, "REAPI write exceeds buffer limit")
            })?;
            consume_write_chunk(
                &mut buffered,
                &chunk,
                &first_resource_name,
                &resource,
                expected_offset,
            )?;
            finished = chunk.finish_write;
        }

        // The terminal frame is accepted only when the client half-closes its
        // request stream. Waiting for EOF closes the race where a replay arrives
        // just after finish_write. The buffer semaphore bounds retained bodies;
        // request cancellation drops both the body and its admission lease.
        if stream.next().await.transpose()?.is_some() {
            return Err(Status::new(
                Code::InvalidArgument,
                "REAPI write sent a chunk after finish_write",
            ));
        }
        if buffered.len() != declared_size
            || crate::reapi_ingress::sha256_digest(&buffered) != resource.hash()
        {
            return Err(Status::new(
                Code::InvalidArgument,
                "REAPI write body does not match its declared digest and size",
            ));
        }

        // The only persistence call in this service. `AdmittedIngress` routes
        // through the production-decorated handler exactly once.
        let _permit = permit;
        admitted
            .cas_write(resource.hash(), resource.size_bytes(), buffered)
            .await?;
        Ok(WriteResponse {
            committed_size: resource.size_bytes(),
        })
    }

    fn acquire_buffer_permit(&self) -> Result<tokio::sync::OwnedSemaphorePermit, Status> {
        self.buffered_operations
            .clone()
            .try_acquire_owned()
            .map_err(|_| {
                Status::new(
                    Code::ResourceExhausted,
                    "REAPI ByteStream capacity exhausted",
                )
            })
    }
}

#[tonic::async_trait]
impl ByteStream for ReapiByteStreamService {
    type ReadStream = ReapiReadStream;

    async fn read(
        &self,
        request: Request<ReadRequest>,
    ) -> Result<Response<Self::ReadStream>, Status> {
        let metadata = request.metadata().clone();
        Ok(Response::new(
            self.read_request(&metadata, request.into_inner()).await?,
        ))
    }

    async fn write(
        &self,
        request: Request<Streaming<WriteRequest>>,
    ) -> Result<Response<WriteResponse>, Status> {
        let metadata = request.metadata().clone();
        Ok(Response::new(
            self.write_stream(&metadata, request.into_inner()).await?,
        ))
    }

    async fn query_write_status(
        &self,
        _request: Request<QueryWriteStatusRequest>,
    ) -> Result<Response<QueryWriteStatusResponse>, Status> {
        Err(Status::unimplemented(
            "REAPI ByteStream resumable uploads are not implemented",
        ))
    }
}

type ReapiReadStream = futures::stream::BoxStream<'static, Result<ReadResponse, Status>>;

struct ReadState {
    body: Vec<u8>,
    cursor: usize,
    end: usize,
    _admitted: AdmittedIngress,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

fn resource_instance(resource_name: &str) -> Result<&str, Status> {
    resource_name
        .split_once('/')
        .map(|(instance, _)| instance)
        .filter(|instance| !instance.is_empty())
        .ok_or_else(|| Status::new(Code::InvalidArgument, "invalid REAPI resource name"))
}

fn bounded_resource_size(resource: &ValidatedBlobResourceName) -> Result<usize, Status> {
    let size = usize::try_from(resource.size_bytes())
        .map_err(|_| Status::new(Code::ResourceExhausted, "REAPI blob exceeds buffer limit"))?;
    if size > REAPI_BYTESTREAM_MAX_BUFFERED_BYTES {
        return Err(Status::new(
            Code::ResourceExhausted,
            "REAPI blob exceeds configured CAS limit",
        ));
    }
    Ok(size)
}

fn require_read_resource(resource: &ValidatedBlobResourceName) -> Result<(), Status> {
    if resource.upload_id().is_some() {
        return Err(Status::new(
            Code::InvalidArgument,
            "REAPI read requires a blob resource name",
        ));
    }
    Ok(())
}

fn require_write_resource(resource: &ValidatedBlobResourceName) -> Result<(), Status> {
    if resource.upload_id().is_none() {
        return Err(Status::new(
            Code::InvalidArgument,
            "REAPI write requires an upload resource name",
        ));
    }
    Ok(())
}

fn validate_read_range(offset: i64, limit: i64, size: usize) -> Result<(), Status> {
    if offset < 0 || usize::try_from(offset).map_or(true, |value| value > size) {
        return Err(Status::new(
            Code::OutOfRange,
            "REAPI read offset is out of range",
        ));
    }
    if limit < 0 {
        return Err(Status::new(
            Code::InvalidArgument,
            "REAPI read limit is negative",
        ));
    }
    Ok(())
}

fn consume_write_chunk(
    buffered: &mut Vec<u8>,
    chunk: &WriteRequest,
    resource_name: &str,
    resource: &ValidatedBlobResourceName,
    expected_offset: i64,
) -> Result<(), Status> {
    if !chunk.resource_name.is_empty() && chunk.resource_name != resource_name {
        return Err(Status::new(
            Code::InvalidArgument,
            "REAPI write resource changed mid-stream",
        ));
    }
    if chunk.write_offset != expected_offset {
        return Err(Status::new(
            Code::InvalidArgument,
            "REAPI write offset is not contiguous",
        ));
    }
    let next_size = buffered
        .len()
        .checked_add(chunk.data.len())
        .ok_or_else(|| Status::new(Code::ResourceExhausted, "REAPI write exceeds buffer limit"))?;
    let declared_size = usize::try_from(resource.size_bytes()).map_err(|_| {
        Status::new(
            Code::ResourceExhausted,
            "REAPI write exceeds configured CAS limit",
        )
    })?;
    if next_size > REAPI_BYTESTREAM_MAX_BUFFERED_BYTES || next_size > declared_size {
        return Err(Status::new(
            Code::ResourceExhausted,
            "REAPI write exceeds configured CAS limit",
        ));
    }
    buffered.extend_from_slice(&chunk.data);
    Ok(())
}

#[cfg(test)]
#[path = "reapi_bytestream/tests.rs"]
mod tests;
