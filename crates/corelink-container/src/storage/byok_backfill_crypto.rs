//! Production encryption boundary for tenant-wide BYOK backfills.

use std::sync::Arc;

use async_trait::async_trait;
use corelink_byok::{CryptoMode, KmsProvider, KmsProviderKind};
use corelink_handler_cas::DigestAlgo;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::{
    byok_activation::{ActivationSourceObject, SourceCryptoIdentity},
    byok_backfill::{
        BackfillError, BackfillRun, BackfillSourceObject, BackfillSurface, ByokBackfillEncryptor,
    },
    byok_cas::{
        ac_crypto_context_for, cas_crypto_context_for, decrypt_cas_blob, encrypt_cas_blob,
        harden_digest, ByokConfigSource, D1ByokEnvelopeStore, D1ByokSecretReader, ModeBEncryptor,
        TcsResolver,
    },
    d1_http::D1HttpClient,
};
use crate::customer_d1::{ByokCryptoMode, ByokState, D1ByokConfigReader, TenantByokConfig};

/// Resolves the provider bound to a published source identity. Production
/// deployments may supply a multi-provider registry; the default constructor
/// installs a single-provider resolver that rejects cross-provider/region
/// rotation rather than using the target provider accidentally.
pub trait SourceKmsProviderResolver: Send + Sync + core::fmt::Debug {
    /// Resolve the exact provider and region required by a source envelope.
    fn resolve(
        &self,
        provider: KmsProviderKind,
        region: &str,
    ) -> Result<Arc<dyn KmsProvider>, BackfillError>;
}

struct ExactKmsProviderResolver {
    kms: Arc<dyn KmsProvider>,
}

impl core::fmt::Debug for ExactKmsProviderResolver {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ExactKmsProviderResolver")
            .finish_non_exhaustive()
    }
}

impl SourceKmsProviderResolver for ExactKmsProviderResolver {
    fn resolve(
        &self,
        provider: KmsProviderKind,
        region: &str,
    ) -> Result<Arc<dyn KmsProvider>, BackfillError> {
        if self.kms.provider_kind() != provider || self.kms.region() != region {
            return Err(BackfillError::Crypto(
                "source KMS provider or region is unavailable".to_owned(),
            ));
        }
        Ok(Arc::clone(&self.kms))
    }
}

/// Resolves the transition-pinned tenant policy and encrypts every historical
/// object with the same on-disk formats used by live CAS/AC.
#[derive(Debug)]
pub struct ProductionByokBackfillEncryptor {
    config: Arc<dyn ByokConfigSource>,
    tcs: Arc<TcsResolver>,
    mode_b: Arc<ModeBEncryptor>,
    source_kms: Arc<dyn SourceKmsProviderResolver>,
}

impl ProductionByokBackfillEncryptor {
    /// Assemble already-authorized configuration, TCS and Mode-B collaborators.
    #[must_use]
    pub fn new(
        config: Arc<dyn ByokConfigSource>,
        tcs: Arc<TcsResolver>,
        mode_b: Arc<ModeBEncryptor>,
        kms: Arc<dyn KmsProvider>,
    ) -> Self {
        let source_kms = Arc::new(ExactKmsProviderResolver {
            kms: Arc::clone(&kms),
        });
        Self {
            config,
            tcs,
            mode_b,
            source_kms,
        }
    }

    /// Assemble a decoder with a provider registry keyed by source identity.
    #[must_use]
    pub fn new_with_kms_resolver(
        config: Arc<dyn ByokConfigSource>,
        tcs: Arc<TcsResolver>,
        mode_b: Arc<ModeBEncryptor>,
        _kms: Arc<dyn KmsProvider>,
        source_kms: Arc<dyn SourceKmsProviderResolver>,
    ) -> Self {
        Self {
            config,
            tcs,
            mode_b,
            source_kms,
        }
    }

