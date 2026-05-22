//! wasm32 stub smoke test for `GcpKmsWasmStub`.
//!
//! Compiled only for `wasm32-unknown-unknown`. Verifies that:
//!   1. The stub constructor `GcpKmsWasmStub::new(region)` is callable.
//!   2. `GcpKmsWasmStub` implements `KmsProvider` (compile-time trait
//!      satisfaction check via `assert_kms_provider`).
//!   3. The async error path `wrap_dek` returns
//!      `BYOKError::Provider("GCP KMS real provider unsupported on
//!      wasm32; ...")` — the explicit-error contract documented in
//!      `specs/_audits/2026-05-15-byok-real-provider-pattern.md` §4.
//!
//! Build gate: `cargo build --tests --target wasm32-unknown-unknown -p
//! corelink-byok-gcp --test wasm32_stub`.

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use corelink_byok_core::{BYOKError, Dek, KmsKeyId, KmsProvider, KmsProviderKind};
use corelink_byok_gcp::GcpKmsWasmStub;
use futures::executor::block_on;
use serde_json::json;

fn assert_kms_provider<T: KmsProvider>() {}

#[test]
fn wasm32_stub_implements_kms_provider_trait() {
    assert_kms_provider::<GcpKmsWasmStub>();
}

#[test]
fn wasm32_stub_wrap_dek_returns_explicit_provider_error() {
    let stub = GcpKmsWasmStub::new("us-central1");
    let dek = Dek { bytes: [0u8; 32] };
    let key = KmsKeyId {
        provider: KmsProviderKind::GcpKms,
        key_arn_or_id:
            "projects/p/locations/us-central1/keyRings/r/cryptoKeys/k/cryptoKeyVersions/1".into(),
        region: "us-central1".into(),
    };
    let aad = json!({ "tenant_id": "t1", "blob_hash": "b1" });
    let err = block_on(stub.wrap_dek(&dek, &key, Some(&aad))).unwrap_err();
    match err {
        BYOKError::Provider(msg) => {
            assert!(
                msg.contains("unsupported on wasm32"),
                "expected wasm32-unsupported message, got: {msg}"
            );
        }
        other => panic!("expected BYOKError::Provider, got: {other:?}"),
    }
}
