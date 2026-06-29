//! BYOK Wave 3a — convergent encryption-at-rest wiring for the **native CAS
//! data plane**.
//!
//! This module is the glue between the Wave-1 crypto primitives
//! ([`corelink_byok`]: [`encrypt_convergent`] / [`decrypt_convergent`] /
//! [`CryptoContext`] / [`Tcs`] / [`KmsProvider`]) and the Wave-2 control-plane
//! read model ([`crate::customer_d1`]: [`TenantByokConfig`] / [`ByokState`]).
//! It is consumed by [`crate::storage::r2_s3::R2CasHandler`] on the CAS
//! write/read path.
//!
//! # GATED-INERT (the safety envelope)
//!
//! Per the audited plan `docs/design/2026-06-28-byok-encryption-at-rest-plan.md`
//! (§7.2, §11 C1/C2), encryption engages **only** for a tenant whose
//! `tenant_byok_config.state == 'active'`. Onboarding (the sole writer of those
//! rows — audit finding H5) is a later wave, so today **zero** tenants are
//! active and this code path is inert: every existing tenant keeps the exact
//! current plaintext behaviour. The handler also threads the BYOK
//! collaborators as `Option`s — when any is `None` the plaintext path runs
//! unchanged.
//!
//! # FAIL-CLOSED (plan §6 — mandatory)
//!
//! For an **active** tenant, an unavailable config/KMS/Tcs MUST surface as an
//! `Err` (5xx) on both write and read — it must NEVER fall back to storing or
//! serving plaintext. Every error arm in this module and its callers preserves
//! that invariant.
//!
//! # Scope (Wave 3a) and deferrals
//!
//! - **In scope:** the native CAS single-shot write+read path, Mode A
//!   (convergent) only, state `active`.
//! - **Wave 3b (now wired here, GATED-INERT):** the AC (`R2AcHandler`) path —
//!   the action cache encrypts its `result_payload` at rest under
//!   [`ac_crypto_context`] (surface `"ac"`, domain-separated from CAS; audit
//!   H1), and the ciphertext-size accounting reconciliation (audit C3) is
//!   single-sourced via [`BYOK_CLB1_OVERHEAD`].
//! - **Deferred to Wave 3c/4 (documented, never silently skipped):**
//!   - §4 HMAC'd-digest key hardening (confirmation-oracle) — the R2 key keeps
//!     the raw plaintext digest for 3a/3b; see [`cas_crypto_context`] /
//!     [`ac_crypto_context`] (Wave 3c).
//!   - Mode B (`crypto_mode = 'random'`) — fail-closed here, NOT plaintext
//!     (Wave 3c).
//!   - The `partial`/backfill dual-read state (audit H7) — fail-closed here
//!     (Wave 4).
//!   - The production `KmsProvider` wiring (Wave 4 onboarding).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use corelink_byok::{
    decrypt_convergent, encrypt_convergent, BYOKError, ConvergentBlob, CryptoAlgo, CryptoContext,
    CryptoMode, KmsKeyId, KmsProvider, Tcs, WrappedDek,
};
use corelink_handler_cas::DigestAlgo;
use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::customer_d1::{
    ByokConfigError, ByokConfigRows, ByokCryptoMode, ByokState, D1ByokConfigReader, TenantByokConfig,
};
use crate::storage::d1_http::D1HttpClient;

/// The CAS surface tag bound into the convergent [`CryptoContext`] — domain
/// separation from other cache surfaces (e.g. the AC surface, [`AC_SURFACE`]).
const CAS_SURFACE: &str = "cas";

/// The AC (action-cache) surface tag bound into the convergent
/// [`CryptoContext`] — domain separation from the CAS surface (Wave 3b, closes
/// audit H1 silent-plaintext). Because `surface` is bound into the JCS bytes
/// that feed BOTH the HKDF `info` and the AEAD AAD (see
/// [`corelink_byok::CryptoContext`]), an AC blob produced under this tag CANNOT
/// be decrypted under a `"cas"` context (and vice-versa) even for the same
/// `(tenant, digest)` — so an AC ciphertext can never be swapped for a CAS one.
const AC_SURFACE: &str = "ac";

/// 4-byte magic prefixing a stored convergent CAS blob (`CoreLink Blob v1`).
///
/// Constant ⇒ does not perturb convergent determinism (identical content still
/// serialises to byte-identical stored bytes ⇒ dedup preserved). Its purpose is
/// fail-closed robustness: on read for an active tenant, a stored object that
/// lacks this magic is NOT a ciphertext blob (e.g. a legacy plaintext object
/// from before activation — a `partial`/backfill concern deferred to Wave 4),
/// so [`decrypt_cas_blob`] refuses it rather than risk mis-decoding.
const BLOB_MAGIC: &[u8; 4] = b"CLB1";

