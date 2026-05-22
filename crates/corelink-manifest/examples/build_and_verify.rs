//! Example: build a manifest, then verify it (round-trip happy path).

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
    ChunkInput, ChunkerAlgorithm, ManifestBuilder, ManifestSigner, ManifestVerifier,
    ManifestVerifierSig,
};
use uuid::Uuid;

fn main() {
    // 1. Wire up a deterministic mock TDK + signer + verifier (production
    //    binds Cloudflare Secrets via CfSecretsTdkHandle).
    let tenant = Uuid::nil();
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(tenant, 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = ManifestSigner::new(Arc::clone(&handle), 1).expect("non-zero key id");
    let sig_verifier = ManifestVerifierSig::new(handle, vec![1]).expect("non-empty key list");

    // 2. Compose chunk inputs (in production these come from corelink-chunker).
    let chunks = vec![
        ChunkInput::new(*blake3::hash(b"chunk-0").as_bytes(), 7),
        ChunkInput::new(*blake3::hash(b"chunk-1").as_bytes(), 7),
        ChunkInput::new(*blake3::hash(b"chunk-2").as_bytes(), 7),
    ];
    let blob_digest = *blake3::hash(b"chunk-0chunk-1chunk-2").as_bytes();

    // 3. Build the sealed manifest.
    let m = ManifestBuilder::new()
        .build(
            tenant,
            blob_digest,
            chunks,
            42_000,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .expect("structural + sig OK");
    println!(
        "built manifest: merkle_root={} chunk_count={} total_size={}",
        hex::encode(m.merkle_root),
        m.chunk_count,
        m.total_size_bytes
    );

    // 4. Verify (server pre-persist + client post-download path).
    ManifestVerifier::new()
        .verify_full(&m, &sig_verifier)
        .expect("verify_full OK");
    println!("verify_full: OK");
}