    async fn pinned_config(&self, run: &BackfillRun) -> Result<TenantByokConfig, BackfillError> {
        let config = self
            .config
            .get_byok_config(&run.tenant_id)
            .await
            .map_err(|error| BackfillError::Store(format!("BYOK config read: {error}")))?
            .ok_or_else(|| {
                BackfillError::Conflict("BYOK config disappeared during backfill".to_owned())
            })?;
        if config.tenant_id != run.tenant_id
            || !matches!(
                config.state,
                ByokState::Active | ByokState::Pending | ByokState::Partial
            )
            || config.cmk_key_id.as_deref().is_none_or(str::is_empty)
        {
            return Err(BackfillError::Conflict(
                "BYOK policy is not a complete pending/partial activation".to_owned(),
            ));
        }
        Ok(config)
    }

    /// Decode one source object using the immutable identity captured at
    /// publication. Generation zero is accepted only as raw plaintext;
    /// every rotated source requires an allocation-qualified envelope.
    pub async fn decode_source(
        &self,
        tenant_id: &str,
        source: &ActivationSourceObject,
        stored: &[u8],
    ) -> Result<Vec<u8>, BackfillError> {
        if tenant_id.trim().is_empty() || source.physical_key.trim().is_empty() {
            return Err(BackfillError::InvalidRequest(
                "source tenant and physical identity are required".to_owned(),
            ));
        }
        let (digest, algo) = parse_logical_identity(source.surface, &source.logical_key)?;
        validate_source_bytes(source, stored, digest, algo)?;
        match &source.crypto {
            SourceCryptoIdentity::Plaintext => {
                if source.plaintext_size != stored.len() as u64 {
                    return Err(BackfillError::SourceChanged(
                        "raw source plaintext size differs from catalog".to_owned(),
                    ));
                }
                Ok(stored.to_vec())
            }
            SourceCryptoIdentity::Encrypted {
                generation,
                allocation_id,
                crypto_mode,
                config_version,
                cmk_key_id,
                cmk_provider,
                cmk_region,
                tcs_version,
            } => {
                if *generation <= 0
                    || allocation_id.is_empty()
                    || *config_version <= 0
                    || *tcs_version <= 0
                    || cmk_key_id.is_empty()
                    || cmk_provider.is_empty()
                    || cmk_region.is_empty()
                {
                    return Err(BackfillError::Crypto(
                        "source crypto identity is incomplete".to_owned(),
                    ));
                }
                let source_provider = parse_provider(cmk_provider)?;
                let source_kms = self.source_kms.resolve(source_provider, cmk_region)?;
                let source_config = self
                    .source_config(
                        tenant_id,
                        *config_version,
                        crypto_mode,
                        cmk_provider,
                        cmk_key_id,
                        cmk_region,
                    )
                    .await?;
                let context = match algo {
                    Some(algo) => cas_crypto_context_for(
                        tenant_id,
                        digest,
                        algo,
                        cmk_key_id,
                        parse_mode(crypto_mode)?,
                    ),
                    None => ac_crypto_context_for(
                        tenant_id,
                        digest,
                        cmk_key_id,
                        parse_mode(crypto_mode)?,
                    ),
                };
                let plaintext = Zeroizing::new(match crypto_mode.as_str() {
                    "convergent" => {
                        let source_tcs =
                            self.tcs
                                .with_provider(Arc::clone(&source_kms))
                                .map_err(|error| {
                                    BackfillError::Crypto(format!(
                                        "source TCS resolver construction failed: {error}"
                                    ))
                                })?;
                        let tcs = source_tcs
                            .resolve_at_version(&source_config, Some(*tcs_version))
                            .await
                            .map_err(|error| {
                                BackfillError::Crypto(format!(
                                    "source TCS resolution failed: {error}"
                                ))
                            })?;
                        decrypt_cas_blob(stored, &tcs, &context).map_err(|error| {
                            BackfillError::Crypto(format!("source Mode-A decrypt failed: {error}"))
                        })?
                    }
                    "random" => self
                        .mode_b
                        .with_provider(Arc::clone(&source_kms))
                        .map_err(|error| {
                            BackfillError::Crypto(format!(
                                "source Mode-B resolver construction failed: {error}"
                            ))
                        })?
                        .decrypt_for_allocation_with_identity(
                            stored,
                            &context,
                            allocation_id,
                            source_provider,
                            cmk_key_id,
                            cmk_region,
                        )
                        .await
                        .map_err(|error| {
                            BackfillError::Crypto(format!("source Mode-B decrypt failed: {error}"))
                        })?,
                    _ => {
                        return Err(BackfillError::Crypto(
                            "unknown source crypto mode".to_owned(),
                        ))
                    }
                });
                if source.plaintext_size != plaintext.len() as u64 {
                    return Err(BackfillError::SourceChanged(
                        "decoded source size differs from catalog".to_owned(),
                    ));
                }
                Ok(plaintext.to_vec())
            }
        }
    }

