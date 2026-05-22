//! Example: streaming progressive verify with O(1)-per-chunk memory.
//!
//! Demonstrates the canonical SpliceBlob read-path pattern: the
//! verifier reads chunk refs one at a time from a [`ChunkRefSource`]
//! (production: D1 cursor over `manifest_chunks`), pulls bytes one
//! chunk at a time from a [`ChunkBytesSource`] (production: R2 GET
//! cursor), verifies each chunk's hash BEFORE pushing bytes to the
//! sink (production: gRPC `ByteStream::Read` response stream), and
//! aborts mid-stream on the first mismatch.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: prints + unwraps acceptable for runnable demonstration"
)]

use std::sync::Arc;

use corelink_ac_core::sig::{MockTdkHandle, TdkHandle};
use corelink_manifest::{
    verify_streaming, ChunkInput, ChunkerAlgorithm, CollectingVerifiedSink,
    InMemoryChunkBytesSource, InMemoryChunkRefSource, ManifestBuilder, ManifestSigner,
    StreamingManifestHeader,
};
use uuid::Uuid;

fn main() {
    let tenant = Uuid::nil();
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(tenant, 1);
    let handle: Arc<dyn TdkHandle> = mock as Arc<dyn TdkHandle>;
    let signer = ManifestSigner::new(handle, 1).expect("non-zero key id");

    // Build a manifest with several chunks.
    let payloads: Vec<&'static [u8]> = vec![b"alpha-block", b"beta-block", b"gamma-block"];
    let chunk_inputs: Vec<ChunkInput> = payloads
        .iter()
        .map(|p| ChunkInput::new(*blake3::hash(p).as_bytes(), p.len() as u32))
        .collect();
    let blob_concat: Vec<u8> = payloads.iter().flat_map(|p| p.iter().copied()).collect();
    let m = ManifestBuilder::new()
        .build(
            tenant,
            *blake3::hash(&blob_concat).as_bytes(),
            chunk_inputs,
            42_000,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .expect("build OK");

    // Wire up the streaming surfaces.
    let header = StreamingManifestHeader::from_manifest(&m);
    let mut refs = InMemoryChunkRefSource::new(m.chunks.clone());
    let mut bytes = InMemoryChunkBytesSource::new();
    for (i, p) in payloads.iter().enumerate() {
        bytes.insert(i as u32, p.to_vec());
    }
    let mut sink = CollectingVerifiedSink::new();

    // Run streaming verify.
    let outcome = verify_streaming(&header, &mut refs, &mut bytes, &mut sink)
        .expect("streaming verify OK");
    println!(
        "streaming verify: chunks={} bytes={}",
        outcome.chunks_streamed, outcome.bytes_streamed
    );
    let collected: Vec<u8> = sink.take().into_iter().flat_map(|(_, b)| b).collect();
    assert_eq!(collected, blob_concat);
    println!("reassembled blob OK ({} bytes)", collected.len());
}
