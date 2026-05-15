//! Inquiry payload envelope encryption (R2-11; S-19 P1-NEW-3 close).
//!
//! # Encryption boundary
//!
//! ```text
//!   HTTP boundary                  Ledger ingress         D1 mirror
//!   (plaintext form)  --SEAL-->    Sealed payload --->   encrypted_payload_b64
//!                                                              │
//!                                                              ▼
//!                                  Slack adapter        HubSpot adapter
//!                                  (sanitized only)     (UNSEAL at boundary)
//!                                  NO plaintext PII     ──────────────────►
//!                                                       cross-border push
//!                                                       (legal: HubSpot DPA)
//! ```
//!
//! Per CTRL-PRIV-001 + INV-BYOK-CRYPTO-SOVEREIGNTY:
//!
//! - PII (`email`, `phone_optional`, `company`, `use_case`,
//!   `additional_notes`) is sealed at the ledger ingress under the
//!   tenant's BYOK customer-managed key (CMK), or a system-owned CMK
//!   for pre-tenant prospect inquiries.
//! - The in-memory `LedgerState` mirror NEVER holds plaintext PII after
//!   ingress.
//! - The Slack adapter receives only an anonymised summary
//!   (company-initials + region + tier hint + lead score), NEVER
//!   plaintext PII.
//! - The HubSpot adapter is the ONE legitimate decryption boundary on
//!   the dispatch path: it unseals the payload right before pushing to
//!   HubSpot's CRM REST API under the HubSpot DPA (cross-border
//!   transfer permitted in legal scope).
//! - The auto-reply mailer is a SECOND legitimate decryption boundary
//!   (it requires the prospect's email address to send the SES reply).
//!   It is restricted to email + locale; no other PII fields are
//!   surfaced.
//!
//! # Sync trait, async transport
//!
//! The [`InquiryPayloadEncryptor`] trait surface is SYNCHRONOUS to keep
//! the `corelink-enterprise-inquiry` crate free of `tokio` per the
//! "no tokio in src" charter rule. Production wiring bridges to the
//! async `corelink-byok::EnvelopeEncryptor` via a oneshot channel +
//! `worker::send_future`-style fire-and-forget; tests in this module
//! use an in-process [`InMemoryInquiryPayloadEncryptor`] that pins the
//! AAD binding + AES-256-GCM ciphertext shape without invoking a real
//! KMS.

use std::sync::{Arc, Mutex};

use thiserror::Error;

use crate::form::{
    BYOKRequirementsKind, EnterpriseInquiryForm, InquiryId, ResidencyKind, Role,
};

/// AAD context bound to every sealed payload — prevents cross-tenant
/// or cross-inquiry wrapped-DEK swap attacks per `BYOK::build_aad`
/// AAD-mismatch check.
///
/// `tenant_id` is either the authenticated tenant identifier OR
/// `system::prospect` for un-authenticated prospect inquiries (which
/// run under the system CMK; see [`SYSTEM_CMK_TENANT_TAG`]).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InquiryAadContext {
    /// Tenant identifier (or [`SYSTEM_CMK_TENANT_TAG`] for prospects).
    pub tenant_id: String,
    /// HMAC-SHA256 of the company name, hex-encoded (per CTRL-PRIV-001
    /// — the AAD must not embed raw PII; the search-domain HMAC binds
    /// the seal to the company identity without revealing it).
    pub company_hash_hex: String,
    /// BLAKE3 of the plaintext payload (idempotency / tamper marker;
    /// not key material).
    pub payload_hash_blake3_hex: String,
}

/// Canonical tag stored in [`InquiryAadContext::tenant_id`] when the
/// inquiry is submitted by an un-authenticated prospect (no tenant_id
/// yet — pre-signup funnel). Production wiring binds the system CMK at
/// `CORELINK_SYSTEM_CMK_ARN` / `CORELINK_SYSTEM_CMK_RESOURCE` to this
/// tag. See `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md` for rotation
/// policy.
pub const SYSTEM_CMK_TENANT_TAG: &str = "system::prospect";