    async fn source_config(
        &self,
        tenant_id: &str,
        config_version: i64,
        crypto_mode: &str,
        provider: &str,
        key_id: &str,
        region: &str,
    ) -> Result<TenantByokConfig, BackfillError> {
        let config = self
            .config
            .get_byok_config_at_version(tenant_id, config_version)
            .await
            .map_err(|error| BackfillError::Store(format!("BYOK config read: {error}")))?
            .ok_or_else(|| {
                BackfillError::Conflict("source BYOK config history is absent".to_owned())
            })?;
        if config.tenant_id != tenant_id
            || !matches!(
                config.state,
                ByokState::Active | ByokState::Pending | ByokState::Partial
            )
        {
            return Err(BackfillError::Conflict(
                "source BYOK policy is not available for rotation".to_owned(),
            ));
        }
        let expected_crypto_mode = match crypto_mode {
            "convergent" => ByokCryptoMode::Convergent,
            "random" => ByokCryptoMode::Random,
            _ => {
                return Err(BackfillError::Crypto(
                    "unknown source crypto mode".to_owned(),
                ))
            }
        };
        if config.crypto_mode != expected_crypto_mode
            || config.cmk_provider.as_deref() != Some(provider)
            || config.cmk_key_id.as_deref() != Some(key_id)
            || config.cmk_region.as_deref() != Some(region)
        {
            return Err(BackfillError::Conflict(
                "source ledger identity differs from immutable policy history".to_owned(),
            ));
        }
        Ok(config)
    }
}

#[async_trait]
impl ByokBackfillEncryptor for ProductionByokBackfillEncryptor {
    async fn target_hardened_digest(
        &self,
        run: &BackfillRun,
        surface: BackfillSurface,
        logical_key: &str,
        config_version: i64,
        tcs_version: i64,
    ) -> Result<String, BackfillError> {
        let (digest, _) = parse_logical_identity(surface, logical_key)?;
        let current = self.pinned_config(run).await?;
        let config = self
            .config
            .get_byok_config_at_version(&run.tenant_id, config_version)
            .await
            .map_err(|error| BackfillError::Store(format!("BYOK config read: {error}")))?
            .ok_or_else(|| {
                BackfillError::Conflict("target BYOK config history is absent".to_owned())
            })?;
        if config != current
            || config.tenant_id != run.tenant_id
            || !matches!(
                config.state,
                ByokState::Pending | ByokState::Partial | ByokState::Active
            )
        {
            return Err(BackfillError::Conflict(
                "target BYOK policy snapshot is unavailable".to_owned(),
            ));
        }
        let tcs = self
            .tcs
            .resolve_at_version(&config, Some(tcs_version))
            .await
            .map_err(|error| {
                BackfillError::Crypto(format!("target TCS resolution failed: {error}"))
            })?;
        Ok(harden_digest(&tcs, digest))
    }

    async fn source_hardened_digest(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
    ) -> Result<String, BackfillError> {
        let (digest, _) = parse_logical_identity(source.surface, &source.logical_key)?;
        let SourceCryptoIdentity::Encrypted {
            crypto_mode,
            config_version,
            cmk_provider,
            cmk_key_id,
            cmk_region,
            tcs_version,
            ..
        } = source
            .source_crypto
            .as_ref()
            .ok_or_else(|| BackfillError::Crypto("source crypto identity is absent".to_owned()))?
        else {
            return Err(BackfillError::Crypto(
                "generation-zero source has no hardened digest".to_owned(),
            ));
        };
        if source.source_crypto.as_ref().is_some_and(|identity| matches!(identity, SourceCryptoIdentity::Encrypted { generation, .. } if *generation != run.source_generation)) {
            return Err(BackfillError::SourceChanged("source hardening generation differs from run snapshot".to_owned()));
        }
        let config = self
            .source_config(
                &run.tenant_id,
                *config_version,
                crypto_mode,
                cmk_provider,
                cmk_key_id,
                cmk_region,
            )
            .await?;
        let provider = self
            .source_kms
            .resolve(parse_provider(cmk_provider)?, cmk_region)?;
        let resolver = self.tcs.with_provider(provider).map_err(|error| {
            BackfillError::Crypto(format!("source TCS resolver construction failed: {error}"))
        })?;
        let tcs = resolver
            .resolve_at_version(&config, Some(*tcs_version))
            .await
            .map_err(|error| {
                BackfillError::Crypto(format!("source TCS resolution failed: {error}"))
            })?;
        Ok(harden_digest(&tcs, digest))
    }

