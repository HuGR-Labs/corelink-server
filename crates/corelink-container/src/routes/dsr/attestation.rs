//! Signed + served erasure attestation on `VerifiedComplete` (WI-S11-008
//! Wave 1, G3 — Artifact 1 / OKF brutal-review H1 closure).
//!
//! When the 24h verify sweep lands [`ErasureDecision::VerifiedComplete`] for a
//! DSR, [`sign_and_persist`] produces a REAL Ed25519-signed certificate of
//! erasure and persists it so the public verifier
//! (`GET /v1/public/attestation/{request_id}` +
//! `GET /v1/public/keys/erasure/{region}.pub`) can serve it. A customer /
//! auditor verifies the signature offline against the per-region public key.
//!
//! ## What is signed (binds REAL per-backend verification — finding #1)
//!
//! CoreLink's launch erasure is a real D1 / R2 / Stripe delete-set (NOT a BYOK
//! KMS crypto-erase), so the attestation records the *mechanism truthfully*:
//! `kms_provider = "corelink_d1r2_erase"` and `kms_key_id = "dsr:{dsr_id}"`.
//!
//! The signed `evidence_hash` is bound to the ACTUAL per-backend verification
//! results carried by the orchestrator's [`ErasureDecision::VerifiedComplete`]
//! decision (`Vec<BackendCompletion>`), NOT a synthetic self-referential
//! constant. [`verified_evidence_segments`] emits one segment per canonical
//! backend = `"{backend}:{outcome}:{hex(verification_hash)}"`, hashed via
//! [`EvidenceBundle::validated_hash`] (so the "empty mandatory field" guard is
//! load-bearing — an empty/short completion set is a hard refuse, not a vacuous
//! pass).
//!
//! ## Fail-CLOSED signing gate (finding #1)
//!
//! The signer emits a proof ONLY when it is bound to real, complete deletion
//! evidence. It REFUSES (no row written; the erasure is still done + audited +
//! ledgered) when ANY of:
//!
//! - the completion set is not the canonical 12 (a partial/short decision);
//! - any backend's outcome is not successful; or
//! - any backend's `verification_hash` is not [`CANONICAL_EMPTY_TENANT_HASH`]
//!   (the re-fingerprint did not prove the backend empty / fully-redacted —
//!   includes the Stripe re-verify mismatch and any unverified arm).
//!
//! (The Stripe arm is a REAL re-fingerprint — see `adapter_stripe.rs` — so a
//! `CANONICAL_EMPTY_TENANT_HASH` from Stripe means a genuine live-verified
//! redaction, not a hardcoded no-op.)
//!
//! ## Region key — no silent mis-attribution (finding #1)
//!
//! The per-region signing key is reproduced deterministically from a write-only
//! secret seed ([`ErasureSigningKey::from_seed`]). At `VerifiedComplete` the
//! tenant row is normally already deleted, so there is no authoritative
//! per-tenant region source in this layer. We MUST NOT silently default to the
//! deployment home region (that would let an EU tenant's erasure be signed with
//! the wrong key). The env region (`ERASURE_ATTESTATION_REGION`) is honoured
//! ONLY when the operator has EXPLICITLY asserted this deployment is
//! single-region (`ERASURE_ATTESTATION_SINGLE_REGION` truthy) — an audited
//! operator decision, NOT a silent default. Otherwise the region is treated as
//! unavailable and the attestation is withheld (fail-CLOSED).
//!
//! ## STRICT all-or-nothing ordering (the anti-theater invariant)
//!
//! A D1 index row that claims to be a signed certificate but isn't verifiable
//! is the exact "theater" closed here (a forgeable, unsigned digest pointing at
//! a non-existent R2 object). So [`persist_signed_attestation`] is strictly
//! ordered and fails CLOSED at every step:
//!
//! 1. **R2 PUT FIRST** — write the signed bundle JSON
//!    (`{payload, signature_ed25519, canonical_payload_jcs}`) to
//!    `erasure_attestations/{request_id}.json` in the region's audit bucket.
//! 2. **Only on R2 success**, upsert the `erasure_public_keys` row — the served
//!    certificate MUST have a verifiable public key BEFORE the index row that
//!    flips the verifier to "served" exists (a row without a pubkey would be an
//!    unverifiable served cert = theater; so the pubkey is persisted first).
//! 3. **Only then**, INSERT the `erasure_attestations` index row carrying BOTH
//!    `signature_ed25519` AND `canonical_payload_jcs` (plus `r2_key`,
//!    `evidence_hash`, …). The verifier treats a row with either NULL as "not a
//!    verifiable signed attestation".
//!
//! If ANY step (region / R2 client / seed / sign / R2 PUT / D1) fails we log and
//! return WITHOUT persisting the index row: never a D1 row missing the
//! signature/canonical, and never an index row before the R2 object exists (no
//! dangling `r2_key`). Persistence is `INSERT OR IGNORE` keyed on
//! `request_id == dsr_id`, so a re-sweep of the same DSR never double-writes and
//! retries any transient gap.
//!
//! ## Non-blocking
//!
//! The attestation is an *additional evidence artifact on top of* a completed +
//! audited + ledgered erasure; it MUST NOT fail the verify response (the
//! erasure already happened and is recorded in `audit_outbox`).