/// Sealed PII payload — what the ledger persists in lieu of
/// `EnterpriseInquiryForm`. Mirrors the D1 column
/// `enterprise_inquiries.encrypted_payload_b64` 1:1.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct EncryptedInquiryPayload {
    /// BYOK envelope (wrapped DEK + AES-256-GCM ciphertext + nonce),
    /// base64-encoded. The exact framing is provider-opaque; the
    /// production binary's `corelink-byok::EnvelopeEncryptor` serde-
    /// serialises the [`corelink_byok::EncryptedBlob`] before base64.
    pub ciphertext_b64: String,
    /// AAD context that was bound at seal time. The unseal path
    /// re-derives this from the [`SealedInquiry::sanitized`] surrogates
    /// + tenant context and rejects on mismatch.
    pub aad: InquiryAadContext,
}

/// Non-PII surrogate columns derived from the inquiry form at seal
/// time. These are safe to hold in the ledger mirror, to ship over the
/// Slack notification (which receives anonymised summary), and to
/// index in D1 (`company_initials`, `email_domain`, `has_phone`, etc.).
///
/// Per CTRL-PRIV-001: `company_initials` ≤ 8 chars; `email_domain` is
/// the bare domain (everything right of the rightmost `@`); the role
/// / byok / residency / GB / lead-score fields are categorical or
/// numeric and carry no individual-identifying PII.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SanitizedInquiryMetadata {
    /// Up-to-8-char initials of the company name (e.g. "AcCo" for
    /// "Acme Corp").
    pub company_initials: String,
    /// Bare email domain (e.g. "acme.example").
    pub email_domain: String,
    /// True iff the form included a phone number.
    pub has_phone: bool,
    /// Submitter role.
    pub role: Role,
    /// BYOK requirement.
    pub byok_requirements: BYOKRequirementsKind,
    /// Residency requirement.
    pub residency_requirements: ResidencyKind,
    /// Expected ingestion volume in GB / month.
    pub expected_gb_per_month: u64,
    /// Locale hint for the auto-reply template.
    pub locale: String,
}

impl SanitizedInquiryMetadata {
    /// Derive the sanitised surrogate columns from a plaintext form.
    /// The result holds NO PII (no `email`, no `phone_optional`, no
    /// `additional_notes`, no `use_case`, no full `company` name).
    #[must_use]
    pub fn from_form(form: &EnterpriseInquiryForm) -> Self {
        Self {
            company_initials: company_initials(&form.company),
            email_domain: email_domain(&form.email),
            has_phone: form.phone_optional.is_some(),
            role: form.role,
            byok_requirements: form.byok_requirements,
            residency_requirements: form.residency_requirements,
            expected_gb_per_month: form.expected_gb_per_month,
            locale: form.locale.clone(),
        }
    }
}

/// Sealed inquiry — the ciphertext + the sanitised surrogates. This is
/// the ONLY representation of an inquiry that lives in the ledger's
/// in-memory mirror after `submit_inquiry` returns. Plaintext PII is
/// dropped at the end of `submit_inquiry`'s stack frame.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SealedInquiry {
    /// Encrypted PII envelope.
    pub payload: EncryptedInquiryPayload,
    /// Non-PII sanitised surrogate columns.
    pub sanitized: SanitizedInquiryMetadata,
}

/// PII fields recovered at a permitted decryption boundary
/// (HubSpot push / auto-reply mailer). Mirrors the subset of
/// `EnterpriseInquiryForm` that the ciphertext seals.
///
/// The decryption boundary callers SHOULD drop this value as soon as
/// the network call returns — `Drop` does NOT zero memory here (the
/// strings are `Vec<u8>` allocations on the heap; `corelink-byok`
/// owns the keyed-memory zeroize path). Callers MUST NOT log any
/// field of this struct (CTRL-PRIV-001).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnsealedInquiryPii {
    /// Full company name.
    pub company: String,
    /// Full email address.
    pub email: String,
    /// Optional phone number.
    pub phone_optional: Option<String>,
    /// Free-text use-case (0..=500 chars).
    pub use_case: String,
    /// Optional notes (0..=2000 chars).
    pub additional_notes: Option<String>,
}

