//! wasm32 stub smoke test for `AzureKeyVaultWasmStub`.
//!
//! Compiled only for `wasm32-unknown-unknown`. Verifies that:
//!   1. The stub constructor `AzureKeyVaultWasmStub::new(region, url)`
//!      is callable and returns `Ok` for a valid Key Vault URL.
//!   2. `AzureKeyVaultWasmStub` implements `KmsProvider`.
//!   3. `wrap_dek` returns `BYOKError::Provider("Azure Key Vault real
//!      provider unsupported on wasm32; ...")` — explicit-error
//!      contract per `specs/_audits/2026-05-15-byok-real-provider-
//!      pattern.md` §4.

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use corelink_byok::{BYOKError, Dek, KmsKeyId, KmsProvider, KmsProviderKind};
use corelink_byok_azure::AzureKeyVaultWasmStub;
use futures::executor::block_on;
use serde_json::json;

fn assert_kms_provider<T: KmsProvider>() {}

#[test]
fn wasm32_stub_implements_kms_provider_trait() {
    assert_kms_provider::<AzureKeyVaultWasmStub>();
}

#[test]
fn wasm32_stub_wrap_dek_returns_explicit_provider_error() {
    let stub = AzureKeyVaultWasmStub::new("eastus", "https://example.vault.azure.net")
        .expect("stub constructor accepts valid Key Vault URL");
    let dek = Dek { bytes: [0u8; 32] };
    let key = KmsKeyId {
        provider: KmsProviderKind::AzureKeyVault,
        key_arn_or_id: "https://example.vault.azure.net/keys/my-key/abcd".into(),
        region: "eastus".into(),
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