use std::sync::Arc;

use serde_json::{json, Value};
use zeroize::Zeroizing;

use corelink_erasure_attestation::{
    ErasureAttestationPayload, ErasureAttestationSigner, ErasureSigningKey, EvidenceBundle, Region,
};
use corelink_privacy_erasure_worker::backends::CANONICAL_EMPTY_TENANT_HASH;
use corelink_privacy_erasure_worker::event::{BackendCompletion, BACKEND_COUNT};

use super::d1util::{clamp_ms, d1_query_blocking};
use crate::storage::d1_http::D1HttpClient;
use crate::storage::r2_s3::R2S3Client;

/// `kms_provider` recorded in the attestation — truthful: CoreLink's launch
/// erasure is a D1/R2/Stripe delete-set, not a BYOK KMS crypto-erase.
const ERASE_MECHANISM: &str = "corelink_d1r2_erase";

/// Load the 32-byte per-region attestation signing seed from
/// `ERASURE_ATTESTATION_SEED_HEX` (64 hex chars). `None` when unset/malformed
/// → caller skips signing (fail-CLOSED). Held in [`Zeroizing`] so the secret is
/// wiped from memory after the [`ErasureSigningKey`] is constructed.
fn load_seed() -> Option<Zeroizing<[u8; 32]>> {
    let hex_str = std::env::var("ERASURE_ATTESTATION_SEED_HEX").ok()?;
    let hex_str = hex_str.trim();
    if hex_str.len() != 64 {
        return None;
    }
    let mut bytes = Zeroizing::new([0u8; 32]);
    hex::decode_to_slice(hex_str, bytes.as_mut()).ok()?;
    Some(bytes)
}

/// Monotonic signing-key id from `ERASURE_ATTESTATION_KEY_ID` (defaults to 1 —
/// the launch key; the S-13 rotation worker bumps it on rotation).
fn key_id() -> u64 {
    std::env::var("ERASURE_ATTESTATION_KEY_ID")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(1)
}