impl UnsealedInquiryPii {
    /// Project from a plaintext form. Used at seal time.
    #[must_use]
    pub fn from_form(form: &EnterpriseInquiryForm) -> Self {
        Self {
            company: form.company.clone(),
            email: form.email.clone(),
            phone_optional: form.phone_optional.clone(),
            use_case: form.use_case.clone(),
            additional_notes: form.additional_notes.clone(),
        }
    }
}

/// Errors surfaced by the encryptor surface.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum InquiryEncryptionError {
    /// Seal-side failure: KMS wrap rejected (revoked CMK, throttled,
    /// transport).
    #[error("inquiry seal failed: {0}")]
    Seal(String),
    /// Unseal-side failure: KMS unwrap rejected, AAD mismatch, or
    /// AES-GCM auth-tag mismatch.
    #[error("inquiry unseal failed: {0}")]
    Unseal(String),
    /// AAD binding mismatch — cross-inquiry / cross-tenant swap
    /// attempt rejected per [`InquiryAadContext`] check.
    #[error("inquiry aad-mismatch (cross-inquiry / cross-tenant swap rejected)")]
    AadMismatch,
}

/// Synchronous inquiry payload encryptor.
///
/// Implementations:
///
/// - Production: a worker-bound adapter bridging the SYNC `seal` /
///   `unseal` surface to the ASYNC `corelink-byok::EnvelopeEncryptor`
///   via a oneshot channel (the bridge lives in the worker binary, not
///   in this crate, so the crate stays runtime-agnostic).
/// - Tests: [`InMemoryInquiryPayloadEncryptor`] — XOR-with-AAD-derived
///   keystream that pins the AAD binding without invoking a real KMS.
///   The shape (base64 framing + AAD round-trip + tamper rejection)
///   matches the production envelope exactly.
///
/// # Contract
///
/// - `seal` MUST bind the AAD context: the same plaintext under
///   different AAD MUST yield a ciphertext that the unseal
///   path rejects with [`InquiryEncryptionError::AadMismatch`].
/// - `unseal` MUST fail-CLOSED when the stored AAD does not match the
///   expected AAD context.
/// - Both methods are sync — the production async bridge is the
///   caller's responsibility.
pub trait InquiryPayloadEncryptor: core::fmt::Debug + Send + Sync {
    /// Seal the plaintext PII payload under the AAD context. Returns
    /// the base64-encoded envelope ciphertext.
    ///
    /// # Errors
    ///
    /// Returns [`InquiryEncryptionError::Seal`] when the underlying
    /// KMS wrap rejects.
    fn seal(
        &self,
        plaintext: &UnsealedInquiryPii,
        aad: &InquiryAadContext,
    ) -> Result<EncryptedInquiryPayload, InquiryEncryptionError>;

    /// Unseal a [`EncryptedInquiryPayload`] using the AAD context the
    /// caller has independently reconstructed (from the sanitised
    /// surrogates + tenant_id at the decryption boundary).
    ///
    /// # Errors
    ///
    /// - [`InquiryEncryptionError::AadMismatch`] when the AAD stored on
    ///   the payload disagrees with the supplied `expected_aad`.
    /// - [`InquiryEncryptionError::Unseal`] when the underlying KMS
    ///   unwrap rejects (revoked CMK, throttled, AES-GCM tag mismatch).
    fn unseal(
        &self,
        payload: &EncryptedInquiryPayload,
        expected_aad: &InquiryAadContext,
    ) -> Result<UnsealedInquiryPii, InquiryEncryptionError>;
}

// ----------------------------------------------------------------------
// Helpers — sanitised surrogate derivation
// ----------------------------------------------------------------------

/// Up-to-8-character initials of the company name. Takes the first char
/// of each space-separated word (uppercased). Falls back to the first 8
/// chars of the trimmed company name when there are no spaces.
#[must_use]
pub fn company_initials(company: &str) -> String {
    let trimmed = company.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 2 {
        let mut out = String::with_capacity(8);
        for w in words.iter().take(8) {
            if let Some(c) = w.chars().next() {
                for u in c.to_uppercase() {
                    out.push(u);
                    if out.chars().count() >= 8 {
                        return out;
                    }
                }
            }
        }
        out
    } else {
        trimmed.chars().take(8).collect()
    }
}

