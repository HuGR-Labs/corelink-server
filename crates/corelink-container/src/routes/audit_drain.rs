//! Internal S-09 audit-chain drain endpoint.
//!
//! `POST /_internal/audit/drain` — seals the live `audit_outbox` trail into the
//! BLAKE3 tamper-evident hash chain. Until this drain runs, `audit_outbox` rows
//! are PLAIN, UNCHAINED CloudEvents envelopes (`emitted_at IS NULL`; see
//! `routes/dsr/audit.rs`), so the live audit trail is **not** tamper-evident —
//! a D1 writer could alter or delete a row undetected. The BLAKE3
//! [`HashChainBuilder`] (`corelink_audit_chain::chain`) is real + property-tested
//! but had no live producer; this is that producer.
//!
//! ## What it does
//!
//! For each `(tenant_id, region)` partition that has pending rows
//! (`emitted_at IS NULL`):
//!
//! 1. **Resume** the chain head. The authoritative resume point is the durable
//!    sealed-rows tail (the sealed row with the MAX `sequence_number`); the
//!    `audit_chain_head` checkpoint is the fast path. When the sealed tail is
//!    AHEAD of the checkpoint — a prior drain crashed after sealing rows but
//!    before advancing the checkpoint — we trust the sealed rows. This makes the
//!    drain crash-safe (the rows, not the checkpoint, are the source of truth).
//!    The builder is seeded via [`HashChainBuilder::resume`] (or
//!    [`HashChainBuilder::new`] at GENESIS).
//! 2. **Seal** each pending row IN ORDER (`enqueued_at, id`): compute the
//!    RFC-8785 JCS-canonical bytes of the payload, link it into the chain
//!    (`chain_hash = BLAKE3(prev_hash || canonical_jcs)` via
//!    [`link_chain_hash_from_canonical`]), and atomically write the sealed
//!    columns + flip `emitted_at`. The row UPDATE is guarded by
//!    `emitted_at IS NULL`, so a re-run never double-seals (IDEMPOTENT). The
//!    computation is fully deterministic, so two concurrent drains compute
//!    byte-identical seals — overlapping row writes are identical, never a fork.
//! 3. **Advance** the `audit_chain_head` checkpoint with a compare-and-set on the
//!    value we resumed from (SINGLE-WRITER anti-fork): if the stored head drifted
//!    (a concurrent drain advanced it), abort the partition rather than fork the
//!    chain. The rows we sealed are deterministic/identical to the concurrent
//!    drain's, so they remain safe; we simply do not double-advance.
//!
//! ## CF-6 — keyed (Ed25519-SIGNED) chain head (enterprise-DD finding #4)
//!
//! The BLAKE3 chain above is UNKEYED: an insider with D1 write can rewrite the
//! sealed `audit_outbox` rows, recompute a self-consistent chain + head, overwrite
//! the `audit_chain_head` checkpoint to match, and the unkeyed verifier still
//! passes. To make the head un-forgeable without a secret, every checkpoint
//! ADVANCE (step 3) ALSO Ed25519-SIGNS the canonical head tuple
//! `(tenant_id, region, head_hash, next_sequence)` — RFC-8785 JCS bytes via
//! [`canonical_head_bytes`] — and persists `head_signature` (base64) +
//! `head_signed_at_ms` + `signing_key_id` (migration 0080).
//!
//! On RESUME ([`check_head_on_resume`]), once a seed is configured (the signing
//! regime is ACTIVE), the resumed head MUST carry a signature that verifies under
//! the CURRENT key id against the canonical tuple + the seed-derived public key.
//! Anything else is TAMPER → the drain refuses to extend the chain (fail-CLOSED,
//! SEV-1): a verify failure, a NULL signature, a foreign `signing_key_id`, or a
//! signed head with no seed to verify it. This closes the laundering hole where an
//! insider with D1 write (but no seed) STRIPS the signature — they cannot re-sign,
//! and the honest drain no longer re-signs an unverifiable head for them.
//!
//! Legitimate pre-0080 legacy heads and coordinated seed/key ROTATIONS are handled
//! by an EXPLICIT, loudly-logged operator migration window
//! (`AUDIT_CHAIN_TRUST_UNSIGNED_RESUME=1`, default OFF) that temporarily tolerates
//! NULL / foreign-key-id heads while the fleet is re-signed — never a silent
//! tolerance. (Full D1-write-insider resistance additionally needs an external
//! anchor — Rekor / R2 Object-Lock — and signed key-rotation records; both remain
//! roadmap.)
//!
//! ### Key source (REUSED — no new secret)
//!
//! The signing key is the per-region erasure-attestation Ed25519 key
//! ([`ErasureSigningKey::from_seed`], seed `ERASURE_ATTESTATION_SEED_HEX`,
//! key id `ERASURE_ATTESTATION_KEY_ID`). The Ed25519 keypair is PURELY
//! seed-derived (`key.rs::from_seed` ignores region/timestamps for key material),
//! so one seed signs every partition and the partition's region is bound in the
//! SIGNED tuple — not in the key. An optional dedicated `AUDIT_CHAIN_SIGNING_SEED_HEX`
//! / `AUDIT_CHAIN_SIGNING_KEY_ID` takes precedence for operators who want key
//! separation. When NO seed is configured the head is advanced UNSIGNED
//! (legacy/tolerated) — preserving drain liveness in dev/CI.
//!
//! ## Why `link_chain_hash_from_canonical` and not `HashChainBuilder::append`
//!
//! `HashChainBuilder::append` takes a typed `corelink_audit_chain::AuditEvent` and
//! re-canonicalizes it. The `audit_outbox.payload_json` rows are GENERIC
//! CloudEvents JSON written by several sinks (`routes/dsr/audit.rs` etc.), NOT the
//! crate's `AuditEvent` struct, so `append` cannot consume them. Instead we
//! canonicalize the raw payload with `serde_jcs` and drive
//! `link_chain_hash_from_canonical` directly on those bytes — which is exactly the
//! verifier path the crate documents (the verifier reads the persisted JCS bytes
//! off the row and recomputes BLAKE3, never re-canonicalizing). The builder is
//! still used to seed `(head, next_sequence)` honestly via `resume`/`new`.
//!
//! Gated by the internal-auth shared secret (constant-time), mirroring
//! [`crate::routes::dsr`] byte-for-byte. Env-gated mount in [`crate::main`]
//! (unmounted in dev/CI without the erase key + D1).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::Engine as _;
use ed25519_dalek::{Signature, Signer as _, Verifier as _};
use serde::Serialize;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use corelink_audit_chain::{link_chain_hash_from_canonical, ChainHash, HashChainBuilder};
use corelink_erasure_attestation::{ErasureSigningKey, Region};