/// Whether the operator has EXPLICITLY asserted this deployment is
/// single-region via `ERASURE_ATTESTATION_SINGLE_REGION` (truthy). This makes
/// `ERASURE_ATTESTATION_REGION` an audited operator decision rather than a
/// silent default for the (live) case where the tenant row is already deleted.
fn single_region_asserted() -> bool {
    std::env::var("ERASURE_ATTESTATION_SINGLE_REGION")
        .map(|v| {
            matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Resolve the attestation [`Region`] WITHOUT silent mis-attribution
/// (finding #1, region).
///
/// The env region (`ERASURE_ATTESTATION_REGION`) is honoured ONLY when the
/// operator has EXPLICITLY asserted single-region
/// (`ERASURE_ATTESTATION_SINGLE_REGION` truthy) — the only thing that makes
/// attributing the erasure to that region correct once the tenant row is gone.
/// Returns `None` (caller withholds the attestation, fail-CLOSED) otherwise, so
/// the same R2 audit bucket the [`R2S3Client`] is bound to (also region-derived
/// from the SAME env) can never disagree with the region in the signed payload.
fn resolve_region_from_env() -> Option<Region> {
    if !single_region_asserted() {
        return None;
    }
    std::env::var("ERASURE_ATTESTATION_REGION")
        .ok()
        .and_then(|s| Region::parse(s.trim()))
}

/// Build the evidence-digest segments from the REAL per-backend
/// verification outcomes carried by the `VerifiedComplete` decision — or `None`
/// (refuse to sign) unless EVERY canonical backend is provably verified-empty.
///
/// Fail-CLOSED conditions (each ⇒ `None`, attestation withheld):
/// - the completion set is not the canonical 12 (a partial/short decision);
/// - any backend's outcome is not successful; or
/// - any backend's `verification_hash` is not [`CANONICAL_EMPTY_TENANT_HASH`]
///   (the re-fingerprint did NOT prove the backend empty / fully-redacted —
///   includes a Stripe re-verify mismatch or any unverified arm).
///
/// On success returns one canonical segment per backend
/// (`"{backend}:{outcome}:{hex(verification_hash)}"`) in completion order, to be
/// hashed into the signed `evidence_hash` so the proof binds the actual
/// per-backend evidence.
fn verified_evidence_segments(completions: &[BackendCompletion]) -> Option<Vec<String>> {
    if completions.len() != BACKEND_COUNT {
        return None;
    }
    let mut segments = Vec::with_capacity(completions.len());
    for c in completions {
        // A complete proof requires every backend to be a SUCCESSFUL outcome
        // AND to have re-fingerprinted to the canonical verified-empty sentinel.
        if !c.outcome.is_successful() {
            return None;
        }
        if c.verification_hash != CANONICAL_EMPTY_TENANT_HASH {
            return None;
        }
        segments.push(format!(
            "{backend}:{outcome}:{vhash}",
            backend = c.backend,
            outcome = c.outcome.as_str(),
            vhash = hex::encode(c.verification_hash),
        ));
    }
    Some(segments)
}

/// Persistence sinks for a signed attestation (R2 object store + D1 index),
/// abstracted so the strict R2-FIRST → D1 ordering is unit-testable with
/// in-memory fakes. The live impl ([`LiveAttestationSinks`]) wraps the real
/// network clients; no production code path constructs anything else.
trait AttestationSinks {
    /// PUT the signed bundle JSON under `key` in the region's R2 audit bucket.
    fn put_audit_object(&self, key: &str, bytes: Vec<u8>) -> Result<(), String>;
    /// Execute one parameterised D1 statement (the index / public-key upsert).
    fn exec_d1(&self, sql: &str, params: Vec<Value>) -> Result<(), String>;
}

/// Live sinks: the regional R2 audit-bucket client + the D1 HTTP client.
struct LiveAttestationSinks<'a> {
    r2: &'a Arc<R2S3Client>,
    d1: &'a Arc<D1HttpClient>,
}

impl AttestationSinks for LiveAttestationSinks<'_> {
    fn put_audit_object(&self, key: &str, bytes: Vec<u8>) -> Result<(), String> {
        // Same async→sync bridge as `d1util::d1_query_blocking`: we are always
        // called from the verify handler running on the multi-thread runtime.
        let handle = tokio::runtime::Handle::current();
        tokio::task::block_in_place(|| handle.block_on(self.r2.put(key, bytes)))
    }

    fn exec_d1(&self, sql: &str, params: Vec<Value>) -> Result<(), String> {
        d1_query_blocking(self.d1, sql, params).map(|_| ())
    }
}

/// Sign + persist an erasure attestation for a `VerifiedComplete` DSR.
///
/// Non-blocking but fail-CLOSED at every step: returns silently (NOTHING
/// persisted) when the region cannot be trustworthily resolved, the R2 audit
/// client is absent, the seed secret is unset/malformed, the per-backend
/// evidence is incomplete/unverified, signing fails, or the R2 PUT / D1 writes
/// fail. The caller invokes this ONLY on the `VerifiedComplete` arm and passes
/// that decision's real per-backend `completions`.
pub(super) fn sign_and_persist(
    d1: &Arc<D1HttpClient>,
    r2_audit: Option<&Arc<R2S3Client>>,
    dsr_id: &str,
    tenant_id: &str,
    verified_at_ms: u64,
    completions: &[BackendCompletion],
) {
    // 1. Region — env + EXPLICIT single-region assertion (never mis-attribute).
    let Some(region) = resolve_region_from_env() else {
        tracing::warn!(
            dsr_id = %dsr_id,
            "dsr/verify: no trustworthy attestation region (no explicit \
             ERASURE_ATTESTATION_SINGLE_REGION assertion / unparseable region) \
             — NOT signing attestation (fail-CLOSED)"
        );
        return;
    };
    // 2. R2 audit client — R2 is the authoritative store; without it we cannot
    //    write the object the index would point at, so we MUST NOT persist.
    let Some(r2) = r2_audit else {
        tracing::warn!(
            dsr_id = %dsr_id,
            "dsr/verify: no R2 audit-bucket client configured — NOT signing \
             attestation (fail-CLOSED; never a dangling r2_key)"
        );
        return;
    };
    // 3. Seed + key id.
    let Some(seed) = load_seed() else {
        tracing::debug!(
            dsr_id = %dsr_id,
            "dsr/verify: ERASURE_ATTESTATION_SEED_HEX unset/malformed — skipping attestation"
        );
        return;
    };
    let kid = key_id();

    let sinks = LiveAttestationSinks { r2, d1 };
    persist_signed_attestation(
        &sinks,
        region,
        &seed,
        kid,
        dsr_id,
        tenant_id,
        verified_at_ms,
        completions,
    );
}

/// Core sign + STRICT-ordered persist, generic over [`AttestationSinks`] so the
/// R2-FIRST → pubkey → index ordering and its fail-CLOSED behaviour are covered
/// by in-memory tests. See the module-level "STRICT all-or-nothing ordering".
#[allow(clippy::too_many_arguments, reason = "explicit, no shared config struct")]
fn persist_signed_attestation<S: AttestationSinks>(
    sinks: &S,
    region: Region,
    seed: &[u8; 32],
    kid: u64,
    dsr_id: &str,
    tenant_id: &str,
    verified_at_ms: u64,
    completions: &[BackendCompletion],
) {
    // Fail-CLOSED evidence gate (finding #1): bind the proof to the REAL
    // per-backend verification results.
    let Some(evidence_segments) = verified_evidence_segments(completions) else {
        tracing::warn!(
            dsr_id = %dsr_id,
            backend_completions = completions.len(),
            "dsr/verify: per-backend verification evidence incomplete/unverified \
             — NOT signing attestation (fail-CLOSED)"
        );
        return;
    };

    // Deterministic key from the seed (stable public key across restarts).
    let signing_key = ErasureSigningKey::from_seed(kid, region, verified_at_ms, 0, *seed);
    let public_key = signing_key.public_key();
    let signer = ErasureAttestationSigner::new(signing_key);

    // Evidence bundle binds the REAL per-backend results; `validated_hash`
    // makes the "empty mandatory field" guard load-bearing.
    let kms_key_id = format!("dsr:{dsr_id}");
    let bundle = EvidenceBundle {
        audit_chain_segment_ids: evidence_segments,
        kms_destroy_ts: verified_at_ms,
        kms_key_id: kms_key_id.clone(),
        tenant_id: tenant_id.to_string(),
    };
    let evidence_hash = match bundle.validated_hash() {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(
                dsr_id = %dsr_id,
                error = %e,
                "dsr/verify: evidence bundle failed validation — NOT signing (fail-CLOSED)"
            );
            return;
        }
    };
    let payload = ErasureAttestationPayload {
        tenant_id: tenant_id.to_string(),
        request_id: dsr_id.to_string(),
        destroyed_ts: verified_at_ms,
        kms_provider: ERASE_MECHANISM.to_string(),
        kms_key_id,
        evidence_hash,
        region,
        attestation_key_id: kid,
    };

    let attestation = match signer.sign(payload) {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(dsr_id = %dsr_id, error = %e, "dsr/verify: attestation sign failed");
            return;
        }
    };

    // The authoritative R2 audit object (signed bundle JSON). The verifier
    // re-checks `signature_ed25519` against `canonical_payload_jcs`.
    let bundle_json = match serde_json::to_vec(&attestation) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(dsr_id = %dsr_id, error = %e, "dsr/verify: attestation serialize failed");
            return;
        }
    };
    // Object key WITHIN the region's audit bucket (the client is bound to that
    // bucket); the D1 `r2_key` column stores the bucket-qualified path (0032).
    let object_key = format!("erasure_attestations/{dsr_id}.json");
    let r2_key = format!("{}/{object_key}", region.audit_bucket());

    // (1) R2 PUT FIRST — the index row must never precede the object.
    if let Err(e) = sinks.put_audit_object(&object_key, bundle_json) {
        tracing::error!(
            dsr_id = %dsr_id,
            error = %e,
            "dsr/verify: R2 audit-object PUT failed — NOT persisting index (fail-CLOSED)"
        );
        return;
    }

    // (2) Public key BEFORE the index row: a served certificate must have a
    // verifiable public key before the row that flips the verifier to "served"
    // exists. Idempotent on (key_id, region). A failure here aborts WITHOUT the
    // index row (an unverifiable served cert would be theater).
    let pub_sql = "INSERT OR IGNORE INTO erasure_public_keys \
         (key_id, region, state, created_at_ms, overlap_until_ms, public_key_pem) \
         VALUES (?1, ?2, 'active', ?3, ?4, ?5)";
    if let Err(e) = sinks.exec_d1(
        pub_sql,
        vec![
            json!(kid),
            json!(region.as_str()),
            json!(clamp_ms(public_key.created_at_ms)),
            json!(clamp_ms(public_key.overlap_until_ms)),
            json!(public_key.pem),
        ],
    ) {
        tracing::error!(
            dsr_id = %dsr_id,
            error = %e,
            "dsr/verify: erasure_public_keys upsert failed — NOT persisting index (fail-CLOSED)"
        );
        return;
    }

    // (3) Index row — carries BOTH signature_ed25519 AND canonical_payload_jcs
    // (0079) so the row is independently verifiable; the verifier 404s a row
    // with either NULL. INSERT OR IGNORE keyed on request_id (re-sweep-safe).
    let att_sql = "INSERT OR IGNORE INTO erasure_attestations \
         (request_id, tenant_id, region, attestation_key_id, r2_key, signed_at_ms, \
          kms_provider, kms_key_id, evidence_hash, signature_ed25519, canonical_payload_jcs) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)";
    if let Err(e) = sinks.exec_d1(
        att_sql,
        vec![
            json!(attestation.payload.request_id),
            json!(attestation.payload.tenant_id),
            json!(region.as_str()),
            json!(kid),
            json!(r2_key),
            json!(clamp_ms(verified_at_ms)),
            json!(attestation.payload.kms_provider),
            json!(attestation.payload.kms_key_id),
            json!(attestation.payload.evidence_hash),
            json!(attestation.signature_ed25519),
            json!(attestation.canonical_payload_jcs),
        ],
    ) {
        tracing::error!(
            dsr_id = %dsr_id,
            error = %e,
            "dsr/verify: erasure_attestations index INSERT failed (R2 object + pubkey already \
             durable; re-sweep retries) — NOT confirming attestation"
        );
        return;
    }

    tracing::info!(
        dsr_id = %dsr_id,
        region = %region,
        attestation_key_id = kid,
        "dsr/verify: signed + persisted erasure attestation (R2→pubkey→index, VerifiedComplete)"
    );
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "tests"
)]
mod tests {
    use std::cell::RefCell;

