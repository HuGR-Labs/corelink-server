//! Example: walk an entire happy-path multipart upload (initiate
//! → upload-parts → complete) against the in-memory fake.
//!
//! Run with `cargo run -p corelink-r2-multipart --example happy_path`.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: trace the lifecycle to stdout for newcomers"
)]

use bytes::Bytes;
use corelink_r2_multipart::{
    Bucket, InMemoryMultipartAdapter, InitiateRequest, MultipartAdapter, PartNumber,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

#[tokio::main]
async fn main() {
    let adapter = InMemoryMultipartAdapter::new();
    let tenant = Uuid::from_u128(0x1234_5678_9ABC_DEF0);
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([7u8; 32]));
    let prefix = derive_prefix(&tdk, tenant);
    let digest_hex = "abcdef0123456789".repeat(4);

    let upload = adapter
        .initiate(InitiateRequest::new(
            tenant,
            &prefix,
            Bucket::Chunk,
            "sam",
            &digest_hex,
        ))
        .await
        .expect("initiate");
    println!("initiated upload_id={} key={}", upload.upload_id, upload.object_key);

    let mut parts = Vec::new();
    for i in 1u32..=4 {
        let body = Bytes::from(format!("part-{i}-body").into_bytes());
        let etag = adapter
            .upload_part(tenant, &upload, PartNumber::new(i).expect("pn"), body)
            .await
            .expect("upload_part");
        println!("uploaded part {i} → etag={}", etag.as_str());
        parts.push((PartNumber::new(i).expect("pn"), etag));
    }

    let object = adapter
        .complete(tenant, &upload, parts)
        .await
        .expect("complete");
    println!(
        "completed: bucket={} key={} etag={} size={}",
        object.bucket, object.object_key, object.etag, object.size_bytes
    );
}
