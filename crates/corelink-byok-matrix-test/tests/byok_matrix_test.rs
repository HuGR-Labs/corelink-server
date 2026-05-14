//! 16-combination matrix test: 4 providers × 4 ops {write, read, wrap, unwrap}.
//!
//! All 16 cells MUST be green in CI per PR (auto-fail gate).
//! Weekly cron staging run via `.github/workflows/byok_matrix_weekly.yml`.
//!
//! # Provider mock mode
//!
//! All providers run in mock mode in CI. Integration mode (real API) is
//! triggered by `CORELINK_BYOK_{GCP,AZURE,VAULT}_MOCK != "1"` plus real
//! credentials (staging only; weekly cron).
//!
//! # Matrix layout
//!
//! ```text
//! Provider          | write | read  | wrap  | unwrap
//! ------------------|-------|-------|-------|--------
//! gcp_kms           |   ✓   |   ✓   |   ✓   |   ✓
//! azure_key_vault   |   ✓   |   ✓   |   ✓   |   ✓
//! hashicorp_vault   |   ✓   |   ✓   |   ✓   |   ✓
//! aws_kms (mock)    |   ✓   |   ✓   |   ✓   |   ✓
//! ```
//!
//! The AWS KMS provider is implemented via a minimal mock to complete the
//! 16-cell matrix (WI-S14-004 not yet merged; AwsMockProvider fills the slot).

#![allow(clippy::uninlined_format_args, clippy::format_in_format_args, clippy::expect_used, clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
use corelink_byok::{
    BYOKError, DekCache, Dek, KmsAccessStatus, KmsKeyId, KmsProvider, KmsProviderKind, FipsLevel,
    WrappedDek,
};
use corelink_byok_gcp::GcpKmsProvider;
use corelink_byok_azure::AzureKeyVaultProvider;
use corelink_byok_vault::VaultProvider;
use async_trait::async_trait;
use serde_json::json;

// ──────────────────────────────────────────────────────────────────────────────
// AWS mock provider (fills matrix slot until WI-S14-004 merges)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct AwsMockProvider {
    region: String,
}

impl AwsMockProvider {
    fn new(region: &str) -> Self {
        Self { region: region.to_string() }
    }
}

#[async_trait]
impl KmsProvider for AwsMockProvider {
    fn provider_kind(&self) -> KmsProviderKind { KmsProviderKind::AwsKms }
    fn region(&self) -> &str { &self.region }
    fn fips_level(&self) -> FipsLevel { FipsLevel::Fips140_3_L1 }

    async fn wrap_dek(
        &self,
        dek: &Dek,
        key_id: &KmsKeyId,
        encryption_context: Option<&serde_json::Value>,
    ) -> Result<WrappedDek, BYOKError> {
        let mut ct = dek.bytes.to_vec();
        for b in &mut ct { *b ^= 0xBB; }
        Ok(WrappedDek {
            provider: KmsProviderKind::AwsKms,
            key_id: key_id.clone(),
            ciphertext: ct,
            encryption_context: encryption_context.cloned(),
        })
    }

    async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        if wrapped.provider != KmsProviderKind::AwsKms {
            return Err(BYOKError::EnvelopeError("wrong provider".to_string()));
        }
        if wrapped.ciphertext.len() != 32 {
            return Err(BYOKError::EnvelopeError("bad len".to_string()));
        }
        let mut bytes = [0u8; 32];
        for (i, &b) in wrapped.ciphertext.iter().enumerate() {
            bytes[i] = b ^ 0xBB;
        }
        Ok(Dek { bytes })
    }

    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Ok(KmsAccessStatus::Ok)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Matrix test helpers
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct MatrixResult {
    provider: &'static str,
    op: &'static str,
    passed: bool,
    error: Option<String>,
}

impl MatrixResult {
    fn pass(provider: &'static str, op: &'static str) -> Self {
        Self { provider, op, passed: true, error: None }
    }
    fn fail(provider: &'static str, op: &'static str, err: String) -> Self {
        Self { provider, op, passed: false, error: Some(err) }
    }
}