    use serde_json::Value;

    use corelink_erasure_attestation::{
        verify_attestation_signature, ErasureAttestation, ErasureAttestationPayload,
        ErasureAttestationSigner, ErasureSigningKey, EvidenceBundle, Region,
    };

    use super::{persist_signed_attestation, AttestationSinks, ERASE_MECHANISM};

    /// Crypto-contract coverage for the `corelink_erasure_attestation` crate: a
    /// payload signed with a seed-derived key verifies against the public key
    /// derived from the SAME seed (the contract the persisted public-key row +
    /// the public verifier rely on).
    #[test]
    fn signed_attestation_verifies_against_derived_public_key() {
        let seed = [9u8; 32];
        let dsr_id = "00000000-0000-7000-8000-000000000abc";
        let tenant_id = "00000000-0000-7000-8000-000000000def";
        let verified_at_ms = 1_700_000_000_000u64;
        let region = Region::Weur;
        let kid = 1u64;

        let signing_key = ErasureSigningKey::from_seed(kid, region, verified_at_ms, 0, seed);
        let public_key = signing_key.public_key();
        let signer = ErasureAttestationSigner::new(signing_key);

        let kms_key_id = format!("dsr:{dsr_id}");
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec![format!("dsr.erasure.completed.v1:{dsr_id}")],
            kms_destroy_ts: verified_at_ms,
            kms_key_id: kms_key_id.clone(),
            tenant_id: tenant_id.to_string(),
        };
        let payload = ErasureAttestationPayload {
            tenant_id: tenant_id.to_string(),
            request_id: dsr_id.to_string(),
            destroyed_ts: verified_at_ms,
            kms_provider: ERASE_MECHANISM.to_string(),
            kms_key_id,
            evidence_hash: EvidenceBundle::compute_hash(&bundle),
            region,
            attestation_key_id: kid,
        };
        let att = signer.sign(payload).unwrap();