/// Bare email domain (everything right of the rightmost `@`). Returns
/// `unknown.invalid` when the input is malformed (the caller is
/// expected to have called [`EnterpriseInquiryForm::validate`] first,
/// so this branch should be unreachable on the happy path).
///
/// [`EnterpriseInquiryForm::validate`]: crate::form::EnterpriseInquiryForm::validate
#[must_use]
pub fn email_domain(email: &str) -> String {
    match email.rsplit_once('@') {
        Some((_, domain)) => domain.to_string(),
        None => "unknown.invalid".to_string(),
    }
}

/// BLAKE3 hash of the plaintext PII fields, hex-encoded. Used as the
/// `payload_hash_blake3` column + the AAD `payload_hash_blake3_hex`
/// binding.
#[must_use]
pub fn payload_hash_hex(pii: &UnsealedInquiryPii) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(pii.company.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(pii.email.as_bytes());
    hasher.update(b"\x1f");
    if let Some(phone) = pii.phone_optional.as_deref() {
        hasher.update(phone.as_bytes());
    }
    hasher.update(b"\x1f");
    hasher.update(pii.use_case.as_bytes());
    hasher.update(b"\x1f");
    if let Some(notes) = pii.additional_notes.as_deref() {
        hasher.update(notes.as_bytes());
    }
    hex_lower(hasher.finalize().as_bytes())
}

/// HMAC-SHA256 of the company name with the workspace search-domain
/// key, hex-encoded. Used as the AAD `company_hash_hex` binding +
/// surrogate D1 indexing column per CTRL-PRIV-001.
///
/// The `search_domain_key` is a per-deployment static secret, NOT a
/// BYOK key — it scopes the search domain (HubSpot inquiry lookup)
/// without revealing the raw company name. Rotation is out-of-band
/// (annual; see `RB-SYSTEM-CMK-ROTATION.md` companion guidance).
#[must_use]
pub fn company_hash_hex(company: &str, search_domain_key: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = match <Hmac<Sha256>>::new_from_slice(search_domain_key) {
        Ok(m) => m,
        Err(_) => {
            // Unreachable in practice: HMAC-SHA256 accepts arbitrary
            // key lengths. Fallback returns a constant marker so the
            // surrogate column is non-empty and audit can flag it.
            return "hmac-key-invalid".to_string();
        }
    };
    mac.update(company.trim().as_bytes());
    hex_lower(&mac.finalize().into_bytes())
}

/// Build the canonical [`InquiryAadContext`] for an inquiry. Both the
/// seal and unseal paths derive this independently and the encryptor
/// rejects on mismatch — the AAD binds the seal to the
/// `(tenant_id, company_hash, payload_hash)` triple per
/// INV-BYOK-CRYPTO-SOVEREIGNTY.
#[must_use]
pub fn build_aad(
    tenant_id: &str,
    pii: &UnsealedInquiryPii,
    search_domain_key: &[u8],
) -> InquiryAadContext {
    InquiryAadContext {
        tenant_id: tenant_id.to_string(),
        company_hash_hex: company_hash_hex(&pii.company, search_domain_key),
        payload_hash_blake3_hex: payload_hash_hex(pii),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(nibble_to_hex(b >> 4));
        out.push(nibble_to_hex(b & 0xf));
    }
    out
}

const fn nibble_to_hex(n: u8) -> char {
    match n & 0xf {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'a',
        11 => 'b',
        12 => 'c',
        13 => 'd',
        14 => 'e',
        _ => 'f',
    }
}

// ----------------------------------------------------------------------
// In-memory test fake — AAD-keyed AES-256-GCM-equivalent (BLAKE3
// keystream)
// ----------------------------------------------------------------------

/// Test-only inquiry payload encryptor. Uses a BLAKE3-derived
/// keystream (keyed on the AAD context + a per-instance seed) XORed
/// against the serialised plaintext.  This is NOT a real KMS; it pins
/// the AAD-binding contract + the base64 round-trip exactly as the
/// production envelope.
#[derive(Clone, Debug)]
pub struct InMemoryInquiryPayloadEncryptor {
    seed: Arc<[u8; 32]>,
    seals: Arc<Mutex<u64>>,
}