use crate::storage::d1_http::D1HttpClient;

const INTERNAL_AUTH_HEADER: &str = "x-corelink-internal-auth";

/// Shared state for the audit-chain drain route.
#[derive(Clone)]
pub struct AuditDrainState {
    internal_auth_key: String,
    d1: Arc<D1HttpClient>,
    /// 32-byte Ed25519 seed for the CF-6 keyed chain head. `None` ⇒ heads are
    /// advanced UNSIGNED (legacy/tolerated). Held in [`Zeroizing`] (wiped on drop)
    /// behind an [`Arc`] so cloning the state never copies the secret bytes.
    signing_seed: Option<Arc<Zeroizing<[u8; 32]>>>,
    /// The Ed25519 key id stamped into `audit_chain_head.signing_key_id`.
    signing_key_id: u64,
    /// SECURE DEFAULT `false`: with a seed configured, a resumed head carrying a
    /// NULL signature or a foreign `signing_key_id` is UNVERIFIABLE and treated as
    /// TAMPER (fail-CLOSED) — an insider with D1 write (but no seed) cannot strip
    /// the signature to launder a chain rewrite. Set `AUDIT_CHAIN_TRUST_UNSIGNED_RESUME=1`
    /// ONLY as an explicit, loudly-logged operator migration window (bootstrapping
    /// pre-0080 legacy heads, or a coordinated seed/key rotation) — it re-opens the
    /// legacy tolerance and MUST be turned back off once the fleet is re-signed.
    trust_unsigned_resume: bool,
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
            .field("trust_unsigned_resume", &self.trust_unsigned_resume)
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
fn load_signing_seed() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("AUDIT_CHAIN_SIGNING_SEED_HEX")
        .ok()
        .or_else(|| std::env::var("ERASURE_ATTESTATION_SEED_HEX").ok())?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(hex_str, bytes.as_mut()).ok()?;
    Some(bytes)
}

/// Monotonic signing-key id for the chain head — `AUDIT_CHAIN_SIGNING_KEY_ID`
/// (dedicated) else `ERASURE_ATTESTATION_KEY_ID` (reused) else `1` (launch key).
/// Stamped into `audit_chain_head.signing_key_id` so a seed/key rotation is
/// distinguishable from tampering on resume.
fn signing_key_id_from_env() -> u64 {
    std::env::var("AUDIT_CHAIN_SIGNING_KEY_ID")
        .ok()
        .or_else(|| std::env::var("ERASURE_ATTESTATION_KEY_ID").ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(1)
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
    Some(AuditDrainState {
        internal_auth_key,
        d1,
        signing_seed,
        signing_key_id,
        trust_unsigned_resume,
    })
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

/// Parse a 64-char lowercase-hex BLAKE3 digest into a [`ChainHash`]. `None` on
/// any malformed value (wrong length / non-hex).
fn chain_hash_from_hex(s: &str) -> Option<ChainHash> {
    let mut out = [0u8; 32];
    hex::decode_to_slice(s.trim(), &mut out).ok()?;
    Some(ChainHash(out))
}

/// The persisted per-partition chain checkpoint (`audit_chain_head` row).
#[derive(Clone, Debug)]
struct HeadCheckpoint {
    head: ChainHash,
    /// The EXACT stored hex (used verbatim in the compare-and-set WHERE clause).
    head_hex: String,
    next_sequence: u64,
    /// CF-6: base64 Ed25519 signature over the canonical head tuple (`None` for a
    /// pre-0080 / legacy head — tolerated, re-signed on the next advance).
    head_signature: Option<String>,
    /// CF-6: the key id the stored signature was produced under (lets a
    /// seed/key rotation be told apart from tampering on resume).
    signing_key_id: Option<u64>,
}

/// One `audit_outbox` row sealed into the BLAKE3 chain. Pure value — the seal is
/// computed in memory, then written.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SealedRow {
    id: String,
    sequence_number: u64,
    prev_hash_hex: String,
    chain_hash_hex: String,
    /// The EXACT RFC-8785 JCS bytes that were hashed (what the verifier re-hashes).
    canonical_jcs: String,
}

/// Outcome of draining a single partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartitionOutcome {
    /// `n` rows sealed and the head advanced.
    Sealed(u64),
    /// The stored head drifted under us (a concurrent drain advanced it) — the
    /// partition was aborted to avoid forking the chain.
    Drift,
    /// No pending rows (raced away between the partition scan and the read).
    Empty,
}

/// Pure chain-seal computation. Given the resume `(start_head, start_seq)` and the
/// pending rows in deterministic order, compute the sealed link for each row.
/// Returns `(sealed_rows, new_head, new_next_sequence)`.
///
/// Fully deterministic: identical inputs yield identical outputs — the basis of
/// the concurrent-drain fork-freedom (two drains compute byte-identical seals).
///
/// # Errors
/// Aborts the whole partition (returns `Err`) if any row's payload cannot be
/// JCS-canonicalized (would break sequence contiguity if skipped). Our own sinks
/// only ever write valid JSON, so this is defensive.
fn seal_rows(
    start_head: ChainHash,
    start_seq: u64,
    rows: &[(String, Value)],
) -> Result<(Vec<SealedRow>, ChainHash, u64), String> {
    let mut prev = start_head;
    let mut seq = start_seq;
    let mut out = Vec::with_capacity(rows.len());
    for (id, payload) in rows {
        let jcs =
            serde_jcs::to_vec(payload).map_err(|e| format!("JCS canonicalize row {id}: {e}"))?;
        // `chain_hash = BLAKE3(prev_hash || canonical_jcs)` over the EXACT bytes
        // we persist as `canonical_jcs` — the verifier re-hashes these.
        let chain = link_chain_hash_from_canonical(&prev, &jcs);
        let canonical_jcs =
            String::from_utf8(jcs).map_err(|e| format!("JCS bytes not UTF-8 for row {id}: {e}"))?;
        out.push(SealedRow {
            id: id.clone(),
            sequence_number: seq,
            prev_hash_hex: prev.to_hex(),
            chain_hash_hex: chain.to_hex(),
            canonical_jcs,
        });
        prev = chain;
        seq = seq.saturating_add(1);
    }
    Ok((out, prev, seq))
}

