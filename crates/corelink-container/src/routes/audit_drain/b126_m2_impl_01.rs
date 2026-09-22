const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Shared state for the audit-chain drain route.
#[derive(Clone)]
#[non_exhaustive]
pub struct AuditDrainState {
    internal_auth_key: String,
    d1: Arc<D1HttpClient>,
    /// 32-byte Ed25519 seed for the CF-6 keyed chain head. `None` ⇒ heads are
    /// advanced UNSIGNED (legacy/tolerated). Held in [`Zeroizing`] (wiped on drop)
    /// behind an [`Arc`] so cloning the state never copies the secret bytes.
    signing_seed: Option<Arc<Zeroizing<[u8; 32]>>>,
    /// The Ed25519 key id stamped into `audit_chain_head.signing_key_id`.
    signing_key_id: u64,
    /// Forwarded write-only link keys retained in zeroizing memory. The
    /// signed epoch/ledger runtime is intentionally parked until cutover;
    /// legacy sealing below never consults this map.
    link_keyring: Option<Arc<LinkKeyring>>,
    /// SECURE DEFAULT `false`: with a seed configured, a resumed head carrying a
    /// NULL signature or a foreign `signing_key_id` is UNVERIFIABLE and treated as
    /// TAMPER (fail-CLOSED) — an insider with D1 write (but no seed) cannot strip
    /// the signature to launder a chain rewrite. Set `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME=1`
    /// ONLY as an explicit, loudly-logged operator migration window (bootstrapping
    /// pre-0080 legacy heads, or a coordinated seed/key rotation) — it re-opens the
    /// legacy tolerance and MUST be turned back off once the fleet is re-signed.
    trust_unsigned_resume: bool,
    /// Global per-call row budget: at most this many rows are sealed across ALL
    /// partitions in one `POST /_internal/audit/drain`, so a cold backlog can
    /// never make a single call exceed the edge subrequest timeout (one D1-HTTP
    /// UPDATE per row, ~0.3s each). A capped call returns `incomplete: true`; a
    /// caller (the hourly cron, or a manual loop) re-calls until `incomplete`
    /// is false. `AUDIT_DRAIN_BATCH_LIMIT`, default 512. The value remains a
    /// per-call budget; the caller's bounded sweep and the chunked writer
    /// provide the outer backpressure limits.
    batch_limit: i64,
    /// B-038: serialize drains per `(tenant_id, region)` with a lease + seal-loop
    /// fence. SECURE-INERT DEFAULT `false`: when OFF, `drain_partition` behaves
    /// EXACTLY as before (no lease acquire/release, no fence) so merging this is
    /// inert until an operator flips `AUDIT_DRAIN_LEASE_ENABLED` on after a prod
    /// probe. Same empty-is-absent idiom as the other toggles (a forwarded `""`
    /// is treated as absent → OFF).
    lease_enabled: bool,
}

impl std::fmt::Debug for AuditDrainState {
    // Redact the internal-auth secret + signing seed; never let either reach a
    // log/Debug sink.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuditDrainState")
            .field("internal_auth_key", &"<redacted>")
            .field("d1", &"[D1HttpClient]")
            .field(
                "signing_seed",
                &self.signing_seed.as_ref().map(|_| "<redacted>"),
            )
            .field("signing_key_id", &self.signing_key_id)
            .field(
                "link_keyring",
                &self.link_keyring.as_ref().map(|keyring| keyring.len()),
            )
            .field("trust_unsigned_resume", &self.trust_unsigned_resume)
            .field("batch_limit", &self.batch_limit)
            .field("lease_enabled", &self.lease_enabled)
            .finish()
    }
}