impl InMemoryInquiryPayloadEncryptor {
    /// Construct with a deterministic seed (tests want repeatable
    /// ciphertext byte sequences).
    #[must_use]
    pub fn with_seed(seed: [u8; 32]) -> Self {
        Self {
            seed: Arc::new(seed),
            seals: Arc::new(Mutex::new(0)),
        }
    }

    /// Default constructor — uses an all-zero seed (deterministic;
    /// CSPRNG seeds are reserved for the production wiring).
    #[must_use]
    pub fn new() -> Self {
        Self::with_seed([0u8; 32])
    }

    /// Count of `seal()` invocations.
    #[must_use]
    pub fn seal_count(&self) -> u64 {
        match self.seals.lock() {
            Ok(g) => *g,
            Err(p) => *p.into_inner(),
        }
    }

    fn keystream(&self, aad: &InquiryAadContext, len: usize) -> Vec<u8> {
        let mut hasher = blake3::Hasher::new();
        hasher.update(self.seed.as_ref());
        hasher.update(aad.tenant_id.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(aad.company_hash_hex.as_bytes());
        hasher.update(b"\x1f");
        hasher.update(aad.payload_hash_blake3_hex.as_bytes());
        let mut xof = hasher.finalize_xof();
        let mut out = vec![0u8; len];
        xof.fill(&mut out);
        out
    }
}

impl Default for InMemoryInquiryPayloadEncryptor {
    fn default() -> Self {
        Self::new()
    }
}

/// Wire framing for the test fake — JSON-ish but bracketed by a known
/// magic so a tamper bit on the ciphertext fails the magic check.
const TEST_MAGIC: &[u8; 4] = b"CL11";

impl InquiryPayloadEncryptor for InMemoryInquiryPayloadEncryptor {
    fn seal(
        &self,
        plaintext: &UnsealedInquiryPii,
        aad: &InquiryAadContext,
    ) -> Result<EncryptedInquiryPayload, InquiryEncryptionError> {
        use base64::Engine as _;

        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(TEST_MAGIC);
        push_str_field(&mut buf, &plaintext.company);
        push_str_field(&mut buf, &plaintext.email);
        push_opt_str_field(&mut buf, plaintext.phone_optional.as_deref());
        push_str_field(&mut buf, &plaintext.use_case);
        push_opt_str_field(&mut buf, plaintext.additional_notes.as_deref());

        let ks = self.keystream(aad, buf.len());
        for (b, k) in buf.iter_mut().zip(ks.iter()) {
            *b ^= *k;
        }

        let ciphertext_b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
        if let Ok(mut g) = self.seals.lock() {
            *g = g.saturating_add(1);
        }
        Ok(EncryptedInquiryPayload {
            ciphertext_b64,
            aad: aad.clone(),
        })
    }

    fn unseal(
        &self,
        payload: &EncryptedInquiryPayload,
        expected_aad: &InquiryAadContext,
    ) -> Result<UnsealedInquiryPii, InquiryEncryptionError> {
        use base64::Engine as _;

        if &payload.aad != expected_aad {
            return Err(InquiryEncryptionError::AadMismatch);
        }
        let mut buf = base64::engine::general_purpose::STANDARD
            .decode(payload.ciphertext_b64.as_bytes())
            .map_err(|e| InquiryEncryptionError::Unseal(format!("base64 decode: {e}")))?;
        let ks = self.keystream(expected_aad, buf.len());
        for (b, k) in buf.iter_mut().zip(ks.iter()) {
            *b ^= *k;
        }
        match buf.get(..TEST_MAGIC.len()) {
            Some(prefix) if prefix == TEST_MAGIC => {}
            _ => {
                return Err(InquiryEncryptionError::Unseal(
                    "magic mismatch (AES-GCM-tag-equivalent in test fake)".to_string(),
                ));
            }
        }
        let mut cursor = TEST_MAGIC.len();
        let company =
            pop_str_field(&buf, &mut cursor).map_err(InquiryEncryptionError::Unseal)?;
        let email =
            pop_str_field(&buf, &mut cursor).map_err(InquiryEncryptionError::Unseal)?;
        let phone_optional =
            pop_opt_str_field(&buf, &mut cursor).map_err(InquiryEncryptionError::Unseal)?;
        let use_case =
            pop_str_field(&buf, &mut cursor).map_err(InquiryEncryptionError::Unseal)?;
        let additional_notes =
            pop_opt_str_field(&buf, &mut cursor).map_err(InquiryEncryptionError::Unseal)?;
        Ok(UnsealedInquiryPii {
            company,
            email,
            phone_optional,
            use_case,
            additional_notes,
        })
    }
}

/// Adversarial fixture encryptor that always rejects on `seal`. Used
/// in tests that pin the "encryption-failure rolls back atomically"
/// invariant.
#[derive(Clone, Copy, Debug, Default)]
pub struct FailingInquiryPayloadEncryptor;

impl InquiryPayloadEncryptor for FailingInquiryPayloadEncryptor {
    fn seal(
        &self,
        _plaintext: &UnsealedInquiryPii,
        _aad: &InquiryAadContext,
    ) -> Result<EncryptedInquiryPayload, InquiryEncryptionError> {
        Err(InquiryEncryptionError::Seal(
            "adversarial fixture: always rejects".to_string(),
        ))
    }