/// On-disk byte overhead of the stored `CLB1` convergent blob over its
/// plaintext — **single-sourced here with the [`encrypt_cas_blob`] format** so
/// quota accounting (audit C3) can never drift from the wire layout.
///
/// Breakdown: `MAGIC ‖ nonce ‖ ciphertext` where `ciphertext = plaintext ‖
/// GCM-tag`, i.e. **4** (magic `CLB1`) + **12** (AES-256-GCM nonce) + **16**
/// (GCM authentication tag) = **32** bytes. The plaintext length is unchanged
/// (AES-GCM is length-preserving), so `stored_len == plaintext_len +
/// BYOK_CLB1_OVERHEAD`. The CAS and AC surfaces share the identical format, so
/// the AC plane (Wave 3b) reuses this same const.
pub const BYOK_CLB1_OVERHEAD: u64 = BLOB_MAGIC.len() as u64 + 12 + 16;

/// Config-cache default TTL — 60 s. After warm-up the non-BYOK hot path adds no
/// D1 hop (a HIT is in-memory; a MISS does ONE D1 read and caches the result,
/// including the "not configured / inactive" answer).
pub const BYOK_CONFIG_TTL_SECONDS: u64 = 60;

/// Tcs-cache default TTL — 300 s (the [`corelink_byok::DekCache`] hard ceiling;
/// the unwrapped Tcs is the convergence secret and lives only inside this
/// window). INV-BYOK-CRYPTO-SOVEREIGNTY.
pub const BYOK_TCS_TTL_SECONDS: u64 = 300;

/// Memory bound for the in-process per-tenant caches (mirrors `DekCache`).
const MAX_CACHE_ENTRIES: usize = 10_000;

/// Lock a std `Mutex` without ever panicking on poison (charter: no `unwrap`).
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ─── Config source seam + cache ────────────────────────────────────────────

/// Async source of the per-tenant BYOK config read model. The production impl
/// is [`D1ByokConfigReader`] over D1; tests supply a hermetic mock.
#[async_trait]
pub trait ByokConfigSource: Send + Sync + core::fmt::Debug {
    /// Load the tenant's BYOK config (`Ok(None)` ⇒ not configured).
    ///
    /// # Errors
    ///
    /// Fail-CLOSED: a transport / parse failure is a [`ByokConfigError`] —
    /// NEVER coerced into "encryption off".
    async fn get_byok_config(
        &self,
        tenant: &str,
    ) -> Result<Option<TenantByokConfig>, ByokConfigError>;
}

#[async_trait]
impl<R: ByokConfigRows + core::fmt::Debug + 'static> ByokConfigSource for D1ByokConfigReader<R> {
    async fn get_byok_config(
        &self,
        tenant: &str,
    ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
        D1ByokConfigReader::get_byok_config(self, tenant).await
    }
}

/// One config-cache entry (the answer + its expiry). The answer is cached even
/// when it is `None` (not configured) / inactive, so non-BYOK tenants do not
/// re-hit D1.
#[derive(Debug, Clone)]
struct ConfigEntry {
    cfg: Option<TenantByokConfig>,
    expires_at: Instant,
}

/// In-memory, per-tenant cache over [`ByokConfigSource`] with a ~60 s TTL.
///
/// A cache MISS does ONE D1 read; a HIT is in-memory. The cached answer
/// includes the "not configured / inactive" result, so after warm-up the
/// non-BYOK hot path adds no D1 hop (frozen policy §1).
pub struct ByokConfigCache {
    source: Arc<dyn ByokConfigSource>,
    inner: Mutex<HashMap<String, ConfigEntry>>,
    ttl: Duration,
}

