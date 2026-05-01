//! Example: tenant B captures tenant A's upload_id and tries to
//! replay an upload-part. The adapter rejects with
//! `MultipartError::CrossTenantUpload`.
//!
//! Run with `cargo run -p corelink-r2-multipart --example cross_tenant_replay`.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: surface the rejection path to stdout"
)]

use bytes::Bytes;
use corelink_r2_multipart::{
    Bucket, InMemoryMultipartAdapter, InitiateRequest, MultipartAdapter, MultipartError,
    PartNumber,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

#[tokio::main]
async fn main() {
    let adapter = InMemoryMultipartAdapter::new();
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
    let tenant_a = Uuid::from_u128(1);
    let tenant_b = Uuid::from_u128(2);
    let prefix_a = derive_prefix(&tdk, tenant_a);
    let digest_hex = "0123456789abcdef".repeat(4);

    let upload = adapter
        .initiate(InitiateRequest::new(
            tenant_a,
            &prefix_a,
            Bucket::Chunk,
            "sam",
            &digest_hex,
        ))
        .await
        .expect("initiate");
    println!("tenant A initiated upload_id={}", upload.upload_id);

    // Tenant B captures the upload_id and tries to upload a part.
    let result = adapter
        .upload_part(
            tenant_b,
            &upload,
            PartNumber::new(1).expect("pn"),
            Bytes::from_static(b"smuggled-bytes"),
        )
        .await;
    match result {
        Err(MultipartError::CrossTenantUpload { upload_id }) => {
            println!("tenant B rejected: cross-tenant replay on `{upload_id}`");
        }
        other => panic!("expected CrossTenantUpload; got {other:?}"),
    }

    // Tenant A continues their work normally.
    let etag = adapter
        .upload_part(
            tenant_a,
            &upload,
            PartNumber::new(1).expect("pn"),
            Bytes::from_static(b"legitimate"),
        )
        .await
        .expect("upload_part");
    let object = adapter
        .complete(tenant_a, &upload, vec![(PartNumber::new(1).expect("pn"), etag)])
        .await
        .expect("complete");
    println!("tenant A completed: etag={}", object.etag);
}
