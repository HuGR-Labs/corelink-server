// ─── Crypto context + blob (de)serialisation (pure, surface-agnostic) ───────

/// HKDF/AAD namespace tag for the CAS keyspace's digest function — domain
/// separation between the BLAKE3 and SHA-256 keyspaces.
const fn namespace_for(algo: DigestAlgo) -> &'static str {
    match algo {
        DigestAlgo::Blake3 => "blake3",
        DigestAlgo::Sha256 => "sha256",
    }
}

/// **§4 key-hardening (audit H-4) — close the confirmation oracle.**
///
/// Returns `hex(HMAC-SHA256(key = TCS, msg = plaintext_digest))`.
///
/// For a BYOK-active tenant the *physical* R2 object key embeds THIS value
/// instead of the raw plaintext digest, so an attacker with R2 read who *guesses*
/// a plaintext cannot confirm its presence by computing its digest — the
/// hardened key reveals nothing without the per-tenant `TCS` (which dies with the
/// CMK).
///
/// # Invariants (audit H-4)
///
/// - **Computed ON-THE-FLY** at every write+read; a raw-digest→hardened map is
///   NEVER persisted (a map would re-leak the digest under R2-read).
/// - **Deterministic per `(TCS, digest)`** ⇒ identical plaintext within a tenant
///   maps to the identical hardened key ⇒ intra-tenant convergent dedup (the
///   write-path HEAD check) still hits. A *different* tenant's TCS yields a
///   different hardened key (no cross-tenant correlation).
/// - This is the STORAGE-KEY digest only. The convergent [`CryptoContext`]'s
///   `plaintext_digest` (bound into the AEAD AAD + re-verified on the decrypted
///   plaintext) stays the REAL digest — the two are deliberately distinct.
#[must_use]
pub fn harden_digest(tcs: &Tcs, plaintext_digest: &str) -> String {
    // HMAC-SHA256 accepts a key of any length; the 32-byte TCS never errors.
    let mut mac = <Hmac<Sha256>>::new_from_slice(&tcs.bytes)
        .unwrap_or_else(|_| unreachable!("HMAC-SHA256 accepts any key length"));
    mac.update(plaintext_digest.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Build the [`CryptoContext`] for a CAS object under an explicit policy `mode`.
///
/// `total_len` is fixed to `0` (like [`CryptoContext::legacy`]) so the read side
/// reconstructs a byte-identical context WITHOUT knowing the plaintext length
/// before decrypt. `plaintext_digest` is ALWAYS the raw content digest (the §4
/// hardening applies to the storage key, never to the AAD-bound digest).
#[must_use]
pub fn cas_crypto_context_for(
    tenant: &str,
    digest: &str,
    algo: DigestAlgo,
    key_id: &str,
    mode: CryptoMode,
) -> CryptoContext {
    CryptoContext::new_single_shot(
        tenant,
        digest,
        CryptoAlgo::Aes256Gcm,
        namespace_for(algo),
        mode,
        CAS_SURFACE,
        key_id,
        0,
    )
}

/// Build the convergent (Mode A) [`CryptoContext`] for a CAS object — the
/// `mode = Convergent` specialisation of [`cas_crypto_context_for`].
#[must_use]
pub fn cas_crypto_context(
    tenant: &str,
    digest: &str,
    algo: DigestAlgo,
    key_id: &str,
) -> CryptoContext {
    cas_crypto_context_for(tenant, digest, algo, key_id, CryptoMode::Convergent)
}

/// Build the [`CryptoContext`] for an **AC** result payload under an explicit
/// policy `mode`. Binds [`AC_SURFACE`] (`"ac"`) so the derived key + AEAD AAD
/// are domain-separated from CAS: an AC ciphertext can never be decrypted as (or
/// swapped with) a CAS ciphertext. The AC content identity is the
/// `action_digest` (BLAKE3 keyspace — see `R2AcHandler::r2_key`).
#[must_use]
pub fn ac_crypto_context_for(
    tenant: &str,
    action_digest: &str,
    key_id: &str,
    mode: CryptoMode,
) -> CryptoContext {
    CryptoContext::new_single_shot(
        tenant,
        action_digest,
        CryptoAlgo::Aes256Gcm,
        namespace_for(DigestAlgo::Blake3),
        mode,
        AC_SURFACE,
        key_id,
        0,
    )
}

/// Build the convergent (Mode A) [`CryptoContext`] for an AC result payload —
/// the `mode = Convergent` specialisation of [`ac_crypto_context_for`].
#[must_use]
pub fn ac_crypto_context(tenant: &str, action_digest: &str, key_id: &str) -> CryptoContext {
    ac_crypto_context_for(tenant, action_digest, key_id, CryptoMode::Convergent)
}

/// Encrypt CAS plaintext into the stored on-disk representation
/// (`MAGIC ‖ nonce ‖ ciphertext`). Convergent ⇒ identical content yields
/// byte-identical output ⇒ dedup preserved.
///
/// # Errors
///
/// Propagates any [`BYOKError`] from the convergent encryptor (fail-closed —
/// the caller must NOT store plaintext on error).
pub fn encrypt_cas_blob(
    plaintext: &[u8],
    tcs: &Tcs,
    ctx: &CryptoContext,
) -> Result<Vec<u8>, BYOKError> {
    let blob = encrypt_convergent(plaintext, tcs, ctx)?;
    let mut out = Vec::with_capacity(BLOB_MAGIC.len() + blob.nonce.len() + blob.ciphertext.len());
    out.extend_from_slice(BLOB_MAGIC);
    out.extend_from_slice(&blob.nonce);
    out.extend_from_slice(&blob.ciphertext);
    Ok(out)
}

/// Decrypt a stored CAS blob (`MAGIC ‖ nonce ‖ ciphertext`) back to plaintext.
///
/// # Errors
///
/// Returns `Err` on a malformed/non-magic object (fail-closed: the caller must
/// NOT serve the raw stored bytes for an active tenant) or any AEAD failure.
pub fn decrypt_cas_blob(stored: &[u8], tcs: &Tcs, ctx: &CryptoContext) -> Result<Vec<u8>, String> {
    let magic = stored
        .get(..BLOB_MAGIC.len())
        .ok_or_else(|| "stored CAS blob too short for BYOK magic".to_owned())?;
    if magic != BLOB_MAGIC {
        return Err("stored CAS object is not a BYOK convergent blob (bad magic)".to_owned());
    }
    let nonce_end = BLOB_MAGIC.len() + 12;
    let nonce_slice = stored
        .get(BLOB_MAGIC.len()..nonce_end)
        .ok_or_else(|| "stored CAS blob too short for nonce".to_owned())?;
    let ciphertext = stored
        .get(nonce_end..)
        .ok_or_else(|| "stored CAS blob missing ciphertext".to_owned())?
        .to_vec();
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(nonce_slice);
    let blob = ConvergentBlob { ciphertext, nonce };
    decrypt_convergent(&blob, tcs, ctx).map_err(|e| format!("byok decrypt: {e}"))
}

// ─── Mode B (random, max-isolation — plan §2) ───────────────────────────────

/// 4-byte magic prefixing a stored Mode-B (random-DEK) blob (`CoreLink Blob
/// v2`). DISTINCT from the convergent [`BLOB_MAGIC`] (`CLB1`) so the read path
/// can never confuse the two encodings, and a non-magic (legacy plaintext)
/// object is refused fail-closed. Unlike `CLB1`, a Mode-B blob carries NO inline
/// nonce — the per-blob nonce + wrapped DEK live in the `byok_envelope` D1 row.
const MODE_B_MAGIC: &[u8; 4] = b"CLB2";

/// On-disk byte overhead of a stored `CLB2` (Mode-B) blob over its plaintext.
///
/// Layout `MAGIC ‖ ciphertext` where `ciphertext = plaintext ‖ GCM-tag`, i.e.
/// **4** (magic `CLB2`) + **16** (GCM tag) = **20** bytes. UNLIKE the convergent
/// `CLB1` (which carries an inline 12-byte nonce → 32 bytes), Mode B stores the
/// nonce in the `byok_envelope` D1 row, so its on-disk overhead is smaller.
/// Single-sourced here so quota accounting (audit C3) can never drift.
pub const BYOK_CLB2_OVERHEAD: u64 = MODE_B_MAGIC.len() as u64 + 16;

/// The `byok_envelope.blob_hash` primary-key component for a `(surface, digest)`
/// pair. The PK is surface-qualified (`"cas:<digest>"` / `"ac:<digest>"`) so a
/// CAS blob and an AC entry that happen to share a digest string never collide
/// on one envelope row (which would orphan one ciphertext). The KMS
/// `encryption_context` AAD bound at wrap time stays the RAW
/// `{tenant_id, blob_hash:digest}` (see [`CryptoContext::kms_encryption_context`]);
/// only this storage PK is qualified.
#[must_use]
fn envelope_blob_key(surface: &str, digest: &str) -> String {
    format!("{surface}:{digest}")
}

/// Unix-epoch-ms clock for `byok_envelope.created_at_ms`.
fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(i64::MAX)
}

/// Canonical snake_case D1 label for a [`KmsProviderKind`] (matches the
/// `byok_envelope.kms_provider` CHECK constraint, mig 0030).
const fn provider_kind_str(k: KmsProviderKind) -> &'static str {
    match k {
        KmsProviderKind::AwsKms => "aws_kms",
        KmsProviderKind::GcpKms => "gcp_kms",
        KmsProviderKind::AzureKeyVault => "azure_key_vault",
        KmsProviderKind::HashicorpVault => "hashicorp_vault",
    }
}