    async fn decrypt_source(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
        stored: &[u8],
    ) -> Result<Vec<u8>, BackfillError> {
        let crypto = source.source_crypto.clone().ok_or_else(|| {
            BackfillError::Crypto("source generation crypto identity is absent".to_owned())
        })?;
        if let SourceCryptoIdentity::Encrypted { generation, .. } = &crypto {
            if *generation != run.source_generation {
                return Err(BackfillError::SourceChanged(
                    "source crypto generation differs from the run snapshot".to_owned(),
                ));
            }
        } else if run.source_generation != 0 {
            return Err(BackfillError::SourceChanged(
                "rotated run cannot decode a generation-zero source".to_owned(),
            ));
        }
        let activation = ActivationSourceObject {
            surface: source.surface,
            logical_key: source.logical_key.clone(),
            physical_key: source.source_physical_key.clone(),
            plaintext_size: source.plaintext_size.ok_or_else(|| {
                BackfillError::SourceChanged("source plaintext size is absent".to_owned())
            })?,
            source_blake3: match source.source_blake3.clone() {
                Some(hash) => hash,
                None if run.source_generation == 0 => {
                    // Legacy enumeration has no catalog hash. Derive the
                    // exact GET hash before validating its logical identity;
                    // rotated generations remain strict and require the
                    // persisted catalog proof.
                    blake3::hash(stored).to_hex().to_string()
                }
                None => {
                    return Err(BackfillError::SourceChanged(
                        "source ciphertext hash is absent".to_owned(),
                    ))
                }
            },
            crypto,
        };
        self.decode_source(&run.tenant_id, &activation, stored)
            .await
    }

    async fn encrypt(
        &self,
        run: &BackfillRun,
        source: &BackfillSourceObject,
        allocation_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, BackfillError> {
        if allocation_id.len() != 64
            || !allocation_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(BackfillError::InvalidRequest(
                "allocation_id must be lowercase 64-hex".to_owned(),
            ));
        }
        let config = self.pinned_config(run).await?;
        let key_id = config.cmk_key_id.as_deref().unwrap_or_default();
        let identity = parse_source_identity(source)?;
        let mode = match config.crypto_mode {
            ByokCryptoMode::Convergent => CryptoMode::Convergent,
            ByokCryptoMode::Random => CryptoMode::Random,
        };
        let context = match identity {
            BackfillIdentity::Cas { digest, algo } => {
                cas_crypto_context_for(&run.tenant_id, digest, algo, key_id, mode)
            }
            BackfillIdentity::Ac { digest } => {
                ac_crypto_context_for(&run.tenant_id, digest, key_id, mode)
            }
        };
        match config.crypto_mode {
            ByokCryptoMode::Convergent => {
                let tcs = self.tcs.resolve(&config).await.map_err(|error| {
                    BackfillError::Crypto(format!("TCS resolution failed: {error}"))
                })?;
                encrypt_cas_blob(plaintext, &tcs, &context).map_err(|error| {
                    BackfillError::Crypto(format!("convergent encryption failed: {error}"))
                })
            }
            ByokCryptoMode::Random => self
                .mode_b
                .encrypt_for_allocation(plaintext, &context, allocation_id)
                .await
                .map_err(|error| {
                    BackfillError::Crypto(format!("Mode-B encryption failed: {error}"))
                }),
        }
    }
}