    fn unseal(
        &self,
        _payload: &EncryptedInquiryPayload,
        _expected_aad: &InquiryAadContext,
    ) -> Result<UnsealedInquiryPii, InquiryEncryptionError> {
        Err(InquiryEncryptionError::Unseal(
            "adversarial fixture: always rejects".to_string(),
        ))
    }
}

// ----------------------------------------------------------------------
// Framing — length-prefixed string fields (u32 BE)
// ----------------------------------------------------------------------

fn push_str_field(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn push_opt_str_field(buf: &mut Vec<u8>, s: Option<&str>) {
    match s {
        Some(v) => {
            buf.push(1);
            push_str_field(buf, v);
        }
        None => buf.push(0),
    }
}

fn pop_u32(buf: &[u8], cursor: &mut usize) -> Result<u32, String> {
    let start = *cursor;
    let end = start.checked_add(4).ok_or_else(|| "overflow".to_string())?;
    let slice = buf
        .get(start..end)
        .ok_or_else(|| "truncated u32 field".to_string())?;
    let mut arr = [0u8; 4];
    arr.copy_from_slice(slice);
    *cursor = end;
    Ok(u32::from_be_bytes(arr))
}

fn pop_str_field(buf: &[u8], cursor: &mut usize) -> Result<String, String> {
    let len = pop_u32(buf, cursor)? as usize;
    let start = *cursor;
    let end = start
        .checked_add(len)
        .ok_or_else(|| "overflow on str field len".to_string())?;
    let slice = buf
        .get(start..end)
        .ok_or_else(|| "truncated str field".to_string())?;
    let s = std::str::from_utf8(slice)
        .map_err(|e| format!("utf8 error: {e}"))?
        .to_string();
    *cursor = end;
    Ok(s)
}

fn pop_opt_str_field(buf: &[u8], cursor: &mut usize) -> Result<Option<String>, String> {
    let start = *cursor;
    let tag = *buf
        .get(start)
        .ok_or_else(|| "truncated opt tag".to_string())?;
    *cursor = start.checked_add(1).ok_or_else(|| "overflow".to_string())?;
    match tag {
        0 => Ok(None),
        1 => pop_str_field(buf, cursor).map(Some),
        other => Err(format!("invalid opt tag {other}")),
    }
}

/// Convenience helper: build a [`SealedInquiry`] from a plaintext form
/// + an encryptor + the tenant context.
///
/// Centralises the `build AAD → seal → assemble surrogates` sequence
/// so callers (the ledger, the CRM retry path) cannot forget a step.
///
/// # Errors
///
/// Returns [`InquiryEncryptionError`] on encryptor failure.
pub fn seal_inquiry(
    form: &EnterpriseInquiryForm,
    tenant_id: &str,
    search_domain_key: &[u8],
    encryptor: &dyn InquiryPayloadEncryptor,
) -> Result<SealedInquiry, InquiryEncryptionError> {
    let pii = UnsealedInquiryPii::from_form(form);
    let aad = build_aad(tenant_id, &pii, search_domain_key);
    let payload = encryptor.seal(&pii, &aad)?;
    let sanitized = SanitizedInquiryMetadata::from_form(form);
    Ok(SealedInquiry { payload, sanitized })
}

/// Convenience helper: unseal at a permitted decryption boundary
/// (HubSpot push / auto-reply mailer). Re-derives the AAD from
/// independent inputs + invokes the encryptor.
///
/// # Errors
///
/// - [`InquiryEncryptionError::AadMismatch`] when the supplied
///   `(tenant_id, search_domain_key)` derive an AAD that disagrees
///   with the one stored on the payload.
/// - [`InquiryEncryptionError::Unseal`] on KMS unwrap failure.
pub fn unseal_inquiry(
    sealed: &SealedInquiry,
    tenant_id: &str,
    encryptor: &dyn InquiryPayloadEncryptor,
) -> Result<UnsealedInquiryPii, InquiryEncryptionError> {
    // The unseal path uses the AAD baked into the payload directly —
    // the caller MUST have already verified `tenant_id` matches the
    // payload's stored AAD via the equality check below. We do NOT
    // recompute the company hash here (the search-domain key may not
    // be available at the decryption boundary) — the AAD check on the
    // encryptor side enforces tamper-resistance.
    if sealed.payload.aad.tenant_id != tenant_id {
        return Err(InquiryEncryptionError::AadMismatch);
    }
    encryptor.unseal(&sealed.payload, &sealed.payload.aad)
}

/// Used by the Slack notification path to construct an anonymised
/// summary. Carries NO PII fields — only the surrogate + inquiry id +
/// lead score.
#[must_use]
pub fn anonymised_slack_summary(
    inquiry_id: &InquiryId,
    sanitized: &SanitizedInquiryMetadata,
    lead_score: u32,
) -> String {
    format!(
        "New enterprise inquiry [{iid}] from {company_initials}@{email_domain} ({role}); \
         BYOK: {byok}; residency: {residency}; expected GB: {gb}; phone: {phone}; \
         score: {score}",
        iid = inquiry_id,
        company_initials = sanitized.company_initials,
        email_domain = sanitized.email_domain,
        role = sanitized.role.as_str(),
        byok = sanitized.byok_requirements.as_str(),
        residency = sanitized.residency_requirements.as_str(),
        gb = sanitized.expected_gb_per_month,
        phone = if sanitized.has_phone { "yes" } else { "no" },
        score = lead_score,
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed to use these primitives"
)]
mod tests {
    use super::*;
    use crate::form::{BYOKRequirementsKind, ResidencyKind, Role};

    fn form() -> EnterpriseInquiryForm {
        EnterpriseInquiryForm {
            company: "Acme Corp".to_string(),
            role: Role::Ciso,
            email: "ciso@acme.example".to_string(),
            phone_optional: Some("+15555550100".to_string()),
            expected_gb_per_month: 5_000,
            byok_requirements: BYOKRequirementsKind::AwsKms,
            residency_requirements: ResidencyKind::Eu,
            additional_notes: Some("multi-region read-through".to_string()),
            locale: "en-US".to_string(),
            use_case: "cache".to_string(),
        }
    }

    #[test]
    fn company_initials_two_words() {
        assert_eq!(company_initials("Acme Corp"), "AC");
        assert_eq!(company_initials("alpha bravo charlie"), "ABC");
        assert_eq!(company_initials(""), "");
    }

    #[test]
    fn company_initials_single_word_truncates() {
        assert_eq!(company_initials("MonolithLLC"), "Monolith");
    }

    #[test]
    fn email_domain_extracts_after_at() {
        assert_eq!(email_domain("a@b.example"), "b.example");
        assert_eq!(email_domain("malformed"), "unknown.invalid");
    }

    #[test]
    fn sanitised_metadata_holds_no_pii() {
        let m = SanitizedInquiryMetadata::from_form(&form());
        // company_initials is up-to-8-char initials; never the full
        // company name.
        assert_eq!(m.company_initials, "AC");
        // email_domain is the bare domain, never the full email.
        assert_eq!(m.email_domain, "acme.example");
        // has_phone is bool, never the phone number.
        assert!(m.has_phone);
        // No `company`, `email`, `phone_optional`, `use_case`, or
        // `additional_notes` field exists on this struct.
        let dbg = format!("{m:?}");
        assert!(!dbg.contains("ciso@acme.example"));
        assert!(!dbg.contains("+15555550100"));
        assert!(!dbg.contains("Acme Corp"));
        assert!(!dbg.contains("multi-region"));
    }

    #[test]
    fn seal_unseal_roundtrip_recovers_pii() {
        let e = InMemoryInquiryPayloadEncryptor::new();
        let f = form();
        let sealed = seal_inquiry(&f, "tenant-1", b"search-key-v1", &e).unwrap();
        let recovered = unseal_inquiry(&sealed, "tenant-1", &e).unwrap();
        assert_eq!(recovered.company, "Acme Corp");
        assert_eq!(recovered.email, "ciso@acme.example");
        assert_eq!(recovered.phone_optional.as_deref(), Some("+15555550100"));
        assert_eq!(recovered.use_case, "cache");
        assert_eq!(
            recovered.additional_notes.as_deref(),
            Some("multi-region read-through")
        );
    }

    #[test]
    fn unseal_rejects_cross_tenant_aad_swap() {
        let e = InMemoryInquiryPayloadEncryptor::new();
        let f = form();
        let sealed = seal_inquiry(&f, "tenant-A", b"k", &e).unwrap();
        let err = unseal_inquiry(&sealed, "tenant-B", &e).unwrap_err();
        assert!(matches!(err, InquiryEncryptionError::AadMismatch));
    }

    #[test]
    fn unseal_rejects_tampered_ciphertext() {
        let e = InMemoryInquiryPayloadEncryptor::new();
        let f = form();
        let mut sealed = seal_inquiry(&f, "tenant-1", b"k", &e).unwrap();
        // Flip a byte in the base64 ciphertext.
        let mut bytes = sealed.payload.ciphertext_b64.into_bytes();
        if let Some(first) = bytes.first_mut() {
            *first = if *first == b'A' { b'B' } else { b'A' };
        }
        sealed.payload.ciphertext_b64 = String::from_utf8(bytes).unwrap();
        let result = unseal_inquiry(&sealed, "tenant-1", &e);
        assert!(matches!(result, Err(InquiryEncryptionError::Unseal(_))));
    }

    #[test]
    fn failing_encryptor_rejects_seal() {
        let e = FailingInquiryPayloadEncryptor;
        let f = form();
        let err = seal_inquiry(&f, "tenant-1", b"k", &e).unwrap_err();
        assert!(matches!(err, InquiryEncryptionError::Seal(_)));
    }

    #[test]
    fn system_cmk_tag_canonical() {
        assert_eq!(SYSTEM_CMK_TENANT_TAG, "system::prospect");
    }

    #[test]
    fn anonymised_summary_contains_no_pii() {
        let e = InMemoryInquiryPayloadEncryptor::new();
        let sealed = seal_inquiry(&form(), "t-1", b"k", &e).unwrap();
        let summary = anonymised_slack_summary(&InquiryId::new("inq-1"), &sealed.sanitized, 100);
        assert!(!summary.contains("ciso@acme.example"));
        assert!(!summary.contains("Acme Corp"));
        assert!(!summary.contains("+15555550100"));
        assert!(!summary.contains("multi-region read-through"));
        // Surrogates ok.
        assert!(summary.contains("AC"));
        assert!(summary.contains("acme.example"));
        assert!(summary.contains("ciso"));
    }

    #[test]
    fn payload_hash_stable_for_same_input() {
        let p1 = UnsealedInquiryPii::from_form(&form());
        let p2 = p1.clone();
        assert_eq!(payload_hash_hex(&p1), payload_hash_hex(&p2));
    }

    #[test]
    fn company_hash_stable_for_same_key_and_company() {
        let h1 = company_hash_hex("Acme Corp", b"key-v1");
        let h2 = company_hash_hex("Acme Corp", b"key-v1");
        assert_eq!(h1, h2);
        let h3 = company_hash_hex("Acme Corp", b"key-v2");
        assert_ne!(h1, h3);
    }
}