/// Load the 32-byte Ed25519 signing seed for the CF-6 keyed chain head.
///
/// Prefers a dedicated `AUDIT_CHAIN_SIGNING_SEED_HEX`; otherwise REUSES the
/// erasure-attestation seed `ERASURE_ATTESTATION_SEED_HEX` (same key material —
/// the keypair is purely seed-derived, see `key.rs::from_seed`). 64 hex chars →
/// 32 bytes. `None` when neither is set / malformed (caller then advances the
/// head UNSIGNED). Held in [`Zeroizing`] so the secret is wiped after the
/// [`ErasureSigningKey`] is constructed.
///
/// MUST treat an EMPTY value as absent at each stage (via `non_empty_env`), not
/// just `.ok()`: the DO forward-list sends every key as `this.env.X ?? ""`, so an
/// UNSET dedicated `AUDIT_CHAIN_SIGNING_SEED_HEX` arrives as `""` (present, empty).
/// A plain `std::env::var(...).ok()` returns `Some("")`, which SHORT-CIRCUITS the
/// `.or_else` fallback to the erasure-attestation seed — the head then advances
/// UNSIGNED even though `ERASURE_ATTESTATION_SEED_HEX` is set and forwarded (the
/// CF-6-silently-off prod bug: `signed=0` across every partition head).
fn load_signing_seed() -> Option<Zeroizing<[u8; 32]>> {
    // Read raw (may be `Some("")` from the DO forward-list); the empty-is-absent
    // + select + decode rule lives in the pure `resolve_seed` so it is unit-tested
    // without a `set_var` race (see tests).
    resolve_seed(
        std::env::var("AUDIT_CHAIN_SIGNING_SEED_HEX").ok(),
        std::env::var("ERASURE_ATTESTATION_SEED_HEX").ok(),
    )
}

/// Pure seed resolution: first NON-EMPTY of `(dedicated, reused)`, then 64-hex →
/// 32 bytes. A `Some("")` (an unset secret forwarded as `""`) is treated as
/// ABSENT so the dedicated slot never masks the reused erasure-attestation seed.
fn resolve_seed(dedicated: Option<String>, reused: Option<String>) -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = [dedicated, reused]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_owned())
        .find(|s| !s.is_empty())?;
    if hex_str.len() != 64 {
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(&hex_str, bytes.as_mut()).ok()?;
    Some(bytes)
}

/// Monotonic signing-key id for the chain head — `AUDIT_CHAIN_SIGNING_KEY_ID`
/// (dedicated) else `ERASURE_ATTESTATION_KEY_ID` (reused) else `1` (launch key).
/// Stamped into `audit_chain_head.signing_key_id` so a seed/key rotation is
/// distinguishable from tampering on resume.
///
/// Same empty-is-absent rule as [`load_signing_seed`]: a forwarded `""` for the
/// dedicated `AUDIT_CHAIN_SIGNING_KEY_ID` must NOT mask the reused
/// `ERASURE_ATTESTATION_KEY_ID`, so the key id stamped alongside the head stays
/// consistent with the seed actually used to sign it.
fn signing_key_id_from_env() -> u64 {
    resolve_key_id(
        std::env::var("AUDIT_CHAIN_SIGNING_KEY_ID").ok(),
        std::env::var("ERASURE_ATTESTATION_KEY_ID").ok(),
    )
}

/// Pure key-id resolution: first NON-EMPTY of `(dedicated, reused)` parsed as
/// `u64`, else `1`. Mirrors [`resolve_seed`]'s empty-is-absent rule so a
/// forwarded `""` dedicated id never masks the reused erasure-attestation id.
fn resolve_key_id(dedicated: Option<String>, reused: Option<String>) -> u64 {
    [dedicated, reused]
        .into_iter()
        .flatten()
        .map(|s| s.trim().to_owned())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(1)
}

/// Parse the forwarded B-054 write-only keyring without ever retaining raw
/// JSON or key bytes in ordinary `String`/`Vec` state. Empty is the deliberate
/// pre-cutover value emitted for an unset Worker secret; any non-empty malformed
/// value unmounts the route rather than silently falling back to E0.
fn load_link_keyring() -> Result<Option<Arc<LinkKeyring>>, String> {
    let raw = match std::env::var("AUDIT_CHAIN_LINK_KEYS_JSON") {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => return Ok(None),
        Err(error) => return Err(format!("read AUDIT_CHAIN_LINK_KEYS_JSON: {error}")),
    };
    let raw = Zeroizing::new(raw);
    if raw.trim().is_empty() {
        return Ok(None);
    }
    LinkKeyring::parse_json(&raw)
        .map(|keyring| Some(Arc::new(keyring)))
        .map_err(|error| format!("parse AUDIT_CHAIN_LINK_KEYS_JSON: {error}"))
}