/// Parse the snake_case `byok_envelope.kms_provider` value (fail-closed on an
/// unknown label).
fn parse_provider_kind(s: &str) -> Result<KmsProviderKind, String> {
    match s {
        "aws_kms" => Ok(KmsProviderKind::AwsKms),
        "gcp_kms" => Ok(KmsProviderKind::GcpKms),
        "azure_key_vault" => Ok(KmsProviderKind::AzureKeyVault),
        "hashicorp_vault" => Ok(KmsProviderKind::HashicorpVault),
        other => Err(format!(
            "byok_envelope.kms_provider: unknown value {other:?}"
        )),
    }
}

/// A decoded `byok_envelope` row — the per-blob Mode-B crypto metadata (the
/// wrapped DEK + the KMS identity + the AAD + the 12-byte AES-GCM nonce). The
/// body ciphertext lives separately in R2.
#[derive(Debug, Clone)]
pub struct ByokEnvelopeRow {
    /// KMS-wrapped DEK ciphertext (provider-opaque).
    pub wrapped_dek: Vec<u8>,
    /// Provider that performed the wrap.
    pub kms_provider: KmsProviderKind,
    /// CMK identifier (ARN / resource name / URI / path).
    pub kms_key_id: String,
    /// CMK region.
    pub kms_region: String,
    /// The wrap-time KMS `encryption_context` AAD (`{tenant_id, blob_hash}`).
    pub encryption_context: Value,
    /// 12-byte AES-256-GCM nonce.
    pub nonce: [u8; 12],
}

impl ByokEnvelopeRow {
    /// Reconstruct the [`WrappedDek`] needed to unwrap / re-encrypt under this
    /// row's persisted DEK.
    #[must_use]
    fn to_wrapped_dek(&self) -> WrappedDek {
        let key_id = KmsKeyId {
            provider: self.kms_provider,
            key_arn_or_id: self.kms_key_id.clone(),
            region: self.kms_region.clone(),
        };
        WrappedDek {
            provider: self.kms_provider,
            key_id,
            ciphertext: self.wrapped_dek.clone(),
            encryption_context: Some(self.encryption_context.clone()),
        }
    }
}
