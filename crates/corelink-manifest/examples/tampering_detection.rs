//! Example: tampering detection demonstrates the cripto-load-bearing
//! invariants of the dual-side verifier — server pre-persist +
//! client post-download both reject any byte mutation.

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
    ManifestVerifierSig, VerifyError,
};
use uuid::Uuid;

fn main() {
    let tenant = Uuid::nil();
    let mock = Arc::new(MockTdkHandle::new());
    mock.install_default(tenant, 1);
    let handle: Arc<dyn TdkHandle> = Arc::clone(&mock) as Arc<dyn TdkHandle>;
    let signer = ManifestSigner::new(Arc::clone(&handle), 1).expect("non-zero key id");
    let verifier = ManifestVerifierSig::new(handle, vec![1]).expect("non-empty list");

    let chunks = vec![
        ChunkInput::new(*blake3::hash(b"a").as_bytes(), 1),
        ChunkInput::new(*blake3::hash(b"b").as_bytes(), 1),
        ChunkInput::new(*blake3::hash(b"c").as_bytes(), 1),
    ];
    let mut m = ManifestBuilder::new()
        .build(
            tenant,
            *blake3::hash(b"abc").as_bytes(),
            chunks,
            42,
            ChunkerAlgorithm::Fixed2MiB,
            &signer,
            1,
        )
        .expect("build OK");

    // Tamper one byte of one chunk's digest.
    println!("baseline merkle_root: {}", hex::encode(m.merkle_root));
    m.chunks[1].digest[0] ^= 0x01;
    let err = ManifestVerifier::new()
        .verify_full(&m, &verifier)
        .expect_err("verify must reject");
    match err {
        VerifyError::Structure(corelink_manifest::ManifestError::RootMismatch) => {
            println!("tamper detected: RootMismatch (canonical fail-fast)");
        }
        _ => panic!("unexpected error: {err:?}"),
    }
}