/// Map a stored partition region string to the attestation [`Region`] enum used
/// to construct the [`ErasureSigningKey`].
///
/// The Ed25519 keypair is PURELY seed-derived (`key.rs::from_seed` ignores
/// region/timestamps for key material) and the partition region is bound in the
/// SIGNED canonical tuple — NOT in the key. So a macro region with no attestation
/// enum (e.g. `apac`/`afr`) maps to a harmless default here WITHOUT changing the
/// key or the signed message; signing/verification of those partitions still
/// works and stays region-bound via [`canonical_head_bytes`].
fn region_for_key(region: &str) -> Region {
    Region::parse(region).unwrap_or(Region::Wnam)
}

/// The canonical head tuple `(tenant_id, region, head_hash, next_sequence)` that
/// is Ed25519-signed. RFC-8785 JCS (`serde_jcs`, the SAME canonicalizer the row
/// seals + the erasure attestation use) sorts keys lexicographically, yielding the
/// deterministic byte string
/// `{"head_hash":"<64hex>","next_sequence":<int>,"region":"<r>","tenant_id":"<uuid>"}`.
#[derive(Serialize)]
struct CanonicalAuditHead<'a> {
    head_hash: &'a str,
    next_sequence: u64,
    region: &'a str,
    tenant_id: &'a str,
}

/// B-054 version-2 signed-head tuple.  Epoch and witness-ledger facts are
/// included in the signed bytes so a valid legacy head cannot be relabelled as
/// E0/v2 or replayed after a forward transition.  This shape is intentionally
/// separate from [`CanonicalAuditHead`], whose v1 bytes are immutable for
/// historical heads.
#[derive(Serialize)]
struct CanonicalAuditHeadV2<'a> {
    epoch_id: u64,
    epoch_ledger_hash: &'a str,
    epoch_ledger_sequence: u64,
    head_hash: &'a str,
    head_message_version: u8,
    next_sequence: u64,
    region: &'a str,
    signing_key_id: u64,
    tenant_id: &'a str,
}

/// RFC-8785 JCS bytes of the canonical head tuple (what is signed / verified).
fn canonical_head_bytes(
    tenant_id: &str,
    region: &str,
    head_hash: &str,
    next_sequence: u64,
) -> Result<Vec<u8>, String> {
    serde_jcs::to_vec(&CanonicalAuditHead {
        head_hash,
        next_sequence,
        region,
        tenant_id,
    })
    .map_err(|e| format!("JCS canonicalize audit head: {e}"))
}

/// RFC-8785 JCS bytes for the B-054 v2 signed head.  The function is kept pure
/// so the eventual witness/D1 transaction can compare the exact bytes without
/// duplicating field ordering or accepting a missing version.
#[allow(
    dead_code,
    clippy::too_many_arguments,
    reason = "B-054 v2 writer is gated until external witness wiring lands"
)]
fn canonical_head_v2_bytes(
    tenant_id: &str,
    region: &str,
    head_hash: &str,
    next_sequence: u64,
    epoch_id: u64,
    epoch_ledger_sequence: u64,
    epoch_ledger_hash: &str,
    signing_key_id: u64,
) -> Result<Vec<u8>, String> {
    serde_jcs::to_vec(&CanonicalAuditHeadV2 {
        epoch_id,
        epoch_ledger_hash,
        epoch_ledger_sequence,
        head_hash,
        head_message_version: 2,
        next_sequence,
        region,
        signing_key_id,
        tenant_id,
    })
    .map_err(|e| format!("JCS canonicalize versioned audit head: {e}"))
}

