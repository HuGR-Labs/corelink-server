/// Opaque OOB roots for archive authentication. Mutable D1 data cannot
/// construct this value.
pub(crate) struct B054ArchiveTrustRoots(
    std::collections::BTreeMap<String, ed25519_dalek::VerifyingKey>,
);

/// Root-authenticated audit signing key. Its raw key is deliberately not
/// exposed; archive callers may only inspect the id or verify exact bytes.
#[derive(Clone)]
pub(crate) struct B054ArchiveSigningKey(B054AuthenticatedSigningKey);

/// Signing-key-authenticated public commitment for one keyed epoch link
/// key. Secret link-key material is never part of this value.
#[derive(Clone, Debug)]
pub(crate) struct B054ArchiveLinkKey(B054AuthenticatedLinkKey);

impl std::fmt::Debug for B054ArchiveSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("B054ArchiveSigningKey")
            .field("key_id", &self.key_id())
            .finish_non_exhaustive()
    }
}

impl B054ArchiveSigningKey {
    pub(crate) fn key_id(&self) -> u64 {
        self.0.record.signing_key_id
    }

    pub(crate) fn verify_exact_signature(
        &self,
        payload: &[u8],
        signature_b64: &str,
    ) -> Result<(), String> {
        if payload.is_empty() || payload.len() > B054_ADMIN_MAX_JCS_BYTES * 4 {
            return Err("B054 signed archive payload length outside bound".to_owned());
        }
        b054_admin_verify_raw_signature(
            &self.0.verifying_key,
            payload,
            signature_b64,
            "B054 archive artifact",
        )
    }
}

impl B054ArchiveLinkKey {
    pub(crate) fn key_id(&self) -> u64 {
        self.0.record.link_key_id
    }

    pub(crate) fn signing_key_id(&self) -> u64 {
        self.0.record.signing_key_id
    }

    pub(crate) fn key_commitment_hex(&self) -> &str {
        &self.0.record.key_commitment_hex
    }
}

/// Write-only archive manifest signer. Seed bytes stay in zeroizing memory
/// and are neither exposed nor included in Debug output.
pub(crate) struct B054ArchiveManifestSigner {
    key_id: u64,
    seed: std::sync::Arc<Zeroizing<[u8; 32]>>,
}

impl std::fmt::Debug for B054ArchiveManifestSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("B054ArchiveManifestSigner")
            .field("key_id", &self.key_id)
            .field("seed", &"<redacted>")
            .finish()
    }
}

impl B054ArchiveManifestSigner {
    pub(crate) fn key_id(&self) -> u64 {
        self.key_id
    }

    pub(crate) fn sign_exact(&self, payload: &[u8], region: &str) -> Result<String, String> {
        if payload.is_empty()
            || payload.len() > B054_ADMIN_MAX_JCS_BYTES * 4
            || !matches!(region, "wnam" | "enam" | "weur" | "sam" | "apac" | "afr")
        {
            return Err("B054 archive signing input outside contract".to_owned());
        }
        let signer = ErasureSigningKey::from_seed(
            self.key_id,
            region_for_key(region),
            0,
            0,
            **self.seed,
        );
        Ok(base64::engine::general_purpose::STANDARD
            .encode(signer.signing_key.sign(payload).to_bytes()))
    }
}

pub(crate) fn b054_archive_parse_trust_roots(
    raw: &str,
) -> Result<B054ArchiveTrustRoots, String> {
    b054_parse_trust_root_public_keys(raw).map(B054ArchiveTrustRoots)
}

pub(crate) fn b054_archive_authenticate_signing_registry(
    roots: &B054ArchiveTrustRoots,
    registry_jcs: Vec<u8>,
    registry_signature_b64: String,
) -> Result<B054ArchiveSigningKey, String> {
    b054_authenticate_signing_registry(
        B054SignedArtifact {
            jcs: registry_jcs,
            signature_b64: registry_signature_b64,
        },
        &roots.0,
    )
    .map(B054ArchiveSigningKey)
}

pub(crate) fn b054_archive_authenticate_link_registry(
    signing: &B054ArchiveSigningKey,
    registry_jcs: Vec<u8>,
    registry_signature_b64: String,
) -> Result<B054ArchiveLinkKey, String> {
    b054_authenticate_link_registry(
        B054SignedArtifact {
            jcs: registry_jcs,
            signature_b64: registry_signature_b64,
        },
        &signing.0,
    )
    .map(B054ArchiveLinkKey)
}

pub(crate) fn b054_archive_load_manifest_signer(
) -> Result<Option<std::sync::Arc<B054ArchiveManifestSigner>>, String> {
    let Some(seed) = load_signing_seed() else {
        return Ok(None);
    };
    let key_id = signing_key_id_from_env();
    if key_id == 0 || key_id > JS_SAFE_MAX {
        return Err("B054 archive signing key id outside contract".to_owned());
    }
    Ok(Some(std::sync::Arc::new(B054ArchiveManifestSigner {
        key_id,
        seed: std::sync::Arc::new(seed),
    })))
}