/// Run all 4 ops against a provider and return 4 MatrixResults.
async fn run_provider_matrix<P: KmsProvider>(
    provider: &P,
    provider_name: &'static str,
    key_id: KmsKeyId,
    ctx: &serde_json::Value,
) -> [MatrixResult; 4] {
    let dek = match Dek::generate() {
        Ok(d) => d,
        Err(e) => {
            let err = e.to_string();
            return [
                MatrixResult::fail(provider_name, "write", err.clone()),
                MatrixResult::fail(provider_name, "read", err.clone()),
                MatrixResult::fail(provider_name, "wrap", err.clone()),
                MatrixResult::fail(provider_name, "unwrap", err),
            ];
        }
    };
    let dek_bytes = dek.bytes;

    // ── wrap ──────────────────────────────────────────────────────────────────
    let wrap_result = provider.wrap_dek(&dek, &key_id, Some(ctx)).await;
    let wrap_cell = match &wrap_result {
        Ok(_) => MatrixResult::pass(provider_name, "wrap"),
        Err(e) => MatrixResult::fail(provider_name, "wrap", e.to_string()),
    };

    // ── unwrap ────────────────────────────────────────────────────────────────
    let unwrap_cell = match wrap_result {
        Ok(ref wrapped) => {
            match provider.unwrap_dek(wrapped).await {
                Ok(unwrapped) if unwrapped.bytes == dek_bytes => {
                    MatrixResult::pass(provider_name, "unwrap")
                }
                Ok(_) => MatrixResult::fail(
                    provider_name,
                    "unwrap",
                    "unwrapped DEK bytes do not match original".to_string(),
                ),
                Err(e) => MatrixResult::fail(provider_name, "unwrap", e.to_string()),
            }
        }
        Err(ref e) => MatrixResult::fail(
            provider_name,
            "unwrap",
            format!("skipped (wrap failed: {e})"),
        ),
    };

    // ── write (verify wrap produces WrappedDek with correct provider) ──────────
    let write_cell = match provider.wrap_dek(&Dek::generate().expect("entropy"), &key_id, Some(ctx)).await {
        Ok(w) if w.provider == provider.provider_kind() => {
            MatrixResult::pass(provider_name, "write")
        }
        Ok(w) => MatrixResult::fail(
            provider_name,
            "write",
            format!("WrappedDek.provider {:?} != expected {:?}", w.provider, provider.provider_kind()),
        ),
        Err(e) => MatrixResult::fail(provider_name, "write", e.to_string()),
    };

    // ── read (check_access + DEK cache roundtrip) ─────────────────────────────
    let read_cell = match provider.check_access(&key_id).await {
        Ok(KmsAccessStatus::Ok) => {
            // Also verify DEK cache roundtrip.
            let cache = match DekCache::new(300) {
                Ok(c) => c,
                Err(e) => return [
                    write_cell,
                    MatrixResult::fail(provider_name, "read", e.to_string()),
                    wrap_cell,
                    unwrap_cell,
                ],
            };
            let dek2 = match Dek::generate() {
                Ok(d) => d,
                Err(e) => return [
                    write_cell,
                    MatrixResult::fail(provider_name, "read", e.to_string()),
                    wrap_cell,
                    unwrap_cell,
                ],
            };
            let dek2_bytes = dek2.bytes;
            let wrapped2 = match provider.wrap_dek(&dek2, &key_id, Some(ctx)).await {
                Ok(w) => w,
                Err(e) => return [
                    write_cell,
                    MatrixResult::fail(provider_name, "read", format!("wrap for cache test: {e}")),
                    wrap_cell,
                    unwrap_cell,
                ],
            };
            if let Err(e) = cache.put(&wrapped2, dek2).await {
                return [
                    write_cell,
                    MatrixResult::fail(provider_name, "read", format!("cache put: {e}")),
                    wrap_cell,
                    unwrap_cell,
                ];
            }
            match cache.get(&wrapped2).await {
                Some(cached) if cached.bytes == dek2_bytes => MatrixResult::pass(provider_name, "read"),
                Some(_) => MatrixResult::fail(provider_name, "read", "cached DEK bytes mismatch".to_string()),
                None => MatrixResult::fail(provider_name, "read", "DEK cache miss immediately after put".to_string()),
            }
        }
        Ok(s) => MatrixResult::fail(provider_name, "read", format!("check_access returned {s:?}")),
        Err(e) => MatrixResult::fail(provider_name, "read", e.to_string()),
    };

    [write_cell, read_cell, wrap_cell, unwrap_cell]
}