/// Resolve the authoritative resume `(head, next_sequence)` for a partition.
/// Prefers the durable sealed-rows tail when it is AHEAD of the checkpoint (a
/// prior drain crashed after sealing rows but before advancing the head); else
/// the `audit_chain_head` checkpoint; else GENESIS (`None`).
fn resolve_resume(
    checkpoint: Option<(ChainHash, u64)>,
    sealed_tail: Option<(ChainHash, u64)>,
) -> Option<(ChainHash, u64)> {
    // sealed_tail carries (chain_hash, sequence_number) of the MAX sealed row; the
    // next sequence is that + 1.
    let from_tail = sealed_tail.map(|(h, seq)| (h, seq.saturating_add(1)));
    match (checkpoint, from_tail) {
        (Some((_, cp_seq)), Some((th, t_seq))) if t_seq > cp_seq => Some((th, t_seq)),
        (Some(cp), _) => Some(cp),
        (None, Some(t)) => Some(t),
        (None, None) => None,
    }
}

/// Outcome of the CF-6 head-signature check on resume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeadResumeCheck {
    /// Safe to resume + (re-)sign on advance: genesis (no checkpoint); no signing
    /// regime configured and the head was never signed (dev/CI); or an explicit
    /// operator migration window (`trust_unsigned_resume`) is tolerating a NULL /
    /// foreign-key-id head during a legacy-bootstrap or seed-rotation re-sign.
    Proceed,
    /// A signature signed under the CURRENT key id verified OK.
    Verified,
    /// A NON-NULL signature signed under the current key id did NOT verify, OR a
    /// signed head is present but no seed is configured to verify it: TAMPER /
    /// fail-CLOSED — refuse to extend the chain from this head.
    FailClosed,
}

/// Verify the stored head signature of the checkpoint we resume from (CF-6).
///
/// A checkpoint carrying a non-NULL signature signed under the CURRENT key id MUST
/// verify against the canonical head tuple + the seed-derived public key. A NULL
/// signature (legacy), a signature under a different key id (rotation), or no
/// checkpoint at all are tolerated ([`HeadResumeCheck::Proceed`]); the head is
/// (re-)signed on advance. A signature that fails to verify — or a signed head
/// with no seed available to verify it — is [`HeadResumeCheck::FailClosed`].
fn check_head_on_resume(
    checkpoint: Option<&HeadCheckpoint>,
    signing_seed: Option<&[u8; 32]>,
    current_key_id: u64,
    tenant_id: &str,
    region: &str,
    trust_unsigned_resume: bool,
) -> HeadResumeCheck {
    let Some(cp) = checkpoint else {
        // Genesis — no prior head to verify.
        return HeadResumeCheck::Proceed;
    };
    let Some(seed) = signing_seed else {
        // No signing regime configured (dev/CI). Nothing to verify against: a
        // never-signed (NULL) head is tolerated; a head that CLAIMS a signature we
        // cannot check is still fail-CLOSED (can't prove integrity).
        return if cp.head_signature.is_none() {
            HeadResumeCheck::Proceed
        } else {
            HeadResumeCheck::FailClosed
        };
    };
    // Signing regime ACTIVE. Every legit advance signs the head under the current
    // key, so a NULL signature or a foreign `signing_key_id` is UNVERIFIABLE. An
    // insider with D1 write (but no seed) strips/rotates the signature to launder a
    // chain rewrite — so by default that is TAMPER (fail-CLOSED). A genuine
    // legacy/rotation migration is an EXPLICIT, logged operator opt-in
    // (`trust_unsigned_resume`), never a silent tolerance.
    let Some(sig) = cp.head_signature.as_deref() else {
        return if trust_unsigned_resume {
            HeadResumeCheck::Proceed
        } else {
            HeadResumeCheck::FailClosed
        };
    };
    if cp.signing_key_id != Some(current_key_id) {
        return if trust_unsigned_resume {
            HeadResumeCheck::Proceed
        } else {
            HeadResumeCheck::FailClosed
        };
    }
    // Signed under the current key id ⇒ MUST verify. A failure here is an active
    // forgery (head rewritten while keeping a current-key signature) and is ALWAYS
    // tamper — never softened by the migration escape.
    if verify_head(
        seed,
        current_key_id,
        tenant_id,
        region,
        &cp.head_hex,
        cp.next_sequence,
        sig,
    ) {
        HeadResumeCheck::Verified
    } else {
        HeadResumeCheck::FailClosed
    }
}

/// Read the `(tenant_id, region)` partitions with pending (unsealed) rows.
async fn read_pending_partitions(d1: &D1HttpClient) -> Result<Vec<(String, String)>, String> {
    let rows = d1
        .query(
            "SELECT DISTINCT tenant_id, region FROM audit_outbox WHERE emitted_at IS NULL",
            &[],
        )
        .await?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let t = r.get("tenant_id").and_then(Value::as_str)?.to_owned();
            let region = r.get("region").and_then(Value::as_str)?.to_owned();
            Some((t, region))
        })
        .collect())
}