impl core::fmt::Debug for ByokConfigCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ByokConfigCache")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl ByokConfigCache {
    /// Build over an async config source with an explicit TTL (seconds).
    #[must_use]
    pub fn new(source: Arc<dyn ByokConfigSource>, ttl_seconds: u64) -> Self {
        Self {
            source,
            inner: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Build with the canonical [`BYOK_CONFIG_TTL_SECONDS`] TTL.
    #[must_use]
    pub fn with_default_ttl(source: Arc<dyn ByokConfigSource>) -> Self {
        Self::new(source, BYOK_CONFIG_TTL_SECONDS)
    }

    /// Return the tenant's cached config, doing at most ONE D1 read on a miss.
    ///
    /// # Errors
    ///
    /// Fail-CLOSED: a source error is propagated (the caller refuses to
    /// downgrade an active tenant to plaintext on an undetermined config).
    pub async fn get(&self, tenant: &str) -> Result<Option<TenantByokConfig>, ByokConfigError> {
        let now = Instant::now();
        {
            let guard = lock(&self.inner);
            if let Some(entry) = guard.get(tenant) {
                if entry.expires_at > now {
                    return Ok(entry.cfg.clone());
                }
            }
        }
        // Miss / expired: ONE D1 read (no lock held across the await).
        let cfg = self.source.get_byok_config(tenant).await?;
        {
            let mut guard = lock(&self.inner);
            // Opportunistically drop expired entries before inserting so the
            // map stays bounded without a full LRU.
            if guard.len() >= MAX_CACHE_ENTRIES {
                let cutoff = Instant::now();
                guard.retain(|_, e| e.expires_at > cutoff);
            }
            guard.insert(
                tenant.to_owned(),
                ConfigEntry {
                    cfg: cfg.clone(),
                    expires_at: now + self.ttl,
                },
            );
        }
        Ok(cfg)
    }
}

// ─── Tcs secret source seam + resolver (Mode A, TCS resolution) ─────────────

/// The decoded `tenant_byok_secret` row needed to unwrap the Tcs.
#[derive(Debug, Clone)]
pub struct WrappedTcsRow {
    /// CMK-wrapped Tenant Convergence Secret ciphertext (provider-opaque).
    pub tcs_wrapped: Vec<u8>,
    /// CMK identity used to wrap (echoes `tenant_byok_config.cmk_key_id`).
    pub cmk_key_id: Option<String>,
    /// Tcs version (bumped on rotation; bound into the unwrap AAD).
    pub tcs_version: i64,
}

/// Async source of the CMK-wrapped Tcs (`tenant_byok_secret`). Production impl
/// is [`D1ByokSecretReader`]; tests supply a mock.
#[async_trait]
pub trait ByokSecretSource: Send + Sync + core::fmt::Debug {
    /// Load the tenant's wrapped-Tcs row (`Ok(None)` ⇒ not wrapped yet).
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` on any D1 transport / decode failure.
    async fn get_wrapped_tcs(&self, tenant: &str) -> Result<Option<WrappedTcsRow>, String>;
}

/// Production [`ByokSecretSource`] over the async D1 row seam (reuses
/// [`ByokConfigRows`], which [`D1HttpClient`] already implements).
#[derive(Debug)]
pub struct D1ByokSecretReader<R = D1HttpClient> {
    rows: Arc<R>,
}

impl<R: ByokConfigRows> D1ByokSecretReader<R> {
    /// Wire the reader over an async row source.
    #[must_use]
    pub fn new(rows: Arc<R>) -> Self {
        Self { rows }
    }
}

#[async_trait]
impl<R: ByokConfigRows + core::fmt::Debug + 'static> ByokSecretSource for D1ByokSecretReader<R> {
    async fn get_wrapped_tcs(&self, tenant: &str) -> Result<Option<WrappedTcsRow>, String> {
        let rows = self
            .rows
            .query_rows(
                "SELECT tcs_wrapped, cmk_key_id, tcs_version \
                 FROM tenant_byok_secret WHERE tenant_id = ?1 LIMIT 1",
                vec![json!(tenant)],
            )
            .await?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let Some(wrapped_val) = row.get("tcs_wrapped") else {
            return Ok(None);
        };
        if wrapped_val.is_null() {
            // Row exists but the Tcs is not wrapped yet (onboarding incomplete).
            return Ok(None);
        }
        let tcs_wrapped = decode_blob(wrapped_val)?;
        let cmk_key_id = row.get("cmk_key_id").and_then(Value::as_str).map(str::to_owned);
        let tcs_version = row.get("tcs_version").and_then(Value::as_i64).unwrap_or(1);
        Ok(Some(WrappedTcsRow {
            tcs_wrapped,
            cmk_key_id,
            tcs_version,
        }))
    }
}

/// Decode a D1-over-HTTP BLOB JSON value into raw bytes. D1 surfaces a BLOB as
/// either an array of byte integers or a base64 string; both are accepted.
fn decode_blob(v: &Value) -> Result<Vec<u8>, String> {
    match v {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                let n = it
                    .as_u64()
                    .ok_or_else(|| "tcs_wrapped: non-numeric byte in BLOB array".to_owned())?;
                out.push(u8::try_from(n).map_err(|_| "tcs_wrapped: byte out of range".to_owned())?);
            }
            Ok(out)
        }
        Value::String(s) => base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(|e| format!("tcs_wrapped base64 decode: {e}")),
        _ => Err("tcs_wrapped: unsupported BLOB JSON encoding".to_owned()),
    }
}

/// One Tcs-cache entry — the unwrapped 32-byte secret + expiry. The bytes are
/// held in [`Zeroizing`] so eviction / drop clears them.
struct TcsEntry {
    bytes: Zeroizing<[u8; 32]>,
    expires_at: Instant,
}

/// In-memory per-tenant cache of the UNWRAPPED Tcs, ≤300 s TTL (mirrors the
/// [`corelink_byok::DekCache`] discipline; this is the only place the plaintext
/// Tcs lives, transiently).
struct TcsCache {
    inner: Mutex<HashMap<String, TcsEntry>>,
    ttl: Duration,
}

impl TcsCache {
    /// Build a cache with `ttl_seconds` ≤ 300 (INV-BYOK-CRYPTO-SOVEREIGNTY).
    fn new(ttl_seconds: u64) -> Result<Self, BYOKError> {
        if ttl_seconds > BYOK_TCS_TTL_SECONDS {
            return Err(BYOKError::DekCacheTtlViolation {
                attempted_seconds: ttl_seconds,
            });
        }
        Ok(Self {
            inner: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
        })
    }

    fn get(&self, tenant: &str) -> Option<[u8; 32]> {
        let now = Instant::now();
        let mut guard = lock(&self.inner);
        match guard.get(tenant) {
            Some(entry) if entry.expires_at > now => Some(*entry.bytes),
            Some(_) => {
                guard.remove(tenant);
                None
            }
            None => None,
        }
    }

    fn put(&self, tenant: &str, bytes: [u8; 32]) {
        let mut guard = lock(&self.inner);
        if guard.len() >= MAX_CACHE_ENTRIES {
            let cutoff = Instant::now();
            guard.retain(|_, e| e.expires_at > cutoff);
        }
        guard.insert(
            tenant.to_owned(),
            TcsEntry {
                bytes: Zeroizing::new(bytes),
                expires_at: Instant::now() + self.ttl,
            },
        );
    }
}

/// Resolves the per-tenant Tcs (Mode A convergence secret): read
/// `tenant_byok_secret.tcs_wrapped` → [`KmsProvider::unwrap_dek`] → [`Tcs`],
/// cached for ≤300 s.
///
/// This is the only place the plaintext Tcs is materialised, and only inside
/// the bounded cache window.
pub struct TcsResolver {
    secrets: Arc<dyn ByokSecretSource>,
    kms: Arc<dyn KmsProvider>,
    cache: TcsCache,
}

impl core::fmt::Debug for TcsResolver {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TcsResolver").finish_non_exhaustive()
    }
}

impl TcsResolver {
    /// Build a resolver over a secret source + a KMS provider with an explicit
    /// Tcs-cache TTL (seconds).
    ///
    /// # Errors
    ///
    /// Returns [`BYOKError::DekCacheTtlViolation`] if `ttl_seconds > 300`.
    pub fn new(
        secrets: Arc<dyn ByokSecretSource>,
        kms: Arc<dyn KmsProvider>,
        ttl_seconds: u64,
    ) -> Result<Self, BYOKError> {
        Ok(Self {
            secrets,
            kms,
            cache: TcsCache::new(ttl_seconds)?,
        })
    }

    /// Build with the canonical [`BYOK_TCS_TTL_SECONDS`] TTL.
    ///
    /// # Errors
    ///
    /// Infallible in practice (300 s is within bounds); the `Result` mirrors
    /// [`Self::new`].
    pub fn with_default_ttl(
        secrets: Arc<dyn ByokSecretSource>,
        kms: Arc<dyn KmsProvider>,
    ) -> Result<Self, BYOKError> {
        Self::new(secrets, kms, BYOK_TCS_TTL_SECONDS)
    }

    /// Resolve the tenant's plaintext Tcs (cached). On a miss: read the wrapped
    /// Tcs, unwrap it via the customer CMK, cache, and return.
    ///
    /// # Errors
    ///
    /// Fail-CLOSED: a missing secret row, a D1 error, or a KMS unwrap failure
    /// is a [`BYOKError`] — the caller turns this into a 5xx and never stores /
    /// serves plaintext for an active tenant.
    pub async fn resolve(&self, cfg: &TenantByokConfig) -> Result<Tcs, BYOKError> {
        if let Some(bytes) = self.cache.get(&cfg.tenant_id) {
            return Ok(Tcs::from_bytes(bytes));
        }
        let row = self
            .secrets
            .get_wrapped_tcs(&cfg.tenant_id)
            .await
            .map_err(BYOKError::Provider)?
            .ok_or_else(|| {
                BYOKError::Provider(format!(
                    "tenant_byok_secret.tcs_wrapped missing for active tenant {}",
                    cfg.tenant_id
                ))
            })?;

        // The injected KMS provider IS the custody authority — use its kind for
        // the wrapped-DEK envelope (the cfg.cmk_provider string was validated at
        // onboarding; the provider matches it by construction).
        let provider = self.kms.provider_kind();
        let key_arn = cfg
            .cmk_key_id
            .clone()
            .or(row.cmk_key_id)
            .ok_or_else(|| {
                BYOKError::Provider(format!("no CMK key id for active tenant {}", cfg.tenant_id))
            })?;
        let key_id = KmsKeyId {
            provider,
            key_arn_or_id: key_arn,
            region: cfg.cmk_region.clone().unwrap_or_default(),
        };
        let wrapped = WrappedDek {
            provider,
            key_id,
            ciphertext: row.tcs_wrapped,
            encryption_context: Some(tcs_encryption_context(&cfg.tenant_id, row.tcs_version)),
        };
        let dek = self.kms.unwrap_dek(&wrapped).await?;
        let bytes = dek.bytes;
        self.cache.put(&cfg.tenant_id, bytes);
        Ok(Tcs::from_bytes(bytes))
    }
}

/// The KMS `encryption_context` (AAD) bound when wrapping a tenant's Tcs.
///
/// The onboarding wave (the wrapper) MUST bind the IDENTICAL context. Mirrors
/// the `{tenant_id, blob_hash}` shape the providers + `DekCache` expect, with
/// the Tcs version standing in for `blob_hash`.
fn tcs_encryption_context(tenant: &str, version: i64) -> Value {
    json!({ "tenant_id": tenant, "blob_hash": format!("tcs:v{version}") })
}

// ─── Crypto context + blob (de)serialisation (pure, surface-agnostic) ───────

/// HKDF/AAD namespace tag for the CAS keyspace's digest function — domain
/// separation between the BLAKE3 and SHA-256 keyspaces.
const fn namespace_for(algo: DigestAlgo) -> &'static str {
    match algo {
        DigestAlgo::Blake3 => "blake3",
        DigestAlgo::Sha256 => "sha256",
    }
}

/// Build the convergent (Mode A) [`CryptoContext`] for a CAS object.
///
/// `total_len` is fixed to `0` (like [`CryptoContext::legacy`]) so the read
/// side reconstructs a byte-identical context WITHOUT knowing the plaintext
/// length before decrypt. `plaintext_digest` is the raw content digest for 3a
/// (the §4 HMAC'd-digest key hardening is deferred to Wave 3b).
#[must_use]
pub fn cas_crypto_context(
    tenant: &str,
    digest: &str,
    algo: DigestAlgo,
    key_id: &str,
) -> CryptoContext {
    CryptoContext::new_single_shot(
        tenant,
        digest,
        CryptoAlgo::Aes256Gcm,
        namespace_for(algo),
        CryptoMode::Convergent,
        CAS_SURFACE,
        key_id,
        0,
    )
}

/// Build the convergent (Mode A) [`CryptoContext`] for an **AC** result payload
/// (Wave 3b — closes audit H1 silent-plaintext on the action cache).
///
/// Mirrors [`cas_crypto_context`] but binds [`AC_SURFACE`] (`"ac"`) instead of
/// `"cas"`, so the derived key + AEAD AAD are domain-separated from the CAS
/// surface: an AC ciphertext can never be decrypted as (or swapped with) a CAS
/// ciphertext. The AC content identity is the `action_digest` (the AC keyspace
/// is BLAKE3 — see `R2AcHandler::r2_key`). Like the CAS context, `total_len` is
/// fixed to `0` so the read side reconstructs a byte-identical context without
/// knowing the plaintext length, and the raw digest is used for 3b (§4 HMAC'd-
/// digest key hardening stays deferred to a later wave).
#[must_use]
pub fn ac_crypto_context(tenant: &str, action_digest: &str, key_id: &str) -> CryptoContext {
    CryptoContext::new_single_shot(
        tenant,
        action_digest,
        CryptoAlgo::Aes256Gcm,
        namespace_for(DigestAlgo::Blake3),
        CryptoMode::Convergent,
        AC_SURFACE,
        key_id,
        0,
    )
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

/// The Wave-3a engagement decision for a tenant's `state`.
///
/// Wave 3a wires ONLY `active`. `partial` (backfill dual-read — audit H7) is
/// deferred to Wave 4 and fail-closed here (refuse rather than risk plaintext
/// for an encrypting tenant). Every other state is the plaintext path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByokEngagement {
    /// Run the plaintext path unchanged (not configured / inactive).
    Plaintext,
    /// Engage Mode A convergent encryption (state `active`).
    Encrypt,
    /// Active-but-unsupported in 3a → caller must FAIL CLOSED (never plaintext).
    FailClosed(&'static str),
}

/// Decide engagement for a resolved config (single source of truth shared by
/// the write and read paths).
#[must_use]
pub fn engagement_for(cfg: &TenantByokConfig) -> ByokEngagement {
    match cfg.state {
        ByokState::Active => match cfg.crypto_mode {
            ByokCryptoMode::Convergent => ByokEngagement::Encrypt,
            // Mode B (random) is deferred to Wave 3b — never plaintext.
            ByokCryptoMode::Random => {
                ByokEngagement::FailClosed("BYOK Mode B (random) not wired (deferred to Wave 3b)")
            }
        },
        // Backfill dual-read (audit H7) deferred to Wave 4 — never plaintext.
        ByokState::Partial => {
            ByokEngagement::FailClosed("BYOK partial/backfill dual-read not wired (deferred to Wave 4)")
        }
        ByokState::Inactive | ByokState::Pending | ByokState::Shredded => ByokEngagement::Plaintext,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "tests are allowed these primitives"
)]
mod tests {
    use super::*;
    use crate::customer_d1::ByokMode;
    use corelink_byok::{Dek, KmsAccessStatus, KmsKeyId, KmsProviderKind};

    const TENANT: &str = "tenant-byok-a";

    fn active_cfg(crypto_mode: ByokCryptoMode, state: ByokState) -> TenantByokConfig {
        TenantByokConfig {
            tenant_id: TENANT.to_owned(),
            mode: ByokMode::Byok,
            crypto_mode,
            cmk_provider: Some("aws".to_owned()),
            cmk_key_id: Some("arn:aws:kms:iad:1:key/cmk".to_owned()),
            cmk_region: Some("iad".to_owned()),
            state,
        }
    }

    // ── Mock config source ──────────────────────────────────────────────────

    #[derive(Debug)]
    struct MockConfigSource {
        cfg: Option<TenantByokConfig>,
        calls: Mutex<usize>,
        fail: bool,
    }
    impl MockConfigSource {
        fn ok(cfg: Option<TenantByokConfig>) -> Self {
            Self { cfg, calls: Mutex::new(0), fail: false }
        }
        fn failing() -> Self {
            Self { cfg: None, calls: Mutex::new(0), fail: true }
        }
        fn call_count(&self) -> usize {
            *lock(&self.calls)
        }
    }
    #[async_trait]
    impl ByokConfigSource for MockConfigSource {
        async fn get_byok_config(
            &self,
            _tenant: &str,
        ) -> Result<Option<TenantByokConfig>, ByokConfigError> {
            *lock(&self.calls) += 1;
            if self.fail {
                return Err(ByokConfigError::Transport("mock down".to_owned()));
            }
            Ok(self.cfg.clone())
        }
    }

    // ── Mock secret source + KMS provider ───────────────────────────────────

    #[derive(Debug)]
    struct MockSecretSource {
        row: Option<WrappedTcsRow>,
    }
    #[async_trait]
    impl ByokSecretSource for MockSecretSource {
        async fn get_wrapped_tcs(&self, _tenant: &str) -> Result<Option<WrappedTcsRow>, String> {
            Ok(self.row.clone())
        }
    }

    /// A mock KMS that "unwraps" by treating the wrapped ciphertext as the raw
    /// 32-byte secret (good enough to exercise the resolution + round-trip).
    #[derive(Debug)]
    struct MockKms {
        fail: bool,
        calls: Mutex<usize>,
    }
    impl MockKms {
        fn ok() -> Self {
            Self { fail: false, calls: Mutex::new(0) }
        }
        fn failing() -> Self {
            Self { fail: true, calls: Mutex::new(0) }
        }
        fn unwrap_count(&self) -> usize {
            *lock(&self.calls)
        }
    }
    #[async_trait]
    impl KmsProvider for MockKms {
        fn provider_kind(&self) -> KmsProviderKind {
            KmsProviderKind::AwsKms
        }
        fn region(&self) -> &str {
            "iad"
        }
        fn fips_level(&self) -> corelink_byok::FipsLevel {
            corelink_byok::FipsLevel::Fips140_3_L1
        }
        async fn wrap_dek(
            &self,
            dek: &Dek,
            key_id: &KmsKeyId,
            encryption_context: Option<&Value>,
        ) -> Result<WrappedDek, BYOKError> {
            Ok(WrappedDek {
                provider: KmsProviderKind::AwsKms,
                key_id: key_id.clone(),
                ciphertext: dek.bytes.to_vec(),
                encryption_context: encryption_context.cloned(),
            })
        }
        async fn unwrap_dek(&self, wrapped: &WrappedDek) -> Result<Dek, BYOKError> {
            *lock(&self.calls) += 1;
            if self.fail {
                return Err(BYOKError::Provider("mock kms down".to_owned()));
            }
            if wrapped.ciphertext.len() != 32 {
                return Err(BYOKError::DekLengthInvalid { got: wrapped.ciphertext.len() });
            }
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(&wrapped.ciphertext);
            Ok(Dek { bytes })
        }
        async fn check_access(&self, _key_id: &KmsKeyId) -> Result<KmsAccessStatus, BYOKError> {
            Ok(KmsAccessStatus::Ok)
        }
    }

    fn wrapped_tcs() -> WrappedTcsRow {
        WrappedTcsRow {
            tcs_wrapped: vec![7u8; 32],
            cmk_key_id: Some("arn:aws:kms:iad:1:key/cmk".to_owned()),
            tcs_version: 1,
        }
    }

    // ── ByokConfigCache ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn config_cache_hit_does_one_d1_read() {
        let src = Arc::new(MockConfigSource::ok(Some(active_cfg(
            ByokCryptoMode::Convergent,
            ByokState::Active,
        ))));
        let cache = ByokConfigCache::new(src.clone(), 60);
        let a = cache.get(TENANT).await.unwrap();
        let b = cache.get(TENANT).await.unwrap();
        assert!(a.is_some());
        assert_eq!(a, b);
        assert_eq!(src.call_count(), 1, "second get must be an in-memory HIT");
    }

    #[tokio::test]
    async fn config_cache_caches_the_none_answer() {
        // Non-BYOK tenants must not re-hit D1.
        let src = Arc::new(MockConfigSource::ok(None));
        let cache = ByokConfigCache::new(src.clone(), 60);
        assert!(cache.get(TENANT).await.unwrap().is_none());
        assert!(cache.get(TENANT).await.unwrap().is_none());
        assert_eq!(src.call_count(), 1, "the not-configured answer must be cached");
    }

    #[tokio::test]
    async fn config_cache_propagates_source_error_fail_closed() {
        let src = Arc::new(MockConfigSource::failing());
        let cache = ByokConfigCache::new(src, 60);
        assert!(matches!(
            cache.get(TENANT).await,
            Err(ByokConfigError::Transport(_))
        ));
    }

    #[tokio::test]
    async fn config_cache_zero_ttl_refetches() {
        let src = Arc::new(MockConfigSource::ok(None));
        let cache = ByokConfigCache::new(src.clone(), 0);
        cache.get(TENANT).await.unwrap();
        cache.get(TENANT).await.unwrap();
        assert_eq!(src.call_count(), 2, "a 0s TTL entry is always expired");
    }

    // ── TcsResolver ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn tcs_resolver_unwraps_then_caches() {
        let kms = Arc::new(MockKms::ok());
        let resolver = TcsResolver::new(
            Arc::new(MockSecretSource { row: Some(wrapped_tcs()) }),
            kms.clone(),
            300,
        )
        .unwrap();
        let cfg = active_cfg(ByokCryptoMode::Convergent, ByokState::Active);
        let t1 = resolver.resolve(&cfg).await.unwrap();
        let t2 = resolver.resolve(&cfg).await.unwrap();
        assert_eq!(t1.bytes, [7u8; 32]);
        assert_eq!(t1.bytes, t2.bytes);
        assert_eq!(kms.unwrap_count(), 1, "the unwrapped Tcs must be cached (≤300s)");
    }

    #[tokio::test]
    async fn tcs_resolver_fail_closed_when_secret_missing() {
        let resolver = TcsResolver::new(
            Arc::new(MockSecretSource { row: None }),
            Arc::new(MockKms::ok()),
            300,
        )
        .unwrap();
        let cfg = active_cfg(ByokCryptoMode::Convergent, ByokState::Active);
        assert!(resolver.resolve(&cfg).await.is_err());
    }

    #[tokio::test]
    async fn tcs_resolver_fail_closed_when_kms_down() {
        let resolver = TcsResolver::new(
            Arc::new(MockSecretSource { row: Some(wrapped_tcs()) }),
            Arc::new(MockKms::failing()),
            300,
        )
        .unwrap();
        let cfg = active_cfg(ByokCryptoMode::Convergent, ByokState::Active);
        assert!(resolver.resolve(&cfg).await.is_err());
    }

    #[test]
    fn tcs_cache_rejects_ttl_over_300() {
        assert!(TcsCache::new(301).is_err());
        assert!(TcsCache::new(300).is_ok());
    }

    // ── engagement_for ────────────────────────────────────────────────────────

    #[test]
    fn engagement_truth_table() {
        assert_eq!(
            engagement_for(&active_cfg(ByokCryptoMode::Convergent, ByokState::Active)),
            ByokEngagement::Encrypt
        );
        assert!(matches!(
            engagement_for(&active_cfg(ByokCryptoMode::Random, ByokState::Active)),
            ByokEngagement::FailClosed(_)
        ));
        assert!(matches!(
            engagement_for(&active_cfg(ByokCryptoMode::Convergent, ByokState::Partial)),
            ByokEngagement::FailClosed(_)
        ));
        for s in [ByokState::Inactive, ByokState::Pending, ByokState::Shredded] {
            assert_eq!(
                engagement_for(&active_cfg(ByokCryptoMode::Convergent, s)),
                ByokEngagement::Plaintext
            );
        }
    }

    // ── blob round-trip + convergent dedup + tamper ──────────────────────────

    fn ctx() -> CryptoContext {
        cas_crypto_context(TENANT, &"a".repeat(64), DigestAlgo::Blake3, "arn:cmk")
    }

    fn ac_ctx() -> CryptoContext {
        ac_crypto_context(TENANT, &"a".repeat(64), "arn:cmk")
    }

    #[test]
    fn blob_roundtrip_and_ciphertext_differs_from_plaintext() {
        let tcs = Tcs::from_bytes([3u8; 32]);
        let pt = b"sensitive build artifact bytes".to_vec();
        let stored = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        assert_ne!(stored, pt, "stored bytes must be ciphertext, not plaintext");
        assert_eq!(&stored[..4], BLOB_MAGIC);
        let out = decrypt_cas_blob(&stored, &tcs, &ctx()).unwrap();
        assert_eq!(out, pt, "round-trip must recover the plaintext");
    }

    #[test]
    fn convergent_dedup_same_content_same_ciphertext() {
        let tcs = Tcs::from_bytes([9u8; 32]);
        let pt = b"identical content".to_vec();
        let a = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        let b = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        assert_eq!(a, b, "convergent ⇒ identical stored bytes (dedup-idempotent)");
    }

    #[test]
    fn decrypt_rejects_non_magic_bytes_fail_closed() {
        // A plaintext object served to an active tenant must NOT be returned raw.
        let tcs = Tcs::from_bytes([1u8; 32]);
        assert!(decrypt_cas_blob(b"raw plaintext, no magic", &tcs, &ctx()).is_err());
        assert!(decrypt_cas_blob(b"", &tcs, &ctx()).is_err());
    }

    #[test]
    fn decrypt_wrong_tcs_fails_closed() {
        let stored = encrypt_cas_blob(b"x", &Tcs::from_bytes([1u8; 32]), &ctx()).unwrap();
        assert!(decrypt_cas_blob(&stored, &Tcs::from_bytes([2u8; 32]), &ctx()).is_err());
    }

    #[test]
    fn decode_blob_accepts_array_and_base64() {
        assert_eq!(decode_blob(&json!([1, 2, 3])).unwrap(), vec![1u8, 2, 3]);
        let b64 = base64::engine::general_purpose::STANDARD.encode([4u8, 5, 6]);
        assert_eq!(decode_blob(&json!(b64)).unwrap(), vec![4u8, 5, 6]);
        assert!(decode_blob(&json!(true)).is_err());
        assert!(decode_blob(&json!([1, 999])).is_err());
    }

    #[test]
    fn namespace_is_algo_separated() {
        assert_ne!(
            namespace_for(DigestAlgo::Blake3),
            namespace_for(DigestAlgo::Sha256)
        );
    }

    // ── AC surface (Wave 3b) ──────────────────────────────────────────────────

    #[test]
    fn ac_blob_roundtrips_under_ac_context() {
        // The AC payload encrypts + decrypts cleanly under an `"ac"`-surface ctx.
        let tcs = Tcs::from_bytes([5u8; 32]);
        let pt = b"action-result-metadata payload".to_vec();
        let stored = encrypt_cas_blob(&pt, &tcs, &ac_ctx()).unwrap();
        assert_ne!(stored, pt, "AC stored bytes must be ciphertext");
        assert_eq!(&stored[..4], BLOB_MAGIC);
        assert_eq!(decrypt_cas_blob(&stored, &tcs, &ac_ctx()).unwrap(), pt);
    }

    #[test]
    fn ac_and_cas_surfaces_are_domain_separated() {
        // Frozen policy H1: an AC blob must NOT decrypt under a CAS context, and
        // a CAS blob must NOT decrypt under an AC context — `surface` is bound
        // into the derived key + AEAD AAD, so swapping surfaces fails closed even
        // for the identical (tenant, digest, tcs).
        let tcs = Tcs::from_bytes([6u8; 32]);
        let pt = b"swap-me".to_vec();
        let ac_blob = encrypt_cas_blob(&pt, &tcs, &ac_ctx()).unwrap();
        let cas_blob = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
        // Different surface ⇒ different ciphertext for identical input.
        assert_ne!(ac_blob, cas_blob, "surface must perturb the derivation");
        // Cross-surface decrypt must fail (never returns plaintext).
        assert!(decrypt_cas_blob(&ac_blob, &tcs, &ctx()).is_err(), "AC blob must not decrypt as CAS");
        assert!(decrypt_cas_blob(&cas_blob, &tcs, &ac_ctx()).is_err(), "CAS blob must not decrypt as AC");
    }

    #[test]
    fn clb1_overhead_matches_the_wire_format() {
        // Single-source check: the stored blob is exactly plaintext + 32 (4 magic
        // + 12 nonce + 16 GCM tag), so the accounting const can never drift.
        assert_eq!(BYOK_CLB1_OVERHEAD, 32);
        let tcs = Tcs::from_bytes([8u8; 32]);
        for pt_len in [0usize, 1, 17, 4096] {
            let pt = vec![b'z'; pt_len];
            let stored = encrypt_cas_blob(&pt, &tcs, &ctx()).unwrap();
            assert_eq!(
                stored.len() as u64,
                pt_len as u64 + BYOK_CLB1_OVERHEAD,
                "stored len must be plaintext + CLB1 overhead for pt_len={pt_len}"
            );
        }
    }
}
