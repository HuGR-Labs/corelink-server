//! Example: enumerate orphan multipart sessions older than 7 days
//! and abort them — what the WI-S05-006 sweeper cron will do
//! against the production R2 binding.
//!
//! Run with `cargo run -p corelink-r2-multipart --example orphan_sweeper`.

#![allow(
    clippy::print_stdout,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "example: trace the sweeper invocation to stdout"
)]

use std::time::{Duration, SystemTime};

use corelink_r2_multipart::{
    Bucket, InMemoryMultipartAdapter, InitiateRequest, MultipartAdapter, MultipartUpload,
};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

#[tokio::main]
async fn main() {
    let adapter = InMemoryMultipartAdapter::new();
    let tenant = Uuid::from_u128(0xBEEF_FACE);
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([3u8; 32]));
    let prefix = derive_prefix(&tdk, tenant);

    // Initiate 3 sessions; never complete any.
    let mut digest_seed = 0u64;
    let mut hex_buf = String::new();
    for _ in 0..3 {
        digest_seed = digest_seed.wrapping_add(1);
        hex_buf.clear();
        for _ in 0..16 {
            hex_buf.push_str(&format!("{:04x}", digest_seed as u16));
        }
        hex_buf.truncate(64);
        let upload = adapter
            .initiate(InitiateRequest::new(
                tenant,
                &prefix,
                Bucket::Chunk,
                "sam",
                &hex_buf,
            ))
            .await
            .expect("initiate");
        println!("initiated orphan {}", upload.upload_id);
    }

    // Pretend the sweeper runs 8 days from now.
    let future_now = SystemTime::now() + Duration::from_secs(8 * 86_400);
    let max_age = Duration::from_secs(7 * 86_400);
    let orphans = adapter
        .list_orphans(Bucket::Chunk, future_now, max_age)
        .await
        .expect("list_orphans");
    println!("found {} orphans", orphans.len());

    for orphan in &orphans {
        let synthesised = MultipartUpload::new(
            orphan.upload_id.clone(),
            orphan.tenant_id,
            Bucket::Chunk,
            orphan.object_key.clone(),
            orphan.initiated_at,
        );
        adapter
            .abort(orphan.tenant_id, &synthesised)
            .await
            .expect("abort");
        println!("aborted {}", orphan.upload_id);
    }

    // Re-enumerate — should be empty.
    let after = adapter
        .list_orphans(Bucket::Chunk, future_now, max_age)
        .await
        .expect("list_orphans");
    assert!(after.is_empty(), "all orphans aborted");
    println!("post-sweep orphan count: 0");
}