/// Read the `audit_chain_head` checkpoint for a partition.
async fn read_checkpoint(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Option<HeadCheckpoint>, String> {
    let rows = d1
        .query(
            "SELECT head_hash, next_sequence, head_signature, signing_key_id \
             FROM audit_chain_head \
             WHERE tenant_id = ?1 AND region = ?2",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let head_hex = row
        .get("head_hash")
        .and_then(Value::as_str)
        .ok_or("audit_chain_head.head_hash missing/non-text")?
        .to_owned();
    let head = chain_hash_from_hex(&head_hex)
        .ok_or("audit_chain_head.head_hash is not a 64-hex BLAKE3 digest")?;
    let next_sequence = row
        .get("next_sequence")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("audit_chain_head.next_sequence missing/negative/non-integer")?;
    // CF-6: nullable signature columns (0080). A NULL is a legacy head.
    let head_signature = row
        .get("head_signature")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let signing_key_id = row
        .get("signing_key_id")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok());
    Ok(Some(HeadCheckpoint {
        head,
        head_hex,
        next_sequence,
        head_signature,
        signing_key_id,
    }))
}

/// Read the durable sealed-rows tail (MAX sequence sealed row) for a partition.
async fn read_sealed_tail(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Option<(ChainHash, u64)>, String> {
    let rows = d1
        .query(
            "SELECT sequence_number, chain_hash FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 \
               AND emitted_at IS NOT NULL AND sequence_number IS NOT NULL \
             ORDER BY sequence_number DESC LIMIT 1",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let seq = row
        .get("sequence_number")
        .and_then(Value::as_i64)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or("audit_outbox.sequence_number negative/non-integer on a sealed row")?;
    let chain_hex = row
        .get("chain_hash")
        .and_then(Value::as_str)
        .ok_or("audit_outbox.chain_hash missing on a sealed row")?;
    let chain = chain_hash_from_hex(chain_hex)
        .ok_or("audit_outbox.chain_hash is not a 64-hex BLAKE3 digest")?;
    Ok(Some((chain, seq)))
}

/// Read the pending rows of a partition in deterministic seal order.
async fn read_pending_rows(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
) -> Result<Vec<(String, Value)>, String> {
    let rows = d1
        .query(
            "SELECT id, payload_json FROM audit_outbox \
             WHERE tenant_id = ?1 AND region = ?2 AND emitted_at IS NULL \
             ORDER BY enqueued_at, id",
            &[json!(tenant_id), json!(region)],
        )
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row
            .get("id")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.id missing/non-text")?
            .to_owned();
        let payload_str = row
            .get("payload_json")
            .and_then(Value::as_str)
            .ok_or("audit_outbox.payload_json missing/non-text")?;
        let payload: Value = serde_json::from_str(payload_str)
            .map_err(|e| format!("audit_outbox.payload_json row {id} is not valid JSON: {e}"))?;
        out.push((id, payload));
    }
    Ok(out)
}

/// Write the sealed columns for one row, guarded by `emitted_at IS NULL` so a
/// re-run (or a concurrent drain) never double-seals.
async fn write_seal(d1: &D1HttpClient, row: &SealedRow, now: i64) -> Result<(), String> {
    let seq = i64::try_from(row.sequence_number).map_err(|_| "sequence_number exceeds i64")?;
    d1.query(
        "UPDATE audit_outbox \
         SET sequence_number = ?1, prev_hash = ?2, chain_hash = ?3, \
             canonical_jcs = ?4, chained_at = ?5, emitted_at = ?5 \
         WHERE id = ?6 AND emitted_at IS NULL",
        &[
            json!(seq),
            json!(row.prev_hash_hex),
            json!(row.chain_hash_hex),
            json!(row.canonical_jcs),
            json!(now),
            json!(row.id),
        ],
    )
    .await?;
    Ok(())
}

/// Advance the `audit_chain_head` checkpoint with a compare-and-set on the value
/// we resumed from (single-writer anti-fork). Returns `true` when the head was
/// committed, `false` on drift (a concurrent drain advanced it first).
///
/// Uses `RETURNING` to detect whether the guarded write actually matched: a
/// guarded `UPDATE` (or an `INSERT OR IGNORE`) that hits no row / a conflict
/// returns zero rows ⇒ drift.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit, no shared config struct"
)]
async fn advance_head_cas(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    expected: Option<&HeadCheckpoint>,
    new_head_hex: &str,
    new_seq: u64,
    now: i64,
    // CF-6: the keyed-head columns persisted alongside the advance. `signature`
    // is `None` when no seed is configured (head advanced UNSIGNED / legacy).
    signature: Option<&str>,
    signed_at_ms: Option<i64>,
    signing_key_id: Option<u64>,
) -> Result<bool, String> {
    let new_seq_i = i64::try_from(new_seq).map_err(|_| "next_sequence exceeds i64")?;
    let signing_key_id_i = signing_key_id
        .map(|k| i64::try_from(k).map_err(|_| "signing_key_id exceeds i64"))
        .transpose()?;
    let rows = match expected {
        Some(cp) => {
            let exp_seq =
                i64::try_from(cp.next_sequence).map_err(|_| "next_sequence exceeds i64")?;
            d1.query(
                "UPDATE audit_chain_head \
                 SET head_hash = ?1, next_sequence = ?2, updated_at = ?3, \
                     head_signature = ?8, head_signed_at_ms = ?9, signing_key_id = ?10 \
                 WHERE tenant_id = ?4 AND region = ?5 \
                   AND head_hash = ?6 AND next_sequence = ?7 \
                 RETURNING tenant_id",
                &[
                    json!(new_head_hex),
                    json!(new_seq_i),
                    json!(now),
                    json!(tenant_id),
                    json!(region),
                    json!(cp.head_hex),
                    json!(exp_seq),
                    json!(signature),
                    json!(signed_at_ms),
                    json!(signing_key_id_i),
                ],
            )
            .await?
        }
        None => {
            // No checkpoint row (genesis, or a crashed drain never created one):
            // claim it. A concurrent drain that inserted first wins the PK; our
            // ON CONFLICT DO NOTHING then returns zero rows ⇒ drift.
            d1.query(
                "INSERT INTO audit_chain_head \
                     (tenant_id, region, head_hash, next_sequence, updated_at, \
                      head_signature, head_signed_at_ms, signing_key_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(tenant_id, region) DO NOTHING \
                 RETURNING tenant_id",
                &[
                    json!(tenant_id),
                    json!(region),
                    json!(new_head_hex),
                    json!(new_seq_i),
                    json!(now),
                    json!(signature),
                    json!(signed_at_ms),
                    json!(signing_key_id_i),
                ],
            )
            .await?
        }
    };
    Ok(!rows.is_empty())
}