/// Ed25519-sign the canonical head tuple with the seed-derived key. Returns the
/// base64 signature stored in `audit_chain_head.head_signature`.
fn sign_head(
    seed: &[u8; 32],
    key_id: u64,
    tenant_id: &str,
    region: &str,
    head_hash: &str,
    next_sequence: u64,
) -> Result<String, String> {
    let canonical = canonical_head_bytes(tenant_id, region, head_hash, next_sequence)?;
    // Reuse the erasure-attestation key infra (mirrors `routes/dsr/attestation.rs`).
    let sk = ErasureSigningKey::from_seed(key_id, region_for_key(region), 0, 0, *seed);
    let sig = sk.signing_key.sign(&canonical);
    Ok(base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()))
}

/// Sign a B-054 v2 head tuple.  The caller must obtain the external witness
/// receipt before attempting the D1 CAS; signing alone is not a witness.
#[allow(
    dead_code,
    clippy::too_many_arguments,
    reason = "B-054 v2 writer is gated until external witness wiring lands"
)]
fn sign_head_v2(
    seed: &[u8; 32],
    key_id: u64,
    tenant_id: &str,
    region: &str,
    head_hash: &str,
    next_sequence: u64,
    epoch_id: u64,
    epoch_ledger_sequence: u64,
    epoch_ledger_hash: &str,
) -> Result<String, String> {
    let canonical = canonical_head_v2_bytes(
        tenant_id,
        region,
        head_hash,
        next_sequence,
        epoch_id,
        epoch_ledger_sequence,
        epoch_ledger_hash,
        key_id,
    )?;
    let sk = ErasureSigningKey::from_seed(key_id, region_for_key(region), 0, 0, *seed);
    let sig = sk.signing_key.sign(&canonical);
    Ok(base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()))
}

/// Verify a stored base64 head signature against the canonical head tuple + the
/// seed-derived public key. `false` on ANY mismatch / malformed input (a forged
/// head, a wrong seed, a corrupt signature) — the caller treats `false` as
/// tamper (fail-CLOSED).
fn verify_head(
    seed: &[u8; 32],
    key_id: u64,
    tenant_id: &str,
    region: &str,
    head_hash: &str,
    next_sequence: u64,
    signature_b64: &str,
) -> bool {
    let Ok(canonical) = canonical_head_bytes(tenant_id, region, head_hash, next_sequence) else {
        return false;
    };
    let vk = ErasureSigningKey::from_seed(key_id, region_for_key(region), 0, 0, *seed)
        .public_key()
        .verifying_key;
    let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(signature_b64) else {
        return false;
    };
    let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
        return false;
    };
    vk.verify(&canonical, &Signature::from_bytes(&sig_arr))
        .is_ok()
}

/// Verify a B-054 v2 head signature against the exact authenticated epoch
/// tuple.  Any malformed field or signature returns false (fail closed).
#[allow(
    dead_code,
    clippy::too_many_arguments,
    reason = "B-054 v2 verifier is gated until external witness wiring lands"
)]
fn verify_head_v2(
    seed: &[u8; 32],
    key_id: u64,
    tenant_id: &str,
    region: &str,
    head_hash: &str,
    next_sequence: u64,
    epoch_id: u64,
    epoch_ledger_sequence: u64,
    epoch_ledger_hash: &str,
    signature_b64: &str,
) -> bool {
    let Ok(canonical) = canonical_head_v2_bytes(
        tenant_id,
        region,
        head_hash,
        next_sequence,
        epoch_id,
        epoch_ledger_sequence,
        epoch_ledger_hash,
        key_id,
    ) else {
        return false;
    };
    let vk = ErasureSigningKey::from_seed(key_id, region_for_key(region), 0, 0, *seed)
        .public_key()
        .verifying_key;
    let Ok(sig_bytes) = base64::engine::general_purpose::STANDARD.decode(signature_b64) else {
        return false;
    };
    let Ok(sig_arr): Result<[u8; 64], _> = sig_bytes.try_into() else {
        return false;
    };
    vk.verify(&canonical, &Signature::from_bytes(&sig_arr))
        .is_ok()
}

