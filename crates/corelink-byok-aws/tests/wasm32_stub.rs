//! wasm32 stub smoke test for `AwsKmsWasmStub`.
//!
//! Compiled only for `wasm32-unknown-unknown`. Verifies that:
//!   1. The stub constructor `AwsKmsWasmStub::new(region)` is callable.
//!   2. `AwsKmsWasmStub` implements `KmsProvider` (compile-time trait
//!      satisfaction check via `assert_kms_provider`).
//!   3. `resolved_fips_endpoint()` returns the canonical `kms-fips.`
//!      prefixed hostname.
//!   4. The async error path `wrap_dek` returns
//!      `BYOKError::Provider("AWS KMS real provider unsupported on
//!      wasm32; ...")` — the explicit-error contract documented in
//!      `specs/_audits/2026-05-15-byok-real-provider-pattern.md` §4.
//!
//! The compile of this test against `wasm32-unknown-unknown` is the
//! primary gate, validated by `cargo build --tests --target
//! wasm32-unknown-unknown -p corelink-byok-aws`. Async assertion uses
//! `futures::executor::block_on` — the workspace-pinned `futures`
//! crate's executor works on wasm32 because the stub future is
//! non-yielding (returns immediately).

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use corelink_byok::{BYOKError, Dek, KmsKeyId, KmsProvider, KmsProviderKind};
use corelink_byok_aws::AwsKmsWasmStub;
use futures::executor::block_on;
use serde_json::json;

/// Compile-time gate: the stub must implement `KmsProvider`.
fn assert_kms_provider<T: KmsProvider>() {}

#[test]
fn wasm32_stub_implements_kms_provider_trait() {
    assert_kms_provider::<AwsKmsWasmStub>();
}

#[test]
fn wasm32_stub_resolved_fips_endpoint_starts_with_kms_fips_prefix() {
    let stub = AwsKmsWasmStub::new("us-east-1");
    let endpoint = stub.resolved_fips_endpoint();
    assert!(
        endpoint.starts_with("kms-fips."),
        "expected kms-fips. prefix, got {endpoint}"
    );
}

#[test]
fn wasm32_stub_wrap_dek_returns_explicit_provider_error() {
    let stub = AwsKmsWasmStub::new("us-east-1");
    let dek = Dek { bytes: [0u8; 32] };
    let key = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:123456789012:key/abcd".into(),
        region: "us-east-1".into(),
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