/// Drain a single `(tenant_id, region)` partition.
#[allow(
    clippy::too_many_arguments,
    reason = "explicit, no shared config struct"
)]
async fn drain_partition(
    d1: &D1HttpClient,
    tenant_id: &str,
    region: &str,
    now: i64,
    signing_seed: Option<&[u8; 32]>,
    signing_key_id: u64,
    trust_unsigned_resume: bool,
) -> Result<PartitionOutcome, String> {
    let checkpoint = read_checkpoint(d1, tenant_id, region).await?;

    // CF-6: verify the keyed head signature of the checkpoint we resume from. A
    // non-NULL signature signed under the current key id that does NOT verify (or
    // a signed head with no seed to verify it) is tampering — refuse to extend the
    // chain from a forged head (fail-CLOSED, SEV-1).
    match check_head_on_resume(
        checkpoint.as_ref(),
        signing_seed,
        signing_key_id,
        tenant_id,
        region,
        trust_unsigned_resume,
    ) {
        HeadResumeCheck::FailClosed => {
            tracing::error!(
                tenant_id = %tenant_id,
                region = %region,
                signing_key_id = signing_key_id,
                severity = "SEV-1",
                tamper_detected = true,
                "audit/drain: audit_chain_head signature does NOT verify — TAMPER DETECTED; \
                 refusing to extend the chain from a forged head (fail-CLOSED, CF-6)"
            );
            return Err(
                "audit_chain_head signature verification failed (tamper-detected): refusing to \
                 extend the chain from a forged head"
                    .to_string(),
            );
        }
        HeadResumeCheck::Verified => {
            tracing::debug!(
                tenant_id = %tenant_id,
                region = %region,
                "audit/drain: resumed-head signature verified (CF-6)"
            );
        }
        HeadResumeCheck::Proceed => {}
    }

    let sealed_tail = read_sealed_tail(d1, tenant_id, region).await?;

    // Resume the builder honestly via resume()/new() (GENESIS when there is no
    // prior state at all). The resume point is crash-safe (sealed-tail authoritative).
    let resume = resolve_resume(
        checkpoint.as_ref().map(|c| (c.head, c.next_sequence)),
        sealed_tail,
    );
    let builder = match resume {
        Some((head, seq)) => HashChainBuilder::resume(head, seq),
        None => HashChainBuilder::new(),
    };

    let rows = read_pending_rows(d1, tenant_id, region).await?;
    if rows.is_empty() {
        return Ok(PartitionOutcome::Empty);
    }

    let (sealed, new_head, new_seq) = seal_rows(*builder.head(), builder.next_sequence(), &rows)?;

    // Seal the rows FIRST (deterministic + idempotent: guarded by emitted_at IS
    // NULL). A crash here leaves correctly-sealed rows the next drain resumes from
    // (sealed-tail authoritative) — never a gap.
    for row in &sealed {
        write_seal(d1, row, now).await?;
    }

    // CF-6: sign the canonical head tuple we are about to commit (when a seed is
    // configured). No seed ⇒ advance UNSIGNED (NULL columns, legacy/tolerated).
    let new_head_hex = new_head.to_hex();
    let (signature, signed_at_ms, key_id_col) = match signing_seed {
        Some(seed) => {
            let sig = sign_head(
                seed,
                signing_key_id,
                tenant_id,
                region,
                &new_head_hex,
                new_seq,
            )?;
            (Some(sig), Some(now), Some(signing_key_id))
        }
        None => (None, None, None),
    };

    // Advance the checkpoint with a compare-and-set on the resumed value.
    let committed = advance_head_cas(
        d1,
        tenant_id,
        region,
        checkpoint.as_ref(),
        &new_head_hex,
        new_seq,
        now,
        signature.as_deref(),
        signed_at_ms,
        key_id_col,
    )
    .await?;

    if committed {
        Ok(PartitionOutcome::Sealed(sealed.len() as u64))
    } else {
        // Drift: a concurrent drain advanced the head. Our sealed rows are
        // byte-identical to that drain's (deterministic), so they are safe; we
        // just do not double-advance the checkpoint.
        Ok(PartitionOutcome::Drift)
    }
}

