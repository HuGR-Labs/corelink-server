//! Criterion benchmark — BYOK envelope wrap/unwrap roundtrip.
//!
//! # Target
//!
//! - p99 < 10 ms (InMemory KMS provider; production KMS adds ~20-30 ms RTT).
//!
//! Measured paths:
//! 1. `wrap_dek` — sealed DEK ciphertext via stub KMS.
//! 2. `unwrap_dek` — reverse path.
//! 3. Full roundtrip — wrap → unwrap on the same DEK + AAD.
//!
//! See `specs/03_architecture/slo_catalog.md` SLO-LATENCY-BYOK-KMS for the
//! production SLO; this bench is the per-PR perf observatory for the
//! pure-crypto + AAD path absent the network round-trip.

#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_docs_in_private_items,
    clippy::unwrap_used,
    reason = "bench harness; macros generate items we do not own"
)]

use async_trait::async_trait;
use corelink_byok::{
    types::{BYOKError, Dek, FipsLevel, KmsAccessStatus, KmsKeyId, KmsProviderKind, WrappedDek},
    KmsProvider,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde_json::Value;

#[derive(Debug)]
struct InMemoryKmsProvider;

#[async_trait]
impl KmsProvider for InMemoryKmsProvider {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AwsKms
    }
    fn region(&self) -> &str {
        "us-east-1"
    }
    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_3_L1
    }
    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        Ok(WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext: dek.bytes.to_vec(),
            encryption_context: encryption_context.cloned(),
        })
    }
    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        let mut bytes = [0u8; 32];
        if wrapped.ciphertext.len() != 32 {
            return Err(BYOKError::EnvelopeError("invalid ciphertext len".into()));
        }
        bytes.copy_from_slice(&wrapped.ciphertext);
        Ok(Dek { bytes })
    }
    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(KmsAccessStatus::Ok)
    }
}

fn key_id() -> KmsKeyId {
    KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:000000000000:key/bench-cmk".to_string(),
        region: "us-east-1".to_string(),
    }
}

fn aad() -> Value {
    serde_json::json!({
        "tenant_id": "bench-tenant",
        "blob_hash": "blake3:bench-blob",
    })
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
}

fn bench_wrap_dek(c: &mut Criterion) {
    let rt = rt();
    let provider = InMemoryKmsProvider;
    let key = key_id();
    let aad = aad();
    c.bench_function("byok/wrap_dek", |b| {
        b.iter(|| {
            rt.block_on(async {
                let dek = Dek::generate().expect("dek");
                let wrapped = provider
                    .wrap_dek(black_box(&dek), black_box(&key), Some(black_box(&aad)))
                    .await
                    .expect("wrap");
                black_box(wrapped);
            });
        });
    });
}

fn bench_unwrap_dek(c: &mut Criterion) {
    let rt = rt();
    let provider = InMemoryKmsProvider;
    let key = key_id();
    let aad = aad();
    let wrapped = rt.block_on(async {
        let dek = Dek::generate().expect("dek");
        provider
            .wrap_dek(&dek, &key, Some(&aad))
            .await
            .expect("seed wrap")
    });
    c.bench_function("byok/unwrap_dek", |b| {
        b.iter(|| {
            rt.block_on(async {
                let dek = provider
                    .unwrap_dek(black_box(&wrapped))
                    .await
                    .expect("unwrap");
                black_box(dek);
            });
        });
    });
}

fn bench_roundtrip(c: &mut Criterion) {
    let rt = rt();
    let provider = InMemoryKmsProvider;
    let key = key_id();
    let aad = aad();
    c.bench_function("byok/wrap_unwrap_roundtrip", |b| {
        b.iter(|| {
            rt.block_on(async {
                let dek = Dek::generate().expect("dek");
                let wrapped = provider
                    .wrap_dek(black_box(&dek), black_box(&key), Some(black_box(&aad)))
                    .await
                    .expect("wrap");
                let unwrapped = provider
                    .unwrap_dek(black_box(&wrapped))
                    .await
                    .expect("unwrap");
                black_box(unwrapped);
            });
        });
    });
}

criterion_group!(benches, bench_wrap_dek, bench_unwrap_dek, bench_roundtrip);
criterion_main!(benches);