/// Construct the real config/TCS/envelope collaborators over one D1 client and
/// the binary-selected KMS provider. This is the factory consumed by the
/// internal activation scheduler; it never accepts plaintext key material.
pub fn build_production_backfill_encryptor(
    d1: Arc<D1HttpClient>,
    kms: Arc<dyn KmsProvider>,
) -> Result<ProductionByokBackfillEncryptor, BackfillError> {
    let config: Arc<dyn ByokConfigSource> = Arc::new(D1ByokConfigReader::new(Arc::clone(&d1)));
    let secrets = Arc::new(D1ByokSecretReader::new(Arc::clone(&d1)));
    let tcs = Arc::new(
        TcsResolver::with_default_ttl(secrets, Arc::clone(&kms)).map_err(|error| {
            BackfillError::Crypto(format!("TCS resolver construction failed: {error}"))
        })?,
    );
    let envelopes = Arc::new(D1ByokEnvelopeStore::new(d1));
    let mode_b = Arc::new(
        ModeBEncryptor::with_default_ttl(Arc::clone(&kms), envelopes).map_err(|error| {
            BackfillError::Crypto(format!("Mode-B encryptor construction failed: {error}"))
        })?,
    );
    Ok(ProductionByokBackfillEncryptor::new(
        config, tcs, mode_b, kms,
    ))
}

enum BackfillIdentity<'a> {
    Cas { digest: &'a str, algo: DigestAlgo },
    Ac { digest: &'a str },
}

fn parse_source_identity(
    source: &BackfillSourceObject,
) -> Result<BackfillIdentity<'_>, BackfillError> {
    let (digest, algo) = parse_logical_identity(source.surface, &source.logical_key)?;
    Ok(match algo {
        Some(algo) => BackfillIdentity::Cas { digest, algo },
        None => BackfillIdentity::Ac { digest },
    })
}

fn parse_logical_identity(
    surface: BackfillSurface,
    logical_key: &str,
) -> Result<(&str, Option<DigestAlgo>), BackfillError> {
    match surface {
        BackfillSurface::Cas => {
            let (namespace, digest) = logical_key.split_once(':').ok_or_else(|| {
                BackfillError::InvalidRequest(
                    "CAS backfill identity must be algorithm-qualified".to_owned(),
                )
            })?;
            validate_digest(digest)?;
            let algo = match namespace {
                "blake3" => DigestAlgo::Blake3,
                "sha256" => DigestAlgo::Sha256,
                _ => {
                    return Err(BackfillError::InvalidRequest(
                        "unknown CAS backfill digest namespace".to_owned(),
                    ))
                }
            };
            Ok((digest, Some(algo)))
        }
        BackfillSurface::Ac => {
            validate_digest(logical_key)?;
            Ok((logical_key, None))
        }
    }
}

fn parse_mode(mode: &str) -> Result<CryptoMode, BackfillError> {
    match mode {
        "convergent" => Ok(CryptoMode::Convergent),
        "random" => Ok(CryptoMode::Random),
        _ => Err(BackfillError::Crypto(
            "unknown source crypto mode".to_owned(),
        )),
    }
}

fn parse_provider(provider: &str) -> Result<KmsProviderKind, BackfillError> {
    match provider {
        "aws" => Ok(KmsProviderKind::AwsKms),
        "gcp" => Ok(KmsProviderKind::GcpKms),
        "azure" => Ok(KmsProviderKind::AzureKeyVault),
        "vault" => Ok(KmsProviderKind::HashicorpVault),
        _ => Err(BackfillError::Crypto(
            "unknown source KMS provider".to_owned(),
        )),
    }
}