/// `POST /_internal/audit/drain` — seal every pending partition. Returns a
/// summary `{ ok, partitions_drained, rows_sealed, partitions_drifted,
/// partitions_failed }`. Internal-auth gated; no request body.
async fn handle_drain(State(state): State<AuditDrainState>, headers: HeaderMap) -> Response {
    if !internal_auth_ok(state.internal_auth_key.as_bytes(), &headers) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let partitions = match read_pending_partitions(&state.d1).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(error = %e, "audit/drain: partition scan failed");
            return (StatusCode::INTERNAL_SERVER_ERROR, "drain scan failed").into_response();
        }
    };
    let now = now_ms();
    let mut partitions_drained: u64 = 0;
    let mut rows_sealed: u64 = 0;
    let mut partitions_drifted: u64 = 0;
    let mut partitions_failed: u64 = 0;
    // CF-6: borrow the keyed-head signing seed once for the whole sweep.
    let signing_seed: Option<&[u8; 32]> = state.signing_seed.as_deref().map(|z| &**z);
    for (tenant_id, region) in partitions {
        match drain_partition(
            &state.d1,
            &tenant_id,
            &region,
            now,
            signing_seed,
            state.signing_key_id,
            state.trust_unsigned_resume,
        )
        .await
        {
            Ok(PartitionOutcome::Sealed(n)) => {
                partitions_drained = partitions_drained.saturating_add(1);
                rows_sealed = rows_sealed.saturating_add(n);
            }
            Ok(PartitionOutcome::Drift) => {
                partitions_drifted = partitions_drifted.saturating_add(1);
                tracing::warn!(
                    region = %region,
                    "audit/drain: head drift (concurrent drain) — partition aborted, no fork"
                );
            }
            Ok(PartitionOutcome::Empty) => {}
            Err(e) => {
                partitions_failed = partitions_failed.saturating_add(1);
                tracing::error!(error = %e, region = %region, "audit/drain: partition failed");
            }
        }
    }
    (
        StatusCode::OK,
        Json(json!({
            "ok": true,
            "partitions_drained": partitions_drained,
            "rows_sealed": rows_sealed,
            "partitions_drifted": partitions_drifted,
            "partitions_failed": partitions_failed,
        })),
    )
        .into_response()
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

    fn rows(payloads: &[Value]) -> Vec<(String, Value)> {
        payloads
            .iter()
            .enumerate()
            .map(|(i, p)| (format!("row-{i:04}"), p.clone()))
            .collect()
    }

    #[test]
    fn fresh_partition_seals_from_genesis_monotonic_and_linked() {
        let input = rows(&[json!({"a": 1}), json!({"b": 2}), json!({"c": 3})]);
        let (sealed, head, next_seq) = seal_rows(ChainHash::genesis(), 0, &input).unwrap();

        assert_eq!(sealed.len(), 3);
        // Monotonic sequence from genesis.
        assert_eq!(sealed[0].sequence_number, 0);
        assert_eq!(sealed[1].sequence_number, 1);
        assert_eq!(sealed[2].sequence_number, 2);
        assert_eq!(next_seq, 3);
        // Genesis prev_hash is 64 zeros.
        assert_eq!(sealed[0].prev_hash_hex, "0".repeat(64));
        // Each row's prev_hash == the previous row's chain_hash (the link).
        assert_eq!(sealed[1].prev_hash_hex, sealed[0].chain_hash_hex);
        assert_eq!(sealed[2].prev_hash_hex, sealed[1].chain_hash_hex);
        // The returned head is the last link.
        assert_eq!(head.to_hex(), sealed[2].chain_hash_hex);
        // Distinct links.
        assert_ne!(sealed[0].chain_hash_hex, sealed[1].chain_hash_hex);
        assert_ne!(sealed[1].chain_hash_hex, sealed[2].chain_hash_hex);
    }

    #[test]
    fn canonical_jcs_stored_is_exactly_what_was_hashed() {
        let payload = json!({"z": 1, "a": 2, "m": {"y": 9, "b": 8}});
        let input = rows(std::slice::from_ref(&payload));
        let (sealed, _, _) = seal_rows(ChainHash::genesis(), 0, &input).unwrap();
        let s = &sealed[0];

        // The stored canonical_jcs is byte-for-byte the RFC-8785 JCS of the payload.
        let expected_jcs = serde_jcs::to_vec(&payload).unwrap();
        assert_eq!(s.canonical_jcs.as_bytes(), expected_jcs.as_slice());

        // And the chain_hash is exactly BLAKE3(prev || canonical_jcs) over THOSE bytes.
        let recomputed =
            link_chain_hash_from_canonical(&ChainHash::genesis(), s.canonical_jcs.as_bytes());
        assert_eq!(recomputed.to_hex(), s.chain_hash_hex);
    }

    #[test]
    fn rerun_is_idempotent_and_deterministic_no_fork() {
        // Two concurrent drains resuming from the SAME state over the SAME rows
        // compute byte-identical seals — so overlapping row writes are identical,
        // never a fork. (The DB-level idempotency is the `emitted_at IS NULL`
        // guard on the UPDATE; the determinism proven here is its foundation.)
        let input = rows(&[json!({"e": "x"}), json!({"e": "y"})]);
        let a = seal_rows(ChainHash::genesis(), 0, &input).unwrap();
        let b = seal_rows(ChainHash::genesis(), 0, &input).unwrap();
        assert_eq!(a.0, b.0);
        assert_eq!(a.1.to_hex(), b.1.to_hex());
        assert_eq!(a.2, b.2);
    }

    #[test]
    fn split_drain_does_not_fork_the_sequence() {
        // Drain A sees [r0]; a new row r1 arrives; Drain B (resuming from the
        // sealed tail after A sealed r0) sees [r1]. The split must produce the
        // SAME chain as a single drain over [r0, r1].
        let r0 = json!({"n": 0});
        let r1 = json!({"n": 1});

        // Single drain over both.
        let combined =
            seal_rows(ChainHash::genesis(), 0, &rows(&[r0.clone(), r1.clone()])).unwrap();

        // Drain A: just r0 from genesis.
        let a = seal_rows(ChainHash::genesis(), 0, &rows(std::slice::from_ref(&r0))).unwrap();
        assert_eq!(a.0[0], combined.0[0]);

        // Drain B resumes from the sealed tail A produced (head=a.1, seq=a.2),
        // then seals r1.
        let b = seal_rows(a.1, a.2, &rows(std::slice::from_ref(&r1))).unwrap();
        // r1's sealed link is identical to the single-drain result.
        assert_eq!(b.0[0].sequence_number, combined.0[1].sequence_number);
        assert_eq!(b.0[0].prev_hash_hex, combined.0[1].prev_hash_hex);
        assert_eq!(b.0[0].chain_hash_hex, combined.0[1].chain_hash_hex);
        assert_eq!(b.1.to_hex(), combined.1.to_hex());
    }

    #[test]
    fn resolve_resume_genesis_when_no_state() {
        assert_eq!(resolve_resume(None, None), None);
    }

    #[test]
    fn resolve_resume_uses_checkpoint_when_no_sealed_tail() {
        let h = ChainHash([0x11; 32]);
        assert_eq!(resolve_resume(Some((h, 5)), None), Some((h, 5)));
    }

    #[test]
    fn resolve_resume_prefers_sealed_tail_when_ahead_of_checkpoint() {
        // Crash/drift recovery: the checkpoint lagged behind the durable sealed
        // rows (a prior drain crashed after sealing but before advancing the head).
        // The sealed tail is authoritative.
        let cp = ChainHash([0x11; 32]);
        let tail = ChainHash([0x22; 32]);
        // Checkpoint says next_sequence = 3; sealed tail row has seq = 4 → tail
        // next = 5 > 3 → resume from the tail.
        assert_eq!(
            resolve_resume(Some((cp, 3)), Some((tail, 4))),
            Some((tail, 5))
        );
    }

    #[test]
    fn resolve_resume_keeps_checkpoint_when_tail_not_ahead() {
        let cp = ChainHash([0x11; 32]);
        let tail = ChainHash([0x22; 32]);
        // Checkpoint next_sequence = 5; sealed tail seq = 4 → tail next = 5, NOT
        // strictly greater → keep the (faster) checkpoint.
        assert_eq!(
            resolve_resume(Some((cp, 5)), Some((tail, 4))),
            Some((cp, 5))
        );
    }

    #[test]
    fn builder_resume_seeds_start_state() {
        // Genesis path: new().
        let g = HashChainBuilder::new();
        assert_eq!(g.head().as_bytes(), &[0u8; 32]);
        assert_eq!(g.next_sequence(), 0);
        // Resume path: resume(head, seq).
        let h = ChainHash([0xAB; 32]);
        let r = HashChainBuilder::resume(h, 7);
        assert_eq!(r.head(), &h);
        assert_eq!(r.next_sequence(), 7);
    }

    #[test]
    fn empty_partition_seals_nothing_and_leaves_head_unchanged() {
        // No pending rows → no seals, head + sequence pass through unchanged.
        let h = ChainHash([0x42; 32]);
        let (sealed, head, next_seq) = seal_rows(h, 9, &[]).unwrap();
        assert!(sealed.is_empty());
        assert_eq!(head, h);
        assert_eq!(next_seq, 9);
    }

    #[test]
    fn chain_hash_from_hex_round_trips_and_rejects_bad() {
        let h = ChainHash([0x5A; 32]);
        assert_eq!(chain_hash_from_hex(&h.to_hex()), Some(h));
        assert_eq!(chain_hash_from_hex("zz"), None);
        assert_eq!(chain_hash_from_hex(&"ab".repeat(31)), None); // 62 hex chars
    }

    // ---- CF-6: keyed (Ed25519-signed) chain head ----

    const SEED_A: [u8; 32] = [0x11; 32];
    const SEED_B: [u8; 32] = [0x22; 32];
    const TENANT: &str = "00000000-0000-7000-8000-00000000aaaa";
    const REGION: &str = "weur";
    const HEAD_HEX: &str = "ab"; // expanded to 64 hex below via repeat
    const KID: u64 = 1;

    fn head_hex() -> String {
        HEAD_HEX.repeat(32) // 64 lowercase hex chars
    }

    /// Build a checkpoint whose stored signature is a GENUINE signature over its
    /// own (tenant, region, head_hex, next_sequence) tuple under `seed`/`kid`.
    fn signed_checkpoint(
        seed: &[u8; 32],
        kid: u64,
        head_hex: &str,
        next_sequence: u64,
    ) -> HeadCheckpoint {
        let sig = sign_head(seed, kid, TENANT, REGION, head_hex, next_sequence).unwrap();
        HeadCheckpoint {
            head: chain_hash_from_hex(head_hex).unwrap(),
            head_hex: head_hex.to_string(),
            next_sequence,
            head_signature: Some(sig),
            signing_key_id: Some(kid),
        }
    }

    #[test]
    fn canonical_head_bytes_is_deterministic_and_jcs_key_ordered() {
        let a = canonical_head_bytes(TENANT, REGION, &head_hex(), 7).unwrap();
        let b = canonical_head_bytes(TENANT, REGION, &head_hex(), 7).unwrap();
        assert_eq!(a, b, "canonical head bytes must be deterministic");
        let s = String::from_utf8(a).unwrap();
        // RFC-8785 JCS sorts keys lexicographically: head_hash < next_sequence <
        // region < tenant_id.
        let pos = |k: &str| s.find(k).unwrap();
        assert!(pos("head_hash") < pos("next_sequence"));
        assert!(pos("next_sequence") < pos("region"));
        assert!(pos("region") < pos("tenant_id"));
    }

    /// A genuine advance signs a head whose signature verifies (round-trip).
    #[test]
    fn genuine_head_signature_verifies() {
        let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
        assert!(
            verify_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42, &sig),
            "a genuine head signature must verify"
        );
    }

    /// A tampered head_hash with the ORIGINAL signature must NOT verify (the core
    /// CF-6 tamper-detection: a D1 writer who rewrites the head is caught).
    #[test]
    fn tampered_head_hash_fails_verification() {
        let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
        let forged = "cd".repeat(32);
        assert_ne!(forged, head_hex());
        assert!(
            !verify_head(&SEED_A, KID, TENANT, REGION, &forged, 42, &sig),
            "a rewritten head_hash must fail verification"
        );
    }

    /// Tampering with any other bound field (sequence / region / tenant) also
    /// fails — the whole tuple is signed.
    #[test]
    fn tampered_tuple_fields_fail_verification() {
        let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
        // Rewound sequence.
        assert!(!verify_head(
            &SEED_A,
            KID,
            TENANT,
            REGION,
            &head_hex(),
            41,
            &sig
        ));
        // Different region.
        assert!(!verify_head(
            &SEED_A,
            KID,
            TENANT,
            "enam",
            &head_hex(),
            42,
            &sig
        ));
        // Different tenant.
        assert!(!verify_head(
            &SEED_A,
            KID,
            "00000000-0000-7000-8000-00000000bbbb",
            REGION,
            &head_hex(),
            42,
            &sig
        ));
    }

    /// A different seed (forged signer) must NOT verify — the head cannot be
    /// re-signed without the write-only seed.
    #[test]
    fn wrong_seed_fails_verification() {
        let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 42).unwrap();
        assert!(!verify_head(
            &SEED_B,
            KID,
            TENANT,
            REGION,
            &head_hex(),
            42,
            &sig
        ));
    }

    /// Malformed signature material is rejected (treated as tamper / fail-CLOSED).
    #[test]
    fn malformed_signature_fails_verification() {
        assert!(!verify_head(
            &SEED_A,
            KID,
            TENANT,
            REGION,
            &head_hex(),
            42,
            "not-base64!!!"
        ));
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 32]);
        assert!(!verify_head(
            &SEED_A,
            KID,
            TENANT,
            REGION,
            &head_hex(),
            42,
            &short
        ));
    }

    /// Resume re-verifies: a genuine signed checkpoint under the current key id →
    /// Verified.
    #[test]
    fn resume_verifies_genuine_signed_head() {
        let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
        assert_eq!(
            check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
            HeadResumeCheck::Verified
        );
    }

    /// Resume catches tampering: a checkpoint whose head_hex was rewritten after
    /// signing → FailClosed (the drain refuses to extend). This is ALWAYS tamper —
    /// the migration escape never softens a verify failure under the current key.
    #[test]
    fn resume_fails_closed_on_tampered_head() {
        let mut cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
        // Insider rewrites the head (keeping the original signature).
        cp.head_hex = "cd".repeat(32);
        cp.head = chain_hash_from_hex(&cp.head_hex).unwrap();
        assert_eq!(
            check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
            HeadResumeCheck::FailClosed
        );
        // Even with the migration escape ON, a current-key verify failure is tamper.
        assert_eq!(
            check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, true),
            HeadResumeCheck::FailClosed
        );
    }

    /// THE LAUNDERING EXPLOIT (backend-audit §3), now CLOSED: an insider with D1
    /// write rewrites the sealed rows + head and STRIPS the signature to NULL (they
    /// lack the write-only seed, so they cannot re-sign). With the signing regime
    /// active, a NULL signature is UNVERIFIABLE → FailClosed by default (was
    /// silently `Proceed`, which let the honest drain re-sign the forged head).
    #[test]
    fn resume_fails_closed_on_stripped_signature() {
        let cp = HeadCheckpoint {
            head: chain_hash_from_hex(&head_hex()).unwrap(),
            head_hex: head_hex(),
            next_sequence: 9,
            head_signature: None,
            signing_key_id: None,
        };
        assert_eq!(
            check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
            HeadResumeCheck::FailClosed,
            "a NULL signature under an active signing regime must be tamper (fail-CLOSED)"
        );
    }

    /// A NULL / foreign-key-id head is tolerated ONLY under the explicit operator
    /// migration escape (`trust_unsigned_resume = true`) — the pre-0080 legacy
    /// bootstrap + coordinated seed-rotation re-sign path.
    #[test]
    fn resume_tolerates_unsigned_only_under_migration_escape() {
        let legacy = HeadCheckpoint {
            head: chain_hash_from_hex(&head_hex()).unwrap(),
            head_hex: head_hex(),
            next_sequence: 9,
            head_signature: None,
            signing_key_id: None,
        };
        assert_eq!(
            check_head_on_resume(Some(&legacy), Some(&SEED_A), KID, TENANT, REGION, true),
            HeadResumeCheck::Proceed
        );
        // Foreign key id (rotation) also tolerated only under the escape.
        let rotated = signed_checkpoint(&SEED_A, 1, &head_hex(), 9);
        assert_eq!(
            check_head_on_resume(Some(&rotated), Some(&SEED_B), 2, TENANT, REGION, true),
            HeadResumeCheck::Proceed
        );
    }

    /// The legacy head is RE-SIGNED on the next advance: signing it with the
    /// configured seed yields a signature that verifies.
    #[test]
    fn legacy_head_gets_signed_on_next_advance() {
        // Simulate the advance signing the new head computed from the legacy state.
        let sig = sign_head(&SEED_A, KID, TENANT, REGION, &head_hex(), 10).unwrap();
        assert!(verify_head(
            &SEED_A,
            KID,
            TENANT,
            REGION,
            &head_hex(),
            10,
            &sig
        ));
        // And a checkpoint carrying that fresh signature now Verifies on resume.
        let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 10);
        assert_eq!(
            check_head_on_resume(Some(&cp), Some(&SEED_A), KID, TENANT, REGION, false),
            HeadResumeCheck::Verified
        );
    }

    /// A head signed under a DIFFERENT key id (seed/key rotation) is TAMPER by
    /// default — it cannot be verified with the current seed, so a foreign key id is
    /// as much a laundering vector as a stripped signature. Legit rotation is the
    /// explicit migration escape (see `resume_tolerates_unsigned_only_under_migration_escape`).
    #[test]
    fn resume_fails_closed_on_foreign_key_id() {
        let cp = signed_checkpoint(&SEED_A, 1, &head_hex(), 9);
        // Current deployment uses key id 2 + a different seed — cannot verify.
        assert_eq!(
            check_head_on_resume(Some(&cp), Some(&SEED_B), 2, TENANT, REGION, false),
            HeadResumeCheck::FailClosed
        );
    }

    /// A signed head with NO seed configured to verify it → fail-CLOSED (cannot
    /// prove integrity of a head that claims to be signed).
    #[test]
    fn resume_fails_closed_when_signed_but_no_seed() {
        let cp = signed_checkpoint(&SEED_A, KID, &head_hex(), 9);
        assert_eq!(
            check_head_on_resume(Some(&cp), None, KID, TENANT, REGION, false),
            HeadResumeCheck::FailClosed
        );
    }

    /// No signing regime (no seed) + a never-signed head → Proceed (dev/CI).
    #[test]
    fn resume_proceeds_unsigned_head_no_seed() {
        let cp = HeadCheckpoint {
            head: chain_hash_from_hex(&head_hex()).unwrap(),
            head_hex: head_hex(),
            next_sequence: 9,
            head_signature: None,
            signing_key_id: None,
        };
        assert_eq!(
            check_head_on_resume(Some(&cp), None, KID, TENANT, REGION, false),
            HeadResumeCheck::Proceed
        );
    }

    /// No checkpoint (genesis) → proceed.
    #[test]
    fn resume_proceeds_with_no_checkpoint() {
        assert_eq!(
            check_head_on_resume(None, Some(&SEED_A), KID, TENANT, REGION, false),
            HeadResumeCheck::Proceed
        );
    }

    /// `region_for_key` is irrelevant to the keypair: a macro region with no enum
    /// (e.g. `apac`) still signs + verifies (the region is bound in the tuple,
    /// not the key).
    #[test]
    fn macro_region_still_signs_and_verifies() {
        let sig = sign_head(&SEED_A, KID, TENANT, "apac", &head_hex(), 5).unwrap();
        assert!(verify_head(
            &SEED_A,
            KID,
            TENANT,
            "apac",
            &head_hex(),
            5,
            &sig
        ));
        // But it is region-bound: a different region must fail.
        assert!(!verify_head(
            &SEED_A,
            KID,
            TENANT,
            "afr",
            &head_hex(),
            5,
            &sig
        ));
    }
}
