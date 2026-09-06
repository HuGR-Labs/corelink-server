use super::*;

// =============================================================================
// wasm32 stub.
// =============================================================================

/// wasm32 stub for Azure Key Vault real provider.
///
/// The Azure KV REST stack (`reqwest` + `regex` + tokio "full") does not
/// compile to `wasm32-unknown-unknown`. Any method call on this stub
/// returns `BYOKError::Provider("Azure Key Vault real provider unsupported
/// on wasm32; ...")`. In production CF Worker deployments envelope
/// operations are forwarded to the native server process via the internal
/// control-plane RPC — see
/// `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AzureKeyVaultWasmStub {
    region: String,
    vault_url: String,
}

#[cfg(target_arch = "wasm32")]
impl AzureKeyVaultWasmStub {
    /// Construct the wasm32 stub for `region` + `vault_url`. Does not
    /// contact Azure.
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::Provider`] if `vault_url` is not a FIPS-eligible
    /// Azure Key Vault host.
    pub fn new(region: &str, vault_url: &str) -> Result<Self, BYOKError> {
        let _ = resolve_fips_host(vault_url)?;
        Ok(Self {
            region: region.to_string(),
            vault_url: vault_url.to_string(),
        })
    }

    /// Same shape as the native
    /// [`AzureKeyVaultRealProvider::resolved_fips_endpoint`]. Returned even
    /// on wasm32 so spec validation can cross-reference the canonical FIPS
    /// host string.
    #[must_use]
    pub fn resolved_fips_endpoint(&self) -> String {
        match resolve_fips_host(&self.vault_url) {
            Ok((host, _)) => host,
            Err(_) => String::new(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
const WASM_UNSUPPORTED_MSG: &str =
    "Azure Key Vault real provider unsupported on wasm32; use CF Worker-side Azure SDK binding instead";

#[cfg(target_arch = "wasm32")]
#[async_trait]
impl KmsProvider for AzureKeyVaultWasmStub {
    fn provider_kind(&self) -> KmsProviderKind {
        KmsProviderKind::AzureKeyVault
    }

    fn region(&self) -> &str {
        &self.region
    }

    fn fips_level(&self) -> FipsLevel {
        FipsLevel::Fips140_2_L2
    }

    async fn wrap_dek(
        &self,
        _dek: &Dek,
        _key_id: &KmsKeyId,
        _encryption_context: Option<&Value>,
    ) -> Result<WrappedDek, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }

    async fn unwrap_dek(&self, _wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }

    async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
        Err(BYOKError::Provider(WASM_UNSUPPORTED_MSG.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
/// Type alias so downstream code can name `AzureKeyVaultRealProvider` on
/// both targets — on wasm32 it resolves to the stub. CF Workers therefore
/// link a working type but every call surfaces the explicit error above.
pub type AzureKeyVaultRealProvider = AzureKeyVaultWasmStub;