// ──────────────────────────────────────────────────────────────────────────────
// 16-combination matrix test
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn byok_matrix_16_combinations_all_green() {
    let ctx = json!({"tenant_id": "T-matrix-001", "blob_hash": "H-matrix-001", "sprint": "S14"});

    // GCP provider (4 cells)
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let gcp_key_id = KmsKeyId {
        provider: KmsProviderKind::GcpKms,
        key_arn_or_id: "projects/corelink-staging/locations/us-east1/keyRings/byok/cryptoKeys/matrix-key".to_string(),
        region: "us-east1".to_string(),
    };
    let gcp_cells = run_provider_matrix(&gcp, "gcp_kms", gcp_key_id, &ctx).await;

    // Azure provider (4 cells)
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    let azure_key_id = KmsKeyId {
        provider: KmsProviderKind::AzureKeyVault,
        key_arn_or_id: "https://corelink-staging.vault.azure.net/keys/matrix-key".to_string(),
        region: "eastus".to_string(),
    };
    let azure_cells = run_provider_matrix(&azure, "azure_key_vault", azure_key_id, &ctx).await;

    // Vault provider (4 cells)
    let vault = VaultProvider::new_mock("us-east-1");
    let vault_key_id = KmsKeyId {
        provider: KmsProviderKind::HashicorpVault,
        key_arn_or_id: "transit/keys/matrix-key".to_string(),
        region: "us-east-1".to_string(),
    };
    let vault_cells = run_provider_matrix(&vault, "hashicorp_vault", vault_key_id, &ctx).await;

    // AWS mock provider (4 cells — placeholder until WI-S14-004 merges)
    let aws = AwsMockProvider::new("us-east-1");
    let aws_key_id = KmsKeyId {
        provider: KmsProviderKind::AwsKms,
        key_arn_or_id: "arn:aws:kms:us-east-1:123456789012:key/matrix-key-uuid".to_string(),
        region: "us-east-1".to_string(),
    };
    let aws_cells = run_provider_matrix(&aws, "aws_kms", aws_key_id, &ctx).await;

    // Collect all 16 cells.
    let all_cells: Vec<MatrixResult> = gcp_cells.into_iter()
        .chain(azure_cells)
        .chain(vault_cells)
        .chain(aws_cells)
        .collect();

    assert_eq!(all_cells.len(), 16, "must be exactly 16 matrix cells");

    // Report failures.
    let failures: Vec<&MatrixResult> = all_cells.iter().filter(|c| !c.passed).collect();
    if !failures.is_empty() {
        let report: String = failures.iter()
            .map(|c| format!("  FAIL [{}][{}]: {}", c.provider, c.op, c.error.as_deref().unwrap_or("unknown")))
            .collect::<Vec<_>>()
            .join("\n");
        panic!("BYOK matrix test: {}/{} cells FAILED\n{}", failures.len(), 16, report);
    }

    // All 16 green.
}

// ──────────────────────────────────────────────────────────────────────────────
// Per-provider FIPS level constant test (Acceptance Criteria §8 scenario 7)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn fips_level_const_per_provider() {
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    let vault = VaultProvider::new_mock("us-east-1");
    let aws = AwsMockProvider::new("us-east-1");

    assert_eq!(aws.fips_level(), FipsLevel::Fips140_3_L1, "AWS KMS: FIPS 140-3 L1");
    assert_eq!(gcp.fips_level(), FipsLevel::Fips140_2_L1, "GCP KMS: FIPS 140-2 L1");
    assert_eq!(azure.fips_level(), FipsLevel::Fips140_2_L2, "Azure Key Vault Premium HSM: FIPS 140-2 L2");
    assert_eq!(vault.fips_level(), FipsLevel::Fips140_3_L1, "Vault Enterprise FIPS: FIPS 140-3 L1");
}