/// Constant-time internal-auth check. Mirrors
/// [`crate::routes::dsr`]`::internal_auth_ok` byte-for-byte so the internal-auth
/// gates stay consistent (pad provided to the secret length, run `ct_eq`, fold in
/// the real length-equality so a longer/shorter value can never match).
#[must_use]
fn internal_auth_ok(expected: &[u8], headers: &HeaderMap) -> bool {
    let provided = headers
        .get(INTERNAL_AUTH_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let provided_bytes = provided.as_bytes();
    let provided_padded: Vec<u8> = if provided_bytes.len() >= expected.len() {
        provided_bytes.get(..expected.len()).unwrap_or(&[]).to_vec()
    } else {
        let mut v = provided_bytes.to_vec();
        v.resize(expected.len(), 0);
        v
    };
    let content_ok = expected.ct_eq(&provided_padded).unwrap_u8();
    let len_ok = u8::from(expected.len() == provided_bytes.len());
    (content_ok & len_ok) == 1
}

/// Build the route state from env. `None` when the dedicated erase key is
/// unset or shorter than 32 chars, OR when D1 `StorageEnv` is not configured
/// (route not mounted — fail-CLOSED). Mirrors the DSR route's gate: requires the
/// dedicated `CORELINK_ERASE_AUTH_KEY` ONLY (NO shared `CORELINK_INTERNAL_AUTH_KEY`
/// fallback — finding H4), preserves the ≥32-char floor (F28/F15). Without
/// D1 there is nothing to seal, so the route is simply not mounted.
#[must_use]
pub fn build_state_from_env() -> Option<AuditDrainState> {
    let internal_auth_key = match crate::routes::admin::erase_auth_key_from_env() {
        Some(k) if k.len() >= 32 => k.to_string(),
        _ => {
            tracing::warn!(
                "no usable CORELINK_ERASE_AUTH_KEY (dedicated; NO shared fallback) \
                 (< 32 chars); /_internal/audit/drain NOT mounted (fail-CLOSED)"
            );
            return None;
        }
    };
    let storage_env = crate::storage::StorageEnv::from_env()?;
    let d1 = Arc::new(crate::storage::d1_http::D1HttpClient::new(&storage_env).ok()?);
    // CF-6: the seed-derived Ed25519 key for the keyed chain head. `None` ⇒ heads
    // are advanced UNSIGNED (legacy/tolerated) — the route still mounts (dev/CI),
    // but the head is NOT tamper-evident until a seed is provisioned.
    let signing_seed = load_signing_seed().map(Arc::new);
    let signing_key_id = signing_key_id_from_env();
    let link_keyring = match load_link_keyring() {
        Ok(keyring) => keyring,
        Err(error) => {
            tracing::warn!(error = %error, "audit/drain: malformed link keyring; route NOT mounted (fail-CLOSED)");
            return None;
        }
    };
    if signing_seed.is_none() {
        tracing::warn!(
            "audit/drain: no AUDIT_CHAIN_SIGNING_SEED_HEX / ERASURE_ATTESTATION_SEED_HEX \
             configured — chain heads will be advanced UNSIGNED (legacy/tolerated); set the \
             seed to make the per-partition chain head tamper-evident (CF-6)"
        );
    }
    // SECURE DEFAULT: with a seed present, an unverifiable resumed head (NULL sig
    // or foreign key id) is TAMPER. The escape is an explicit operator opt-in for a
    // legacy/rotation migration window only.
    let trust_unsigned_resume = std::env::var("AUDIT_CHAIN_TRUST_UNSIGNED_RESUME")
        .ok()
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "TRUE"));
    if trust_unsigned_resume {
        tracing::warn!(
            "audit/drain: AUDIT_CHAIN_TRUST_UNSIGNED_RESUME is ON — resumed heads with a NULL \
             signature or a foreign signing_key_id are TOLERATED (legacy/rotation migration \
             window). This re-opens the CF-6 tamper tolerance; turn it OFF once the fleet is \
             re-signed under the current key."
        );
    }
    // Global per-call row budget. The production configuration sets 512
    // explicitly, and the same bounded value is the safe fallback when a
    // deployment forgets to forward the variable. A non-positive value would
    // seal nothing forever, so invalid overrides fall back to the budget too.
    let batch_limit_raw = std::env::var("AUDIT_DRAIN_BATCH_LIMIT").ok();
    let batch_limit = parse_audit_drain_batch_limit(batch_limit_raw.as_deref());
    // B-038 partition lease + seal-loop fence. SECURE-INERT DEFAULT OFF: absent /
    // forwarded-`""` / anything but an explicit truthy value ⇒ the drain behaves
    // exactly as before. Flipped on by an operator only after the prod probe.
    let lease_enabled = std::env::var("AUDIT_DRAIN_LEASE_ENABLED")
        .ok()
        .is_some_and(|v| matches!(v.trim(), "1" | "true" | "TRUE"));
    if lease_enabled {
        tracing::info!(
            "audit/drain: AUDIT_DRAIN_LEASE_ENABLED is ON — partitions are serialized \
             by a per-partition lease + seal-loop fence (B-038)"
        );
    }
    Some(AuditDrainState {
        internal_auth_key,
        d1,
        signing_seed,
        signing_key_id,
        link_keyring,
        trust_unsigned_resume,
        batch_limit,
        lease_enabled,
    })
}

