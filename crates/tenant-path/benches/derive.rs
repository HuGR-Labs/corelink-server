//! Criterion benchmark for [`derive_prefix`]. Implemented in WI-S01-001 ST-008.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    reason = "bench harness; macros generate items we do not own"
)]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use corelink_tenant_path::{derive_prefix, TenantDerivationKey};
use uuid::Uuid;
use zeroize::Zeroizing;

fn bench_derive_prefix(c: &mut Criterion) {
    let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0xA5; 32]));
    let tid = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001")
        .expect("static UUID literal parses");
    c.bench_function("derive_prefix", |b| {
        b.iter(|| {
            let p = derive_prefix(black_box(&tdk), black_box(tid));
            black_box(p);
        });
    });
}

criterion_group!(benches, bench_derive_prefix);
criterion_main!(benches);