// ──────────────────────────────────────────────────────────────────────────────
// DEK cache TTL hard limit (INV-BYOK-CRYPTO-SOVEREIGNTY)
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn dek_cache_ttl_hard_limit_enforced() {
    assert!(DekCache::new(300).is_ok(), "300s is the max allowed TTL");
    assert!(
        matches!(DekCache::new(301), Err(BYOKError::DekCacheTtlViolation { .. })),
        "301s violates INV-BYOK-CRYPTO-SOVEREIGNTY"
    );
    assert!(
        matches!(DekCache::new(u64::MAX), Err(BYOKError::DekCacheTtlViolation { .. })),
        "MAX TTL violates INV-BYOK-CRYPTO-SOVEREIGNTY"
    );
}

// ──────────────────────────────────────────────────────────────────────────────
// Cross-provider kms_provider field tampering (Acceptance Criteria §8 scenario 9)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn cross_provider_kms_provider_tampering_rejected() {
    // Scenario: D1 row stored kms_provider = "gcp_kms"
    // Attacker tampers D1 to "azure_key_vault"
    // → unwrap via Azure provider fails (wrong provider for stored wrapped DEK)
    let gcp = GcpKmsProvider::new_mock("us-east1");
    let gcp_key_id = KmsKeyId {
        provider: KmsProviderKind::GcpKms,
        key_arn_or_id: "projects/p/locations/us-east1/keyRings/r/cryptoKeys/k".to_string(),
        region: "us-east1".to_string(),
    };
    let dek = Dek::generate().expect("entropy");
    let ctx = json!({"tenant_id": "T-tamper"});
    let gcp_wrapped = gcp.wrap_dek(&dek, &gcp_key_id, Some(&ctx)).await.expect("gcp wrap");

    // Tamper: change provider field in WrappedDek to AzureKeyVault.
    let tampered = WrappedDek {
        provider: KmsProviderKind::AzureKeyVault,
        key_id: KmsKeyId {
            provider: KmsProviderKind::AzureKeyVault,
            ..gcp_wrapped.key_id.clone()
        },
        ..gcp_wrapped
    };

    // Azure provider should reject: provider mismatch in WrappedDek.
    let azure = AzureKeyVaultProvider::new_mock("eastus");
    let result = azure.unwrap_dek(&tampered).await;
    assert!(result.is_err(), "cross-provider tampering must be rejected by Azure adapter");

    // Vault provider should also reject.
    let vault = VaultProvider::new_mock("us-east-1");
    let tampered_vault = WrappedDek {
        provider: KmsProviderKind::HashicorpVault,
        key_id: KmsKeyId {
            provider: KmsProviderKind::HashicorpVault,
            ..KmsKeyId {
                provider: KmsProviderKind::GcpKms,
                key_arn_or_id: "projects/p/locations/us-east1/keyRings/r/cryptoKeys/k".to_string(),
                region: "us-east1".to_string(),
            }
        },
        ciphertext: vec![0u8; 64],
        encryption_context: Some(ctx.clone()),
    };
    let result_vault = vault.unwrap_dek(&tampered_vault).await;
    assert!(result_vault.is_err(), "cross-provider tampering must be rejected by Vault adapter");
}

// ──────────────────────────────────────────────────────────────────────────────
// Vault mTLS cert expiry alert 30d before (Acceptance Criteria §8 scenario 10)
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn vault_mtls_cert_expiry_alert_30d() {
    let mut vault = VaultProvider::new_mock("us-east-1");
    vault.set_mock_cert_days_remaining(Some(29));
    let key_id = KmsKeyId {
        provider: KmsProviderKind::HashicorpVault,
        key_arn_or_id: "transit/keys/k".to_string(),
        region: "us-east-1".to_string(),
    };
    let result = vault.check_access(&key_id).await;
    assert!(
        matches!(result, Err(BYOKError::MtlsCertExpiringSoon { days_remaining: 29 })),
        "must alert on cert expiry < 30d"
    );
}
