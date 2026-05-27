//! wasm32 stub smoke test for `VaultWasmStub`.
//!
//! Compiled only for `wasm32-unknown-unknown`. Verifies that:
//!   1. The stub constructor `VaultWasmStub::new(region, vault_addr)`
//!      is callable.
//!   2. `VaultWasmStub` implements `KmsProvider`.
//!   3. `wrap_dek` returns `BYOKError::Provider("Vault real provider
//!      unsupported on wasm32; ...")` — explicit-error contract per
//!      `specs/_audits/2026-05-15-byok-real-provider-pattern.md` §4.

#![cfg(target_arch = "wasm32")]
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use corelink_byok::{BYOKError, Dek, KmsKeyId, KmsProvider, KmsProviderKind};
use corelink_byok::vault::VaultWasmStub;
use futures::executor::block_on;
use serde_json::json;

fn assert_kms_provider<T: KmsProvider>() {}

#[test]
fn wasm32_stub_implements_kms_provider_trait() {
    assert_kms_provider::<VaultWasmStub>();
}

#[test]
fn wasm32_stub_wrap_dek_returns_explicit_provider_error() {
    let stub = VaultWasmStub::new("on-prem-dc1", "https://vault.internal:8200");
    let dek = Dek { bytes: [0u8; 32] };
    let key = KmsKeyId {
        provider: KmsProviderKind::HashicorpVault,
        key_arn_or_id: "transit/keys/my-key".into(),
        region: "on-prem-dc1".into(),
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