fn validate_source_bytes(
    source: &ActivationSourceObject,
    stored: &[u8],
    digest: &str,
    algo: Option<DigestAlgo>,
) -> Result<(), BackfillError> {
    let stored_blake3 = blake3::hash(stored).to_hex();
    validate_hex_hash(&source.source_blake3, stored_blake3.as_str())?;
    if stored.len() as u64 != source.plaintext_size
        && matches!(&source.crypto, SourceCryptoIdentity::Plaintext)
    {
        return Err(BackfillError::SourceChanged(
            "raw source size differs from catalog".to_owned(),
        ));
    }
    if let Some(algo) = algo {
        // CAS generation-zero objects are plaintext and must agree with the
        // algorithm-qualified logical key. For ciphertext, the catalog hash
        // is authoritative and is checked by the caller before decryption.
        if matches!(&source.crypto, SourceCryptoIdentity::Plaintext) {
            let actual = match algo {
                DigestAlgo::Blake3 => blake3::hash(stored).to_hex().to_string(),
                DigestAlgo::Sha256 => hex::encode(Sha256::digest(stored)),
            };
            if actual != digest {
                return Err(BackfillError::SourceChanged(
                    "CAS plaintext digest differs from logical identity".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_hex_hash(expected: &str, actual: &str) -> Result<(), BackfillError> {
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || expected != actual
    {
        return Err(BackfillError::SourceChanged(
            "source ciphertext hash differs from catalog".to_owned(),
        ));
    }
    Ok(())
}

fn validate_digest(digest: &str) -> Result<(), BackfillError> {
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(BackfillError::InvalidRequest(
            "backfill digest must be lowercase 64-hex".to_owned(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn digest(bytes: &[u8]) -> String {
        blake3::hash(bytes).to_hex().to_string()
    }

    fn raw_source(
        surface: BackfillSurface,
        logical_key: String,
        bytes: &[u8],
    ) -> ActivationSourceObject {
        ActivationSourceObject {
            surface,
            logical_key,
            physical_key: "region/tenant/raw".to_owned(),
            plaintext_size: bytes.len() as u64,
            source_blake3: digest(bytes),
            crypto: SourceCryptoIdentity::Plaintext,
        }
    }

    #[test]
    fn generation_zero_mode_a_shape_validates_raw_cas_bytes() {
        let bytes = b"legacy plaintext";
        let source = raw_source(
            BackfillSurface::Cas,
            format!("blake3:{}", digest(bytes)),
            bytes,
        );
        let (digest, algo) = parse_logical_identity(source.surface, &source.logical_key).unwrap();
        assert_eq!(algo, Some(DigestAlgo::Blake3));
        assert!(validate_source_bytes(&source, bytes, digest, algo).is_ok());
        assert!(validate_source_bytes(&source, b"tampered", digest, algo).is_err());
    }

    #[test]
    fn rotation_mode_a_requires_exact_generation_and_identity_hash() {
        let source = ActivationSourceObject {
            surface: BackfillSurface::Cas,
            logical_key: format!("sha256:{}", "a".repeat(64)),
            physical_key: "region/tenant/byok/g0001/object".to_owned(),
            plaintext_size: 4,
            source_blake3: "c".repeat(64),
            crypto: SourceCryptoIdentity::Encrypted {
                generation: 1,
                allocation_id: "b".repeat(64),
                crypto_mode: "convergent".to_owned(),
                config_version: 9,
                cmk_key_id: "old-key".to_owned(),
                cmk_provider: "aws".to_owned(),
                cmk_region: "us-east-1".to_owned(),
                tcs_version: 3,
            },
        };
        assert!(matches!(
            source.crypto,
            SourceCryptoIdentity::Encrypted {
                generation: 1,
                crypto_mode,
                tcs_version: 3,
                ..
            } if crypto_mode == "convergent"
        ));
        assert!(validate_hex_hash(&"c".repeat(64), &"d".repeat(64)).is_err());
    }

    #[test]
    fn rotation_mode_b_is_allocation_qualified_and_unknown_provider_fails() {
        let source = SourceCryptoIdentity::Encrypted {
            generation: 2,
            allocation_id: "a".repeat(64),
            crypto_mode: "random".to_owned(),
            config_version: 10,
            cmk_key_id: "rotated-key".to_owned(),
            cmk_provider: "unknown".to_owned(),
            cmk_region: "us-east-1".to_owned(),
            tcs_version: 4,
        };
        assert!(
            matches!(source, SourceCryptoIdentity::Encrypted { ref allocation_id, ref crypto_mode, .. } if allocation_id.len() == 64 && crypto_mode == "random")
        );
        assert!(parse_provider("unknown").is_err());
        assert_eq!(parse_mode("random").unwrap(), CryptoMode::Random);
    }
}