        verify_attestation_signature(&att, &public_key).expect("attestation must verify");
        assert_eq!(att.payload.request_id, dsr_id);
        assert_eq!(att.payload.kms_provider, ERASE_MECHANISM);
    }

    /// A tampered payload (different request_id) must NOT verify — the
    /// signature binds the canonical payload, so the persisted index cannot be
    /// retro-pointed at a different DSR.
    #[test]
    fn tampered_request_id_fails_verification() {
        let seed = [3u8; 32];
        let region = Region::Enam;
        let sk = ErasureSigningKey::from_seed(1, region, 0, 0, seed);
        let pk = sk.public_key();
        let signer = ErasureAttestationSigner::new(sk);
        let bundle = EvidenceBundle {
            audit_chain_segment_ids: vec!["seg".to_string()],
            kms_destroy_ts: 1,
            kms_key_id: "dsr:a".to_string(),
            tenant_id: "t".to_string(),
        };
        let payload = ErasureAttestationPayload {
            tenant_id: "t".to_string(),
            request_id: "dsr-a".to_string(),
            destroyed_ts: 1,
            kms_provider: ERASE_MECHANISM.to_string(),
            kms_key_id: "dsr:a".to_string(),
            evidence_hash: EvidenceBundle::compute_hash(&bundle),
            region,
            attestation_key_id: 1,
        };
        let mut att = signer.sign(payload).unwrap();
        att.payload.request_id = "dsr-EVIL".to_string();
        att.canonical_payload_jcs = att.canonical_payload_jcs.replace("dsr-a", "dsr-EVIL");
        assert!(
            verify_attestation_signature(&att, &pk).is_err(),
            "tampered attestation must fail verification"
        );
    }

    // ---- finding #1: fail-CLOSED evidence gate ----

    use corelink_privacy_erasure_worker::backends::CANONICAL_EMPTY_TENANT_HASH;
    use corelink_privacy_erasure_worker::event::{
        canonical_backend_kinds, BackendCompletion, BackendErasureOutcome, BackendKind,
        BACKEND_COUNT,
    };
    use uuid::Uuid;

    fn completion(
        backend: BackendKind,
        outcome: BackendErasureOutcome,
        vhash: [u8; 32],
    ) -> BackendCompletion {
        BackendCompletion {
            dsr_id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            subject_id_hash: [0u8; 32],
            backend,
            outcome,
            idempotency_key: "k".to_string(),
            started_at_ms: 0,
            completed_at_ms: 1,
            retry_count: 0,
            verification_hash: vhash,
        }
    }

    /// Canonical 12, every backend successful + verified-empty.
    fn all_verified_completions() -> Vec<BackendCompletion> {
        canonical_backend_kinds()
            .iter()
            .map(|k| {
                let outcome = if k.is_effective() {
                    BackendErasureOutcome::Erased { records_deleted: 0 }
                } else {
                    BackendErasureOutcome::Pseudonymized { records_redacted: 0 }
                };
                completion(*k, outcome, CANONICAL_EMPTY_TENANT_HASH)
            })
            .collect()
    }

    #[test]
    fn evidence_segments_bind_real_hashes_when_all_verified() {
        let segments = super::verified_evidence_segments(&all_verified_completions())
            .expect("all-verified must produce segments");
        assert_eq!(segments.len(), BACKEND_COUNT);
        let empty_hex = hex::encode(CANONICAL_EMPTY_TENANT_HASH);
        assert!(segments.iter().all(|s| s.contains(&empty_hex)));
        assert!(segments
            .iter()
            .all(|s| !s.contains("dsr.erasure.completed.v1")));
    }

    #[test]
    fn evidence_refuses_when_a_backend_hash_is_not_empty_sentinel() {
        let comps: Vec<BackendCompletion> = canonical_backend_kinds()
            .iter()
            .map(|k| {
                if *k == BackendKind::Stripe {
                    completion(
                        BackendKind::Stripe,
                        BackendErasureOutcome::Pseudonymized { records_redacted: 1 },
                        [0x11; 32],
                    )
                } else {
                    completion(
                        *k,
                        BackendErasureOutcome::Erased { records_deleted: 0 },
                        CANONICAL_EMPTY_TENANT_HASH,
                    )
                }
            })
            .collect();
        assert!(
            super::verified_evidence_segments(&comps).is_none(),
            "an unverified arm must withhold the proof"
        );
    }

    #[test]
    fn evidence_refuses_on_unsuccessful_outcome() {
        let first = *canonical_backend_kinds().first().expect("12 kinds");
        let comps: Vec<BackendCompletion> = canonical_backend_kinds()
            .iter()
            .map(|k| {
                if *k == first {
                    completion(
                        *k,
                        BackendErasureOutcome::Failed { retry_after_seconds: 60 },
                        CANONICAL_EMPTY_TENANT_HASH,
                    )
                } else {
                    completion(
                        *k,
                        BackendErasureOutcome::Erased { records_deleted: 0 },
                        CANONICAL_EMPTY_TENANT_HASH,
                    )
                }
            })
            .collect();
        assert!(super::verified_evidence_segments(&comps).is_none());
    }

    #[test]
    fn evidence_refuses_on_wrong_completion_count() {
        assert!(super::verified_evidence_segments(&[]).is_none());
        let one = vec![completion(
            *canonical_backend_kinds().first().expect("12 kinds"),
            BackendErasureOutcome::Erased { records_deleted: 0 },
            CANONICAL_EMPTY_TENANT_HASH,
        )];
        assert!(super::verified_evidence_segments(&one).is_none());
    }

    // ---- Artifact 1: strict-ordered persistence (R2 → pubkey → index) ----

    /// In-memory [`AttestationSinks`] recording the R2 put + every D1 exec, so
    /// the ordering + all-or-nothing fail-CLOSED contract is testable offline.
    #[derive(Default)]
    struct RecordingSinks {
        fail_put: bool,
        puts: RefCell<Vec<(String, Vec<u8>)>>,
        execs: RefCell<Vec<(String, Vec<Value>)>>,
    }

    impl AttestationSinks for RecordingSinks {
        fn put_audit_object(&self, key: &str, bytes: Vec<u8>) -> Result<(), String> {
            if self.fail_put {
                return Err("simulated R2 outage".to_string());
            }
            self.puts.borrow_mut().push((key.to_string(), bytes));
            Ok(())
        }

        fn exec_d1(&self, sql: &str, params: Vec<Value>) -> Result<(), String> {
            self.execs.borrow_mut().push((sql.to_string(), params));
            Ok(())
        }
    }

    /// Happy path: a verified-complete DSR PUTs the signed bundle to R2 FIRST,
    /// then upserts the public key, then INSERTs the index row carrying BOTH
    /// `signature_ed25519` + `canonical_payload_jcs` + the bucket-qualified
    /// `r2_key`. The R2 object verifies against the seed-derived public key.
    #[test]
    fn round_trip_persists_signature_canonical_and_r2_key() {
        let seed = [9u8; 32];
        let dsr_id = "00000000-0000-7000-8000-000000000abc";
        let tenant_id = "00000000-0000-7000-8000-000000000def";
        let ts = 1_700_000_000_000u64;
        let region = Region::Weur;
        let kid = 1u64;

        let sinks = RecordingSinks::default();
        persist_signed_attestation(
            &sinks,
            region,
            &seed,
            kid,
            dsr_id,
            tenant_id,
            ts,
            &all_verified_completions(),
        );

        // Exactly one R2 PUT, at the canonical object key.
        let puts = sinks.puts.borrow();
        assert_eq!(puts.len(), 1, "exactly one signed bundle PUT");
        assert_eq!(puts[0].0, format!("erasure_attestations/{dsr_id}.json"));

        // The PUT body is the signed bundle and verifies against the seed key.
        let att: ErasureAttestation =
            serde_json::from_slice(&puts[0].1).expect("R2 body is a signed bundle");
        let pk = ErasureSigningKey::from_seed(kid, region, ts, 0, seed).public_key();
        verify_attestation_signature(&att, &pk).expect("persisted bundle must verify");
        assert!(!att.signature_ed25519.is_empty());
        assert!(!att.canonical_payload_jcs.is_empty());

        // Two D1 execs, in order: public key upsert THEN the index row.
        let execs = sinks.execs.borrow();
        assert_eq!(execs.len(), 2, "pubkey upsert + index insert");
        assert!(
            execs[0].0.contains("erasure_public_keys"),
            "pubkey upsert runs before the index row (verifiable-or-nothing)"
        );
        assert!(execs[1].0.contains("erasure_attestations"));

        // The index row carries sig (param 10), canonical (param 11), and the
        // bucket-qualified r2_key (param 5) — never NULL.
        let params = &execs[1].1;
        let r2_key = params[4].as_str().expect("r2_key param");
        let sig = params[9].as_str().expect("signature_ed25519 param");
        let canonical = params[10].as_str().expect("canonical_payload_jcs param");
        assert_eq!(
            r2_key,
            format!("corelink-audit-weur/erasure_attestations/{dsr_id}.json")
        );
        assert_eq!(sig, att.signature_ed25519);
        assert_eq!(canonical, att.canonical_payload_jcs);
        assert!(!sig.is_empty() && !canonical.is_empty());
    }

    /// R2 PUT failure ⇒ persist NOTHING: no D1 write of any kind (no dangling
    /// index row pointing at an object that was never written).
    #[test]
    fn r2_put_failure_persists_nothing() {
        let sinks = RecordingSinks {
            fail_put: true,
            ..Default::default()
        };
        persist_signed_attestation(
            &sinks,
            Region::Weur,
            &[9u8; 32],
            1,
            "00000000-0000-7000-8000-000000000abc",
            "00000000-0000-7000-8000-000000000def",
            1_700_000_000_000,
            &all_verified_completions(),
        );
        assert!(sinks.puts.borrow().is_empty(), "no object recorded on R2 failure");
        assert!(
            sinks.execs.borrow().is_empty(),
            "R2 failure must persist NOTHING to D1 (fail-CLOSED, no dangling r2_key)"
        );
    }

    /// Region-unauthorized: with `ERASURE_ATTESTATION_SINGLE_REGION` unset the
    /// region is withheld, so `sign_and_persist` returns BEFORE any sink call —
    /// persisting nothing. (The wrapper resolves the region from env via this
    /// gate; the persist core is never reached.)
    #[test]
    fn region_withheld_without_single_region_assertion() {
        std::env::set_var("ERASURE_ATTESTATION_REGION", "weur");
        std::env::remove_var("ERASURE_ATTESTATION_SINGLE_REGION");
        assert!(
            super::resolve_region_from_env().is_none(),
            "region must be withheld without an explicit single-region assertion"
        );
        // The explicit operator assertion is what authorizes the env region.
        std::env::set_var("ERASURE_ATTESTATION_SINGLE_REGION", "true");
        assert_eq!(super::resolve_region_from_env(), Some(Region::Weur));
        std::env::remove_var("ERASURE_ATTESTATION_REGION");
        std::env::remove_var("ERASURE_ATTESTATION_SINGLE_REGION");
    }
}