/// Resolve the drain's per-call row budget without allowing an absent or
/// malformed deployment variable to silently restore the historical 200-row
/// ceiling. The route remains bounded by this value and the caller's sweep
/// budget; deployment-specific tuning still comes from the environment.
const DEFAULT_AUDIT_DRAIN_BATCH_LIMIT: i64 = 512;

fn parse_audit_drain_batch_limit(raw: Option<&str>) -> i64 {
    raw.and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value >= 1)
        .unwrap_or(DEFAULT_AUDIT_DRAIN_BATCH_LIMIT)
}

/// Mount `POST /_internal/audit/drain`.
pub fn router(state: AuditDrainState) -> Router {
    Router::new()
        .route("/_internal/audit/drain", post(handle_drain))
        .with_state(state)
}

fn now_ms() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    )
    .unwrap_or(i64::MAX)
}

/// TTL for a per-partition drain lease (B-038). Generously bounds ONE `/drain`
/// call (already row-bounded by `batch_limit` + the edge subrequest timeout), so
/// a crashed holder's lease self-expires and the partition is never wedged. NOT
/// tied to the whole backlog drain — each `/drain` call re-acquires. 5 min.
const AUDIT_DRAIN_LEASE_TTL_MS: i64 = 5 * 60 * 1000;

/// A unique-per-invocation lease holder id (uuid v4). Used so `release_lease`
/// only ever deletes a lease we still own (holder-scoped delete).
fn new_lease_holder() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The seal-loop self-fence decision (B-038). When the lease regime is ON, a
/// holder MUST stop writing once its own lease has expired (`now >= expires_ms`)
/// so it cannot still be sealing after a stealer takes the now-expired lease —
/// the residual fork window a bare TTL lease leaves open. Pure so it is
/// unit-tested without D1. When the lease is OFF this is always `false`, so the
/// seal loop runs to completion exactly as before.
fn should_fence(now_ms: i64, my_lease_expires_ms: i64, lease_enabled: bool) -> bool {
    lease_enabled && now_ms >= my_lease_expires_ms
}

/// Outcome of the fenced seal loop over the pre-computed sealed rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FencedSeal {
    /// Every row was written; the caller proceeds to advance the head.
    Complete(u64),
    /// The fence tripped after writing `n` rows (the pre-expiry prefix); the
    /// caller must NOT advance the head — the next drain resumes from the tail.
    Fenced(u64),
}
